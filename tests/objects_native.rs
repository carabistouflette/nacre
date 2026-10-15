//! Tests of actual generated native executables. Requires GCC or Clang.
//! Missing tools are failures, never silent test skips.
mod support;

fn run(source: &str, stdout: &str, allocations: usize) {
    let output = support::compile(source, &["-DNACRE_MEMORY_STATS"]).unwrap()
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), stdout);
    assert_eq!(String::from_utf8(output.stderr).unwrap(),
        format!("NACRE_MEMORY allocations={allocations} frees={allocations} live=0\n"));
}
macro_rules! case {
    ($name:ident, $source:expr, $stdout:expr, $allocations:expr) => {
        #[test] fn $name() { run($source, $stdout, $allocations); }
    };
}
case!(counter, include_str!("../examples/language/counter.ncr"), "42\n", 0);
case!(composition, include_str!("../examples/ownership/composition.ncr"), "My car\n150\n", 1);
case!(ownership, include_str!("../examples/ownership/ownership.ncr"), "42\n42\n7\n", 2);
case!(initializers, include_str!("../examples/language/field_initialization.ncr"), "Settings\ntrue\n200\n", 1);
case!(statics, include_str!("../examples/language/static_methods.ncr"), "36\n49\n", 0);
case!(scopes, include_str!("../examples/ownership/scope_cleanup.ncr"), "0\n2\n100\n", 4);
case!(empty_class, "class Empty {} void main() { Empty e = new Empty(); }", "", 0);
case!(move_local, "class C { int n = 1; } void main() { C a = new C(); C b = a; println(b.n); }", "1\n", 1);
case!(self_assignment, "class C { int n = 1; } void main() { C a = new C(); a = a; println(a.n); }", "1\n", 1);
case!(object_replacement, "class C { int n; C(int n) { this.n = n; } } void main() { C a = new C(1); a = new C(2); println(a.n); }", "2\n", 2);
case!(identity_return, "class C { int n = 9; } C identity(C c) { return c; } void main() { C c = new C(); c = identity(c); println(c.n); }", "9\n", 1);
case!(return_from_inner_block, "class C { int n = 7; } C make() { C spare = new C(); { C result = new C(); return result; } } void main() { C c = make(); println(c.n); }", "7\n", 1);
case!(constructor_consumes_object, "class B { int n = 3; } class A { B b; A(B b) { this.b = b; } } void main() { B b = new B(); A a = new A(b); println(a.b.n); }", "3\n", 1);
case!(constructor_owned_field_replacement, "class B { int n; B(int n) { this.n = n; } } class A { B b; A() { b = new B(1); b = new B(2); } } void main() { A a = new A(); println(a.b.n); }", "2\n", 2);
case!(nested_composition, "class C { int n = 8; } class B { C c = new C(); } class A { B b = new B(); } void main() { A a = new A(); println(a.b.c.n); }", "8\n", 2);
case!(owned_field_mutation, "class B { int n = 1; void inc() { n = n + 1; } } class A { B b = new B(); void inc() { b.inc(); } } void main() { A a = new A(); a.inc(); println(a.b.n); }", "2\n", 1);
case!(read_field_before_rhs_mutation, "class C { int n = 1; int inc() { n = n + 1; return n; } } void main() { C c = new C(); println(c.n + c.inc()); println(c.n); }", "3\n2\n", 0);
case!(borrowed_receiver_nested_call, "class C { int n = 2; int get() { return n; } int add(int v) { n = n + v; return n; } } void main() { C c = new C(); println(c.add(c.get())); }", "4\n", 0);
case!(argument_evaluation_order, "class C { int n; C(int n) { this.n = n; println(n); } } void f(C a, C b) { println(a.n + b.n); } void main() { f(new C(1), new C(2)); }", "1\n2\n3\n", 2);
case!(method_argument_evaluation_order, "class C { int n = 0; int tick() { n = n + 1; println(n); return n; } int add(int a, int b) { return a + b; } } void main() { C c = new C(); println(c.add(c.tick(), c.tick())); }", "1\n2\n3\n", 0);
case!(constructor_initialization_order, "int mark(int n) { println(n); return n; } class C { int x = mark(1); int y = mark(2); C() { println(3); } } void main() { C c = new C(); }", "1\n2\n3\n", 0);
case!(early_return_cleans_locals, "class C {} void f(boolean b) { C outer = new C(); if (b) { C inner = new C(); return; } } void main() { f(true); }", "", 1);
case!(early_return_cleans_parameters, "class C {} void f(C c) { C extra = new C(); return; } void main() { f(new C()); }", "", 1);
case!(constructor_local_cleanup, "class B {} class A { B b; A() { B temporary = new B(); b = new B(); } } void main() { A a = new A(); }", "", 1);
case!(discard_new, "class C {} void main() { new C(); }", "", 1);
case!(discard_object_return, "class C {} C make() { return new C(); } void main() { make(); }", "", 1);
case!(discard_method_object_return, "class B {} class A { B make() { return new B(); } } void main() { A a = new A(); a.make(); }", "", 1);
case!(conditional_move_true, "class C {} void f(C c) {} void main() { C c = new C(); if (true) { f(c); } }", "", 1);
case!(conditional_move_false, "class C {} void f(C c) {} void main() { C c = new C(); if (false) { f(c); } }", "", 1);
case!(conditional_move_else, "class C {} void f(C c) {} void main() { C c = new C(); if (false) { println(1); } else { f(c); } }", "", 1);
case!(short_circuit_not_consumed, "class C {} boolean f(C c) { return true; } void main() { C c = new C(); println(false && f(c)); }", "false\n", 1);
case!(short_circuit_consumed, "class C {} boolean f(C c) { return true; } void main() { C c = new C(); println(true && f(c)); }", "true\n", 1);
case!(continue_and_break_nested_scope_cleanup, "class C {} void main() { C outer = new C(); int i = 0; while (i < 10) { C a = new C(); { C b = new C(); i = i + 1; if (i == 3) { break; } continue; } } }", "", 6);
case!(nested_loop_cleanup, "class C {} void main() { C root = new C(); int i = 0; while (i < 2) { C outer = new C(); int j = 0; while (j < 2) { C inner = new C(); j = j + 1; continue; } i = i + 1; } }", "", 6);
case!(loop_returns_owned_local, "class C { int n = 6; } C make() { while (true) { C c = new C(); return c; } return new C(); } void main() { C c = make(); println(c.n); }", "6\n", 1);
case!(loop_condition_owns_temporary, "class C {} boolean done(C c) { return false; } void main() { while (done(new C())) {} }", "", 1);
case!(field_strings_and_boolean, "class C { String s = \"hello\"; boolean b = true; } void main() { C c = new C(); println(c.s); println(c.b); c.s = \"world\"; c.b = false; println(c.s); println(c.b); }", "hello\ntrue\nworld\nfalse\n", 1);
case!(private_static_factory, "class C { int n = 4; private C() {} static C make() { return new C(); } } void main() { C c = C.make(); println(c.n); }", "4\n", 1);
case!(mangled_object_identifiers, "class printf { int size_t = 3; int free() { return size_t; } } void main() { printf malloc = new printf(); println(malloc.free()); }", "3\n", 0);
case!(many_scoped_allocations, "class C { int n; C(int n) { this.n = n; } } void main() { int i = 0; int sum = 0; while (i < 10000) { C c = new C(i); sum = sum + c.n; i = i + 1; } println(sum); }", "49995000\n", 10000);
case!(many_replacements, "class C {} void main() { C c = new C(); int i = 0; while (i < 10000) { c = new C(); i = i + 1; } }", "", 10001);
