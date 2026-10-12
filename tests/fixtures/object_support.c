/* HANDWRITTEN C FIXTURE, not output of the Rust compiler.
 * scripts/test_object_support.py prepends the exact src/runtime.c.
 * This checks allocator/drop building blocks and representative cleanup shapes,
 * NOT parsing, ownership analysis, or the code generation implementation. */
#include <assert.h>

typedef struct ncr_obj_0 ncr_obj_0;
typedef struct ncr_obj_1 ncr_obj_1;
struct ncr_obj_0 { int32_t ncr_f_0; };
struct ncr_obj_1 { ncr_obj_0 *ncr_f_0; };
static void ncr_drop_0(ncr_obj_0 *p);
static void ncr_drop_1(ncr_obj_1 *p);
static void ncr_drop_0(ncr_obj_0 *p) {
    if (p == NULL) { return; }
    ncr_deallocate(p);
}
static void ncr_drop_1(ncr_obj_1 *p) {
    if (p == NULL) { return; }
    ncr_drop_0(p->ncr_f_0);
    ncr_deallocate(p);
}
static ncr_obj_0 *make_leaf(int32_t value) {
    ncr_obj_0 *p = ncr_allocate(sizeof(ncr_obj_0), 1, 1);
    p->ncr_f_0 = value;
    return p;
}
static void consume(ncr_obj_0 *p) {
    assert(p->ncr_f_0 == 42);
    ncr_drop_0(p);
}
static ncr_obj_0 *make_and_return(void) {
    ncr_obj_0 *local = make_leaf(7);
    ncr_obj_0 *discarded = make_leaf(8);
    ncr_obj_0 *result = local;
    local = NULL;
    ncr_drop_0(discarded);
    discarded = NULL;
    ncr_drop_0(local);
    local = NULL;
    return result;
}
static void scoped_loop(void) {
    ncr_obj_0 *outer = make_leaf(100);
    for (int32_t i = 0; i < 5; ++i) {
        ncr_obj_0 *local = make_leaf(i);
        if (i == 1) {
            ncr_drop_0(local); local = NULL;
            continue;
        }
        if (i == 3) {
            ncr_drop_0(local); local = NULL;
            break;
        }
        ncr_drop_0(local); local = NULL;
    }
    ncr_drop_0(outer); outer = NULL;
}
int main(void) {
    /* move and drop-null */
    ncr_obj_0 *a = make_leaf(42);
    ncr_obj_0 *b = a; a = NULL;
    consume(b); b = NULL;
    ncr_drop_0(b); ncr_drop_0(a);
    /* replace an existing owner */
    a = make_leaf(1);
    ncr_obj_0 *replacement = make_leaf(2);
    ncr_drop_0(a); a = replacement;
    assert(a->ncr_f_0 == 2);
    ncr_drop_0(a); a = NULL;
    /* borrowed receiver and nested owned field */
    for (int i = 0; i < 10000; ++i) {
        ncr_obj_1 *parent = ncr_allocate(sizeof(ncr_obj_1), 1, 1);
        parent->ncr_f_0 = make_leaf(i);
        ncr_obj_0 *borrow = parent->ncr_f_0;
        borrow->ncr_f_0 = ncr_add(borrow->ncr_f_0, 1);
        assert(parent->ncr_f_0->ncr_f_0 == i + 1);
        ncr_drop_1(parent); parent = NULL;
    }
    a = make_and_return();
    assert(a->ncr_f_0 == 7);
    ncr_drop_0(a); a = NULL;
    scoped_loop();
    assert(ncr_allocations == 20010);
    assert(ncr_frees == ncr_allocations);
    ncr_memory_report();
    return 0;
}
