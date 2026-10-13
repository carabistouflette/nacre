//! End-to-end CLI tests. Build/run cases require a C compiler.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{env, fs};
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn nacre(args: &[&str]) -> Output { Command::new(env!("CARGO_BIN_EXE_nacre")).args(args).output().unwrap() }
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let p = env::temp_dir().join(format!("nacre-cli-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&p).unwrap(); Self(p)
    }
    fn path(&self, name: &str) -> PathBuf { self.0.join(name) }
}
impl Drop for TestDir { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
fn path_str(path: &Path) -> &str { path.to_str().unwrap() }

#[test]
fn unknown_command() { assert!(!nacre(&["wat"]).status.success()); }
#[test]
fn missing_filename() { assert!(!nacre(&["run"]).status.success()); }
#[test]
fn nonexistent_file() { assert!(!nacre(&["check", "file-that-does-not-exist.ncr"]).status.success()); }
#[test]
fn help_and_version_outputs_are_english() {
    let help = nacre(&["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));
    let version = nacre(&["--version"]);
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("nacre "));
}
#[test]
fn checks_valid_file() { let out = nacre(&["check", "examples/hello.ncr"]); assert!(out.status.success()); }
#[test]
fn rejects_invalid_file_with_caret() {
    let out = nacre(&["check", "examples/invalid/type_error.ncr"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("type_error.ncr:2:15: error:"), "{text}");
    assert!(text.contains("^^^^"));
}
#[test]
fn source_cannot_be_overwritten() {
    let temp = TestDir::new();
    let path = temp.path("source.ncr");
    let source = "void main() {}";
    fs::write(&path, source).unwrap();
    let out = nacre(&["emit-c", path_str(&path), "-o", path_str(&path)]);
    assert!(!out.status.success());
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}
#[test]
fn run_native_program() {
    let out = nacre(&["run", "examples/hello.ncr"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "Bonjour depuis Nacre !\n42\ntrue\n");
}
#[test]
fn build_persistent_native_program() {
    let temp = TestDir::new();
    let output = temp.path(if cfg!(windows) { "hello native.exe" } else { "hello native" });
    let out = nacre(&["build", "examples/hello.ncr", "-o", path_str(&output)]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let result = Command::new(&output).output().unwrap();
    assert!(result.status.success());
    assert_eq!(String::from_utf8(result.stdout).unwrap(), "Bonjour depuis Nacre !\n42\ntrue\n");
}
#[test]
fn runtime_error_exit_code_is_propagated() {
    let temp = TestDir::new();
    let path = temp.path("zero.ncr");
    fs::write(&path, "void main() { println(1 / 0); }").unwrap();
    let out = nacre(&["run", path_str(&path)]);
    assert_eq!(out.status.code(), Some(70));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("runtime error"), "{stderr}");
    assert!(stderr.contains("[1:"), "{stderr}");
}

#[test]
fn run_counter_v1() {
    let out = nacre(&["run", "examples/counter.ncr"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout, b"42\n");
}
#[test]
fn rejects_moved_object_v1() {
    let out = nacre(&["check", "examples/invalid/use_after_move.ncr"]);
    assert!(!out.status.success());
}
#[test]
fn rejects_private_field_v1() {
    let out = nacre(&["check", "examples/invalid/private_field.ncr"]);
    assert!(!out.status.success());
}

#[test]
fn emit_flow_does_not_require_a_c_compiler() {
    let out = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["emit-flow", "examples/ownership_loop.ncr"])
        .env("NACRE_CC", "nacre-deliberately-missing-compiler")
        .output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("loop-header"));
}
#[test]
fn emit_flow_writes_a_file() {
    let temp = TestDir::new();
    let path = temp.path("flow graph.txt");
    let out = nacre(&["emit-flow", "examples/ownership_loop.ncr", "-o", path_str(&path)]);
    assert!(out.status.success());
    assert!(fs::read_to_string(&path).unwrap().contains("ownership CFG"));
}
#[test]
fn emit_flow_cannot_overwrite_source() {
    let temp = TestDir::new();
    let path = temp.path("source.ncr");
    fs::write(&path, "void main() {}").unwrap();
    assert!(!nacre(&["emit-flow", path_str(&path), "-o", path_str(&path)]).status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), "void main() {}");
}
#[test]
fn run_with_memory_stats() {
    let out = nacre(&["run", "examples/ownership_loop.ncr", "--memory-stats"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout, b"13\n");
    assert_eq!(out.stderr, b"NACRE_MEMORY allocations=4 frees=4 live=0\n");
}
#[test]
fn memory_stats_is_not_a_check_option() {
    assert!(!nacre(&["check", "examples/hello.ncr", "--memory-stats"]).status.success());
}
#[test]
fn duplicate_memory_stats_flag_is_rejected() {
    assert!(!nacre(&["run", "examples/hello.ncr", "--memory-stats", "--memory-stats"]).status.success());
}
#[test]
fn missing_emit_flow_output_path_is_rejected() {
    assert!(!nacre(&["emit-flow", "examples/hello.ncr", "-o"]).status.success());
}
#[test]
fn v2_move_error_has_notes() {
    let out = nacre(&["check", "examples/invalid/use_after_drop.ncr"]);
    assert!(!out.status.success());
    let message = String::from_utf8_lossy(&out.stderr);
    assert!(message.contains("E2001"));
    assert!(message.contains("note:"));
}

#[test]
fn emits_borrow_analysis_to_stdout() {
    let out = nacre(&["emit-borrows", "examples/borrowing.ncr"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("lexical borrow analysis"));
    assert!(text.contains("loan #"));
}
#[test]
fn emits_borrow_analysis_to_file() {
    let temp = TestDir::new();
    let path = temp.path("borrow report.txt");
    let out = nacre(&["emit-borrows", "examples/borrowing.ncr", "-o", path_str(&path)]);
    assert!(out.status.success());
    assert!(fs::read_to_string(path).unwrap().contains("end loan"));
}
#[test]
fn borrow_analysis_protects_the_source_file() {
    let temp = TestDir::new();
    let source = temp.path("source.ncr");
    fs::write(&source, "void main() {}").unwrap();
    assert!(!nacre(&["emit-borrows", path_str(&source), "-o", path_str(&source)]).status.success());
    assert_eq!(fs::read_to_string(source).unwrap(), "void main() {}");
}
#[test]
fn borrow_analysis_rejects_memory_stats_flag() {
    assert!(!nacre(&["emit-borrows", "examples/borrowing.ncr", "--memory-stats"]).status.success());
}
#[test]
fn borrower_diagnostic_reaches_the_cli() {
    let out = nacre(&["check", "examples/invalid/borrow_then_drop.ncr"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("E3002") && text.contains("note:"));
}
