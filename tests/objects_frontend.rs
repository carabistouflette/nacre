//! V1 semantic tests. No C toolchain required for this test target.
use nacre::{compile_to_c, validate};

macro_rules! accepts {
    ($name:ident, $source:expr) => { #[test] fn $name() {
        let source = $source; validate(source).unwrap_or_else(|e| panic!("{}", e.render("test.ncr", source)));
    }};
}
macro_rules! rejects {
    ($name:ident, $source:expr) => { #[test] fn $name() {
        let source = $source; assert!(validate(source).is_err(), "invalid source accepted");
    }};
}

accepts!(counter_example, include_str!("../examples/language/counter.ncr"));
accepts!(composition_example, include_str!("../examples/ownership/composition.ncr"));
accepts!(ownership_example, include_str!("../examples/ownership/ownership.ncr"));
accepts!(initializers_example, include_str!("../examples/language/field_initialization.ncr"));
accepts!(static_example, include_str!("../examples/language/static_methods.ncr"));
accepts!(scopes_example, include_str!("../examples/ownership/scope_cleanup.ncr"));
accepts!(empty_class, "class Empty {} void main() { Empty e = new Empty(); }");
accepts!(constructor_implicit_fields, "class C { int x; C(int v) { x = v; } int get() { return x; } } void main() { C c = new C(7); println(c.get()); }");
accepts!(forward_field_types, "class A { B b = new B(); } class B { int x = 1; } void main() { A a = new A(); println(a.b.x); }");
accepts!(parameter_shadows_field, "class C { int x; C(int x) { this.x = x; } } void main() { C c = new C(1); }");
accepts!(move_then_reinitialize, "class C {} void main() { C a = new C(); C b = a; a = new C(); }");
accepts!(object_return, "class C {} C make() { C c = new C(); return c; } void main() { C c = make(); }");
accepts!(identity_function, "class C {} C identity(C c) { return c; } void main() { C c = new C(); c = identity(c); }");
accepts!(same_variable_move_assignment, "class C { int n = 1; } void main() { C c = new C(); c = c; println(c.n); }");
accepts!(move_in_terminating_branch, "class C { int n = 1; } void consume(C c) {} void f(boolean b) { C c = new C(); if (b) { consume(c); return; } println(c.n); } void main() {}");
accepts!(both_branches_reinitialize, "class C { int n = 1; } void consume(C c) {} void main() { C c = new C(); consume(c); if (true) { c = new C(); } else { c = new C(); } println(c.n); }");
accepts!(local_loop_move, "class C {} void take(C c) {} void main() { int i = 0; while (i < 3) { C c = new C(); take(c); i = i + 1; } }");
accepts!(nested_borrowed_calls, "class C { int n = 1; int get() { return n; } int add(int x) { n = n + x; return n; } } void main() { C c = new C(); println(c.add(c.get())); }");
accepts!(private_method_from_same_class, "class C { private int f() { return 7; } int g() { return f(); } } void main() { C c = new C(); println(c.g()); }");
accepts!(private_constructor_factory, "class C { private C() {} static C create() { return new C(); } } void main() { C c = C.create(); }");
accepts!(field_init_reads_previous_field, "class C { int x = 2; int y = this.x + 3; } void main() { C c = new C(); }");
accepts!(method_returns_owned_new_object, "class B { int n = 9; } class A { B make() { return new B(); } } void main() { A a = new A(); B b = a.make(); println(b.n); }");
accepts!(class_and_local_distinct_namespaces, "class C { int n = 1; } void main() { C C = new C(); println(C.n); }");
accepts!(nested_field_mutation, "class B { int n = 1; } class A { B b = new B(); } void main() { A a = new A(); a.b.n = 5; }");
accepts!(constructor_early_return_after_init, "class C { int x; C(boolean b) { x = 1; if (b) { return; } x = 2; } } void main() {}");
accepts!(public_after_static_modifier, "class Main { static public void main() {} }");
accepts!(discard_new_object, "class C {} void main() { new C(); }");
accepts!(discard_object_return, "class C {} C make() { return new C(); } void main() { make(); }");
accepts!(branch_local_drops, "class C {} void main() { if (true) { C a = new C(); } else { C b = new C(); } }");
accepts!(owned_field_method, "class B { int n = 1; void inc() { n = n + 1; } } class A { B b = new B(); void f() { b.inc(); } } void main() { A a = new A(); a.f(); }");

rejects!(moved_local, include_str!("../examples/invalid/use_after_move.ncr"));
rejects!(private_field, include_str!("../examples/invalid/private_field.ncr"));
rejects!(uninitialized_field, include_str!("../examples/invalid/uninitialized_field.ncr"));
rejects!(moving_receiver, include_str!("../examples/invalid/move_receiver.ncr"));
rejects!(duplicate_class, "class C {} class C {} void main() {}");
rejects!(duplicate_field, "class C { int x = 0; int x = 1; } void main() {}");
rejects!(duplicate_method, "class C { int f() { return 1; } int f(int x) { return x; } } void main() {}");
rejects!(duplicate_constructor, "class C { C() {} C(int x) {} } void main() {}");
rejects!(bad_constructor_arity, "class C { C(int x) {} } void main() { C c = new C(); }");
rejects!(bad_constructor_type, "class C { C(int x) {} } void main() { C c = new C(false); }");
rejects!(unknown_field_type, "class C { Missing x; } void main() {}");
rejects!(unknown_return_type, "Missing make() { return 1; } void main() {}");
rejects!(nominal_class_types, "class A {} class B {} void main() { A a = new B(); }");
rejects!(unknown_field, "class C {} void main() { C c = new C(); println(c.x); }");
rejects!(unknown_method, "class C {} void main() { C c = new C(); c.f(); }");
rejects!(field_on_primitive, "void main() { int n = 1; println(n.x); }");
rejects!(method_on_primitive, "void main() { int n = 1; n.f(); }");
rejects!(this_in_free_function, "void main() { println(this.x); }");
rejects!(this_in_static, "class Main { int n = 1; static void main() { println(this.n); } }");
rejects!(implicit_field_in_static, "class Main { int n = 1; static void main() { println(n); } }");
rejects!(instance_call_via_class, "class C { void f() {} } void main() { C.f(); }");
rejects!(static_call_via_instance, "class C { static void f() {} } void main() { C c = new C(); c.f(); }");
rejects!(implicit_instance_call_in_static, "class Main { void f() {} static void main() { f(); } }");
rejects!(private_method, "class C { private void f() {} } void main() { C c = new C(); c.f(); }");
rejects!(private_static_method, "class C { private static void f() {} } void main() { C.f(); }");
rejects!(private_constructor, "class C { private C() {} } void main() { C c = new C(); }");
rejects!(constructor_read_before_init, "class C { int x; C() { println(x); x = 1; } } void main() {}");
rejects!(initializer_read_later_field, "class C { int x = y; int y = 1; } void main() {}");
rejects!(field_initializer_cannot_see_parameter, "class C { int x = seed; C(int seed) {} } void main() {}");
rejects!(method_before_complete_init, "class C { int x; C() { f(); x = 1; } void f() {} } void main() {}");
rejects!(constructor_return_before_init, "class C { int x; C() { return; } } void main() {}");
rejects!(conditional_field_init, "class C { int x; C() { if (true) { x = 1; } else { x = 2; } } } void main() {}");
rejects!(nested_block_field_init, "class C { int x; C() { { x = 1; } } } void main() {}");
rejects!(uninitialized_string_field, "class C { String s; } void main() {}");
rejects!(moved_parameter, "class C {} void f(C c) { C b = c; C d = c; } void main() {}");
rejects!(moved_argument_then_reuse, "class C { int n = 1; } void f(C c) {} void main() { C c = new C(); f(c); println(c.n); }");
rejects!(move_twice_in_arguments, "class C {} void f(C a, C b) {} void main() { C c = new C(); f(c, c); }");
rejects!(potential_move_branch, "class C { int n = 1; } void f(C c) {} void main() { C c = new C(); if (true) { f(c); } println(c.n); }");
rejects!(short_circuit_move, "class C { int n = 1; } boolean f(C c) { return true; } void main() { C c = new C(); println(false && f(c)); println(c.n); }");
accepts!(move_outer_object_then_break_v2, "class C {} void f(C c) {} void main() { C c = new C(); while (true) { f(c); break; } }");
rejects!(move_outer_in_loop_condition, "class C {} boolean f(C c) { return false; } void main() { C c = new C(); while (f(c)) {} }");
accepts!(move_outer_loop_local_then_inner_break_v2, "class C {} void f(C c) {} void main() { while (true) { C c = new C(); while (true) { f(c); break; } break; } }");
rejects!(return_this, "class C { C f() { return this; } } void main() {}");
rejects!(move_out_of_field, "class B {} class A { B b = new B(); } void main() { A a = new A(); B b = a.b; }");
rejects!(move_ancestor_of_receiver, "class B { int f(int n) { return n; } } class A { B b = new B(); } int consume(A a) { return 1; } void main() { A a = new A(); println(a.b.f(consume(a))); }");
rejects!(move_assignment_receiver, "class A { int n = 1; } int consume(A a) { return 1; } void main() { A a = new A(); a.n = consume(a); }");
rejects!(temporary_receiver, "class C { int f() { return 1; } } void main() { println(new C().f()); }");
rejects!(temporary_field_read, "class C { int n = 1; } void main() { println(new C().n); }");
rejects!(recursive_fields, "class Node { Node next; Node(Node n) { next = n; } } void main() {}");
rejects!(mutually_recursive_fields, "class A { B b; } class B { A a; } void main() {}");
rejects!(printing_object, "class C {} void main() { C c = new C(); println(c); }");
rejects!(static_field, "class C { static int n = 1; } void main() {}");
rejects!(static_constructor, "class C { static C() {} } void main() {}");
rejects!(void_field, "class C { void n; } void main() {}");
rejects!(ambiguous_entry, "void main() {} class Main { static void main() {} }");
rejects!(duplicate_visibility, "class C { public private int n = 1; } void main() {}");
rejects!(duplicate_static, "class Main { static static void main() {} }");
rejects!(reserved_system_class, "class System {} void main() {}");
rejects!(reserved_system_local, "void main() { int System = 1; }");
rejects!(bad_assignment_target, "void main() { (1 + 2) = 3; }");
rejects!(inheritance_still_unsupported, "class A {} class B extends A {} void main() {}");

#[test]
fn ownership_emission_contains_move_and_drop() {
    let c = compile_to_c("class C {} void main() { C a = new C(); C b = a; }").unwrap();
    assert!(c.contains("ownership transferred"));
    assert!(c.contains("ncr_drop_0(ncr_v_1)"));
    assert!(c.contains("calloc(1, size)"));
}
#[test]
fn object_codegen_is_deterministic() {
    let source = include_str!("../examples/ownership/composition.ncr");
    assert_eq!(compile_to_c(source).unwrap(), compile_to_c(source).unwrap());
}
#[test]
fn caps_class_count() {
    let mut source = String::new();
    for i in 0..129 { source.push_str(&format!("class C{i} {{}} ")); }
    source.push_str("void main() {}");
    assert!(validate(&source).is_err());
}
