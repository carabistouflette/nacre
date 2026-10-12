/* HANDWRITTEN V4 representation/ABI fixture, NOT output of the Rust compiler.
 * Prepend the exact src/runtime.c. Tests fat-pointer moves, typed vtable
 * adapters, dynamic destruction, borrowed views and specialized field layouts.
 */
#include <assert.h>

typedef struct { int32_t n; } Leaf;
typedef struct { Leaf *child; } Composite;
typedef struct { void *data; const struct VTable *vtable; } Interface;
struct VTable {
    void (*destroy)(void *);
    int32_t (*get)(void *);
    void (*increment)(void *);
    Interface (*make)(void *);
    int32_t (*consume)(void *, Interface);
};
typedef struct { Interface value; } BoxInterface;
typedef struct { int32_t value; } BoxInt;
typedef struct { ncr_string value; } BoxString;
typedef struct { Interface target; } BorrowedView;

static Interface make_leaf(int32_t n);
static void drop_interface(Interface value) {
    if (value.data != NULL) { value.vtable->destroy(value.data); }
}
static int32_t get_interface(Interface value) { return value.vtable->get(value.data); }
static void increment_interface(Interface value) { value.vtable->increment(value.data); }
static void leaf_destroy(void *p) { ncr_deallocate((Leaf *)p); }
static int32_t leaf_get(void *p) { return ((Leaf *)p)->n; }
static void leaf_inc(void *p) { ((Leaf *)p)->n = ncr_add(((Leaf *)p)->n, 1); }
static Interface leaf_make(void *p) { return make_leaf(ncr_add(leaf_get(p), 5)); }
static int32_t leaf_consume(void *p, Interface other) {
    int32_t result = ncr_add(leaf_get(p), get_interface(other));
    drop_interface(other); return result;
}
static const struct VTable LEAF_VTABLE = {
    .destroy = leaf_destroy, .get = leaf_get, .increment = leaf_inc,
    .make = leaf_make, .consume = leaf_consume,
};
static Interface make_leaf(int32_t n) {
    Leaf *p = ncr_allocate(sizeof(Leaf), 1, 1); p->n = n;
    return (Interface){ (void *)p, &LEAF_VTABLE };
}
static void composite_destroy(void *p) {
    Composite *c = (Composite *)p;
    ncr_deallocate(c->child); ncr_deallocate(c);
}
static int32_t composite_get(void *p) { return ncr_add(((Composite *)p)->child->n, 100); }
static void composite_inc(void *p) { leaf_inc(((Composite *)p)->child); }
static Interface composite_make(void *p) { return make_leaf(ncr_add(composite_get(p), 5)); }
static int32_t composite_consume(void *p, Interface other) {
    int32_t result = ncr_add(composite_get(p), get_interface(other));
    drop_interface(other); return result;
}
static const struct VTable COMPOSITE_VTABLE = {
    .destroy = composite_destroy, .get = composite_get, .increment = composite_inc,
    .make = composite_make, .consume = composite_consume,
};
static Interface make_composite(int32_t n) {
    Composite *p = ncr_allocate(sizeof(Composite), 1, 1);
    p->child = ncr_allocate(sizeof(Leaf), 1, 1); p->child->n = n;
    return (Interface){ (void *)p, &COMPOSITE_VTABLE };
}
static void drop_box_interface(BoxInterface *p) {
    if (p == NULL) { return; }
    drop_interface(p->value); ncr_deallocate(p);
}
int main(void) {
    for (int32_t n = 0; n < 10000; ++n) {
        Interface owner = make_leaf(n);
        size_t before_borrow = ncr_allocations;
        Interface borrowed = owner;
        increment_interface(borrowed);
        assert(ncr_allocations == before_borrow);
        assert(get_interface(owner) == n + 1);
        BoxInterface *box = ncr_allocate(sizeof(BoxInterface), 1, 1);
        box->value = owner; owner = (Interface){NULL, NULL};
        assert(get_interface(box->value) == n + 1);
        drop_interface(owner); /* Moved-from values have safe cleanup. */
        drop_box_interface(box);

        Interface composite = make_composite(n);
        Interface made = composite.vtable->make(composite.data);
        Interface argument = made; made = (Interface){NULL, NULL};
        int32_t sum = composite.vtable->consume(composite.data, argument);
        argument = (Interface){NULL, NULL};
        assert(sum == 2 * n + 205);
        drop_interface(argument); drop_interface(made); drop_interface(composite);
    }
    BoxInt *integer = ncr_allocate(sizeof(BoxInt), 1, 1); integer->value = 42;
    assert(integer->value == 42); ncr_deallocate(integer);
    static const unsigned char text[] = "hello";
    BoxString *string = ncr_allocate(sizeof(BoxString), 1, 1);
    string->value = (ncr_string){text, 5}; assert(string->value.length == 5);
    ncr_deallocate(string);

    Interface owner = make_leaf(42);
    BorrowedView *view = ncr_allocate(sizeof(BorrowedView), 1, 1);
    view->target = owner; assert(get_interface(view->target) == 42);
    ncr_deallocate(view); /* The borrowed target is NOT destroyed. */
    increment_interface(owner); assert(get_interface(owner) == 43); drop_interface(owner);
    assert(ncr_allocations == 50004 && ncr_frees == 50004);
    ncr_memory_report(); return 0;
}
