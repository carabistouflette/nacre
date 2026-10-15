//! Inspection, deterministic output and CLI integration. LLVM-only tests are
//! explicitly ignored by default and require `cargo test ... -- --ignored`.
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use nacre::{compile_to_c, emit_borrows, emit_flow, emit_types, native};

const INTERFACES: &str = include_str!("../examples/language/interfaces.ncr");
const GENERICS: &str = include_str!("../examples/language/generic_box.ncr");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("nacre-v4-test-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap(); Self(path)
    }
}
impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

#[test]
fn lists_specializations_once() {
    let source = "class Box<T>{T v;Box(T v){this.v=v;}}void main(){Box<int> a=new Box<int>(1);Box<int> b=new Box<int>(2);}";
    let text = emit_types(source).unwrap();
    assert_eq!(text.matches("class Box<int>\n").count(), 1);
    assert!(!text.contains("Box<T>"));
}
#[test]
fn lists_generic_interface_implementations() {
    let text = emit_types(include_str!("../examples/language/generic_interfaces.ncr")).unwrap();
    assert!(text.contains("interface Source<int>"));
    assert!(text.contains("implements Source<String>"));
}
#[test]
fn unused_template_not_emitted() {
    let text = emit_types("class Unused<T>{}void main(){}").unwrap();
    assert!(!text.contains("Unused"));
}
#[test]
fn interface_c_is_deterministic() { assert_eq!(compile_to_c(INTERFACES).unwrap(), compile_to_c(INTERFACES).unwrap()); }
#[test]
fn generic_c_is_deterministic() { assert_eq!(compile_to_c(GENERICS).unwrap(), compile_to_c(GENERICS).unwrap()); }
#[test]
fn interface_upcast_preserves_flow_moves() {
    let text = emit_flow("interface I{}class C implements I{}void main(){C c=new C();I i=c;}").unwrap();
    assert!(text.contains("Move") || text.contains("move"));
}
#[test]
fn interface_borrow_preserves_trace() {
    let text = emit_borrows(include_str!("../examples/ownership/interface_borrowing.ncr")).unwrap();
    assert!(text.contains("loan #"));
    assert!(text.contains("shared"));
}
#[test]
fn inspection_does_not_bypass_validation() {
    assert!(emit_types("interface I{void f();}class C implements I{}void main(){}").is_err());
}
#[test]
fn invalid_native_optimization_is_rejected_without_toolchain() {
    assert!(native::compile_c_optimized("", "-O9", &[]).is_err());
}
#[test]
fn invalid_llvm_optimization_is_rejected_without_toolchain() {
    assert!(native::emit_llvm("", "-O9").is_err());
}
#[test]
fn cli_emit_types() {
    let out = Command::new(env!("CARGO_BIN_EXE_nacre")).args(["emit-types", "examples/language/generic_box.ncr"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Box<int>"));
}
#[test]
fn cli_emit_types_protects_source() {
    let temp = Temp::new();
    let path = temp.0.join("source.ncr");
    fs::write(&path, INTERFACES).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_nacre")).arg("emit-types").arg(&path).arg("-o").arg(&path).output().unwrap();
    assert!(!out.status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), INTERFACES);
}
#[test]
fn cli_optimization_levels_preserve_output() {
    for level in ["-O0", "-O1", "-O2", "-O3", "-Os"] {
        let out = Command::new(env!("CARGO_BIN_EXE_nacre"))
            .args(["run", "examples/language/interfaces.ncr", level, "--memory-stats"]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(out.stdout, b"Woof!\nWoof!\nMeow!\n");
        assert_eq!(out.stderr, b"NACRE_MEMORY allocations=2 frees=2 live=0\n");
    }
}
#[test]
fn cli_duplicate_optimization_is_rejected() {
    let out = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["run", "examples/language/interfaces.ncr", "-O0", "-O3"]).output().unwrap();
    assert!(!out.status.success());
}
#[test]
fn cli_check_does_not_accept_ir_only_options() {
    let out = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["check", "examples/language/interfaces.ncr", "-O0"]).output().unwrap();
    assert!(!out.status.success());
}
#[test]
#[ignore = "requires Clang; run cargo test --test features_v4_tools -- --ignored"]
fn llvm_ir_from_real_frontend() {
    let c = compile_to_c(INTERFACES).unwrap();
    for level in ["-O0", "-O2"] {
        let ir = native::emit_llvm(&c, level).unwrap();
        assert!(ir.contains("define ") && ir.contains("@main("));
    }
}
#[test]
#[ignore = "requires Clang; run cargo test --test features_v4_tools -- --ignored"]
fn cli_emit_llvm() {
    let temp = Temp::new();
    let path = temp.0.join("test.ll");
    let out = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["emit-llvm", "examples/language/generic_interfaces.ncr", "-O2", "-o"])
        .arg(&path).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(fs::read_to_string(path).unwrap().contains("@main("));
}
