/* Nacre runtime support: no GC and no VM; unique-owned object allocation.
 * libc is still used for output and process termination.
 */
#define _XOPEN_SOURCE 700
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <float.h>
#include <errno.h>
#include <string.h>
#include <unistd.h>
#include <locale.h>
#include <regex.h>

typedef struct {
    const unsigned char *data;
    size_t length;
} ncr_string;

/* Strict RFC 3629 scalar decoder shared by Text and OS boundary templates.
   NUL is a valid scalar; callers decide whether their C ABI permits it. */
static bool ncr_utf8_decode(
    const unsigned char *data, size_t length, size_t *index, uint32_t *scalar
) {
    if (data == NULL || index == NULL || scalar == NULL || *index >= length) { return false; }
    size_t cursor = *index;
    unsigned char first = data[cursor++];
    if (first <= 0x7f) {
        *index = cursor;
        *scalar = first;
        return true;
    }
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
    if (continuation_count > length - cursor) { return false; }
    for (size_t i = 0; i < continuation_count; ++i) {
        unsigned char next = data[cursor++];
        if ((next & 0xc0) != 0x80) { return false; }
        value = (value << 6) | (uint32_t)(next & 0x3f);
    }
    if (value < minimum || value > UINT32_C(0x10ffff)
        || (value >= UINT32_C(0xd800) && value <= UINT32_C(0xdfff))) {
        return false;
    }
    *index = cursor;
    *scalar = value;
    return true;
}

static bool ncr_utf8_valid(const unsigned char *data, size_t length) {
    if (length > 0 && data == NULL) { return false; }
    size_t index = 0;
    uint32_t scalar;
    while (index < length) {
        if (!ncr_utf8_decode(data, length, &index, &scalar)) { return false; }
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
    fprintf(stderr, "runtime error [%s:%zu:%zu]: %s\n", file, line, column, message);
#else
    fprintf(stderr, "runtime error [%zu:%zu]: %s\n", line, column, message);
#endif
    exit(70);
}
static _Noreturn void ncr_numeric_cast_fail(size_t line, size_t column) {
    ncr_fail("[E6102] checked numeric cast is out of range or not a Unicode scalar", line, column);
}
static bool ncr_is_unicode_scalar(uint32_t value) {
    return value <= UINT32_C(0x10ffff)
        && !(value >= UINT32_C(0xd800) && value <= UINT32_C(0xdfff));
}
static size_t ncr_utf8_encode_scalar(uint32_t value, unsigned char bytes[4]) {
    if (!ncr_is_unicode_scalar(value)) { return 0; }
    if (value <= UINT32_C(0x7f)) {
        bytes[0] = (unsigned char)value;
        return 1;
    }
    if (value <= UINT32_C(0x7ff)) {
        bytes[0] = (unsigned char)(0xc0u | (value >> 6));
        bytes[1] = (unsigned char)(0x80u | (value & 0x3fu));
        return 2;
    }
    if (value <= UINT32_C(0xffff)) {
        bytes[0] = (unsigned char)(0xe0u | (value >> 12));
        bytes[1] = (unsigned char)(0x80u | ((value >> 6) & 0x3fu));
        bytes[2] = (unsigned char)(0x80u | (value & 0x3fu));
        return 3;
    }
    bytes[0] = (unsigned char)(0xf0u | (value >> 18));
    bytes[1] = (unsigned char)(0x80u | ((value >> 12) & 0x3fu));
    bytes[2] = (unsigned char)(0x80u | ((value >> 6) & 0x3fu));
    bytes[3] = (unsigned char)(0x80u | (value & 0x3fu));
    return 4;
}
static int64_t ncr_i64_from_bits(uint64_t bits) {
    if (bits <= (uint64_t)INT64_MAX) { return (int64_t)bits; }
    return -1 - (int64_t)(UINT64_MAX - bits);
}
static int32_t ncr_i32_from_bits(uint32_t bits) { return ncr_wrap((int64_t)bits); }
static int64_t ncr_long_add(int64_t a, int64_t b) {
    return ncr_i64_from_bits((uint64_t)a + (uint64_t)b);
}
static int64_t ncr_long_sub(int64_t a, int64_t b) {
    return ncr_i64_from_bits((uint64_t)a - (uint64_t)b);
}
static int64_t ncr_long_mul(int64_t a, int64_t b) {
    return ncr_i64_from_bits((uint64_t)a * (uint64_t)b);
}
static int64_t ncr_long_neg(int64_t value) {
    return ncr_i64_from_bits(UINT64_C(0) - (uint64_t)value);
}
static int64_t ncr_long_div(int64_t a, int64_t b, size_t line, size_t column) {
    if (b == 0) { ncr_fail("[E6103] division by zero", line, column); }
    if (a == INT64_MIN && b == -1) { return INT64_MIN; }
    return a / b;
}
static int64_t ncr_long_rem(int64_t a, int64_t b, size_t line, size_t column) {
    if (b == 0) { ncr_fail("[E6103] division by zero", line, column); }
    if (a == INT64_MIN && b == -1) { return 0; }
    return a % b;
}
static int32_t ncr_i32_and(int32_t a, int32_t b) { return ncr_i32_from_bits((uint32_t)a & (uint32_t)b); }
static int32_t ncr_i32_or(int32_t a, int32_t b) { return ncr_i32_from_bits((uint32_t)a | (uint32_t)b); }
static int32_t ncr_i32_xor(int32_t a, int32_t b) { return ncr_i32_from_bits((uint32_t)a ^ (uint32_t)b); }
static int32_t ncr_i32_not(int32_t value) { return ncr_i32_from_bits(~(uint32_t)value); }
static int64_t ncr_i64_and(int64_t a, int64_t b) { return ncr_i64_from_bits((uint64_t)a & (uint64_t)b); }
static int64_t ncr_i64_or(int64_t a, int64_t b) { return ncr_i64_from_bits((uint64_t)a | (uint64_t)b); }
static int64_t ncr_i64_xor(int64_t a, int64_t b) { return ncr_i64_from_bits((uint64_t)a ^ (uint64_t)b); }
static int64_t ncr_i64_not(int64_t value) { return ncr_i64_from_bits(~(uint64_t)value); }
static int32_t ncr_i32_shl(int32_t value, int32_t count, size_t line, size_t column) {
    if (count < 0 || count >= 32) { ncr_fail("[E6104] shift count must be within the operand width", line, column); }
    return ncr_i32_from_bits((uint32_t)value << (uint32_t)count);
}
static int32_t ncr_i32_shr(int32_t value, int32_t count, size_t line, size_t column) {
    if (count < 0 || count >= 32) { ncr_fail("[E6104] shift count must be within the operand width", line, column); }
    if (count == 0) { return value; }
    uint32_t bits = (uint32_t)value;
    uint32_t shifted = bits >> (uint32_t)count;
    if (value < 0) { shifted |= UINT32_MAX << (32u - (uint32_t)count); }
    return ncr_i32_from_bits(shifted);
}
static int64_t ncr_i64_shl(int64_t value, int32_t count, size_t line, size_t column) {
    if (count < 0 || count >= 64) { ncr_fail("[E6104] shift count must be within the operand width", line, column); }
    return ncr_i64_from_bits((uint64_t)value << (uint32_t)count);
}
static int64_t ncr_i64_shr(int64_t value, int32_t count, size_t line, size_t column) {
    if (count < 0 || count >= 64) { ncr_fail("[E6104] shift count must be within the operand width", line, column); }
    if (count == 0) { return value; }
    uint64_t bits = (uint64_t)value;
    uint64_t shifted = bits >> (uint32_t)count;
    if (value < 0) { shifted |= UINT64_MAX << (64u - (uint32_t)count); }
    return ncr_i64_from_bits(shifted);
}
static uint8_t ncr_cast_byte_u32(uint32_t value, size_t line, size_t column) {
    if (value > UINT8_MAX) { ncr_numeric_cast_fail(line, column); }
    return (uint8_t)value;
}
static uint8_t ncr_cast_byte_i32(int32_t value, size_t line, size_t column) {
    if (value < 0 || value > UINT8_MAX) { ncr_numeric_cast_fail(line, column); }
    return (uint8_t)value;
}
static uint8_t ncr_cast_byte_i64(int64_t value, size_t line, size_t column) {
    if (value < 0 || value > UINT8_MAX) { ncr_numeric_cast_fail(line, column); }
    return (uint8_t)value;
}
static uint8_t ncr_cast_byte_f64(double value, size_t line, size_t column) {
    if (!isfinite(value) || value <= -1.0 || value >= 256.0) { ncr_numeric_cast_fail(line, column); }
    return (uint8_t)value;
}
static uint32_t ncr_cast_char_i32(int32_t value, size_t line, size_t column) {
    if (value < 0 || !ncr_is_unicode_scalar((uint32_t)value)) { ncr_numeric_cast_fail(line, column); }
    return (uint32_t)value;
}
static uint32_t ncr_cast_char_i64(int64_t value, size_t line, size_t column) {
    if (value < 0 || value > INT64_C(0x10ffff) || !ncr_is_unicode_scalar((uint32_t)value)) {
        ncr_numeric_cast_fail(line, column);
    }
    return (uint32_t)value;
}
static uint32_t ncr_cast_char_f64(double value, size_t line, size_t column) {
    if (!isfinite(value) || value <= -1.0 || value >= 1114112.0) { ncr_numeric_cast_fail(line, column); }
    uint32_t scalar = (uint32_t)value;
    if (!ncr_is_unicode_scalar(scalar)) { ncr_numeric_cast_fail(line, column); }
    return scalar;
}
static int32_t ncr_cast_i32_i64(int64_t value, size_t line, size_t column) {
    if (value < INT32_MIN || value > INT32_MAX) { ncr_numeric_cast_fail(line, column); }
    return (int32_t)value;
}
static int32_t ncr_cast_i32_f64(double value, size_t line, size_t column) {
    if (!isfinite(value) || value <= -2147483649.0 || value >= 2147483648.0) { ncr_numeric_cast_fail(line, column); }
    return (int32_t)value;
}
static int64_t ncr_cast_i64_f64(double value, size_t line, size_t column) {
    if (!isfinite(value) || value < -9223372036854775808.0 || value >= 9223372036854775808.0) { ncr_numeric_cast_fail(line, column); }
    return (int64_t)value;
}
static int32_t ncr_div(int32_t a, int32_t b, size_t line, size_t column) {
    if (b == 0) { ncr_fail("division by zero", line, column); }
    if (a == INT32_MIN && b == -1) { return INT32_MIN; }
    return a / b;
}
static int32_t ncr_rem(int32_t a, int32_t b, size_t line, size_t column) {
    if (b == 0) { ncr_fail("modulo by zero", line, column); }
    if (a == INT32_MIN && b == -1) { return 0; }
    return a % b;
}
static void ncr_print_int(int32_t value) { printf("%" PRId32 "\n", value); }
static void ncr_print_byte(uint8_t value) { printf("%u\n", (unsigned)value); }
static void ncr_print_char(uint32_t value) {
    unsigned char bytes[4];
    size_t length = ncr_utf8_encode_scalar(value, bytes);
    if (length == 0) { return; }
    (void)fwrite(bytes, 1, length, stdout);
    (void)fputc('\n', stdout);
}
static void ncr_print_long(int64_t value) { printf("%" PRId64 "\n", value); }
static void ncr_print_double(double value) { printf("%.*g\n", DBL_DECIMAL_DIG, value); }
static void ncr_print_unit(uint8_t value) { (void)value; puts("()"); }
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
        fprintf(stderr, "runtime error [%zu:%zu]: allocation failed\n", line, column);
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
