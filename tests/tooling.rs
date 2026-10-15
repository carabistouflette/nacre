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
    let err = nacre::test_runner::run_tests(&paths, None, false, false, false).unwrap_err();
    assert!(err.contains("source file limit is 64"), "{err}");

    // 2. Verify that comments containing "void main()" do NOT suppress synthetic main
    let comment_file = temp_dir.join("comment_main.ncr");
    fs::write(&comment_file, "// void main() is inside this comment\nvoid test_comment() { assert(1 == 1); }\n").unwrap();
    let exit = nacre::test_runner::run_tests(&[comment_file], None, false, false, false).unwrap();
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
    let c = compile_to_c(valid_source).unwrap();
    let program = native::compile_c(&c).unwrap();
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
    let c = compile_to_c(generic_trait_source).unwrap();
    let program = native::compile_c(&c).unwrap();
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
    let c = bundle.compile_to_c().unwrap();
    let program = native::compile_c(&c).unwrap();
    assert!(program.output().unwrap().status.success());
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
fn async_await_syntax_and_execution() {
    let source = "async int fetch() { return 42; }\nvoid main() {\n    int x = await fetch();\n    assert(x == 42);\n    int y = fetch().await;\n    assert(y == 42);\n}\n";
    let c = compile_to_c(source).unwrap();
    let program = native::compile_c(&c).unwrap();
    assert!(program.output().unwrap().status.success());
}

#[test]
fn async_await_suspends_and_resumes_real_future() {
    let source = r#"
class BackgroundTask implements CancelableTask<int> {
    public Result<int, Text> run(borrow CancellationToken token) {
        return new Result<int, Text>.Ok(100);
    }
}

void main() {
    match (Future<int>.spawn(new BackgroundTask())) {
        Ok(fut) {
            Result<Option<int>, Text> res = await fut;
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
    let c = compile_to_c(source).unwrap();
    let program = native::compile_c(&c).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success());
}

#[test]
fn async_await_suspends_and_resumes_with_barrier_latch() {
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

Result<Unit, Text> runAsyncTest() {
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
    match (runAsyncTest()) {
        Ok(u) {}
        Err(e) { assert(false, "async test failed"); }
    }
}
"#;
    let c = compile_to_c(source).unwrap();
    let program = native::compile_c(&c).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success());
}

#[test]
fn direct_llvm_ir_codegen_and_execution() {
    let source = r#"
int add(int a, int b) {
    return a + b;
}

void main() {
    int x = 15;
    int y = 25;
    int z = add(x, y);
    assert(z == 40);
    println(z);
}
"#;
    let llvm_ir = nacre::emit_llvm_direct(source).unwrap();
    eprintln!("LLVM IR:\n{llvm_ir}");
    assert!(llvm_ir.contains("target triple = \"x86_64-pc-linux-gnu\""), "missing triple");
    assert!(llvm_ir.contains("define i32 @ncr_fn_0"), "missing function 0");
    assert!(llvm_ir.contains("add i32"), "missing add instruction");
    assert!(llvm_ir.contains("@main"), "missing main");

    // If clang is installed, verify direct LLVM compilation and native execution
    if std::process::Command::new("clang").arg("--version").output().is_ok() {
        let program = native::compile_llvm_direct(&llvm_ir, "-O2").unwrap();
        let output = program.output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"40\n");
    }
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
    let c = compile_to_c(non_escaping_source).unwrap();
    let program = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"]).unwrap();
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
    let c2 = compile_to_c(escaping_source).unwrap();
    let program2 = native::compile_c_with_flags(&c2, &["-DNACRE_MEMORY_STATS"]).unwrap();
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
    let c3 = compile_to_c(try_escape_source).unwrap();
    let program3 = native::compile_c_with_flags(&c3, &["-DNACRE_MEMORY_STATS"]).unwrap();
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
    let c = compile_to_c(source_inline).unwrap();
    let program = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"]).unwrap();
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
    let c2 = compile_to_c(source_heap).unwrap();
    let program2 = native::compile_c_with_flags(&c2, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let output2 = program2.output().unwrap();
    assert!(output2.status.success());
    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    assert!(stderr2.contains("allocations=2 frees=2 live=0"), "expected 2 allocations: {stderr2}");
}

#[test]
fn monomorphic_interface_devirtualization_and_polymorphic_fallback() {
    // Single implementation: devirtualized to direct static thunk call
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
    let c_mono = compile_to_c(monomorphic).unwrap();
    assert!(c_mono.contains("ncr_thunk_"), "expected direct static thunk dispatch: {c_mono}");
    let prog_mono = native::compile_c(&c_mono).unwrap();
    assert!(prog_mono.output().unwrap().status.success());

    // Multiple implementations: polymorphic fallback to vtable dispatch
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
    let c_poly = compile_to_c(polymorphic).unwrap();
    assert!(c_poly.contains("vtable->m0"), "expected dynamic vtable dispatch for polymorphic interface: {c_poly}");
    let prog_poly = native::compile_c(&c_poly).unwrap();
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
    let c = compile_to_c(source).unwrap();
    let prog = native::compile_c(&c).unwrap();
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
    let c = compile_to_c(source).unwrap();
    let prog = native::compile_c(&c).unwrap();
    assert!(prog.output().unwrap().status.success());
}
