/* HANDWRITTEN C fixture, NOT output from the Rust compiler.
 * Checks only the pointer representation and ownership cleanup building blocks.
 * It cannot validate the parser, type checker, borrow checker, or C emitter. */
#include <assert.h>

typedef struct { int32_t value; } Counter;
typedef struct {
    Counter *target; /* shared borrowed field: must NOT be freed by drop_view */
    Counter *owned;  /* uniquely owned child: MUST be freed by drop_view */
} View;

static Counter *make_counter(int32_t value) {
    Counter *p = ncr_allocate(sizeof(Counter), 1, 1);
    p->value = value;
    return p;
}
static void drop_counter(Counter *p) {
    if (p != NULL) { ncr_deallocate(p); }
}
static void increment_borrowed(Counter *p) { p->value = ncr_add(p->value, 1); }
static int32_t read_borrowed(Counter *p) { return p->value; }
static View *make_view(Counter *borrowed) {
    View *v = ncr_allocate(sizeof(View), 1, 1);
    v->target = borrowed;
    v->owned = make_counter(7);
    return v;
}
static int32_t read_view(View *v) {
    return ncr_add(read_borrowed(v->target), read_borrowed(v->owned));
}
static void drop_view(View *v) {
    if (v == NULL) { return; }
    drop_counter(v->owned);
    /* No destruction of v->target and no reference counting. */
    ncr_deallocate(v);
}
int main(void) {
    Counter *owner = make_counter(0);
    for (int32_t i = 0; i < 10000; ++i) {
        /* Shared aliases require no additional allocation. */
        Counter *first = owner;
        Counter *second = owner;
        assert(read_borrowed(first) == read_borrowed(second));
        View *view = make_view(owner);
        assert(read_view(view) == i + 7);
        drop_view(view);
        /* Destruction of the view did not destroy its borrowed target. */
        increment_borrowed(owner);
        assert(owner->value == i + 1);
    }
    drop_counter(owner);
    assert(ncr_allocations == 20001);
    assert(ncr_frees == 20001);
    ncr_memory_report();
    return 0;
}
