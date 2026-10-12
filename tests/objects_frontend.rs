//! V1 semantic tests. No C toolchain required for this test target.
use nacre::{compile_to_c, validate};

macro_rules! accepts {
    ($name:ident, $source:expr) => { #[test] fn $name() {
        let source = $source; validate(source).unwrap_or_else(|e| panic!("{}", e.render("test.ncr", source)));
    }};
}
macro_rules! rejects {
    ($name:ident, $source:expr, $expected:expr) => { #[test] fn $name() {
        let source = $source; let error = validate(source).expect_err("invalid source accepted");
        assert!(error.message.contains($expected), "expected {:?}, got {}", $expected, error.render("test.ncr", source));
    }};
}

accepts!(counter_example, include_str!("../examples/counter.ncr"));
accepts!(composition_example, include_str!("../examples/composition.ncr"));
accepts!(ownership_example, include_str!("../examples/ownership.ncr"));
accepts!(initializers_example, include_str!("../examples/field_initializers.ncr"));
accepts!(static_example, include_str!("../examples/static_methods.ncr"));
accepts!(scopes_example, include_str!("../examples/object_scopes.ncr"));
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

rejects!(moved_local, include_str!("../examples/invalid/use_after_move.ncr"), "deplace");
rejects!(private_field, include_str!("../examples/invalid/private_field.ncr"), "private");
rejects!(uninitialized_field, include_str!("../examples/invalid/uninitialized_field.ncr"), "non initialises");
rejects!(moving_receiver, include_str!("../examples/invalid/move_receiver.ncr"), "emprunte");
rejects!(duplicate_class, "class C {} class C {} void main() {}", "deja declaree");
rejects!(duplicate_field, "class C { int x = 0; int x = 1; } void main() {}", "deja declare");
rejects!(duplicate_method, "class C { int f() { return 1; } int f(int x) { return x; } } void main() {}", "pas de surcharge");
rejects!(duplicate_constructor, "class C { C() {} C(int x) {} } void main() {}", "un seul constructeur");
rejects!(bad_constructor_arity, "class C { C(int x) {} } void main() { C c = new C(); }", "attend 1 argument");
rejects!(bad_constructor_type, "class C { C(int x) {} } void main() { C c = new C(false); }", "attendu int");
rejects!(unknown_field_type, "class C { Missing x; } void main() {}", "classe `Missing` inconnue");
rejects!(unknown_return_type, "Missing make() { return 1; } void main() {}", "inconnue");
rejects!(nominal_class_types, "class A {} class B {} void main() { A a = new B(); }", "attendu A, trouve B");
rejects!(unknown_field, "class C {} void main() { C c = new C(); println(c.x); }", "champ `x` inconnu");
rejects!(unknown_method, "class C {} void main() { C c = new C(); c.f(); }", "methode `f` inconnue");
rejects!(field_on_primitive, "void main() { int n = 1; println(n.x); }", "pas un objet");
rejects!(method_on_primitive, "void main() { int n = 1; n.f(); }", "pas un objet");
rejects!(this_in_free_function, "void main() { println(this.x); }", "static");
rejects!(this_in_static, "class Main { int n = 1; static void main() { println(this.n); } }", "static");
rejects!(implicit_field_in_static, "class Main { int n = 1; static void main() { println(n); } }", "static");
rejects!(instance_call_via_class, "class C { void f() {} } void main() { C.f(); }", "exige un objet");
rejects!(static_call_via_instance, "class C { static void f() {} } void main() { C c = new C(); c.f(); }", "nom de classe");
rejects!(implicit_instance_call_in_static, "class Main { void f() {} static void main() { f(); } }", "static");
rejects!(private_method, "class C { private void f() {} } void main() { C c = new C(); c.f(); }", "private");
rejects!(private_static_method, "class C { private static void f() {} } void main() { C.f(); }", "private");
rejects!(private_constructor, "class C { private C() {} } void main() { C c = new C(); }", "private");
rejects!(constructor_read_before_init, "class C { int x; C() { println(x); x = 1; } } void main() {}", "non initialise");
rejects!(initializer_read_later_field, "class C { int x = y; int y = 1; } void main() {}", "non initialise");
rejects!(field_initializer_cannot_see_parameter, "class C { int x = seed; C(int seed) {} } void main() {}", "non declaree");
rejects!(method_before_complete_init, "class C { int x; C() { f(); x = 1; } void f() {} } void main() {}", "non initialises");
rejects!(constructor_return_before_init, "class C { int x; C() { return; } } void main() {}", "non initialises");
rejects!(conditional_field_init, "class C { int x; C() { if (true) { x = 1; } else { x = 2; } } } void main() {}", "directement");
rejects!(nested_block_field_init, "class C { int x; C() { { x = 1; } } } void main() {}", "directement");
rejects!(uninitialized_string_field, "class C { String s; } void main() {}", "non initialises");
rejects!(moved_parameter, "class C {} void f(C c) { C b = c; C d = c; } void main() {}", "deplace");
rejects!(moved_argument_then_reuse, "class C { int n = 1; } void f(C c) {} void main() { C c = new C(); f(c); println(c.n); }", "deplace");
rejects!(move_twice_in_arguments, "class C {} void f(C a, C b) {} void main() { C c = new C(); f(c, c); }", "deplace");
rejects!(potential_move_branch, "class C { int n = 1; } void f(C c) {} void main() { C c = new C(); if (true) { f(c); } println(c.n); }", "potentiellement");
rejects!(short_circuit_move, "class C { int n = 1; } boolean f(C c) { return true; } void main() { C c = new C(); println(false && f(c)); println(c.n); }", "deplace");
accepts!(move_outer_object_then_break_v2, "class C {} void f(C c) {} void main() { C c = new C(); while (true) { f(c); break; } }");
rejects!(move_outer_in_loop_condition, "class C {} boolean f(C c) { return false; } void main() { C c = new C(); while (f(c)) {} }", "deplace");
accepts!(move_outer_loop_local_then_inner_break_v2, "class C {} void f(C c) {} void main() { while (true) { C c = new C(); while (true) { f(c); break; } break; } }");
rejects!(return_this, "class C { C f() { return this; } } void main() {}", "deplacement de this");
rejects!(move_out_of_field, "class B {} class A { B b = new B(); } void main() { A a = new A(); B b = a.b; }", "champ objet interdit");
rejects!(replace_object_field, "class B {} class A { B b = new B(); } void main() { A a = new A(); a.b = new B(); }", "remplacement");
rejects!(double_object_field_initialization, "class B {} class A { B b; A() { b = new B(); b = new B(); } } void main() {}", "remplacement");
rejects!(move_ancestor_of_receiver, "class B { int f(int n) { return n; } } class A { B b = new B(); } int consume(A a) { return 1; } void main() { A a = new A(); println(a.b.f(consume(a))); }", "emprunte");
rejects!(move_assignment_receiver, "class A { int n = 1; } int consume(A a) { return 1; } void main() { A a = new A(); a.n = consume(a); }", "emprunte");
rejects!(temporary_receiver, "class C { int f() { return 1; } } void main() { println(new C().f()); }", "temporaire");
rejects!(temporary_field_read, "class C { int n = 1; } void main() { println(new C().n); }", "temporaire");
rejects!(recursive_fields, "class Node { Node next; Node(Node n) { next = n; } } void main() {}", "cycle");
rejects!(mutually_recursive_fields, "class A { B b; } class B { A a; } void main() {}", "cycle");
rejects!(printing_object, "class C {} void main() { C c = new C(); println(c); }", "afficher un objet");
rejects!(static_field, "class C { static int n = 1; } void main() {}", "champs static");
rejects!(static_constructor, "class C { static C() {} } void main() {}", "constructeur");
rejects!(void_field, "class C { void n; } void main() {}", "champ");
rejects!(ambiguous_entry, "void main() {} class Main { static void main() {} }", "ambigue");
rejects!(duplicate_visibility, "class C { public private int n = 1; } void main() {}", "visibilite");
rejects!(duplicate_static, "class Main { static static void main() {} }", "duplique");
rejects!(reserved_system_class, "class System {} void main() {}", "reserve");
rejects!(reserved_system_local, "void main() { int System = 1; }", "reserve");
rejects!(bad_assignment_target, "void main() { (1 + 2) = 3; }", "cible d'affectation");
rejects!(inheritance_still_unsupported, "class A {} class B extends A {} void main() {}", "pas encore pris en charge");

#[test]
fn ownership_emission_contains_move_and_drop() {
    let c = compile_to_c("class C {} void main() { C a = new C(); C b = a; }").unwrap();
    assert!(c.contains("ownership transferred"));
    assert!(c.contains("ncr_drop_0(ncr_v_1)"));
    assert!(c.contains("calloc(1, size)"));
}
#[test]
fn object_codegen_is_deterministic() {
    let source = include_str!("../examples/composition.ncr");
    assert_eq!(compile_to_c(source).unwrap(), compile_to_c(source).unwrap());
}
#[test]
fn caps_class_count() {
    let mut source = String::new();
    for i in 0..129 { source.push_str(&format!("class C{i} {{}} ")); }
    source.push_str("void main() {}");
    assert!(validate(&source).unwrap_err().message.contains("128 classes"));
}
