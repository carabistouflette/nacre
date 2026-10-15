use nacre::{compile_to_c, fmt, native, validate};
use std::fs;
#[test]
fn assert_builtin_succeeds_and_fails_with_code_70() {
    let passing_source = "void main() { assert(1 + 1 == 2, \"truth\"); }";
    let c = compile_to_c(passing_source).unwrap();
    let program = native::compile_c(&c).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success());

    let failing_source = "void main() { assert(1 + 1 == 3, \"math broke\"); }";
    let c = compile_to_c(failing_source).unwrap();
    let program = native::compile_c(&c).unwrap();
    let output = program.output().unwrap();
    assert_eq!(output.status.code(), Some(70));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("assertion failed [1:15]: math broke"), "{stderr}");
}

#[test]
fn formatter_is_idempotent_and_preserves_comments() {
    let unformatted = r#"// Header comment
class Foo{
private int x ;
public Foo(int v ){
this.x=v;
}
public int get(){
return this.x;
}
}

void main(){
Foo f=new Foo(42);
assert(f.get()==42);
}
"#;

    let formatted_once = fmt::format_source(unformatted).unwrap();
    assert!(formatted_once.contains("// Header comment"));
    assert!(formatted_once.contains("class Foo {"));
    assert!(formatted_once.contains("    private int x;"));
    assert!(formatted_once.contains("    public Foo(int v) {"));

    // Verify idempotence: formatting again produces the exact same string
    let formatted_twice = fmt::format_source(&formatted_once).unwrap();
    assert_eq!(formatted_once, formatted_twice);

    // Verify the formatted code validates perfectly
    validate(&formatted_twice).unwrap();
}

#[test]
fn visual_borrow_diagnostics_display_secondary_spans() {
    let source = "class C { int n = 1; void inc() { n = n + 1; } }\nvoid main() {\n    C c = new C();\n    borrow C r = c;\n    c.inc();\n}";
    let err = validate(source).unwrap_err();
    let rendered = err.render("test.ncr", source);
    assert!(rendered.contains("error: [E3001]"));
    assert!(rendered.contains("::: (test.ncr:4:18)"));
    assert!(rendered.contains("borrow C r = c;"));
    assert!(rendered.contains("note: borrow #0 `r` begins here"));
}

#[test]
fn test_runner_runs_discovered_tests() {
    let temp_dir = std::env::temp_dir().join(format!("ncr_test_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let test_file = temp_dir.join("suite.ncr");

    fs::write(
        &test_file,
        r#"
void test_first() {
    assert(10 > 5);
}

void test_second() {
    assert(2 + 2 == 4);
}
"#,
    ).unwrap();

    let exit_code = nacre::test_runner::run_tests(&[test_file], false, false, false).unwrap();
    assert_eq!(exit_code, 0);

    let _ = fs::remove_dir_all(&temp_dir);
}
