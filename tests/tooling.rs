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

    let exit_code = nacre::test_runner::run_tests(&[test_file.clone()], None, false, false, false).unwrap();
    assert_eq!(exit_code, 0);

    // Test filtering: only run test_first
    let exit_code = nacre::test_runner::run_tests(&[test_file], Some("first"), false, false, false).unwrap();
    assert_eq!(exit_code, 0);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn bench_runner_measures_benchmarks() {
    let temp_dir = std::env::temp_dir().join(format!("ncr_bench_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let bench_file = temp_dir.join("benches.ncr");

    fs::write(
        &bench_file,
        r#"
void bench_simple_math() {
    int x = 1;
    int y = 2;
    int z = x + y;
}
"#,
    ).unwrap();

    let exit_code = nacre::bench_runner::run_benchmarks(&[bench_file], None, false, false, false).unwrap();
    assert_eq!(exit_code, 0);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn from_test_paths_enforces_boundaries() {
    use nacre::project::SourceBundle;
    let temp_dir = std::env::temp_dir().join(format!("ncr_boundary_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);

    // 1. Verify MAX_FILES limit (64)
    let mut paths = Vec::new();
    for i in 0..64 {
        let p = temp_dir.join(format!("file_{i}.ncr"));
        fs::write(&p, format!("void test_{i}() {{}}\n")).unwrap();
        paths.push(p);
    }
    // 64 files with synthetic main -> 65 files -> exceeds MAX_FILES (64)
    let err = SourceBundle::from_test_paths(&paths).unwrap_err();
    assert!(err.contains("source file limit is 64"), "{err}");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn doc_generator_produces_searchable_html() {
    let temp_dir = std::env::temp_dir().join(format!("ncr_doc_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let src_file = temp_dir.join("library.ncr");
    let out_dir = temp_dir.join("html_doc");

    fs::write(
        &src_file,
        r#"
/// A simple math calculator class.
/// Supports arithmetic operations.
class Calculator {
    private int base;

    /// Creates a new Calculator with the given base.
    public Calculator(int base) {
        this.base = base;
    }

    /// Adds an amount to the base and returns the result.
    public readonly int add(int amount) {
        return this.base + amount;
    }
}

/// Standalone helper function.
void main() {}
"#,
    ).unwrap();

    nacre::doc::generate_docs(&[src_file], Some(&out_dir)).unwrap();
    let index_html = fs::read_to_string(out_dir.join("index.html")).unwrap();

    assert!(index_html.contains("Nacre API Documentation"));
    assert!(index_html.contains("Calculator"));
    assert!(index_html.contains("A simple math calculator class."));
    assert!(index_html.contains("Supports arithmetic operations."));
    assert!(index_html.contains("Creates a new Calculator"));
    assert!(index_html.contains("Adds an amount to the base"));
    assert!(index_html.contains("filterItems()")); // Client-side search JS

    let _ = fs::remove_dir_all(&temp_dir);
}
