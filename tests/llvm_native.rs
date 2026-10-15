use nacre::native::{self, NativeProgram};
use nacre::project::SourceBundle;
use nacre::{compile_to_c, compile_llvm_direct, compile_llvm_direct_with_flags};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nacre-llvm-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf { self.0.join(name) }
}
impl Drop for TestDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

fn compile_c(source: &str, flags: &[&str]) -> NativeProgram {
    let generated = compile_to_c(source).unwrap_or_else(|error| panic!("{}", error.render("case.ncr", source)));
    native::compile_c_with_flags(&generated, flags).unwrap()
}

fn assert_direct_matches_c(source: &str, expected: &[u8], flags: &[&str]) -> Output {
    let c_output = compile_c(source, flags).output().unwrap();
    assert!(c_output.status.success(), "C backend failed: {}", String::from_utf8_lossy(&c_output.stderr));
    assert_eq!(c_output.stdout, expected);

    let direct_output = compile_llvm_direct_with_flags(source, flags).unwrap().output().unwrap();
    assert!(direct_output.status.success(), "direct LLVM backend failed: {}", String::from_utf8_lossy(&direct_output.stderr));
    assert_eq!(direct_output.stdout, c_output.stdout);
    if flags.contains(&"-DNACRE_MEMORY_STATS") {
        assert_balanced_memory(&direct_output);
    }
    direct_output
}

fn assert_balanced_memory(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = stderr.lines().find(|line| line.starts_with("NACRE_MEMORY "))
        .unwrap_or_else(|| panic!("missing memory report: {stderr}"));
    let fields: Vec<_> = report.split_ascii_whitespace().collect();
    assert_eq!(fields[0], "NACRE_MEMORY", "{report}");
    let allocations = fields[1].strip_prefix("allocations=").unwrap();
    let frees = fields[2].strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{report}");
    assert_eq!(fields[3], "live=0", "{report}");
    assert_eq!(fields.len(), 4, "{report}");
}

#[test]
fn direct_scalar_arithmetic_control_flow_and_short_circuit_execute() {
    let source = r#"
boolean rightSide() { println("evaluated"); return true; }
void main() {
    int index = 0;
    int sum = 0;
    while (index < 5) {
        sum = sum + index;
        index = index + 1;
    }
    println(sum);
    println(false && rightSide());
    println(true || rightSide());
    println(true && rightSide());
}
"#;
    let expected = b"10\nfalse\ntrue\nevaluated\ntrue\n";
    let c_output = compile_c(source, &[]).output().unwrap();
    assert!(c_output.status.success(), "{}", String::from_utf8_lossy(&c_output.stderr));
    assert_eq!(c_output.stdout, expected);

    let direct = compile_llvm_direct(source).unwrap().output().unwrap();
    assert!(direct.status.success(), "{}", String::from_utf8_lossy(&direct.stderr));
    assert_eq!(direct.stdout, c_output.stdout);
}

#[test]
fn direct_stdlib_executes_text_list_map_option_and_result() {
    let source = r#"
Result<Text, Text> choose(boolean success) {
    if (success) { return new Result<Text, Text>.Ok(new Text("ok")); }
    return new Result<Text, Text>.Err(new Text("failure"));
}
void main() {
    List<Text> words = new List<Text>();
    words.add(new Text("café"));
    {
        borrow Text word = words.getRef(0);
        println(word.byteLength());
        word.print();
    }
    List<int> sizes = new List<int>();
    sizes.add(5);
    match (sizes.tryGet(9)) {
        None { println("none"); }
        Some(size) { println(size); }
    }

    Map<Text, int> lengths = new Map<Text, int>();
    lengths.put(new Text("café"), 7);
    Text query = new Text("café");
    match (lengths.tryGet(query)) {
        None { println("missing map value"); }
        Some(value) { println(value); }
    }
    match (choose(true)) {
        Ok(value) { value.print(); }
        Err(error) { error.print(); }
    }
    match (choose(false)) {
        Ok(value) { value.print(); }
        Err(error) { error.print(); }
    }
}
"#;
    assert_direct_matches_c(
        source,
        "5\ncafé\nnone\n7\nok\nfailure\n".as_bytes(),
        &["-DNACRE_MEMORY_STATS"],
    );
}

#[test]
fn direct_aggregate_abi_preserves_interface_dispatch_and_owned_sum_cleanup() {
    let source = r#"
enum Choice<T> { Empty; Item(T); }
interface NumberLike { readonly int value(); }
class Box implements NumberLike {
    private int stored;
    public Box(int stored) { this.stored = stored; }
    public readonly int value() { return this.stored; }
}
Choice<int> number() { return new Choice<int>.Item(42); }
Choice<int> identity(Choice<int> value) { return value; }
Choice<NumberLike> boxed() { return new Choice<NumberLike>.Item(new Box(9)); }
Choice<Option<int>> nested() {
    return new Choice<Option<int>>.Item(new Option<int>.Some(11));
}
int readNumber(Choice<int> value) {
    int result = 0;
    match (value) {
        Empty { }
        Item(number) { result = number; }
    }
    return result;
}
int readInterface(Choice<NumberLike> value) {
    match (value) {
        Empty { return 0; }
        Item(box) { return box.value(); }
    }
}
int readNested(Choice<Option<int>> value) {
    int result = 0;
    match (value) {
        Empty { }
        Item(inner) {
            match (inner) {
                None { result = 0; }
                Some(number) { result = number; }
            }
        }
    }
    return result;
}
void main() {
    println(readInterface(boxed()));
    println(readNested(nested()));
    println(readNumber(identity(number())));
}
"#;
    assert_direct_matches_c(
        source,
        b"9\n11\n42\n",
        &["-DNACRE_MEMORY_STATS"],
    );
}

#[test]
fn direct_runtime_links_gmp_and_preserves_big_integer_results() {
    let source = r#"
void main() {
    BigInt value = BigInt.fromLong(123456789L);
    BigInt factor = BigInt.fromLong(2L);
    BigInt doubled = value.multiply(factor);
    match (doubled.toText(10)) {
        Err(error) { println("conversion error"); }
        Ok(text) { text.print(); }
    }
}
"#;
    assert_direct_matches_c(
        source,
        b"246913578\n",
        &["-DNACRE_MEMORY_STATS"],
    );
}

#[test]
fn direct_runtime_links_zlib_for_byte_buffer_compression() {
    let source = r#"
Result<Unit, Text> exercise() {
    ByteBuffer input = ByteBuffer.withCapacity(0L)?;
    input.append((byte)104); input.append((byte)101); input.append((byte)108);
    input.append((byte)108); input.append((byte)111); input.append((byte)32);
    input.append((byte)119); input.append((byte)111); input.append((byte)114);
    input.append((byte)108); input.append((byte)100);
    ByteBuffer zipped = Compression.gzip(input, 9)?;
    ByteBuffer decoded = Compression.gunzip(zipped, input.length())?;
    println(decoded.length() == 11L && decoded.get(0L) == (byte)104);
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(unit) { println("gzip ok"); }
        Err(error) { error.print(); }
    }
}
"#;
    assert_direct_matches_c(
        source,
        b"true\ngzip ok\n",
        &["-DNACRE_MEMORY_STATS"],
    );
}

#[test]
fn direct_source_bundle_compiles_all_source_units_as_one_program() {
    let bundle = SourceBundle::from_sources(vec![
        ("math.ncr".to_owned(), "int square(int value) { return value * value; }".to_owned()),
        ("main.ncr".to_owned(), "void main() { println(square(7)); }".to_owned()),
    ]).unwrap();
    let generated = bundle.compile_to_c().unwrap();
    let c_program = native::compile_c_with_flags(&generated, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let c_output = c_program.output().unwrap();
    assert!(c_output.status.success(), "{}", String::from_utf8_lossy(&c_output.stderr));
    assert_eq!(c_output.stdout, b"49\n");

    let direct = bundle.compile_llvm_direct_with_flags(&["-DNACRE_MEMORY_STATS"]).unwrap();
    let direct_output = direct.output().unwrap();
    assert!(direct_output.status.success(), "{}", String::from_utf8_lossy(&direct_output.stderr));
    assert_eq!(direct_output.stdout, c_output.stdout);
    assert_balanced_memory(&direct_output);
}

#[test]
fn direct_bundle_runtime_failure_identifies_its_source_file_and_line() {
    let bundle = SourceBundle::from_sources(vec![
        ("math.ncr".to_owned(), "int divide(int by) {\n    return 5 / by;\n}\n".to_owned()),
        ("main.ncr".to_owned(), "void main() { println(divide(0)); }\n".to_owned()),
    ]).unwrap();
    let c_program = native::compile_c(&bundle.compile_to_c().unwrap()).unwrap();
    let direct_program = bundle.compile_llvm_direct().unwrap();
    for program in [&c_program, &direct_program] {
        let output = program.output().unwrap();
        assert!(!output.status.success());
        let diagnostic = String::from_utf8(output.stderr).unwrap();
        assert!(diagnostic.contains("math.ncr:2:"), "{diagnostic}");
    }
}

#[test]
fn cli_run_and_build_select_direct_llvm_without_overwriting_sources() {
    let temp = TestDir::new();
    let source = temp.path("main.ncr");
    fs::write(&source, "void main() { println(42); }\n").unwrap();
    let source_text = source.to_str().unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["run", source_text, "--direct", "--native"])
        .env("NACRE_CC", "nacre-deliberately-missing")
        .output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(run.stdout, b"42\n");

    let executable = temp.path(if cfg!(windows) { "program.exe" } else { "program" });
    let build = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["build", source_text, "--direct", "-o", executable.to_str().unwrap()])
        .env("NACRE_CC", "nacre-deliberately-missing")
        .output().unwrap();
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let output = Command::new(executable).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"42\n");
}

#[test]
fn direct_runtime_bitcode_cache_is_reused_across_programs() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TestDir::new();
    let cache = temp.path("cache");
    fs::create_dir(&cache).unwrap();
    let compiler = temp.path("record-clang");
    let c_compiles = temp.path("c-compiles");
    fs::write(&compiler, r#"#!/bin/sh
for arg do
    case "$arg" in
        *.c) printf 'C frontend\n' >> "$NACRE_TEST_C_COMPILES"; break ;;
    esac
done
exec "$NACRE_TEST_CLANG" "$@"
"#).unwrap();
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
    let clang = std::env::var_os("NACRE_CLANG").unwrap_or_else(|| "clang".into());
    let source = temp.path("main.ncr");
    for (text, expected) in [("first program", &b"first program\n"[..]), ("second program", &b"second program\n"[..])] {
        fs::write(&source, format!("void main() {{ println(\"{text}\"); }}\n")).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_nacre"))
            .args(["run", source.to_str().unwrap(), "--direct"])
            .env("NACRE_CACHE_DIR", &cache)
            .env("NACRE_ENABLE_TEST_CACHE", "1")
            .env("NACRE_CLANG", &compiler)
            .env("NACRE_TEST_CLANG", &clang)
            .env("NACRE_TEST_C_COMPILES", &c_compiles)
            .env_remove("NACRE_NO_CACHE")
            .output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, expected);
        assert_eq!(fs::read_to_string(&c_compiles).unwrap(), "C frontend\n",
            "changing only user code must not invoke the C frontend again");
    }
}

#[test]
fn direct_build_protects_an_explicitly_selected_manifest() {
    let temp = TestDir::new();
    let package = temp.path("package");
    fs::create_dir(&package).unwrap();
    let manifest = package.join("Nacre.toml");
    let source_dir = package.join("src");
    fs::create_dir(&source_dir).unwrap();
    let source = source_dir.join("main.ncr");
    let manifest_text = "[package]\nname = \"direct-protection\"\nversion = \"0.1.0\"\nsources = [\"src\"]\nentry = \"src/main.ncr\"\n";
    fs::write(&manifest, manifest_text).unwrap();
    fs::write(&source, "void main() {}\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["build", package.to_str().unwrap(), "--direct", "-o", manifest.to_str().unwrap()])
        .output().unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&manifest).unwrap(), manifest_text);
}


#[test]
fn direct_try_propagation_preserves_primitive_and_owned_error_payloads() {
    let source = r#"
Result<int, int> failure() { return new Result<int, int>.Err(7); }
Result<int, int> propagate() { int value = failure()?; return new Result<int, int>.Ok(value); }
Result<int, Text> textFailure() { return new Result<int, Text>.Err(new Text("preserved")); }
Result<int, Text> textPropagate() { int value = textFailure()?; return new Result<int, Text>.Ok(value); }
void main() {
    match (propagate()) { Ok(value) { println(value); } Err(error) { println(error); } }
    match (textPropagate()) { Ok(value) { println(value); } Err(error) { error.print(); } }
}
"#;
    assert_direct_matches_c(source, b"7\npreserved\n", &["-DNACRE_MEMORY_STATS"]);
}

#[test]
fn direct_unary_plus_preserves_negative_zero() {
    assert_direct_matches_c(
        "double identity(double value) { return +value; }\nvoid main() { println(1.0 / identity(-0.0)); }\n",
        b"-inf\n",
        &[],
    );
}
