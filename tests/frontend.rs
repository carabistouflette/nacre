//! These tests require only Rust, not a C compiler.
use nacre::{compile_to_c, validate};

macro_rules! rejects {
    ($name:ident, $source:expr) => {
        #[test]
        fn $name() {
            assert!(validate($source).is_err(), "invalid source accepted");
        }
    };
}

#[test]
fn accepts_all_examples() {
    for source in [
        include_str!("../examples/language/hello_world.ncr"),
        include_str!("../examples/algorithms/fibonacci.ncr"),
        include_str!("../examples/algorithms/fizz_buzz.ncr"),
        include_str!("../examples/language/control_flow.ncr"),
        include_str!("../examples/text/text_basics.ncr"),
        include_str!("../examples/language/integer_overflow.ncr"),
    ] { validate(source).unwrap(); }
}

#[test]
fn accepts_empty_main() { validate("void main() {}").unwrap(); }

#[test]
fn accepts_forward_call_and_mutual_recursion() {
    validate("boolean even(int n) { if (n == 0) { return true; } return odd(n - 1); }
        boolean odd(int n) { if (n == 0) { return false; } return even(n - 1); }
        void main() { println(even(4)); }").unwrap();
}

#[test]
fn accepts_block_scope_and_disjoint_names() {
    validate("void main() { { int a = 1; println(a); } { int a = 2; println(a); } }").unwrap();
}

#[test]
fn accepts_both_return_branches() {
    validate("int f(boolean b) { if (b) { return 1; } else { return 2; } } void main() { println(f(true)); }").unwrap();
}

#[test]
fn accepts_comments_and_crlf() {
    validate("/* comment */\r\nvoid main() { // hello\r\nprintln(2); /* end */ }").unwrap();
}

#[test]
fn accepts_comments_and_bare_cr_line_endings() {
    validate("void main() { // hello\rprintln(2);\r}").unwrap();
}

#[test]
fn generation_is_deterministic() {
    let source = include_str!("../examples/algorithms/fibonacci.ncr");
    assert_eq!(compile_to_c(source).unwrap(), compile_to_c(source).unwrap());
}


#[test]
fn error_has_source_location() {
    let source = "void main() {\n    int age = true;\n}";
    let diagnostic = validate(source).unwrap_err().render("test.ncr", source);
    assert!(diagnostic.contains("test.ncr:2:15:"), "{diagnostic}");
    assert!(diagnostic.contains("^^^^"));
}


#[test]
fn rejects_extreme_nesting_cleanly() {
    let source = format!("void main() {{ println({}1{}); }}", "(".repeat(150), ")".repeat(150));
    assert!(validate(&source).is_err());
}

#[test]
fn rejects_overly_deep_left_associative_expression() {
    let source = format!("void main() {{ println(1{}); }}", "+1".repeat(150));
    assert!(validate(&source).is_err());
}

rejects!(missing_main, "int f() { return 1; }");
rejects!(bad_main_result, "int main() { return 0; }");
rejects!(bad_main_parameters, "void main(int argc) {}");
rejects!(bad_variable_type, "void main() { int age = true; }");
rejects!(bad_assignment_type, "void main() { int a = 1; a = false; }");
rejects!(unknown_variable, "void main() { println(x); }");
rejects!(unknown_assignment, "void main() { x = 1; }");
rejects!(self_initialization, "void main() { int x = x; }");
rejects!(unknown_function, "void main() { f(); }");
rejects!(bad_arity, "int f(int n) { return n; } void main() { f(); }");
rejects!(bad_argument_type, "int f(int n) { return n; } void main() { f(false); }");
rejects!(duplicate_function, "void main() {} void main() {}");
rejects!(reserved_builtin, "void println() {} void main() {}");
rejects!(duplicate_parameter, "void f(int n, int n) {} void main() {}");
rejects!(duplicate_local, "void main() { int x = 1; int x = 2; }");
rejects!(shadowing_local, "void main() { int x = 1; { int x = 2; } }");
rejects!(shadowing_parameter, "void f(int n) { int n = 2; } void main() {}");
rejects!(out_of_scope, "void main() { { int x = 1; } println(x); }");
#[test]
fn uninitialized_but_unused_local_is_allowed_v2() { validate("void main() { int x; }").unwrap(); }
rejects!(integer_condition, "void main() { if (1) { println(1); } }");
rejects!(integer_loop_condition, "void main() { while (1) {} }");
rejects!(bad_return, "int f() { return true; } void main() {}");
rejects!(missing_return, "int f(int n) { if (n > 0) { return n; } } void main() {}");
rejects!(bare_return_in_int, "int f() { return; } void main() {}");
rejects!(value_return_in_void, "void main() { return 1; }");
rejects!(void_return_expression, "void main() { return println(1); }");
rejects!(unreachable_statement, "void main() { return; println(1); }");
rejects!(break_outside_loop, "void main() { break; }");
rejects!(continue_outside_loop, "void main() { continue; }");
rejects!(println_void, "void f() {} void main() { println(f()); }");
rejects!(println_empty, "void main() { println(); }");
rejects!(println_two_arguments, "void main() { println(1, 2); }");
rejects!(bool_arithmetic, "void main() { println(true + 1); }");
rejects!(bool_ordering, "void main() { println(false < true); }");
rejects!(integer_logic, "void main() { println(1 && 2); }");
rejects!(integer_not, "void main() { println(!1); }");
rejects!(bool_negation, "void main() { println(-true); }");
rejects!(mixed_equality, "void main() { println(1 == true); }");
rejects!(string_equality, "void main() { println(\"a\" == \"a\"); }");
rejects!(string_concat, "void main() { println(\"a\" + \"b\"); }");
rejects!(positive_literal_overflow, "void main() { println(2147483648); }");
rejects!(negative_literal_overflow, "void main() { println(-2147483649); }");
rejects!(giant_literal, "void main() { println(9999999999999999999999999999999); }");
rejects!(hexadecimal_literal, "void main() { println(0xff); }");
rejects!(unterminated_string, "void main() { println(\"abc); }");
rejects!(unterminated_comment, "void main() { /*");
rejects!(unknown_escape, r#"void main() { println("\x"); }"#);
rejects!(missing_semicolon, "void main() { println(1) }");
rejects!(missing_brace, "void main() {");
rejects!(required_if_braces, "void main() { if (true) println(1); }");
rejects!(unknown_object_class, "void main() { new Person(); }");
rejects!(instance_method, "class Main { void main() {} }");
#[test]
fn accepts_two_classes() { validate("class A { static void main() {} } class B {}").unwrap(); }
rejects!(arbitrary_qualified_call, "void main() { Math.abs(1); }");
rejects!(useless_expression_statement, "void main() { 1 + 2; }");

rejects!(increment_not_supported, "void main() { int i = 0; i++; }");
rejects!(decrement_not_supported, "void main() { int i = 0; i--; }");
