//! Native compilation with a system C11 compiler. Commands never invoke a shell.
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Result<Self, String> {
        let base = env::temp_dir();
        let base = if base.is_absolute() { base } else { env::current_dir().map_err(|e| e.to_string())?.join(base) };
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
        for _ in 0..32 {
            let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!("nacre-{}-{stamp}-{serial}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)] {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("failed to create a temporary directory: {e}")),
            }
        }
        Err("failed to reserve a unique temporary directory".to_owned())
    }
}
impl Drop for TempDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

pub struct NativeProgram { executable: PathBuf, _directory: TempDir }
impl NativeProgram {
    pub fn path(&self) -> &Path { &self.executable }
    pub fn run(&self, args: &[OsString]) -> Result<ExitStatus, String> {
        Command::new(&self.executable).args(args).status().map_err(|e| format!("failed to run program: {e}"))
    }
    pub fn output(&self) -> Result<Output, String> {
        Command::new(&self.executable).output().map_err(|e| format!("failed to run program: {e}"))
    }
    pub fn save(&self, output: &Path) -> Result<(), String> {
        create_parent(output)?;
        fs::copy(&self.executable, output).map_err(|e| format!("failed to write {}: {e}", output.display()))?;
        Ok(())
    }
}

pub fn find_c_compiler() -> Result<OsString, String> {
    for variable in ["NACRE_CC", "CC"] {
        if let Some(cc) = env::var_os(variable) {
            let available = Command::new(&cc).arg("--version").output().is_ok_and(|o| o.status.success());
            if available { return Ok(cc); }
            return Err(format!("{variable}={cc:?}: C compiler not found. Set it to a compiler executable only, without options (for example, CC=clang)."));
        }
    }
    for cc in ["cc", "clang", "gcc"] {
        if Command::new(cc).arg("--version").output().is_ok_and(|o| o.status.success()) {
            return Ok(OsString::from(cc));
        }
    }
    Err("no C compiler found. Install GCC or Clang with C headers and a linker, then check `cc --version`. `check` and `emit-c` do not require a C compiler.".to_owned())
}

pub fn compile_c(source: &str) -> Result<NativeProgram, String> {
    compile_c_with_flags(source, &[])
}

/// Additional trusted compiler flags, useful for counters and sanitizers in tests.
/// Each flag is a separate argument; no shell expansion is performed.
pub fn compile_c_with_flags(source: &str, flags: &[&str]) -> Result<NativeProgram, String> {
    compile_c_optimized(source, "-O2", flags)
}
/// Compile at an explicitly selected level; flags are trusted separate arguments.
pub fn compile_c_optimized(source: &str, optimization: &str, flags: &[&str]) -> Result<NativeProgram, String> {
    check_optimization(optimization)?;
    let compiler = find_c_compiler()?;
    let directory = TempDir::new()?;
    let c_path = directory.0.join("program.c");
    let executable = directory.0.join(if cfg!(windows) { "program.exe" } else { "program" });
    fs::write(&c_path, source).map_err(|e| format!("failed to write temporary C source: {e}"))?;
    let output = Command::new(&compiler)
        .arg(optimization)
        .args(["-std=c11", "-Wall", "-Wextra", "-pedantic", "-Wno-unused-function", "-Wno-unused-variable", "-Wno-unused-parameter", "-Wno-unused-but-set-variable"])
        .args(flags)
        .arg(&c_path).arg("-o").arg(&executable)
        .args(if cfg!(unix) { &["-lm"][..] } else { &[][..] })
        .output().map_err(|e| format!("failed to start {compiler:?}: {e}"))?;
    if !output.status.success() {
        return Err(format!("C compiler {compiler:?} failed:\n{}\nTo inspect the generated code: nacre emit-c source.ncr -o debug.c", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(NativeProgram { executable, _directory: directory })
}

pub fn create_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    Ok(())
}

/// LLVM textual IR via Clang's C frontend, not a direct Nacre LLVM backend.
/// NACRE_CLANG selects a compiler executable only, never a shell command.
pub fn emit_llvm(source: &str, optimization: &str) -> Result<String, String> {
    check_optimization(optimization)?;
    let compiler = env::var_os("NACRE_CLANG").unwrap_or_else(|| OsString::from("clang"));
    let directory = TempDir::new()?;
    let c_path = directory.0.join("program.c");
    fs::write(&c_path, source).map_err(|e| e.to_string())?;
    let output = Command::new(&compiler)
        .args(["-std=c11", optimization, "-S", "-emit-llvm", "-fno-ident"])
        .arg(&c_path).args(["-o", "-"])
        .output().map_err(|e| format!("emit-llvm requires Clang (NACRE_CLANG={compiler:?}): {e}"))?;
    if !output.status.success() {
        return Err(format!("Clang LLVM generation failed:\n{}", String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("LLVM output is not valid UTF-8: {e}"))
}
fn check_optimization(level: &str) -> Result<(), String> {
    if matches!(level, "-O0" | "-O1" | "-O2" | "-O3" | "-Os") { Ok(()) }
    else { Err("invalid optimization level: choose -O0, -O1, -O2, -O3, or -Os".to_owned()) }
}
