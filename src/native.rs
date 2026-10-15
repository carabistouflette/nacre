//! Native C11/LLVM compilation with explicit tool arguments; commands never invoke a shell.
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

mod dependencies;
use dependencies::NativeLibraries;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
const RUNTIME_C_FLAGS: &[&str] = &[
    "-std=c11",
    "-pthread",
    "-g",
    "-fdebug-compilation-dir=.",
    "-fno-math-errno",
    "-Wall",
    "-Wextra",
    "-pedantic",
    "-Wno-unused-function",
    "-Wno-unused-variable",
    "-Wno-unused-parameter",
    "-Wno-unused-but-set-variable",
];
const LINUX_SECTION_FLAGS: &[&str] = &["-ffunction-sections", "-fdata-sections"];
const BITCODE_FLAGS: &[&str] = &["-emit-llvm", "-c"];
const USER_IR_FLAGS: &[&str] = &["-pthread", "-g", "-fno-math-errno"];

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

fn pgo_directory<'a>(flags: &[&'a str]) -> Option<&'a str> {
    flags.iter().find_map(|flag| {
        flag.strip_prefix("-fprofile-generate=")
            .or_else(|| flag.strip_prefix("-fprofile-use="))
            .or_else(|| matches!(*flag, "-fprofile-generate" | "-fprofile-use").then_some("."))
    })
}

fn cache_dir() -> Option<PathBuf> {
    if env::var_os("NACRE_NO_CACHE").is_some() || (env::var_os("CARGO_MANIFEST_DIR").is_some() && env::var_os("NACRE_ENABLE_TEST_CACHE").is_none()) {
        return None;
    }
    if let Some(dir) = env::var_os("NACRE_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|p| p.join("nacre"))
}

fn compute_cache_key(compiler: &std::ffi::OsStr, optimization: &str, flags: &[&str], source: &str) -> String {
    let mut h: u128 = 0xcbf29ce484222325u64 as u128;
    let prime: u128 = 0x100000001b3u64 as u128;
    for b in compiler.as_encoded_bytes() {
        h = (h ^ (*b as u128)).wrapping_mul(prime);
    }
    for b in optimization.as_bytes() {
        h = (h ^ (*b as u128)).wrapping_mul(prime);
    }
    for flag in flags {
        for b in flag.as_bytes() {
            h = (h ^ (*b as u128)).wrapping_mul(prime);
        }
        h = (h ^ 0xff).wrapping_mul(prime);
    }
    for b in source.as_bytes() {
        h = (h ^ (*b as u128)).wrapping_mul(prime);
    }
    format!("{h:032x}")
}

/// Compile at an explicitly selected level; flags are trusted separate arguments.
pub fn compile_c_optimized(source: &str, optimization: &str, flags: &[&str]) -> Result<NativeProgram, String> {
    check_optimization(optimization)?;
    let compiler = find_c_compiler()?;
    let directory = TempDir::new()?;
    let executable = directory.0.join(if cfg!(windows) { "program.exe" } else { "program" });

    let profile_directory = pgo_directory(flags);
    // Profiles are mutable external inputs, not part of the executable cache key.
    let cache = profile_directory.is_none().then(cache_dir).flatten();
    let cache_entry = cache.as_ref().map(|c| c.join(compute_cache_key(&compiler, optimization, flags, source)));
    if let Some(cached_path) = &cache_entry
        && cached_path.is_file()
    {
        let _ = fs::remove_file(&executable);
        if fs::copy(cached_path, &executable).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&executable, fs::Permissions::from_mode(0o755));
            }
            return Ok(NativeProgram { executable, _directory: directory });
        }
    }

    let c_path = directory.0.join("program.c");
    let written = if profile_directory.is_some() {
        fs::File::create(&c_path).and_then(|mut file| {
            // GCC includes source locations in its profile checksums.
            file.write_all(b"#line 1 \"program.c\"\n")?;
            file.write_all(source.as_bytes())
        })
    } else {
        fs::write(&c_path, source)
    };
    written.map_err(|e| format!("failed to write temporary C source: {e}"))?;
    let libraries = NativeLibraries::from_source(source);
    let mut command = Command::new(&compiler);
    command
        .arg(optimization)
        .args(["-std=c11", "-pthread", "-g", "-fno-math-errno", "-Wall", "-Wextra", "-pedantic", "-Wno-unused-function", "-Wno-unused-variable", "-Wno-unused-parameter", "-Wno-unused-but-set-variable"])
        .args(if cfg!(target_os = "linux") { &["-ffunction-sections", "-fdata-sections"][..] } else { &[][..] })
        .args(libraries.header_flags())
        .args(flags);
    if let Some(profile_directory) = profile_directory {
        // Keep GCC's auxiliary identity stable across separate temporary builds.
        command.arg("-dumpdir").arg(Path::new(profile_directory).join("nacre-"))
            .args(["-dumpbase", "program.c"]);
    }
    let output = command
        .arg(&c_path).arg("-o").arg(&executable)
        .args(if cfg!(unix) { &["-lm"][..] } else { &[][..] })
        .args(if cfg!(target_os = "linux") { &["-Wl,--gc-sections"][..] } else { &[][..] })
        .args(libraries.link_flags())
        .output().map_err(|e| format!("failed to start {compiler:?}: {e}"))?;
    if !output.status.success() {
        return Err(format!("C compiler {compiler:?} failed:\n{}\nTo inspect the generated code: nacre emit-c source.ncr -o debug.c", String::from_utf8_lossy(&output.stderr)));
    }
    if let Some(cached_path) = &cache_entry {
        if let Some(parent) = cached_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let tmp_cached = cached_path.with_file_name(format!(
            "{}.tmp.{}",
            cached_path.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id()
        ));
        if fs::copy(&executable, &tmp_cached).is_ok() {
            let _ = fs::rename(&tmp_cached, cached_path);
        }
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
        .args(["-std=c11", optimization, "-pthread", "-S", "-emit-llvm", "-fno-ident"])
        .args(NativeLibraries::from_source(source).header_flags())
        .arg(&c_path).args(["-o", "-"])
        .output().map_err(|e| format!("emit-llvm requires Clang (NACRE_CLANG={compiler:?}): {e}"))?;
    if !output.status.success() {
        return Err(format!("Clang LLVM generation failed:\n{}", String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("LLVM output is not valid UTF-8: {e}"))
}

/// Compile direct LLVM IR together with the stdlib-only C runtime through LLVM bitcode.
/// User code is never sent through the C frontend.
pub(crate) fn compile_llvm_direct_with_runtime(
    llvm_ir: &str,
    runtime_c: &str,
    optimization: &str,
    flags: &[&str],
) -> Result<NativeProgram, String> {
    check_optimization(optimization)?;
    let compiler = env::var_os("NACRE_CLANG").unwrap_or_else(|| OsString::from("clang"));
    let linker = env::var_os("NACRE_LLVM_LINK").unwrap_or_else(|| OsString::from("llvm-link"));
    let compiler_version = tool_output(&compiler, &["--version"], "Clang")?;
    let linker_version = tool_output(&linker, &["--version"], "llvm-link")?;
    let target = tool_output(&compiler, &["-print-target-triple"], "Clang")?;
    let libraries = NativeLibraries::from_source(runtime_c);
    let section_flags: &[&str] = if cfg!(target_os = "linux") { LINUX_SECTION_FLAGS } else { &[] };
    let cache_entry = cache_dir().filter(|_| pgo_directory(flags).is_none()).map(|cache| {
        let key = compute_runtime_cache_key(
            [
                compiler.as_encoded_bytes(), compiler_version.as_bytes(),
                linker.as_encoded_bytes(), linker_version.as_bytes(),
                target.as_bytes(), optimization.as_bytes(),
            ],
            flags,
            section_flags,
            libraries.header_flags(),
            runtime_c,
        );
        cache.join(format!("llvm-runtime-{key}.bc"))
    });
    let directory = TempDir::new()?;
    let executable = directory.0.join(if cfg!(windows) { "program.exe" } else { "program" });
    let runtime_bitcode = directory.0.join("runtime.bc");
    let user_path = directory.0.join("program.ll");
    let linked_bitcode = directory.0.join("linked.bc");

    fs::write(&user_path, llvm_ir).map_err(|e| format!("failed to write temporary direct LLVM IR: {e}"))?;

    let cached_runtime = cache_entry.as_ref().filter(|path| {
        fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
    });
    let runtime_bitcode_input = if let Some(cached) = cached_runtime {
        cached.as_path()
    } else {
        let runtime_path = directory.0.join("runtime.c");
        fs::write(&runtime_path, runtime_c).map_err(|e| format!("failed to write temporary LLVM runtime C source: {e}"))?;
        let mut command = Command::new(&compiler);
        command
            .arg(optimization)
            .args(RUNTIME_C_FLAGS)
            .args(section_flags)
            .args(libraries.header_flags())
            .args(flags.iter().copied().filter(|flag| !is_link_only_flag(flag)))
            .args(BITCODE_FLAGS)
            .arg(&runtime_path)
            .arg("-o")
            .arg(&runtime_bitcode);
        let output = command.output().map_err(|e| format!("failed to run Clang {compiler:?} for LLVM runtime bitcode: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "Clang compilation of LLVM runtime bitcode failed:\n{}",
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        if let Some(cache_entry) = &cache_entry {
            cache_bitcode(&runtime_bitcode, cache_entry);
        }
        runtime_bitcode.as_path()
    };

    let output = Command::new(&linker)
        .args(["--only-needed", "--internalize"])
        .arg(&user_path)
        .arg(runtime_bitcode_input)
        .arg("-o")
        .arg(&linked_bitcode)
        .output()
        .map_err(|e| format!("direct LLVM compilation requires llvm-link ({linker:?}): {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "llvm-link {linker:?} failed to link direct LLVM and runtime bitcode:\n{}",
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    let output = Command::new(&compiler)
        .arg(optimization)
        .args(USER_IR_FLAGS)
        .args(section_flags)
        .args(flags)
        .arg(&linked_bitcode)
        .args(if cfg!(unix) { &["-lm"][..] } else { &[][..] })
        .args(if cfg!(target_os = "linux") { &["-Wl,--gc-sections"][..] } else { &[][..] })
        .args(libraries.link_flags())
        .arg("-o")
        .arg(&executable)
        .output()
        .map_err(|e| format!("failed to run Clang {compiler:?} for native LLVM linking: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Clang native linking of direct LLVM program failed:\n{}",
            String::from_utf8_lossy(&output.stderr),
        ));
    }
    Ok(NativeProgram { executable, _directory: directory })
}

fn tool_output(tool: &std::ffi::OsStr, args: &[&str], name: &str) -> Result<String, String> {
    let output = Command::new(tool)
        .args(args)
        .output()
        .map_err(|e| format!("direct LLVM compilation requires {name} ({tool:?}): {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{name} {tool:?} failed for {}:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr),
        ));
    }
    let mut result = String::from_utf8_lossy(&output.stdout).into_owned();
    result.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(result)
}

fn is_link_only_flag(flag: &str) -> bool {
    flag.starts_with("-l") || flag.starts_with("-L") || flag.starts_with("-Wl,")
}

fn compute_runtime_cache_key(
    identity: [&[u8]; 6],
    flags: &[&str],
    section_flags: &[&str],
    header_flags: impl Iterator<Item = &'static str>,
    runtime_c: &str,
) -> String {
    let mut hash = 0xcbf29ce484222325u64 as u128;
    let prime = 0x100000001b3u64 as u128;
    for part in identity {
        for &byte in part {
            hash = (hash ^ byte as u128).wrapping_mul(prime);
        }
        hash = (hash ^ 0xff).wrapping_mul(prime);
    }
    for group in [RUNTIME_C_FLAGS, section_flags] {
        hash_flag_group(&mut hash, group.iter().copied());
    }
    hash_flag_group(&mut hash, header_flags);
    for group in [flags, BITCODE_FLAGS] {
        hash_flag_group(&mut hash, group.iter().copied());
    }
    for variable in ["CPATH", "C_INCLUDE_PATH", "SDKROOT", "MACOSX_DEPLOYMENT_TARGET", "CLANG_CONFIG_FILE", "CCC_OVERRIDE_OPTIONS"] {
        for &byte in variable.as_bytes() {
            hash = (hash ^ byte as u128).wrapping_mul(prime);
        }
        hash = (hash ^ 0xff).wrapping_mul(prime);
        if let Some(value) = env::var_os(variable) {
            for &byte in value.as_encoded_bytes() {
                hash = (hash ^ byte as u128).wrapping_mul(prime);
            }
        }
        hash = (hash ^ 0xff).wrapping_mul(prime);
    }
    for &byte in runtime_c.as_bytes() {
        hash = (hash ^ byte as u128).wrapping_mul(prime);
    }
    format!("{hash:032x}")
}

fn hash_flag_group<'a>(hash: &mut u128, flags: impl Iterator<Item = &'a str>) {
    const PRIME: u128 = 0x100000001b3;
    *hash = (*hash ^ 0xfe).wrapping_mul(PRIME);
    for flag in flags {
        for &byte in flag.as_bytes() {
            *hash = (*hash ^ u128::from(byte)).wrapping_mul(PRIME);
        }
        *hash = (*hash ^ 0xff).wrapping_mul(PRIME);
    }
}

fn cache_bitcode(source: &Path, cache_entry: &Path) {
    let Some(parent) = cache_entry.parent() else { return };
    if fs::create_dir_all(parent).is_err() { return; }
    let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let temporary = cache_entry.with_file_name(format!(
        "{}.tmp.{}.{}",
        cache_entry.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        serial,
    ));
    if fs::copy(source, &temporary).is_err() {
        let _ = fs::remove_file(&temporary);
        return;
    }
    if fs::rename(&temporary, cache_entry).is_err() {
        let _ = fs::remove_file(&temporary);
    }
}

fn check_optimization(level: &str) -> Result<(), String> {
    if matches!(level, "-O0" | "-O1" | "-O2" | "-O3" | "-Os") { Ok(()) }
    else { Err("invalid optimization level: choose -O0, -O1, -O2, -O3, or -Os".to_owned()) }
}
