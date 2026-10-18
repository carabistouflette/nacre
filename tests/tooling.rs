use nacre::{compile_to_c, fmt, native, validate};
mod support;
use std::fs;
#[test]
fn assert_builtin_succeeds_and_fails_with_code_70() {
    let passing_source = "void main() { assert(1 + 1 == 2, \"truth\"); }";
    let program = support::compile(passing_source, &[]).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success());

    let failing_source = "void main() { assert(1 + 1 == 3, \"math broke\"); }";
    let program = support::compile(failing_source, &[]).unwrap();
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
fn formatter_exits_for_each_header_before_formatting_body_and_following_statements() {
    let source = "void main(){List<int> values=new List<int>();for(int value:(values)){println(value);}println(2);}";
    let formatted = fmt::format_source(source).unwrap();
    assert!(
        formatted.contains("for (int value : (values)) {"),
        "{formatted}",
    );
    assert!(
        formatted.contains("        println(value);\n    }\n    println(2);\n}\n"),
        "{formatted}",
    );

    assert_eq!(fmt::format_source(&formatted).unwrap(), formatted);
    validate(&formatted).unwrap();
}

#[test]
fn formatter_preserves_non_ascii_source_characters_outside_literals() {
    let formatted = fmt::format_source("void main(){€;}").unwrap();
    assert_eq!(formatted, "void main() {\n    €;\n}\n");
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

    let exit_code = nacre::test_runner::run_tests(std::slice::from_ref(&test_file), None, false, false, false, false).unwrap();
    assert_eq!(exit_code, 0);

    // Test filtering: only run test_first
    let exit_code = nacre::test_runner::run_tests(&[test_file], Some("first"), false, false, false, false).unwrap();
    assert_eq!(exit_code, 0);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_runner_emits_json_results_and_failures() {
    let temp_dir = std::env::temp_dir().join(format!("ncr_test_json_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();
    let test_file = temp_dir.join("suite.ncr");
    fs::write(
        &test_file,
        r#"
void test_passed() {
    assert(1 == 1);
}

void test_failed() {
    assert(1 == 2, "json failure detail");
}
"#,
    ).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["test", "--json"])
        .arg(&test_file)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["type"], "test");
    assert_eq!(payload["passed"], 1);
    assert_eq!(payload["failed"], 1);
    assert_eq!(payload["filtered_out"], 0);
    let results = payload["tests"].as_array().unwrap();
    let passed = results.iter().find(|test| test["name"] == "test_passed").unwrap();
    assert_eq!(passed["status"], "passed");
    let failed = results.iter().find(|test| test["name"] == "test_failed").unwrap();
    assert_eq!(failed["status"], "failed");
    assert!(failed["failure"].as_str().unwrap().contains("json failure detail"));
    assert!(payload["duration_seconds"].as_f64().is_some());
    let large_message = "x".repeat(256 * 1024);
    let large_test = temp_dir.join("large_output.ncr");
    fs::write(
        &large_test,
        format!(
            "void test_large_failure() {{ println(\"{large_message}\"); assert(false, \"{large_message}\"); }}\n"
        ),
    ).unwrap();
    let large_output = std::process::Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["test", "--json"])
        .arg(&large_test)
        .output()
        .unwrap();
    assert_eq!(large_output.status.code(), Some(1));
    let large_payload: serde_json::Value = serde_json::from_slice(&large_output.stdout).unwrap();
    let large_failure = large_payload["tests"][0]["failure"].as_str().unwrap();
    assert!(large_failure.contains(
        "child output capture exceeded 65536 bytes per stream (stderr); excess bytes were discarded"
    ));
    assert!(large_failure.len() <= 64 * 1024 + 160);

    let empty_output = std::process::Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["test", "--json"])
        .arg(&test_file)
        .args(["--", "no_such_test"])
        .output()
        .unwrap();
    assert!(empty_output.status.success());
    let empty_payload: serde_json::Value = serde_json::from_slice(&empty_output.stdout).unwrap();
    assert_eq!(empty_payload["passed"], 0);
    assert_eq!(empty_payload["failed"], 0);
    assert_eq!(empty_payload["filtered_out"], 2);
    assert!(empty_payload["tests"].as_array().unwrap().is_empty());

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

    let exit_code = nacre::bench_runner::run_benchmarks(std::slice::from_ref(&bench_file), None, false, false, false, false).unwrap();
    assert_eq!(exit_code, 0);
    let exit_code_json = nacre::bench_runner::run_benchmarks(&[bench_file], None, false, false, false, true).unwrap();
    assert_eq!(exit_code_json, 0);
    let noisy_bench = temp_dir.join("noisy_bench.ncr");
    let noisy_output = "x".repeat(16 * 1024);
    fs::write(
        &noisy_bench,
        format!("void bench_chatty() {{ println(\"{noisy_output}\"); }}\n"),
    ).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["bench", "--json"])
        .arg(&noisy_bench)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["failed"], 1);
    assert!(payload["benchmarks"][0]["failure"].as_str().unwrap().contains(
        "child output capture exceeded 65536 bytes per stream (stdout); excess bytes were discarded"
    ));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn from_test_paths_enforces_boundaries_and_parses_ast() {
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
    let err = nacre::test_runner::run_tests(&paths, None, false, false, false, false).unwrap_err();
    assert!(err.contains("source file limit is 64"), "{err}");

    // 2. Verify that comments containing "void main()" do NOT suppress synthetic main
    let comment_file = temp_dir.join("comment_main.ncr");
    fs::write(&comment_file, "// void main() is inside this comment\nvoid test_comment() { assert(1 == 1); }\n").unwrap();
    let exit = nacre::test_runner::run_tests(&[comment_file], None, false, false, false, false).unwrap();
    assert_eq!(exit, 0);

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

#[test]
fn dwarf_debug_line_directives_map_to_ncr_files() {
    let source = "void main() {\n    int a = 1;\n    int b = 2;\n    int c = a + b;\n    assert(c == 3);\n}\n";
    let c = compile_to_c(source).unwrap();
    assert!(c.contains("#line 2"), "missing line 2 directive");
    assert!(c.contains("#line 3"), "missing line 3 directive");
    assert!(c.contains("#line 4"), "missing line 4 directive");
    assert!(c.contains("#line 5"), "missing line 5 directive");

    // Also verify multi-source bundle compiles with filename in #line directive
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("entry.ncr".to_owned(), source.to_owned()),
    ]).unwrap();
    let bundle_c = bundle.compile_to_c().unwrap();
    assert!(bundle_c.contains("#line 2 \"entry.ncr\""), "missing file in line directive");

    let program = native::compile_c(&bundle_c).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success());

    // Verify that filenames with quotes and backslashes are escaped safely in C preprocessor
    let bundle_escaped = nacre::project::SourceBundle::from_sources(vec![
        ("dir/\"special\"\\file.ncr".to_owned(), source.to_owned()),
    ]).unwrap();
    let escaped_c = bundle_escaped.compile_to_c().unwrap();
    assert!(escaped_c.contains("#line 2 \"dir/\\\"special\\\"\\\\file.ncr\""), "path was not escaped in directive");
    let program_escaped = native::compile_c(&escaped_c).unwrap();
    assert!(program_escaped.output().unwrap().status.success());
}

#[test]
fn trait_static_polymorphism_and_validation() {
    let valid_source = r#"
trait Measurable {
    int magnitude();
}

class Vector2D {
    private int x;
    private int y;
    public Vector2D(int x, int y) {
        this.x = x;
        this.y = y;
    }
}

impl Measurable for Vector2D {
    int magnitude() {
        return this.x + this.y;
    }
}

void main() {
    Vector2D v = new Vector2D(3, 4);
    assert(v.magnitude() == 7);
}
"#;
    let program = support::compile(valid_source, &[]).unwrap();
    assert!(program.output().unwrap().status.success());

    // Verify rejection of missing method
    let missing_method = r#"
trait T {
    void mustHave();
}
class C {}
impl T for C {}
void main() {}
"#;
    let err = validate(missing_method).unwrap_err();
    assert!(err.message.contains("missing method `mustHave` required by trait `T`"), "{err}");

    // Verify rejection of unknown trait
    let unknown_trait = r#"
class C {}
impl NonExistent for C {}
void main() {}
"#;
    let err = validate(unknown_trait).unwrap_err();
    assert!(err.message.contains("unknown trait `NonExistent`"), "{err}");

    // Verify generic class with trait bound monomorphization
    let generic_trait_source = r#"
trait Measurable {
    readonly int magnitude();
}
class Vector2D {
    private int x;
    private int y;
    public Vector2D(int x, int y) { this.x = x; this.y = y; }
}
impl Measurable for Vector2D {
    readonly int magnitude() { return this.x + this.y; }
}
class Metrics<T: Measurable> {
    private T item;
    public Metrics(T item) { this.item = item; }
    public readonly int getMagnitude() { return this.item.magnitude(); }
}
void main() {
    Vector2D v = new Vector2D(10, 20);
    Metrics<Vector2D> m = new Metrics<Vector2D>(v);
    assert(m.getMagnitude() == 30);
}
"#;
    let program = support::compile(generic_trait_source, &[]).unwrap();
    assert!(program.output().unwrap().status.success());

    // Verify rejection when type does not satisfy trait bound
    let unsatisfied_bound = r#"
trait Measurable { readonly int magnitude(); }
class NotMeasurable {}
class Metrics<T: Measurable> {
    private T item;
    public Metrics(T item) { this.item = item; }
}
void main() {
    NotMeasurable n = new NotMeasurable();
    Metrics<NotMeasurable> m = new Metrics<NotMeasurable>(n);
}
"#;
    let err = validate(unsatisfied_bound).unwrap_err();
    assert!(err.message.contains("does not satisfy trait bound `Measurable`"), "{err}");
}

#[test]
fn trait_implementations_reject_parameter_type_mismatches() {
    let source = r#"
trait Converter {
    int convert(int value);
}

class ConverterImpl {}
impl Converter for ConverterImpl {
    int convert(boolean value) { return 1; }
}

void main() {}
"#;
    let err = validate(source).unwrap_err();
    assert!(
        err.message.contains("parameter") && err.message.contains("convert"),
        "{err}"
    );
}

#[test]
fn trait_implementations_compare_resolved_parameter_type_names() {
    let values = r#"
module api.values;
public class Value {}
"#;
    let contract = r#"
module api.contracts;
import api.values.*;
public trait Consumer { void accept(Value value); }
"#;
    let model_values = r#"
module model.values;
public class Value {}
"#;
    let model = r#"
module model;
import api.contracts.*;
import model.values.*;

public class Item {}
impl Consumer for Item { void accept(Value value) {} }
public void main() {}
"#;
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("values.ncr".to_owned(), values.to_owned()),
        ("contract.ncr".to_owned(), contract.to_owned()),
        ("model_values.ncr".to_owned(), model_values.to_owned()),
        ("model.ncr".to_owned(), model.to_owned()),
    ])
    .unwrap();
    let err = bundle.validate().unwrap_err();
    assert!(
        err.message.contains("parameter 1") && err.message.contains("api.values::Value"),
        "{err}"
    );
}

#[test]
fn impl_methods_resolve_imports_from_the_impl_source_unit() {
    let class = r#"
module model;
public class Item {}
"#;
    let values = r#"
module api.values;
public class Value {}
"#;
    let contract = r#"
module api.contracts;
import api.values.*;
public trait Consumer { void accept(Value value); }
"#;
    let implementation = r#"
module model;
import api.contracts.*;
import api.values.*;
impl Consumer for Item { void accept(Value value) {} }
public void main() {}
"#;
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("class.ncr".to_owned(), class.to_owned()),
        ("values.ncr".to_owned(), values.to_owned()),
        ("contract.ncr".to_owned(), contract.to_owned()),
        ("impl.ncr".to_owned(), implementation.to_owned()),
    ])
    .unwrap();
    bundle.validate().unwrap();
}

#[test]
fn trait_implementations_match_receiver_mutability() {
    let source = r#"
trait Readable { readonly int read(); }
class Value {}
impl Readable for Value { int read() { return 1; } }
void main() {}
"#;
    let err = validate(source).unwrap_err();
    assert!(
        err.message
            .contains("requires a readonly receiver"),
        "{err}"
    );
}

#[test]
fn trait_implementations_match_method_visibility() {
    let source = r#"
trait Runnable { void run(); }
class Worker {}
impl Runnable for Worker { private void run() {} }
void main() {}
"#;
    let err = validate(source).unwrap_err();
    assert!(err.message.contains("requires public visibility"), "{err}");
}


#[test]
fn multi_module_trait_and_impl_resolution() {
    let source_a = r#"
module geometry.traits;

public trait Scalable {
    int scale(int factor);
}
"#;
    let source_b = r#"
module geometry.shapes;

import geometry.traits.*;

public class Rectangle {
    private int width;
    public Rectangle(int w) { this.width = w; }
}

impl Scalable for Rectangle {
    int scale(int factor) {
        return this.width * factor;
    }
}

public void main() {
    Rectangle r = new Rectangle(5);
    assert(r.scale(3) == 15);
}
"#;
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("traits.ncr".to_owned(), source_a.to_owned()),
        ("shapes.ncr".to_owned(), source_b.to_owned()),
    ]).unwrap();
    let program = support::compile(&bundle, &[]).unwrap();
    assert!(program.output().unwrap().status.success());
}

#[test]
fn generic_trait_bounds_accept_named_module_implementations() {
    let contracts = r#"
module library.contracts;
public trait Marker {}
"#;
    let model = r#"
module library.model;
import library.contracts.*;

public class Item {}
impl Marker for Item {}

public class Holder<T: Marker> {
    private T item;
    public Holder(T item) { this.item = item; }
    public int value() { return 42; }
}
"#;
    let app = r#"
module app;
import library.model.*;

public void main() {
    Item item = new Item();
    Holder<Item> holder = new Holder<Item>(item);
    println(holder.value());
}
"#;
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("contracts.ncr".to_owned(), contracts.to_owned()),
        ("model.ncr".to_owned(), model.to_owned()),
        ("app.ncr".to_owned(), app.to_owned()),
    ]).unwrap();
    let program = support::compile(&bundle, &[]).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"42\n");
}

#[test]
fn generic_trait_bounds_reject_same_named_trait_from_other_module() {
    let api_a = r#"
module api.a;
public trait Marker { void markA(); }
"#;
    let api_b = r#"
module api.b;
public trait Marker { void markB(); }
"#;
    let model = r#"
module model;
import api.a.*;

public class Item {}
impl Marker for Item { void markA() {} }
"#;
    let consumer = r#"
module consumer;
import api.b.*;
import model.*;

public class Box<T: Marker> {
    private T item;
    public Box(T item) { this.item = item; }
}

public void main() {
    Item item = new Item();
    Box<Item> box = new Box<Item>(item);
}
"#;
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("api_a.ncr".to_owned(), api_a.to_owned()),
        ("api_b.ncr".to_owned(), api_b.to_owned()),
        ("model.ncr".to_owned(), model.to_owned()),
        ("consumer.ncr".to_owned(), consumer.to_owned()),
    ]).unwrap();
    let err = bundle.validate().unwrap_err();
    assert!(err.message.contains("does not satisfy trait bound"), "{err}");
    assert!(err.message.contains("api.b::Marker"), "{err}");
}

#[test]
fn private_imported_trait_cannot_be_used_as_generic_bound() {
    let private_api = r#"
module private_api;
private trait Marker {}
"#;
    let consumer = r#"
module consumer;
import private_api.*;

public class Holder<T: Marker> {}
public void main() {}
"#;
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("private_api.ncr".to_owned(), private_api.to_owned()),
        ("consumer.ncr".to_owned(), consumer.to_owned()),
    ]).unwrap();
    let err = bundle.validate().unwrap_err();
    assert!(err.message.contains("trait `Marker` is private to module `private_api`"), "{err}");
}

#[test]
fn private_imported_trait_cannot_be_implemented_outside_its_module() {
    let private_api = r#"
module private_api;
private trait Marker {}
"#;
    let consumer = r#"
module consumer;
import private_api.*;

public class Item {}
impl Marker for Item {}
public void main() {}
"#;
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("private_api.ncr".to_owned(), private_api.to_owned()),
        ("consumer.ncr".to_owned(), consumer.to_owned()),
    ]).unwrap();
    let err = bundle.validate().unwrap_err();
    assert!(err.message.contains("trait `Marker` is private to module `private_api`"), "{err}");
}

#[test]
fn private_root_trait_cannot_be_used_from_named_module() {
    let private_api = "private trait Marker {}\n";
    let consumer = r#"
module consumer;
public class Item {}
impl Marker for Item {}
public void main() {}
"#;
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("private_api.ncr".to_owned(), private_api.to_owned()),
        ("consumer.ncr".to_owned(), consumer.to_owned()),
    ]).unwrap();
    let err = bundle.validate().unwrap_err();
    assert!(err.message.contains("trait `Marker` is private to the unnamed module"), "{err}");
}

#[test]
fn ambiguous_imported_traits_are_rejected() {
    let source_a = "module mod_a;\npublic trait SharedTrait { void run(); }\n";
    let source_b = "module mod_b;\npublic trait SharedTrait { void run(); }\n";
    let source_c = r#"
module main_mod;
import mod_a.*;
import mod_b.*;

public class Worker {}
impl SharedTrait for Worker {
    void run() {}
}
public void main() {}
"#;
    let bundle = nacre::project::SourceBundle::from_sources(vec![
        ("a.ncr".to_owned(), source_a.to_owned()),
        ("b.ncr".to_owned(), source_b.to_owned()),
        ("c.ncr".to_owned(), source_c.to_owned()),
    ]).unwrap();
    let err = bundle.validate().unwrap_err();
    assert!(err.message.contains("ambiguous trait `SharedTrait`"), "{err}");
}

#[test]
fn await_rejects_non_future_expressions() {
    for source in [
        "void main() { int value = await 7; }",
        "int value() { return 7; } void main() { int result = value().await; }",
    ] {
        let error = validate(source).unwrap_err();
        assert!(
            error.message.contains("await requires a `Future<T>` value"),
            "{error}"
        );
    }
}

#[test]
fn await_rejects_temporary_future_values() {
    let source = r#"
class BackgroundTask implements CancelableTask<int> {
    public Result<int, Text> run(borrow CancellationToken token) {
        return new Result<int, Text>.Ok(1);
    }
}
Future<int> pass(Future<int> future) { return future; }
void main() {
    match (Future<int>.spawn(new BackgroundTask())) {
        Ok(future) {
            Result<Option<int>, Text> result = await pass(future);
            drop(result);
        }
        Err(error) { drop(error); }
    }
}
"#;
    let error = validate(source).unwrap_err();
    assert!(
        error.message.contains("await requires a Future local or field"),
        "{error}"
    );
}

#[test]
fn async_function_modifiers_are_rejected() {
    for source in [
        "async int fetch() { return 42; } void main() {}",
        "class Worker { async int fetch() { return 42; } } void main() {}",
    ] {
        let error = validate(source).unwrap_err();
        assert!(
            error.message.contains("`async` declarations are not supported"),
            "{error}"
        );
    }
}

#[test]
fn postfix_await_waits_for_real_future() {
    let source = r#"
class BackgroundTask implements CancelableTask<int> {
    public Result<int, Text> run(borrow CancellationToken token) {
        return new Result<int, Text>.Ok(100);
    }
}

void main() {
    match (Future<int>.spawn(new BackgroundTask())) {
        Ok(fut) {
            Result<Option<int>, Text> res = fut.await;
            match (res) {
                Ok(opt) {
                    match (opt) {
                        Some(val) { assert(val == 100); }
                        None { assert(false, "cancelled"); }
                    }
                }
                Err(err) { assert(false, "failed"); }
            }
        }
        Err(err) { assert(false, "spawn failed"); }
    }
}
"#;
    let program = support::compile(source, &[]).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success());
}

#[test]
fn prefix_await_waits_until_release_barrier() {
    let source = r#"
class SuspendedTask implements CancelableTask<int> {
    private CountDownLatch releaseGate;
    public SuspendedTask(CountDownLatch gate) {
        this.releaseGate = gate;
    }
    public Result<int, Text> run(borrow CancellationToken token) {
        if (!this.releaseGate.wait(2000L)?) {
            return new Result<int, Text>.Err(new Text("gate timed out"));
        }
        return new Result<int, Text>.Ok(777);
    }
}

class ReleaseWorker implements Task<Unit> {
    private CountDownLatch gate;
    public ReleaseWorker(CountDownLatch gate) {
        this.gate = gate;
    }
    public Unit run() {
        match (Time.sleepMillis(50L)) { Ok(u) {} Err(e) {} }
        this.gate.countDown();
        return ();
    }
}

Result<Unit, Text> runAwaitTest() {
    CountDownLatch gate = CountDownLatch.create(1L)?;
    Future<int> fut = Future<int>.spawn(new SuspendedTask(gate.cloneHandle()?))?;
    Thread<Unit> releaser = Thread<Unit>.spawn(new ReleaseWorker(gate))?;
    Result<Option<int>, Text> res = await fut;
    releaser.join()?;
    match (res) {
        Ok(opt) {
            match (opt) {
                Some(val) { assert(val == 777); }
                None { assert(false, "cancelled"); }
            }
        }
        Err(err) { assert(false, "failed"); }
    }
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (runAwaitTest()) {
        Ok(u) {}
        Err(e) { assert(false, "await test failed"); }
    }
}
"#;
    let program = support::compile(source, &[]).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success());
}

#[test]
fn direct_llvm_ir_codegen_and_execution() {
    let source = r#"
int add(int a, int b) {
    return a + b;
}

double compute(double x, double y) {
    return x * y + 2.5;
}

void main() {
    int x = 15;
    int y = 25;
    int z = add(x, y);
    assert(z == 40);
    println(z);

    int sum = 0;
    int i = 0;
    while (true) {
        i = i + 1;
        if (i == 2) { continue; }
        if (i > 5) { break; }
        sum = sum + i;
    }
    assert(sum == 13);
    println(sum);

    double d = compute(3.0, 4.0);
    assert(d == 14.5);
    println(d);
}
"#;
    let c_program = native::compile_c(&nacre::compile_to_c(source).unwrap()).unwrap();
    let c_output = c_program.output().unwrap();
    assert!(c_output.status.success(), "{}", String::from_utf8_lossy(&c_output.stderr));
    let direct_program = nacre::compile_llvm_direct(source).unwrap();
    let direct_output = direct_program.output().unwrap();
    assert!(direct_output.status.success(), "{}", String::from_utf8_lossy(&direct_output.stderr));
    assert_eq!(direct_output.stdout, c_output.stdout);
}

#[test]
fn escape_analysis_stack_promotes_local_objects_and_heap_allocates_escaping() {
    // 1. Non-escaping local object is stack promoted (0 heap allocations)
    let non_escaping_source = r#"
class Point {
    public int x;
    public int y;
    public Point(int x, int y) {
        this.x = x;
        this.y = y;
    }
    public readonly int sum() {
        return this.x + this.y;
    }
}

void main() {
    Point p = new Point(10, 20);
    assert(p.sum() == 30);
}
"#;
    let program = support::compile(non_escaping_source, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("allocations=0 frees=0 live=0"), "expected stack promotion: {stderr}");

    // 2. Escaping object moved into a heap holder is heap allocated (does not dangle)
    let escaping_source = r#"
class Item {
    public int value;
    public Item(int v) { this.value = v; }
}

class Holder {
    public Item item;
    public Holder(Item i) { this.item = i; }
    public readonly int getValue() { return this.item.value; }
}

Holder makeHolder() {
    Item it = new Item(42);
    Holder h = new Holder(it);
    return h;
}

void main() {
    Holder h = makeHolder();
    assert(h.getValue() == 42);
}
"#;
    let program2 = support::compile(escaping_source, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let output2 = program2.output().unwrap();
    assert!(output2.status.success());
    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    assert!(stderr2.contains("allocations=2 frees=2 live=0"), "expected clean heap allocation: {stderr2}");

    // 3. Candidate moved into a returned holder via Try (?) expression escapes and is heap-allocated
    let try_escape_source = r#"
class Data {
    public int n;
    public Data(int n) { this.n = n; }
}

class Wrapper {
    public Data data;
    public Wrapper(Data d) { this.data = d; }
}

Result<Wrapper, Text> wrapData(Data d) {
    return new Result<Wrapper, Text>.Ok(new Wrapper(d));
}

Result<Wrapper, Text> tryWrap() {
    Data d = new Data(99);
    return wrapData(d);
}

Result<Unit, Text> runTryTest() {
    Wrapper w = tryWrap()?;
    assert(w.data.n == 99);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (runTryTest()) {
        Ok(u) {}
        Err(e) { assert(false, "try test failed"); }
    }
}
"#;
    let program3 = support::compile(try_escape_source, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let output3 = program3.output().unwrap();
    assert!(output3.status.success());
    let stderr3 = String::from_utf8_lossy(&output3.stderr);
    assert!(stderr3.contains("allocations=2 frees=2 live=0"), "expected heap allocation: {stderr3}");
}

#[test]
fn small_string_optimization_inline_capacity_23_bytes() {
    // 23 bytes fits inline in Text storage: 1 allocation (the Text object itself), 0 for backing buffer
    let source_inline = r#"
void main() {
    Text t = new Text("12345678901234567890123");
    assert(t.byteLength() == 23);
    assert(t.equalsString("12345678901234567890123"));
}
"#;
    let program = support::compile(source_inline, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("allocations=1 frees=1 live=0"), "expected 1 allocation: {stderr}");

    // 24 bytes exceeds 23-byte inline capacity: allocates heap backing buffer (2 allocations)
    let source_heap = r#"
void main() {
    Text t = new Text("123456789012345678901234");
    assert(t.byteLength() == 24);
    assert(t.equalsString("123456789012345678901234"));
}
"#;
    let program2 = support::compile(source_heap, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let output2 = program2.output().unwrap();
    assert!(output2.status.success());
    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    assert!(stderr2.contains("allocations=2 frees=2 live=0"), "expected 2 allocations: {stderr2}");
}

#[test]
fn monomorphic_and_polymorphic_interface_dispatch_behaves_correctly() {
    // A single implementation dispatches to the matching method.
    let monomorphic = r#"
interface Greeter {
    int greet();
}
class Friendly implements Greeter {
    public int greet() { return 100; }
}
void main() {
    Greeter g = new Friendly();
    assert(g.greet() == 100);
}
"#;
    let prog_mono = support::compile(monomorphic, &[]).unwrap();
    assert!(prog_mono.output().unwrap().status.success());

    // Multiple implementations select each matching method dynamically.
    let polymorphic = r#"
interface Calculator {
    int compute();
}
class Adder implements Calculator {
    public int compute() { return 1; }
}
class Multiplier implements Calculator {
    public int compute() { return 2; }
}
void main() {
    Calculator c1 = new Adder();
    Calculator c2 = new Multiplier();
    assert(c1.compute() == 1);
    assert(c2.compute() == 2);
}
"#;
    let prog_poly = support::compile(polymorphic, &[]).unwrap();
    assert!(prog_poly.output().unwrap().status.success());
}

#[test]
fn vectorized_memchr_text_search_and_substring_operations() {
    let source = r#"
void main() {
    Text t = new Text("the quick brown fox jumps over the lazy dog");
    Text needle1 = new Text("quick");
    Text needle2 = new Text("lazy");
    Text needle3 = new Text("cat");
    Text single = new Text("o");

    assert(t.indexOf(needle1) == 4);
    assert(t.indexOf(needle2) == 35);
    assert(t.indexOf(needle3) == -1);
    assert(t.indexOf(single) == 12);

    Text swift = new Text("swift");
    Text rep = t.replace(needle1, swift);
    assert(rep.indexOf(swift) == 4);
}
"#;
    let prog = support::compile(source, &[]).unwrap();
    assert!(prog.output().unwrap().status.success());
}

#[test]
fn iterator_chaining_and_adapter_composition() {
    let source = r#"
class Square implements Mapper<int, int> {
    public int apply(int n) { return n * n; }
}
class LessThan50 implements Predicate<int> {
    public readonly boolean test(int n) { return n < 50; }
}
void main() {
    List<int> numbers = new List<int>();
    int i = 1;
    while (i <= 10) {
        numbers.add(i);
        i = i + 1;
    }
    Iterator<int> it = numbers.drain();
    MapIterator<int, int> mapped = new MapIterator<int, int>(it, new Square());
    FilterIterator<int> filtered = new FilterIterator<int>(mapped, new LessThan50());
    Iterator<int> taken = Iterators<int>.take(filtered, 3L);
    List<int> result = List<int>.collect(taken);

    assert(result.size() == 3);
    assert(result.get(0) == 1);
    assert(result.get(1) == 4);
    assert(result.get(2) == 9);
}
"#;
    let prog = support::compile(source, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let output = prog.output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("live=0"), "expected memory balanced: {stderr}");
}

#[test]
fn pattern_matching_guards_and_fallback_execution() {
    let source = r#"
enum Number {
    Val(int);
    None;
}

int categorize(Number n) {
    match (n) {
        Val(x) if x > 100 { return 3; }
        Val(x) if x > 10 { return 2; }
        Val(x) if x > 0 { return 1; }
        Val(x) { return 0; }
        None { return -1; }
    }
}

void main() {
    assert(categorize(new Number.Val(500)) == 3);
    assert(categorize(new Number.Val(42)) == 2);
    assert(categorize(new Number.Val(7)) == 1);
    assert(categorize(new Number.Val(-5)) == 0);
    assert(categorize(new Number.None()) == -1);
}
"#;
    let prog = support::compile(source, &[]).unwrap();
    assert!(prog.output().unwrap().status.success());

    // Verify rejection of repeated variant after an unguarded arm
    let repeated_unguarded = r#"
enum Status { Ready; Waiting; }
void main() {
    Status s = new Status.Ready();
    match (s) {
        Ready { println(1); }
        Ready { println(2); }
        Waiting { println(0); }
    }
}
"#;
    assert!(validate(repeated_unguarded).is_err());

    // Verify rejection when a guarded arm does not stop and the function has no trailing return
    let non_stopping_guard = r#"
enum Number { Val(int); None; }
int check(Number n) {
    match (n) {
        Val(x) if x > 10 { int dummy = x + 1; }
        Val(x) { return 1; }
        None { return 0; }
    }
}
void main() {}
"#;
    let err = validate(non_stopping_guard).unwrap_err();
    assert!(err.message.contains("must return int on every path"), "{err}");
}

#[test]
fn pattern_matching_tuple_destructuring_and_wildcard_patterns() {
    let source = r#"
enum Tuple {
    Pair(int, int);
    Triple(int, int, int);
    Single(int);
}

int inspect(Tuple t) {
    match (t) {
        Triple(a, _, c) if a + c > 100 { return a + c; }
        Triple(_, b, _) { return b; }
        Pair(x, _) if x > 50 { return x; }
        Pair(_, y) { return y; }
        Single(_) { return 0; }
    }
}

void main() {
    assert(inspect(new Tuple.Triple(60, 20, 50)) == 110);
    assert(inspect(new Tuple.Triple(10, 99, 10)) == 99);
    assert(inspect(new Tuple.Pair(75, 10)) == 75);
    assert(inspect(new Tuple.Pair(20, 33)) == 33);
    assert(inspect(new Tuple.Single(42)) == 0);
}
"#;
    let prog = support::compile(source, &[]).unwrap();
    assert!(prog.output().unwrap().status.success());
}
