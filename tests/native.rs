//! Integration tests that compile actual native executables. Requires cc/clang/gcc.
//! A missing C compiler is a failure, never a silent skip.
use nacre::native;
mod support;
use std::process::Output;

fn execute(source: &str) -> Output {
    support::compile(source, &[]).unwrap().output().unwrap()
}
fn assert_stdout(source: &str, expected: &str) {
    let output = execute(source);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    assert!(output.stderr.is_empty());
}

#[test]
fn hello() { assert_stdout(include_str!("../examples/language/hello_world.ncr"), "Hello from Nacre!\n42\ntrue\n"); }
#[test]
fn fibonacci() { assert_stdout(include_str!("../examples/algorithms/fibonacci.ncr"), "Fibonacci from 0 to 10:\n0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n55\n"); }
#[test]
fn fizzbuzz() { assert_stdout(include_str!("../examples/algorithms/fizz_buzz.ncr"), "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n16\n17\nFizz\n19\nBuzz\n"); }
#[test]
fn break_continue_and_short_circuit() { assert_stdout(include_str!("../examples/language/control_flow.ncr"), "25\nfalse\n"); }
#[test]
fn overflow_boundaries() { assert_stdout(include_str!("../examples/language/integer_overflow.ncr"), "-2147483648\n2147483647\n1\n-2147483648\n0\n"); }
#[test]
fn static_strings() { assert_stdout(include_str!("../examples/text/text_basics.ncr"), "A UTF-8 string with a café character and no language-level allocation.\nHi!\nOne line\nand another.\n"); }

#[test]
fn empty_program() { assert_stdout("void main() {}", ""); }
#[test]
fn arithmetic_precedence_and_associativity() {
    assert_stdout("void main() { println(2 + 3 * 4); println((2 + 3) * 4); println(100 / 10 / 2); println(10 - 3 - 2); println(-7 / 3); println(-7 % 3); println(7 % -3); println(-(-2147483648)); }",
        "14\n20\n5\n5\n-2\n-1\n1\n-2147483648\n");
}
#[test]
fn boolean_precedence() {
    assert_stdout("void main() { println(true || false && false); println(!false && 2 < 3); println(1 < 2 == true); println(true == false == false); }", "true\ntrue\ntrue\ntrue\n");
}
#[test]
fn left_to_right_evaluation() {
    assert_stdout("int mark(int n) { println(n); return n; } int add(int a, int b) { return a + b; }
        void main() { println(add(mark(1), mark(2))); println(mark(3) + mark(4)); }", "1\n2\n3\n3\n4\n7\n");
}
#[test]
fn boolean_short_circuit_side_effects() {
    assert_stdout("boolean noisy() { println(99); return true; }
        void main() { println(false && noisy()); println(true || noisy()); println(true && noisy()); println(false || noisy()); }", "false\ntrue\n99\ntrue\n99\ntrue\n");
}
#[test]
fn skipped_division_by_zero() {
    assert_stdout("void main() { println(false && 1 / 0 == 0); println(true || 1 / 0 == 0); }", "false\ntrue\n");
}
#[test]
fn loop_condition_recomputed_after_continue() {
    assert_stdout("boolean next(int i) { println(i); return i < 3; }
        void main() { int i = 0; while (next(i)) { i = i + 1; if (i == 2) { continue; } println(10 + i); } }", "0\n11\n1\n2\n13\n3\n");
}
#[test]
fn nested_loops() {
    assert_stdout("void main() { int i = 0; int sum = 0; while (i < 3) { int j = 0; while (j < 3) { j = j + 1; if (j == 2) { break; } sum = sum + 1; } i = i + 1; } println(sum); }", "3\n");
}
#[test]
fn parameters_are_passed_by_value() {
    assert_stdout("void f(int n) { n = n + 1; println(n); } void main() { int n = 5; f(n); println(n); }", "6\n5\n");
}
#[test]
fn forward_and_mutually_recursive_functions() {
    assert_stdout("boolean even(int n) { if (n == 0) { return true; } return odd(n - 1); }
        boolean odd(int n) { if (n == 0) { return false; } return even(n - 1); }
        void main() { println(even(10)); println(odd(10)); }", "true\nfalse\n");
}
#[test]
fn both_branches_return() {
    assert_stdout("int abs(int n) { if (n < 0) { return -n; } else { return n; } }
        void main() { println(abs(-4)); println(abs(4)); }", "4\n4\n");
}
#[test]
fn void_early_return() {
    assert_stdout("void f(int n) { if (n == 0) { return; } println(n); } void main() { f(0); f(3); }", "3\n");
}
#[test]
fn discard_non_void_return_value() {
    assert_stdout("int f() { println(1); return 2; } void main() { f(); }", "1\n");
}
#[test]
fn c_reserved_names_are_safe() {
    assert_stdout("int printf(int int32_t) { return int32_t; } void main() { int size_t = 42; println(printf(size_t)); }", "42\n");
}
#[test]
fn empty_and_embedded_nul_strings() {
    let out = execute(r#"void main() { println(""); println("a\0b"); println("é 🦀"); }"#);
    assert!(out.status.success());
    assert_eq!(out.stdout, "\na\0b\né 🦀\n".as_bytes());
}
#[test]
fn strings_with_quotes_backslash_and_percent() {
    assert_stdout(r#"void main() { println("\"salut\" \\ 100% %n"); }"#, "\"salut\" \\ 100% %n\n");
}
#[test]
fn division_by_zero_is_controlled() {
    let out = execute("void main() { println(1 / 0); }");
    assert_eq!(out.status.code(), Some(70));
    assert!(String::from_utf8_lossy(&out.stderr).contains("[1:"));
}
#[test]
fn modulo_by_zero_is_controlled() {
    let out = execute("void main() { println(1 % 0); }");
    assert_eq!(out.status.code(), Some(70));
    assert!(String::from_utf8_lossy(&out.stderr).contains("[1:"));
}
#[test]
fn generated_file_is_removed_on_drop() {
    let program = support::compile("void main() {}", &[]).unwrap();
    let path = program.path().to_path_buf();
    assert!(path.exists());
    drop(program);
    assert!(!path.exists());
}

#[test]
fn standalone_native_markers_include_required_libraries_and_headers() {
    for source in [
        r#"/* nacre-link: ssl */
#include <openssl/ssl.h>
#include <openssl/evp.h>
#include <stdio.h>
int main(void) {
    SSL_CTX *context = SSL_CTX_new(TLS_method());
    if (context == NULL) { return 1; }
    printf("%d\n", EVP_MD_size(EVP_sha256()));
    SSL_CTX_free(context);
    return 0;
}"#,
        r#"/* nacre-link: xslt */
#include <libxslt/xsltInternals.h>
#include <stdio.h>
int main(void) {
    xmlDocPtr document = xmlNewDoc(BAD_CAST "1.0");
    xsltStylesheetPtr stylesheet = xsltNewStylesheet();
    if (document == NULL || stylesheet == NULL) { return 1; }
    printf("%d\n", xmlStrlen(BAD_CAST "abcdefghijklmnopqrstuvwxyz123456"));
    xmlFreeDoc(document);
    xsltFreeStylesheet(stylesheet);
    return 0;
}"#,
    ] {
        let output = native::compile_c(source).unwrap().output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, b"32\n");
    }
}

#[test]
fn native_dependency_comments_must_be_complete_standalone_markers() {
    let source = r#"#include <libxslt/xsltInternals.h>
int main(void) { xsltFreeStylesheet(xsltNewStylesheet()); return 0; }"#;
    for marker in ["/* nacre-link: xslt */ trailing", "/* nacre-link: unknown */"] {
        assert!(native::compile_c(&format!("{marker}\n{source}")).is_err());
    }
}
