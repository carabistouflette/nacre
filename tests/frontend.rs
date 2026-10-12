//! These tests require only Rust, not a C compiler.
use nacre::{compile_to_c, validate};

macro_rules! rejects {
    ($name:ident, $source:expr, $expected:expr) => {
        #[test]
        fn $name() {
            let err = validate($source).expect_err("source invalide accepte");
            assert!(err.message.contains($expected), "attendu {:?}, obtenu {:?}", $expected, err.message);
        }
    };
}

#[test]
fn accepts_all_examples() {
    for source in [
        include_str!("../examples/hello.ncr"),
        include_str!("../examples/fibonacci.ncr"),
        include_str!("../examples/fizzbuzz.ncr"),
        include_str!("../examples/control_flow.ncr"),
        include_str!("../examples/strings.ncr"),
        include_str!("../examples/overflow.ncr"),
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
    validate("/* commentaire */\r\nvoid main() { // salut\r\nprintln(2); /* fin */ }").unwrap();
}

#[test]
fn generation_is_deterministic() {
    let source = include_str!("../examples/fibonacci.ncr");
    assert_eq!(compile_to_c(source).unwrap(), compile_to_c(source).unwrap());
}


#[test]
fn error_has_source_location() {
    let source = "void main() {\n    int age = true;\n}";
    let diagnostic = validate(source).unwrap_err().render("test.ncr", source);
    assert!(diagnostic.contains("test.ncr:2:15: erreur"), "{diagnostic}");
    assert!(diagnostic.contains("attendu int, trouve boolean"));
    assert!(diagnostic.contains("^^^^"));
}

#[test]
fn unicode_and_eof_diagnostics_do_not_panic() {
    let sources = ["void main() { println(\"été\"); @ }", "void main() {", "void main() { println(\"é\"); é; }"];
    for source in sources {
        let error = validate(source).unwrap_err();
        assert!(error.render("utf8.ncr", source).contains("erreur"));
    }
}

#[test]
fn rejects_extreme_nesting_cleanly() {
    let source = format!("void main() {{ println({}1{}); }}", "(".repeat(150), ")".repeat(150));
    assert!(validate(&source).unwrap_err().message.contains("profonde"));
}

#[test]
fn rejects_overly_deep_left_associative_expression() {
    let source = format!("void main() {{ println(1{}); }}", "+1".repeat(150));
    assert!(validate(&source).unwrap_err().message.contains("profonde"));
}

rejects!(missing_main, "int f() { return 1; }", "manquante");
rejects!(bad_main_result, "int main() { return 0; }", "void main()");
rejects!(bad_main_parameters, "void main(int argc) {}", "sans parametres");
rejects!(bad_variable_type, "void main() { int age = true; }", "attendu int, trouve boolean");
rejects!(bad_assignment_type, "void main() { int a = 1; a = false; }", "type incompatible");
rejects!(unknown_variable, "void main() { println(x); }", "non declaree");
rejects!(unknown_assignment, "void main() { x = 1; }", "non declaree");
rejects!(self_initialization, "void main() { int x = x; }", "non declaree");
rejects!(unknown_function, "void main() { f(); }", "inconnue");
rejects!(bad_arity, "int f(int n) { return n; } void main() { f(); }", "attend 1 argument");
rejects!(bad_argument_type, "int f(int n) { return n; } void main() { f(false); }", "type incompatible");
rejects!(duplicate_function, "void main() {} void main() {}", "deja declaree");
rejects!(reserved_builtin, "void println() {} void main() {}", "reservee");
rejects!(duplicate_parameter, "void f(int n, int n) {} void main() {}", "deja declaree");
rejects!(duplicate_local, "void main() { int x = 1; int x = 2; }", "deja declaree");
rejects!(shadowing_local, "void main() { int x = 1; { int x = 2; } }", "portee active");
rejects!(shadowing_parameter, "void f(int n) { int n = 2; } void main() {}", "portee active");
rejects!(out_of_scope, "void main() { { int x = 1; } println(x); }", "non declaree");
#[test]
fn uninitialized_but_unused_local_is_allowed_v2() { validate("void main() { int x; }").unwrap(); }
rejects!(integer_condition, "void main() { if (1) { println(1); } }", "attendu boolean");
rejects!(integer_loop_condition, "void main() { while (1) {} }", "attendu boolean");
rejects!(bad_return, "int f() { return true; } void main() {}", "attendu int");
rejects!(missing_return, "int f(int n) { if (n > 0) { return n; } } void main() {}", "tous les chemins");
rejects!(bare_return_in_int, "int f() { return; } void main() {}", "attendu int");
rejects!(value_return_in_void, "void main() { return 1; }", "sans valeur");
rejects!(void_return_expression, "void main() { return println(1); }", "sans valeur");
rejects!(unreachable_statement, "void main() { return; println(1); }", "inaccessible");
rejects!(break_outside_loop, "void main() { break; }", "uniquement dans une boucle");
rejects!(continue_outside_loop, "void main() { continue; }", "uniquement dans une boucle");
rejects!(println_void, "void f() {} void main() { println(f()); }", "expression void");
rejects!(println_empty, "void main() { println(); }", "exactement un argument");
rejects!(println_two_arguments, "void main() { println(1, 2); }", "exactement un argument");
rejects!(bool_arithmetic, "void main() { println(true + 1); }", "attendu int");
rejects!(bool_ordering, "void main() { println(false < true); }", "attendu int");
rejects!(integer_logic, "void main() { println(1 && 2); }", "attendu boolean");
rejects!(integer_not, "void main() { println(!1); }", "attendu boolean");
rejects!(bool_negation, "void main() { println(-true); }", "attendu int");
rejects!(mixed_equality, "void main() { println(1 == true); }", "type incompatible");
rejects!(string_equality, "void main() { println(\"a\" == \"a\"); }", "limites a int et boolean");
rejects!(string_concat, "void main() { println(\"a\" + \"b\"); }", "attendu int");
rejects!(positive_literal_overflow, "void main() { println(2147483648); }", "hors limites");
rejects!(negative_literal_overflow, "void main() { println(-2147483649); }", "hors limites");
rejects!(giant_literal, "void main() { println(9999999999999999999999999999999); }", "hors limites");
rejects!(hexadecimal_literal, "void main() { println(0xff); }", "decimal invalide");
rejects!(unterminated_string, "void main() { println(\"abc); }", "non terminee");
rejects!(unterminated_comment, "void main() { /*", "non termine");
rejects!(unknown_escape, r#"void main() { println("\x"); }"#, "echappement");
rejects!(missing_semicolon, "void main() { println(1) }", "`;` attendu");
rejects!(missing_brace, "void main() {", "`}` attendu");
rejects!(required_if_braces, "void main() { if (true) println(1); }", "accolades obligatoires");
rejects!(unknown_object_class, "void main() { new Person(); }", "classe `Person` inconnue");
rejects!(instance_method, "class Main { void main() {} }", "methodes `static`");
#[test]
fn accepts_two_classes() { validate("class A { static void main() {} } class B {}").unwrap(); }
rejects!(arbitrary_qualified_call, "void main() { Math.abs(1); }", "non declaree");
rejects!(useless_expression_statement, "void main() { 1 + 2; }", "seul un appel");

rejects!(increment_not_supported, "void main() { int i = 0; i++; }", "pas pris en charge");
rejects!(decrement_not_supported, "void main() { int i = 0; i--; }", "pas pris en charge");
