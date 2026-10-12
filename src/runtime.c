/* Nacre runtime support: no GC and no VM; unique-owned object allocation.
 * libc is still used for output and process termination.
 */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

typedef struct {
    const unsigned char *data;
    size_t length;
} ncr_string;

/* Strict RFC 3629 validation: reject overlong sequences, surrogates and
   code points above U+10FFFF. NUL is valid UTF-8 and remains length-aware. */
static bool ncr_utf8_valid(const unsigned char *data, size_t length) {
    size_t index = 0;
    while (index < length) {
        unsigned char first = data[index++];
        if (first <= 0x7f) { continue; }
        uint32_t value;
        uint32_t minimum;
        size_t continuation_count;
        if (first >= 0xc2 && first <= 0xdf) {
            value = (uint32_t)(first & 0x1f); minimum = 0x80; continuation_count = 1;
        } else if (first >= 0xe0 && first <= 0xef) {
            value = (uint32_t)(first & 0x0f); minimum = 0x800; continuation_count = 2;
        } else if (first >= 0xf0 && first <= 0xf4) {
            value = (uint32_t)(first & 0x07); minimum = 0x10000; continuation_count = 3;
        } else {
            return false;
        }
        if (continuation_count > length - index) { return false; }
        for (size_t i = 0; i < continuation_count; ++i) {
            unsigned char next = data[index++];
            if ((next & 0xc0) != 0x80) { return false; }
            value = (value << 6) | (uint32_t)(next & 0x3f);
        }
        if (value < minimum || value > UINT32_C(0x10ffff)
            || (value >= UINT32_C(0xd800) && value <= UINT32_C(0xdfff))) {
            return false;
        }
    }
    return true;
}

/* Explicit wrapping: do not rely on C signed overflow or out-of-range casts. */
static int32_t ncr_wrap(int64_t value) {
    uint32_t bits = (uint32_t)value;
    if (bits <= (uint32_t)INT32_MAX) {
        return (int32_t)bits;
    }
    return -1 - (int32_t)(UINT32_MAX - bits);
}
static int32_t ncr_add(int32_t a, int32_t b) { return ncr_wrap((int64_t)a + (int64_t)b); }
static int32_t ncr_sub(int32_t a, int32_t b) { return ncr_wrap((int64_t)a - (int64_t)b); }
static int32_t ncr_mul(int32_t a, int32_t b) { return ncr_wrap((int64_t)a * (int64_t)b); }
static int32_t ncr_neg(int32_t a) { return ncr_wrap(-(int64_t)a); }

static _Noreturn void ncr_fail(const char *message, size_t line, size_t column) {
#ifdef NACRE_MULTI_SOURCE
    const char *file = ncr_source_file(&line);
    fprintf(stderr, "erreur d'execution [%s:%zu:%zu]: %s\n", file, line, column, message);
#else
    fprintf(stderr, "erreur d'execution [%zu:%zu]: %s\n", line, column, message);
#endif
    exit(70);
}
static int32_t ncr_div(int32_t a, int32_t b, size_t line, size_t column) {
    if (b == 0) { ncr_fail("division par zero", line, column); }
    if (a == INT32_MIN && b == -1) { return INT32_MIN; }
    return a / b;
}
static int32_t ncr_rem(int32_t a, int32_t b, size_t line, size_t column) {
    if (b == 0) { ncr_fail("modulo par zero", line, column); }
    if (a == INT32_MIN && b == -1) { return 0; }
    return a % b;
}
static void ncr_print_int(int32_t value) { printf("%" PRId32 "\n", value); }
static void ncr_print_boolean(bool value) { puts(value ? "true" : "false"); }
static void ncr_print_string(ncr_string value) {
    /* A length, rather than strlen, preserves embedded NUL and UTF-8 bytes. */
    (void)fwrite(value.data, 1, value.length, stdout);
    (void)fputc('\n', stdout);
}

/* Object allocation. No tracing, reference counting, or object registry.
   The frontend enforces unique ownership; generated lexical drops do the work.
   Optional counters are diagnostics only and never manage object lifetimes. */
#ifdef NACRE_MEMORY_STATS
static size_t ncr_allocations = 0;
static size_t ncr_frees = 0;
#endif
static void *ncr_allocate(size_t size, size_t line, size_t column) {
    void *p = calloc(1, size);
    if (p == NULL) {
        fprintf(stderr, "erreur execution %zu:%zu: allocation impossible\n", line, column);
        exit(70);
    }
#ifdef NACRE_MEMORY_STATS
    ++ncr_allocations;
#endif
    return p;
}
static void ncr_deallocate(void *p) {
    if (p == NULL) { return; }
#ifdef NACRE_MEMORY_STATS
    ++ncr_frees;
#endif
    free(p);
}
static void ncr_memory_report(void) {
#ifdef NACRE_MEMORY_STATS
    fprintf(stderr, "NACRE_MEMORY allocations=%zu frees=%zu live=%zu\n",
        ncr_allocations, ncr_frees, ncr_allocations - ncr_frees);
#endif
}
