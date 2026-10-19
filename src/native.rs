//! Native C11/LLVM compilation with explicit tool arguments; commands never invoke a shell.
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

mod dependencies;
pub mod toolchain;
use dependencies::NativeLibraries;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
pub(crate) const OUTPUT_CAPTURE_LIMIT_BYTES: usize = 64 * 1024;
const RUNTIME_C_FLAGS: &[&str] = &[
    "-std=c11",
    "-pthread",
    "-g0",
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
    pub(crate) fn output_limited(
        &self,
        args: &[&str],
        max_bytes: usize,
        stdout_capture: StdoutCapture,
    ) -> Result<LimitedOutput, String> {
        let mut command = Command::new(&self.executable);
        command.args(args);
        capture_limited_output(command, max_bytes, stdout_capture)
    }
    pub fn save(&self, output: &Path) -> Result<(), String> {
        create_parent(output)?;
        fs::copy(&self.executable, output).map_err(|e| format!("failed to write {}: {e}", output.display()))?;
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub(crate) enum StdoutCapture {
    Discard,
    Capture,
}
pub(crate) struct LimitedOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

impl LimitedOutput {
    pub(crate) fn truncation_note(&self, max_bytes: usize) -> Option<String> {
        let streams = match (self.stdout_truncated, self.stderr_truncated) {
            (false, false) => return None,
            (true, false) => "stdout",
            (false, true) => "stderr",
            (true, true) => "stdout and stderr",
        };
        Some(format!(
            "child output capture exceeded {max_bytes} bytes per stream ({streams}); excess bytes were discarded"
        ))
    }
}

struct CappedStream {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_capped<R: Read>(mut reader: R, max_bytes: usize) -> std::io::Result<CappedStream> {
    let mut bytes = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 8192];
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        let copied = count.min(max_bytes.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&chunk[..copied]);
        truncated |= copied < count;
    }
    Ok(CappedStream { bytes, truncated })
}

fn spawn_capped_reader<R: Read + Send + 'static>(
    reader: R,
    stream: &'static str,
    max_bytes: usize,
) -> std::io::Result<JoinHandle<std::io::Result<CappedStream>>> {
    thread::Builder::new()
        .name(format!("nacre-{stream}-capture"))
        .spawn(move || read_capped(reader, max_bytes))
}

fn join_capped_reader(
    reader: JoinHandle<std::io::Result<CappedStream>>,
    stream: &str,
) -> Result<CappedStream, String> {
    reader.join()
        .map_err(|_| format!("{stream} capture thread panicked"))?
        .map_err(|error| format!("failed to read child {stream}: {error}"))
}

pub(super) fn capture_limited_output(
    mut command: Command,
    max_bytes: usize,
    stdout_capture: StdoutCapture,
) -> Result<LimitedOutput, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(match stdout_capture {
            StdoutCapture::Capture => Stdio::piped(),
            StdoutCapture::Discard => Stdio::null(),
        })
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to execute program: {error}"))?;
    let stdout = match stdout_capture {
        StdoutCapture::Capture => Some(child.stdout.take().expect("stdout is piped")),
        StdoutCapture::Discard => None,
    };
    let stderr = child.stderr.take().expect("stderr is piped");
    let stdout_reader = if let Some(stdout) = stdout {
        match spawn_capped_reader(stdout, "stdout", max_bytes) {
            Ok(reader) => Some(reader),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("failed to start stdout capture: {error}"));
            }
        }
    } else {
        None
    };
    let stderr_reader = match spawn_capped_reader(stderr, "stderr", max_bytes) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(reader) = stdout_reader {
                let _ = reader.join();
            }
            return Err(format!("failed to start stderr capture: {error}"));
        }
    };
    let status = match child.wait() {
        Ok(status) => status,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(reader) = stdout_reader {
                let _ = reader.join();
            }
            let _ = stderr_reader.join();
            return Err(format!("failed to wait for program: {error}"));
        }
    };
    let stdout = match stdout_reader {
        Some(reader) => join_capped_reader(reader, "stdout")?,
        None => CappedStream { bytes: Vec::new(), truncated: false },
    };
    let stderr = join_capped_reader(stderr_reader, "stderr")?;
    Ok(LimitedOutput {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
    })
}
fn compiler_stderr(output: &LimitedOutput) -> String {
    let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if let Some(note) = output.truncation_note(OUTPUT_CAPTURE_LIMIT_BYTES) {
        if !stderr.is_empty() { stderr.push('\n'); }
        stderr.push_str(&note);
    }
    stderr
}

pub fn find_c_compiler() -> Result<OsString, String> {
    find_c_compiler_with_version().map(|(compiler, _)| compiler)
}

fn find_c_compiler_with_version() -> Result<(OsString, Vec<u8>), String> {
    for variable in ["NACRE_CC", "CC"] {
        if let Some(cc) = env::var_os(variable) {
            if let Some(version) = c_compiler_version(&cc) {
                return Ok((cc, version));
            }
            return Err(format!("{variable}={cc:?}: C compiler not found. Set it to a compiler executable only, without options (for example, CC=clang)."));
        }
    }
    for cc in ["cc", "clang", "gcc"] {
        let compiler = OsString::from(cc);
        if let Some(version) = c_compiler_version(&compiler) {
            return Ok((compiler, version));
        }
    }
    Err("no C compiler found. Install GCC or Clang with C headers and a linker, then check `cc --version`. `check` and `emit-c` do not require a C compiler.".to_owned())
}

fn c_compiler_version(compiler: &std::ffi::OsStr) -> Option<Vec<u8>> {
    let mut command = Command::new(compiler);
    command.arg("--version");
    let output = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Capture).ok()?;
    if !output.status.success() || output.truncation_note(OUTPUT_CAPTURE_LIMIT_BYTES).is_some() { return None; }
    let mut version = Vec::new();
    append_identity_part(&mut version, &output.stdout);
    append_identity_part(&mut version, &output.stderr);
    Some(version)
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

fn append_identity_part(identity: &mut Vec<u8>, part: &[u8]) {
    identity.extend_from_slice(&(part.len() as u64).to_le_bytes());
    identity.extend_from_slice(part);
}

fn cache_compiler_identity(compiler: &std::ffi::OsStr, version: &[u8]) -> Option<Vec<u8>> {
    let mut identity = Vec::new();
    append_identity_part(&mut identity, version);
    if Path::new(compiler).components().count() == 1
        && let Some(path) = env::var_os("PATH") {
            append_identity_part(&mut identity, path.as_encoded_bytes());
        }
    let mut command = Command::new(compiler);
    command.arg("-dumpmachine");
    if let Ok(target) = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Capture)
        && target.status.success() {
            if target.truncation_note(OUTPUT_CAPTURE_LIMIT_BYTES).is_some() { return None; }
            append_identity_part(&mut identity, &target.stdout);
            append_identity_part(&mut identity, &target.stderr);
        }
    Some(identity)
}

const CACHE_HASH_PRIME: u128 = 0x100000001b3;

fn update_cache_hash(hash: &mut u128, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u128::from(*byte)).wrapping_mul(CACHE_HASH_PRIME);
    }
}

fn hash_cache_part(hash: &mut u128, part: &[u8]) {
    update_cache_hash(hash, &(part.len() as u128).to_le_bytes());
    update_cache_hash(hash, part);
}

fn hash_compiler_environment(hash: &mut u128) {
    for variable in [
        "CPATH",
        "C_INCLUDE_PATH",
        "SDKROOT",
        "MACOSX_DEPLOYMENT_TARGET",
        "CLANG_CONFIG_FILE",
        "CCC_OVERRIDE_OPTIONS",
    ] {
        update_cache_hash(hash, variable.as_bytes());
        *hash = (*hash ^ 0xff).wrapping_mul(CACHE_HASH_PRIME);
        if let Some(value) = env::var_os(variable) {
            update_cache_hash(hash, value.as_encoded_bytes());
        }
        *hash = (*hash ^ 0xff).wrapping_mul(CACHE_HASH_PRIME);
    }
}

fn compute_cache_key(
    compiler: &std::ffi::OsStr,
    compiler_identity: &[u8],
    optimization: &str,
    flags: &[&str],
    source: &str,
    dependencies: u128,
) -> String {
    let mut hash = 0xcbf29ce484222325u64 as u128;
    hash_cache_part(&mut hash, compiler.as_encoded_bytes());
    hash_cache_part(&mut hash, compiler_identity);
    hash_cache_part(&mut hash, optimization.as_bytes());
    hash_cache_part(&mut hash, &(flags.len() as u128).to_le_bytes());
    for flag in flags { hash_cache_part(&mut hash, flag.as_bytes()); }
    hash_cache_part(&mut hash, source.as_bytes());
    hash_cache_part(&mut hash, &dependencies.to_le_bytes());
    hash_compiler_environment(&mut hash);
    format!("{hash:032x}")
}

fn parse_make_dependencies(output: &[u8]) -> Option<Vec<PathBuf>> {
    let dependencies = std::str::from_utf8(output).ok()?.strip_prefix("nacre:")?.as_bytes();
    let mut paths = Vec::new();
    let mut path = Vec::new();
    let mut index = 0;
    while index < dependencies.len() {
        let byte = dependencies[index];
        if byte == b'\\' {
            index += 1;
            let escaped = *dependencies.get(index)?;
            if escaped == b'\n' {
                index += 1;
                continue;
            }
            if escaped == b'\r' && dependencies.get(index + 1) == Some(&b'\n') {
                index += 2;
                continue;
            }
            path.push(escaped);
            index += 1;
        } else if byte.is_ascii_whitespace() {
            if !path.is_empty() {
                paths.push(PathBuf::from(String::from_utf8(std::mem::take(&mut path)).ok()?));
            }
            index += 1;
        } else {
            path.push(byte);
            index += 1;
        }
    }
    if !path.is_empty() {
        paths.push(PathBuf::from(String::from_utf8(path).ok()?));
    }
    (!paths.is_empty()).then_some(paths)
}

fn c_compiler_command(
    compiler: &std::ffi::OsStr,
    optimization: &str,
    libraries: &NativeLibraries,
    flags: &[&str],
) -> Command {
    let mut command = Command::new(compiler);
    command.arg(optimization)
        .args(["-std=c11", "-pthread", "-g", "-fno-math-errno", "-Wall", "-Wextra", "-pedantic", "-Wno-unused-function", "-Wno-unused-variable", "-Wno-unused-parameter", "-Wno-unused-but-set-variable"])
        .args(if cfg!(target_os = "linux") { &["-ffunction-sections", "-fdata-sections"][..] } else { &[][..] })
        .args(libraries.header_flags())
        .args(flags);
    command
}

fn dependency_files_fingerprint(output: &[u8], source_path: &Path) -> Option<u128> {
    let source_path = source_path.canonicalize().ok()?;
    let mut paths = parse_make_dependencies(output)?;
    paths.sort();
    paths.dedup();
    let mut fingerprint = 0xcbf29ce484222325u64 as u128;
    for path in paths {
        let canonical = path.canonicalize().ok()?;
        if canonical == source_path { continue; }
        let metadata = fs::metadata(&canonical).ok()?;
        let mut file = fs::File::open(&canonical).ok()?;
        hash_cache_part(&mut fingerprint, path.as_os_str().as_encoded_bytes());
        hash_cache_part(&mut fingerprint, &metadata.len().to_le_bytes());
        let mut buffer = [0; 8192];
        loop {
            let bytes_read = file.read(&mut buffer).ok()?;
            if bytes_read == 0 { break; }
            update_cache_hash(&mut fingerprint, &buffer[..bytes_read]);
        }
    }
    Some(fingerprint)
}

fn dependency_cache_fingerprint(
    compiler: &std::ffi::OsStr,
    optimization: &str,
    flags: &[&str],
    libraries: &NativeLibraries,
    source_path: &Path,
) -> Option<u128> {
    let mut command = c_compiler_command(compiler, optimization, libraries, flags);
    command.args(["-M", "-MT", "nacre", "-MF", "-"]).arg(source_path);
    let output = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Capture).ok()?;
    if !output.status.success() || output.stdout_truncated { return None; }
    dependency_files_fingerprint(&output.stdout, source_path)
}

fn runtime_dependency_fingerprint(
    compiler: &std::ffi::OsStr,
    optimization: &str,
    flags: &[&str],
    section_flags: &[&str],
    libraries: &NativeLibraries,
    source_path: &Path,
) -> Option<u128> {
    let mut command = Command::new(compiler);
    command.arg(optimization)
        .args(RUNTIME_C_FLAGS)
        .args(section_flags)
        .args(libraries.header_flags())
        .args(flags.iter().copied().filter(|flag| !is_link_only_flag(flag)))
        .args(["-M", "-MT", "nacre", "-MF", "-"])
        .arg(source_path);
    let output = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Capture).ok()?;
    if !output.status.success() || output.stdout_truncated { return None; }
    dependency_files_fingerprint(&output.stdout, source_path)
}

/// Compile at an explicitly selected level; flags are trusted separate arguments.
pub fn compile_c_optimized(source: &str, optimization: &str, flags: &[&str]) -> Result<NativeProgram, String> {
    check_optimization(optimization)?;
    let profile_directory = pgo_directory(flags);
    // Profiles are mutable external inputs, not part of the executable cache key.
    let cache = profile_directory.is_none().then(cache_dir).flatten();
    let (compiler, compiler_version) = find_c_compiler_with_version()?;
    let compiler_identity = cache.as_ref().and_then(|_| cache_compiler_identity(&compiler, &compiler_version));
    let directory = TempDir::new()?;
    let executable = directory.0.join(if cfg!(windows) { "program.exe" } else { "program" });

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
    let cache_entry = match (&cache, &compiler_identity) {
        (Some(cache), Some(identity)) => dependency_cache_fingerprint(
            &compiler,
            optimization,
            flags,
            &libraries,
            &c_path,
        ).map(|dependencies| cache.join(compute_cache_key(
            &compiler, identity, optimization, flags, source, dependencies,
        ))),
        _ => None,
    };
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

    let mut command = c_compiler_command(&compiler, optimization, &libraries, flags);
    if let Some(profile_directory) = profile_directory {
        // Keep GCC's auxiliary identity stable across separate temporary builds.
        command.arg("-dumpdir").arg(Path::new(profile_directory).join("nacre-"))
            .args(["-dumpbase", "program.c"]);
    }
    command
        .arg(&c_path).arg("-o").arg(&executable)
        .args(if cfg!(unix) { &["-lm"][..] } else { &[][..] })
        .args(if cfg!(target_os = "linux") { &["-Wl,--gc-sections"][..] } else { &[][..] })
        .args(libraries.link_flags());
    let output = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Discard)
        .map_err(|error| format!("failed to start {compiler:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!("C compiler {compiler:?} failed:\n{}\nTo inspect the generated code: nacre emit-c source.ncr -o debug.c", compiler_stderr(&output)));
    }
    if let Some(cached_path) = &cache_entry {
        if let Some(parent) = cached_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let tmp_cached = cached_path.with_file_name(format!(
            "{}.tmp.{}.{}",
            cached_path.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id(),
            serial,
        ));
        if fs::copy(&executable, &tmp_cached).is_err()
            || fs::rename(&tmp_cached, cached_path).is_err() {
                let _ = fs::remove_file(&tmp_cached);
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
    let ir_path = directory.0.join("program.ll");
    fs::write(&c_path, source).map_err(|e| e.to_string())?;
    let mut command = Command::new(&compiler);
    command
        .args(["-std=c11", optimization, "-pthread", "-S", "-emit-llvm", "-fno-ident"])
        .args(NativeLibraries::from_source(source).header_flags())
        .arg(&c_path).arg("-o").arg(&ir_path);
    let output = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Discard)
        .map_err(|e| format!("emit-llvm requires Clang (NACRE_CLANG={compiler:?}): {e}"))?;
    if !output.status.success() {
        return Err(format!("Clang LLVM generation failed:\n{}", compiler_stderr(&output)));
    }
    let ir = fs::read(&ir_path).map_err(|error| format!("failed to read LLVM output: {error}"))?;
    String::from_utf8(ir).map_err(|e| format!("LLVM output is not valid UTF-8: {e}"))
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
    let tools = toolchain::Toolchain::resolve()?;
    tools.ensure_compatible()?;
    let compiler = tools.compiler();
    let linker = tools.linker();
    let compiler_version = tools.compiler_version();
    let linker_version = tools.linker_version();
    let target = tools.target();
    let libraries = NativeLibraries::from_source(runtime_c);
    let section_flags: &[&str] = if cfg!(target_os = "linux") { LINUX_SECTION_FLAGS } else { &[] };
    let cache = cache_dir().filter(|_| pgo_directory(flags).is_none());
    let directory = TempDir::new()?;
    let executable = directory.0.join(if cfg!(windows) { "program.exe" } else { "program" });
    let runtime_bitcode = directory.0.join("runtime.bc");
    let runtime_path = directory.0.join("runtime.c");
    let user_path = directory.0.join("program.ll");
    let linked_bitcode = directory.0.join("linked.bc");

    fs::write(&user_path, llvm_ir).map_err(|e| format!("failed to write temporary direct LLVM IR: {e}"))?;
    fs::write(&runtime_path, runtime_c).map_err(|e| format!("failed to write temporary LLVM runtime C source: {e}"))?;
    let cache_entry = cache.and_then(|cache| {
        runtime_dependency_fingerprint(compiler, optimization, flags, section_flags, &libraries, &runtime_path)
            .map(|dependencies| {
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
                    dependencies,
                );
                cache.join(format!("llvm-runtime-{key}.bc"))
            })
    });

    let cached_runtime = cache_entry.as_ref().filter(|path| {
        fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
    });
    let runtime_bitcode_input = if let Some(cached) = cached_runtime {
        cached.as_path()
    } else {
        let mut command = Command::new(compiler);
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
        let output = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Discard)
            .map_err(|e| format!("failed to run Clang {compiler:?} for LLVM runtime bitcode: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "Clang compilation of LLVM runtime bitcode failed:\n{}",
                compiler_stderr(&output),
            ));
        }
        if let Some(cache_entry) = &cache_entry {
            cache_bitcode(&runtime_bitcode, cache_entry);
        }
        runtime_bitcode.as_path()
    };

    let mut command = Command::new(linker);
    command.args(["--only-needed", "--internalize"])
        .arg(&user_path)
        .arg(runtime_bitcode_input)
        .arg("-o")
        .arg(&linked_bitcode);
    let output = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Discard)
        .map_err(|e| format!("direct LLVM compilation requires llvm-link ({linker:?}): {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "llvm-link {linker:?} failed to link direct LLVM and runtime bitcode:\n{}",
            compiler_stderr(&output),
        ));
    }

    let mut command = Command::new(compiler);
    command.arg(optimization)
        .args(USER_IR_FLAGS)
        .args(section_flags)
        .args(flags)
        .arg(&linked_bitcode)
        .args(if cfg!(unix) { &["-lm"][..] } else { &[][..] })
        .args(if cfg!(target_os = "linux") { &["-Wl,--gc-sections"][..] } else { &[][..] })
        .args(libraries.link_flags())
        .arg("-o")
        .arg(&executable);
    let output = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Discard)
        .map_err(|e| format!("failed to run Clang {compiler:?} for native LLVM linking: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Clang native linking of direct LLVM program failed:\n{}",
            compiler_stderr(&output),
        ));
    }
    Ok(NativeProgram { executable, _directory: directory })
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
    dependencies: u128,
) -> String {
    let mut hash = 0xcbf29ce484222325u64 as u128;
    for part in identity {
        for &byte in part {
            hash = (hash ^ byte as u128).wrapping_mul(CACHE_HASH_PRIME);
        }
        hash = (hash ^ 0xff).wrapping_mul(CACHE_HASH_PRIME);
    }
    for group in [RUNTIME_C_FLAGS, section_flags] {
        hash_flag_group(&mut hash, group.iter().copied());
    }
    hash_flag_group(&mut hash, header_flags);
    for group in [flags, BITCODE_FLAGS] {
        hash_flag_group(&mut hash, group.iter().copied());
    }
    hash_compiler_environment(&mut hash);
    hash_cache_part(&mut hash, runtime_c.as_bytes());
    hash_cache_part(&mut hash, &dependencies.to_le_bytes());
    format!("{hash:032x}")
}

fn hash_flag_group<'a>(hash: &mut u128, flags: impl Iterator<Item = &'a str>) {
    *hash = (*hash ^ 0xfe).wrapping_mul(CACHE_HASH_PRIME);
    for flag in flags {
        for &byte in flag.as_bytes() {
            *hash = (*hash ^ u128::from(byte)).wrapping_mul(CACHE_HASH_PRIME);
        }
        *hash = (*hash ^ 0xff).wrapping_mul(CACHE_HASH_PRIME);
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
