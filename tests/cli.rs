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
fn doctor_help_and_invalid_flags_are_coherent() {
    let help = nacre(&["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("nacre doctor [--direct] [--zip]"));

    let invalid = nacre(&["doctor", "--unknown"]);
    assert!(!invalid.status.success());
    assert_eq!(
        String::from_utf8_lossy(&invalid.stderr).trim(),
        "usage: nacre doctor [--direct] [--zip]",
    );
}

#[test]
fn doctor_direct_reports_missing_clang_actionably() {
    let temp = TestDir::new();
    let missing = temp.path("missing-clang");
    let out = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["doctor", "--direct"])
        .env("NACRE_CLANG", &missing)
        .env("NACRE_LLVM_LINK", temp.path("missing-llvm-link"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let message = String::from_utf8_lossy(&out.stderr);
    assert!(message.contains("NACRE_CLANG"), "{message}");
    assert!(message.contains("set NACRE_CLANG to its executable path"), "{message}");
}

#[cfg(unix)]
fn write_executable(path: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, script).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
#[test]
fn native_compiler_error_output_is_bounded() {
    let temp = TestDir::new();
    let compiler = temp.path("noisy-c-compiler");
    write_executable(
        &compiler,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf 'fixture compiler\\n'; exit 0; fi\ni=0\nwhile [ \"$i\" -lt 16384 ]; do printf '%s' 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' >&2; i=$((i + 1)); done\nexit 1\n",
    );
    let executable = temp.path("native-program");
    let output = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["build", "examples/language/hello_world.ncr", "-o", path_str(&executable)])
        .env("NACRE_CC", &compiler)
        .env("NACRE_NO_CACHE", "1")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.len() <= 64 * 1024 + 1024, "captured {} diagnostic bytes", output.stderr.len());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("C compiler"), "{message}");
    assert!(message.contains("child output capture exceeded 65536 bytes per stream (stderr)"), "{message}");
    assert!(message.contains("To inspect the generated code"), "{message}");
}

#[cfg(unix)]
#[test]
fn doctor_direct_rejects_mismatched_llvm_before_compilation() {
    let temp = TestDir::new();
    let clang = temp.path("clang-23");
    let linker = temp.path("llvm-link-22");
    write_executable(&clang, "#!/bin/sh\ncase \"$1\" in\n  --version) echo 'clang version 23.1.0' ;;\n  -print-target-triple) echo x86_64-unknown-linux-gnu ;;\n  *) echo UNEXPECTED_CLANG_COMPILATION >&2; exit 97 ;;\nesac\n");
    write_executable(&linker, "#!/bin/sh\ncase \"$1\" in\n  --version) echo 'LLVM version 22.1.0' ;;\n  *) echo UNEXPECTED_LLVM_LINK >&2; exit 98 ;;\nesac\n");
    let out = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["doctor", "--direct"])
        .env("NACRE_CLANG", &clang)
        .env("NACRE_LLVM_LINK", &linker)
        .env("NACRE_CC", temp.path("missing-c-compiler"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let message = String::from_utf8_lossy(&out.stderr);
    let report = String::from_utf8_lossy(&out.stdout);
    assert!(report.contains("Clang ") && report.contains("clang version 23.1.0"), "{report}");
    assert!(report.contains("llvm-link ") && report.contains("LLVM version 22.1.0"), "{report}");
    assert!(report.contains("target x86_64-unknown-linux-gnu"), "{report}");
    assert!(message.contains("direct LLVM toolchain mismatch"), "{message}");
    assert!(message.contains("Clang LLVM 23"), "{message}");
    assert!(message.contains("llvm-link LLVM 22"), "{message}");
    assert!(!message.contains("UNEXPECTED_CLANG_COMPILATION"), "{message}");
    assert!(!message.contains("UNEXPECTED_LLVM_LINK"), "{message}");
}

#[test]
fn unknown_command() { assert!(!nacre(&["wat"]).status.success()); }
#[test]
fn missing_filename() { assert!(!nacre(&["run"]).status.success()); }
#[test]
fn nonexistent_file() { assert!(!nacre(&["check", "file-that-does-not-exist.ncr"]).status.success()); }
#[test]
fn checks_valid_file() { let out = nacre(&["check", "examples/language/hello_world.ncr"]); assert!(out.status.success()); }
#[test]
fn rejects_invalid_file_with_caret() {
    let out = nacre(&["check", "examples/invalid/type_error.ncr"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("type_error.ncr:2:15:"), "{text}");
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
    let out = nacre(&["run", "examples/language/hello_world.ncr"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "Hello from Nacre!\n42\ntrue\n");
}
#[test]
fn build_persistent_native_program() {
    let temp = TestDir::new();
    let output = temp.path(if cfg!(windows) { "hello native.exe" } else { "hello native" });
    let out = nacre(&["build", "examples/language/hello_world.ncr", "-o", path_str(&output)]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let result = Command::new(&output).output().unwrap();
    assert!(result.status.success());
    assert_eq!(String::from_utf8(result.stdout).unwrap(), "Hello from Nacre!\n42\ntrue\n");
}
#[test]
fn runtime_error_exit_code_is_propagated() {
    let temp = TestDir::new();
    let path = temp.path("zero.ncr");
    fs::write(&path, "void main() { println(1 / 0); }").unwrap();
    let out = nacre(&["run", path_str(&path)]);
    assert_eq!(out.status.code(), Some(70));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[1:"), "{stderr}");
}

#[test]
fn run_counter_v1() {
    let out = nacre(&["run", "examples/language/counter.ncr"]);
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
        .args(["emit-flow", "examples/ownership/loop_cleanup.ncr"])
        .env("NACRE_CC", "nacre-deliberately-missing-compiler")
        .output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("loop-header"));
}
#[test]
fn emit_flow_writes_a_file() {
    let temp = TestDir::new();
    let path = temp.path("flow graph.txt");
    let out = nacre(&["emit-flow", "examples/ownership/loop_cleanup.ncr", "-o", path_str(&path)]);
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
    let out = nacre(&["run", "examples/ownership/loop_cleanup.ncr", "--memory-stats"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout, b"13\n");
    assert_eq!(out.stderr, b"NACRE_MEMORY allocations=4 frees=4 live=0\n");
}
#[test]
fn memory_stats_is_not_a_check_option() {
    assert!(!nacre(&["check", "examples/language/hello_world.ncr", "--memory-stats"]).status.success());
}
#[test]
fn duplicate_memory_stats_flag_is_rejected() {
    assert!(!nacre(&["run", "examples/language/hello_world.ncr", "--memory-stats", "--memory-stats"]).status.success());
}
#[test]
fn missing_emit_flow_output_path_is_rejected() {
    assert!(!nacre(&["emit-flow", "examples/language/hello_world.ncr", "-o"]).status.success());
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
    let out = nacre(&["emit-borrows", "examples/ownership/borrowing.ncr"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("lexical borrow analysis"));
    assert!(text.contains("loan #"));
}
#[test]
fn emits_borrow_analysis_to_file() {
    let temp = TestDir::new();
    let path = temp.path("borrow report.txt");
    let out = nacre(&["emit-borrows", "examples/ownership/borrowing.ncr", "-o", path_str(&path)]);
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
    assert!(!nacre(&["emit-borrows", "examples/ownership/borrowing.ncr", "--memory-stats"]).status.success());
}
#[test]
fn borrower_diagnostic_reaches_the_cli() {
    let out = nacre(&["check", "examples/invalid/borrow_then_drop.ncr"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("E3002") && text.contains("note:"));
}
#[test]
fn pgo_generate_and_use_workflow_and_mutual_exclusion() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TestDir::new();
    let source = temp.path("pgo.ncr");
    fs::write(&source, "void main() { int i = 0; while (i < 100) { i = i + 1; } println(i); }").unwrap();
    let bin = temp.path("pgo_bin");
    let pgo_dir = temp.path("pgo_data");
    let compiler = temp.path("gcc-require-profile");
    fs::write(&compiler, "#!/bin/sh\nexec gcc -Werror=missing-profile \"$@\"\n").unwrap();
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
    let invoke = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(args)
        .env("NACRE_CC", &compiler)
        .env("NACRE_CACHE_DIR", temp.path("native-cache"))
        .env("NACRE_ENABLE_TEST_CACHE", "1")
        .env_remove("NACRE_NO_CACHE")
        .output().unwrap();

    let both = invoke(&["build", path_str(&source), "-o", path_str(&bin), "--pgo-generate", "--pgo-use"]);
    assert!(!both.status.success());
    let generate = format!("--pgo-generate={}", pgo_dir.display());
    let build_gen = invoke(&["build", path_str(&source), "-o", path_str(&bin), &generate]);
    assert!(build_gen.status.success(), "{}", String::from_utf8_lossy(&build_gen.stderr));
    let run_res = Command::new(&bin).current_dir(&temp.0).output().unwrap();
    assert!(run_res.status.success());
    assert_eq!(run_res.stdout, b"100\n");

    // Missing training data is a compiler error, not an invisible warning.
    let use_profile = format!("--pgo-use={}", pgo_dir.display());
    let opt = invoke(&["build", path_str(&source), "-o", path_str(&bin), &use_profile]);
    assert!(opt.status.success(), "{}", String::from_utf8_lossy(&opt.stderr));
    let run_opt = Command::new(&bin).output().unwrap();
    assert!(run_opt.status.success());
    assert_eq!(run_opt.stdout, b"100\n");

    // A previous executable cache entry must not conceal changed profile inputs.
    fs::remove_dir_all(&pgo_dir).unwrap();
    fs::create_dir(&pgo_dir).unwrap();
    let missing_profile = invoke(&["build", path_str(&source), "-o", path_str(&bin), &use_profile]);
    assert!(!missing_profile.status.success(), "profile-use reused an executable after its training data was removed");
}
