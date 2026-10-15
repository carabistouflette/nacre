#![forbid(unsafe_code)]
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use nacre::{compile_to_c, native};
use nacre::project::{self, SourceBundle};

const HELP: &str = "Nacre — experimental native compiler written in Rust

Usage:
  nacre check  [source.ncr ... | directory | Nacre.toml]
  nacre test   source.ncr ... | directory [--native] [--lto] [--jemalloc] [-- filter]
  nacre bench  source.ncr ... | directory [--native] [--lto] [--jemalloc] [-- filter]
  nacre fmt    source.ncr ... | directory [--check]
  nacre doc    source.ncr ... | directory [-o output-dir]
  nacre run    [source.ncr ... | directory | Nacre.toml] [--release] [--memory-stats] [--direct] [--native] [--lto] [--jemalloc] [-O0|-O1|-O2|-O3|-Os] [-- arguments...]
  nacre build  [source.ncr ... | directory | Nacre.toml] [-o executable] [--release] [--memory-stats] [--direct] [--native] [--lto] [--jemalloc] [-O0|-O1|-O2|-O3|-Os]
  nacre emit-c source.ncr [-o output.c] [--memory-stats]
  nacre emit-flow source.ncr [-o output.flow]
  nacre emit-borrows source.ncr [-o output.borrows]
  nacre emit-types source.ncr [-o output.types]
  nacre emit-llvm source.ncr [-o output.ll] [--direct] [--memory-stats] [-O0|-O1|-O2|-O3|-Os]
  nacre new new-project-directory
  nacre init new-project-directory
  nacre add <path> [--manifest-path <file>]
  nacre doctor [--direct] [--zip]
  nacre lsp
  nacre --help
  nacre --version

Examples with Cargo:
  cargo run -- check examples/language/hello_world.ncr
  cargo run -- run examples/language/counter.ncr
  cargo run -- build examples/language/counter.ncr -o out/counter
  cargo run -- emit-c examples/language/counter.ncr -o out/counter.c

Multiple files or a .ncr directory are accepted by source commands. A directory or Nacre.toml can select a manifest package; build, run, and check default to . when Nacre.toml exists.
Source files may declare modules and import public names; all requested inputs are compiled together.
--sanitize (run/build): AddressSanitizer + UndefinedBehaviorSanitizer.
--direct (run/build): compile typed Nacre LLVM IR and a stdlib-only C runtime via llvm-link; requires Clang and llvm-link.
--native (run/build): enable host CPU native instructions (-march=native), independent of --direct.
--lto (run/build): enable link-time optimization (-flto); combines with the manifest profile.
--release (run/build): use the manifest release profile; otherwise manifest build/run uses dev.
--jemalloc (run/build): link high-performance jemalloc allocator (-ljemalloc).
--pgo-generate (run/build): enable Profile-Guided Optimization instrumentation (-fprofile-generate).
--pgo-use (run/build): compile using collected PGO profile data (-fprofile-use).
new / init: create a new project; refuse an existing path.
add: add a local path dependency to Nacre.toml (defaults to ./Nacre.toml).
doctor: compile and run end-to-end checks with List and Text; optionally verify the direct LLVM backend and ZIP runtime.
doctor --direct: resolve matching Clang and llvm-link versions, then run a direct-backend program.
doctor --zip: write/read temporary ZIP archives, including an empty archive; requires the corrected libarchive build.
build: default output is out/<package-name> for a manifest package, otherwise out/<source-name>.
emit-c / emit-flow / emit-borrows / emit-types / emit-llvm: write to stdout when -o is omitted.
emit-llvm: default emits LLVM via Clang's C frontend; --direct emits user IR from the typed Nacre backend without C translation.
Native builds and C-front-end LLVM emission: -O2 by default.
emit-flow: ownership graph without a C compiler.
--memory-stats: allocation/deallocation counters on the target program's stderr; unavailable for emit-llvm --direct.
CC or NACRE_CC: path to a standalone C11 executable (cc, clang, gcc), without options.
NACRE_CLANG: Clang executable for LLVM emission and direct native builds.
NACRE_LLVM_LINK: llvm-link executable for direct native builds (defaults to llvm-link).
Existing output files are replaced, except for the source file.
Initial targets: Linux and WSL. macOS has not been tested.
";

fn main() {
    let code = match execute(env::args_os().skip(1).collect()) {
        Ok(code) => code,
        Err(message) => { eprintln!("{message}"); 1 }
    };
    std::process::exit(code);
}

fn execute(args: Vec<OsString>) -> Result<i32, String> {
    let Some(command) = args.first() else { print!("{HELP}"); return Ok(0); };
    if command == "--help" || command == "-h" { print!("{HELP}"); return Ok(0); }
    if command == "--version" { println!("nacre {}", env!("CARGO_PKG_VERSION")); return Ok(0); }
    let command = command.to_str().ok_or("command is not valid UTF-8")?;
    if command == "lsp" {
        if args.len() != 1 { return Err("usage: nacre lsp".to_owned()); }
        nacre::lsp::run()?;
        return Ok(0);
    }
    if matches!(command, "new" | "init") {
        if args.len() != 2 { return Err(format!("usage: nacre {command} new-project-directory")); }
        let path = PathBuf::from(&args[1]);
        project::init(&path)?;
        println!("Project created: {}", path.display());
        return Ok(0);
    }
    if command == "add" {
        let usage = "usage: nacre add <path> [--manifest-path <file>]";
        let mut dependency_path = None;
        let mut manifest_path = PathBuf::from("Nacre.toml");
        let mut manifest_path_seen = false;
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--manifest-path" && !manifest_path_seen {
                manifest_path = PathBuf::from(args.get(i + 1).ok_or(usage)?);
                manifest_path_seen = true;
                i += 2;
            } else if !args[i].to_string_lossy().starts_with('-') && dependency_path.is_none() {
                dependency_path = Some(PathBuf::from(&args[i]));
                i += 1;
            } else {
                return Err(usage.to_owned());
            }
        }
        let dependency_path = dependency_path.ok_or(usage)?;
        let name = nacre::manifest::add_dependency(&manifest_path, &dependency_path)?;
        println!("Added dependency {name}");
        return Ok(0);
    }
    if command == "doctor" {
        let usage = "usage: nacre doctor [--direct] [--zip]";
        let mut direct = false;
        let mut zip = false;
        for flag in &args[1..] {
            if flag == "--direct" && !direct {
                direct = true;
            } else if flag == "--zip" && !zip {
                zip = true;
            } else {
                return Err(usage.to_owned());
            }
        }
        return doctor(direct, zip);
    }
    if !matches!(command, "check" | "run" | "build" | "emit-c" | "emit-flow" | "emit-borrows" | "emit-types" | "emit-llvm" | "test" | "bench" | "fmt" | "doc") {
        return Err(format!("unknown command: {command}\n\n{HELP}"));
    }
    let mut sources = Vec::new();
    let mut output = None;
    let mut memory_stats = false;
    let mut check_only = false;
    let mut sanitize = false;
    let mut native_arch = false;
    let mut lto = false;
    let mut release = false;
    let mut jemalloc = false;
    let mut pgo_generate: Option<PathBuf> = None;
    let mut pgo_use: Option<PathBuf> = None;
    let mut filter: Option<String> = None;
    let mut direct_llvm = false;
    let mut optimization = None;
    let mut program_args = &args[args.len()..];
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--" && command == "run" {
            program_args = &args[i + 1..];
            break;
        } else if args[i] == "--" && matches!(command, "test" | "bench") {
            if let Some(f) = args.get(i + 1) {
                filter = Some(f.to_string_lossy().into_owned());
            }
            break;
        } else if args[i] == "--filter" && matches!(command, "test" | "bench") {
            let f = args.get(i + 1).ok_or("missing pattern after --filter")?;
            filter = Some(f.to_string_lossy().into_owned());
            i += 2;
        } else if args[i] == "-o" && matches!(command, "build" | "emit-c" | "emit-flow" | "emit-borrows" | "emit-types" | "emit-llvm" | "doc") && output.is_none() {
            let path = args.get(i + 1).ok_or("missing path after -o")?;
            output = Some(PathBuf::from(path));
            i += 2;
        } else if args[i] == "--memory-stats" && matches!(command, "run" | "build" | "emit-c" | "emit-llvm") && !memory_stats {
            memory_stats = true;
            i += 1;
        } else if matches!(command, "run" | "build" | "emit-llvm") && optimization.is_none()
            && args[i].to_str().is_some_and(|s| matches!(s, "-O0" | "-O1" | "-O2" | "-O3" | "-Os")) {
            optimization = args[i].to_str();
            i += 1;
        } else if args[i] == "--sanitize" && matches!(command, "run" | "build" | "test" | "bench") && !sanitize {
            sanitize = true; i += 1;
        } else if args[i] == "--native" && matches!(command, "run" | "build" | "test" | "bench") && !native_arch {
            native_arch = true; i += 1;
        } else if args[i] == "--lto" && matches!(command, "run" | "build" | "test" | "bench") && !lto {
            lto = true; i += 1;
        } else if args[i] == "--release" && matches!(command, "run" | "build") && !release {
            release = true;
            i += 1;
        } else if args[i] == "--jemalloc" && matches!(command, "run" | "build" | "test" | "bench") && !jemalloc {
            jemalloc = true; i += 1;
        } else if args[i].to_string_lossy().starts_with("--pgo-generate") && matches!(command, "run" | "build") && pgo_generate.is_none() {
            let path = if let Some(dir) = args[i].to_str().and_then(|s| s.strip_prefix("--pgo-generate=")) {
                PathBuf::from(dir)
            } else if args.get(i + 1).is_some_and(|next| !next.to_string_lossy().starts_with('-')) {
                i += 1;
                PathBuf::from(&args[i])
            } else {
                PathBuf::from(".pgo")
            };
            pgo_generate = Some(path);
            i += 1;
        } else if args[i].to_string_lossy().starts_with("--pgo-use") && matches!(command, "run" | "build") && pgo_use.is_none() {
            let path = if let Some(dir) = args[i].to_str().and_then(|s| s.strip_prefix("--pgo-use=")) {
                PathBuf::from(dir)
            } else if args.get(i + 1).is_some_and(|next| !next.to_string_lossy().starts_with('-')) {
                i += 1;
                PathBuf::from(&args[i])
            } else {
                PathBuf::from(".pgo")
            };
            pgo_use = Some(path);
            i += 1;
        } else if args[i] == "--check" && command == "fmt" && !check_only {
            check_only = true; i += 1;
        } else if args[i] == "--direct" && matches!(command, "emit-llvm" | "run" | "build") && !direct_llvm {
            direct_llvm = true; i += 1;
        } else if !args[i].to_string_lossy().starts_with('-') {
            sources.push(PathBuf::from(&args[i])); i += 1;
        } else { return Err(format!("invalid arguments for {command}\n\n{HELP}")); }
    }
    if sources.is_empty() && matches!(command, "check" | "run" | "build") && Path::new("Nacre.toml").is_file() {
        sources.push(PathBuf::from("."));
    }
    let source_path = sources.first().cloned().ok_or_else(|| format!("missing source file\n\n{HELP}"))?;
    let manifest_path = if sources.len() == 1 { manifest_path_for_source(&sources[0]) } else { None };
    let source_inputs = sources.clone();
    let sources = if command == "fmt" { project::resolve_inputs(&sources)? } else { sources };
    let manifest = if matches!(command, "run" | "build") {
        manifest_path.as_deref().map(nacre::manifest::Manifest::load).transpose()?
    } else {
        None
    };
    if command == "test" {
        return nacre::test_runner::run_tests(&sources, filter.as_deref(), native_arch, lto, jemalloc);
    }
    if command == "bench" {
        return nacre::bench_runner::run_benchmarks(&sources, filter.as_deref(), native_arch, lto, jemalloc);
    }
    if command == "fmt" {
        return nacre::fmt::format_files(&sources, check_only);
    }
    if command == "doc" {
        if let Some(output_path) = output.as_deref() {
            for source in &source_inputs { protect_source_and_manifests(source, output_path)?; }
            if let Some(manifest_path) = &manifest_path { protect_source(manifest_path, output_path)?; }
        }
        nacre::doc::generate_docs(&sources, output.as_deref())?;
        return Ok(0);
    }
    let bundle = SourceBundle::from_paths(&sources)?;
    if let Some(output_path) = output.as_deref() {
        for source in bundle.paths() { protect_source_and_manifests(source, output_path)?; }
        for source in &source_inputs { protect_source_and_manifests(source, output_path)?; }
        if let Some(manifest_path) = &manifest_path { protect_source(manifest_path, output_path)?; }
    }
    if command == "check" {
        bundle.validate().map_err(|d| bundle.render(&d))?;
        println!("OK: {} ({} source file(s))", source_path.display(), bundle.source_count());
        return Ok(0);
    }
    if matches!(command, "emit-flow" | "emit-borrows" | "emit-types") {
        let flow = (if command == "emit-flow" { bundle.emit_flow() } else if command == "emit-types" { bundle.emit_types() } else { bundle.emit_borrows() })
            .map_err(|d| bundle.render(&d))?;
        if let Some(path) = output {
            native::create_parent(&path)?;
            fs::write(&path, flow).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
            eprintln!("Analysis {}: {}", if command == "emit-types" { "types" } else { "ownership" }, path.display());
        } else { io::stdout().write_all(flow.as_bytes()).map_err(|e| e.to_string())?; }
        return Ok(0);
    }
    if command == "emit-llvm" && direct_llvm {
        if memory_stats {
            return Err("--memory-stats cannot be used with emit-llvm --direct".to_owned());
        }
        let llvm_ir = bundle.emit_llvm_direct().map_err(|d| bundle.render(&d))?;
        if let Some(path) = output {
            native::create_parent(&path)?;
            fs::write(&path, llvm_ir).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
            eprintln!("LLVM IR: {}", path.display());
        } else {
            io::stdout().write_all(llvm_ir.as_bytes()).map_err(|e| e.to_string())?;
        }
        return Ok(0);
    }
    if pgo_generate.is_some() && pgo_use.is_some() {
        return Err("cannot specify both --pgo-generate and --pgo-use".to_owned());
    }
    let direct_native = direct_llvm;
    let mut c_source = if direct_native {
        None
    } else {
        Some(bundle.compile_to_c().map_err(|d| bundle.render(&d))?)
    };
    if memory_stats
        && let Some(c_source) = &mut c_source {
            c_source.insert_str(0, "#define NACRE_MEMORY_STATS 1\n");
        }
    if command == "emit-c" || command == "emit-llvm" {
        let mut generated_source = c_source.take().ok_or("generated C source is unavailable")?;
        if command == "emit-llvm" {
            generated_source = native::emit_llvm(&generated_source, optimization.unwrap_or("-O2"))?;
        }
        if let Some(path) = output {
            native::create_parent(&path)?;
            fs::write(&path, generated_source).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
            eprintln!("{} generated: {}", if command == "emit-llvm" { "LLVM" } else { "C" }, path.display());
        } else {
            io::stdout().write_all(generated_source.as_bytes()).map_err(|e| e.to_string())?;
        }
        return Ok(0);
    }
    let profile = manifest.as_ref().map(|manifest| {
        if release { &manifest.profile.release } else { &manifest.profile.dev }
    });
    let profile_optimization = profile.map(|profile| format!("-O{}", profile.opt_level));
    if profile.is_some_and(|profile| profile.lto) { lto = true; }
    let mut flags = Vec::new();
    if direct_native && memory_stats { flags.push("-DNACRE_MEMORY_STATS"); }
    if sanitize {
        flags.extend(["-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-fno-omit-frame-pointer", "-g"]);
        if cfg!(target_os = "linux") { flags.extend(["-fno-pie", "-no-pie"]); }
    }
    if native_arch { flags.push("-march=native"); }
    if lto { flags.push("-flto"); }
    if jemalloc { flags.push("-ljemalloc"); }
    let pgo_flag_gen;
    let pgo_flag_use;
    if let Some(dir) = &pgo_generate {
        fs::create_dir_all(dir).map_err(|e| format!("failed to create PGO directory: {e}"))?;
        let abs = fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
        pgo_flag_gen = format!("-fprofile-generate={}", abs.display());
        flags.push(&pgo_flag_gen);
    }
    if let Some(dir) = &pgo_use {
        let abs = fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
        pgo_flag_use = format!("-fprofile-use={}", abs.display());
        flags.push(&pgo_flag_use);
        flags.push("-fprofile-correction");
    }
    let effective_optimization = optimization.or(profile_optimization.as_deref()).unwrap_or("-O2");
    let program = if direct_native {
        bundle.compile_llvm_direct_optimized_with_flags(effective_optimization, &flags)?
    } else {
        native::compile_c_optimized(
            c_source.as_deref().ok_or("generated C source is unavailable")?,
            effective_optimization,
            &flags,
        )?
    };
    if command == "run" { return Ok(program.run(program_args)?.code().unwrap_or(1)); }
    let path = output.unwrap_or_else(|| {
        let name = manifest.as_ref().and_then(|manifest| manifest.package.as_ref())
            .map(|package| std::ffi::OsString::from(&package.name))
            .unwrap_or_else(|| source_path.file_stem().unwrap_or_else(|| std::ffi::OsStr::new("program")).to_os_string());
        let path = Path::new("out").join(name);
        if cfg!(windows) { path.with_extension("exe") } else { path }
    });
    for source in bundle.paths() { protect_source_and_manifests(source, &path)?; }
    for source in &source_inputs { protect_source_and_manifests(source, &path)?; }
    if let Some(manifest_path) = &manifest_path { protect_source(manifest_path, &path)?; }
    program.save(&path)?;
    eprintln!("Native executable: {}", path.display());
    Ok(0)
}

fn manifest_path_for_source(source: &Path) -> Option<PathBuf> {
    if source.file_name() == Some(std::ffi::OsStr::new("Nacre.toml")) {
        return source.is_file().then(|| source.to_owned());
    }
    if source.is_dir() {
        let manifest = source.join("Nacre.toml");
        return manifest.is_file().then_some(manifest);
    }
    None
}

fn protect_source(source: &Path, output: &Path) -> Result<(), String> {
    if output.exists() {
        let source_real = source.canonicalize().map_err(|e| e.to_string())?;
        let output_real = output.canonicalize().map_err(|e| e.to_string())?;
        let mut same = source_real == output_real;
        #[cfg(unix)] {
            use std::os::unix::fs::MetadataExt;
            let a = fs::metadata(source).map_err(|e| e.to_string())?;
            let b = fs::metadata(output).map_err(|e| e.to_string())?;
            same |= a.dev() == b.dev() && a.ino() == b.ino();
        }
        if same { return Err("refusing to overwrite the source file with compiler output".to_owned()); }
    }
    Ok(())
}
fn protect_source_and_manifests(source: &Path, output: &Path) -> Result<(), String> {
    protect_source(source, output)?;
    let mut directory = if source.is_dir() { Some(source) } else { source.parent() };
    while let Some(path) = directory {
        let manifest = path.join("Nacre.toml");
        if manifest.is_file() {
            protect_source(&manifest, output)?;
        }
        directory = path.parent();
    }
    Ok(())
}


static NEXT_DOCTOR_TEMP: AtomicU64 = AtomicU64::new(0);

struct DoctorTempDir(PathBuf);

impl DoctorTempDir {
    fn new() -> Result<Self, String> {
        let base = env::temp_dir();
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
            .map_err(|error| format!("failed to create a doctor temporary directory: {error}"))?
            .as_nanos();
        for _ in 0..32 {
            let serial = NEXT_DOCTOR_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!("nacre-doctor-{}-{stamp}-{serial}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("failed to create doctor temporary directory {}: {error}", path.display())),
            }
        }
        Err("failed to reserve a unique doctor temporary directory".to_owned())
    }
}

impl Drop for DoctorTempDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

fn doctor(direct: bool, zip: bool) -> Result<i32, String> {
    if direct {
        let tools = native::toolchain::Toolchain::resolve()?;
        println!("Direct LLVM toolchain:");
        println!("  Clang {:?} ({})", tools.compiler(), tools.compiler_version().lines().next().unwrap_or_default().trim());
        println!("  llvm-link {:?} ({})", tools.linker(), tools.linker_version().lines().find(|line| line.contains("version")).unwrap_or(tools.linker_version()).trim());
        println!("  target {}", tools.target().trim());
        tools.ensure_compatible()?;
        println!("OK: matching LLVM major versions");
    }

    let source = "void main() { List<int> xs = new List<int>(); xs.add(42); Text text = new Text(\"test=\"); text.appendInt(xs.pop()); text.print(); }";
    let program = if direct {
        let program = nacre::compile_llvm_direct_optimized_with_flags(source, "-O2", &["-DNACRE_MEMORY_STATS"])?;
        println!("OK: frontend, types, ownership, and borrowing");
        program
    } else {
        let mut generated = compile_to_c(source).map_err(|d| d.render("<doctor>", source))?;
        println!("OK: frontend, types, ownership, and borrowing");
        generated.insert_str(0, "#define NACRE_MEMORY_STATS 1\n");
        native::compile_c(&generated)?
    };
    let result = program.output()?;
    let memory_report = String::from_utf8_lossy(&result.stderr);
    if !result.status.success() || result.stdout != b"test=42\n" || !memory_counters_balanced(&memory_report) {
        return Err(format!("native test failed: {:?}\n{}\n{}", result.status,
            String::from_utf8_lossy(&result.stdout), memory_report));
    }
    println!("OK: compilation and native execution");
    println!("OK: List/Text, allocations balanced, live=0");

    if zip { doctor_zip(direct)?; }
    Ok(0)
}

fn memory_counters_balanced(report: &str) -> bool {
    let mut fields = report.split_whitespace();
    fields.next() == Some("NACRE_MEMORY") && {
        let allocations = fields.next().and_then(|field| field.strip_prefix("allocations="));
        let frees = fields.next().and_then(|field| field.strip_prefix("frees="));
        allocations.is_some()
            && allocations == frees
            && fields.next() == Some("live=0")
            && fields.next().is_none()
    }
}

const ZIP_DOCTOR_SOURCE: &str = r#"
Result<Unit, Text> require(boolean condition, String message) {
    if (!condition) { return new Result<Unit, Text>.Err(new Text(message)); }
    return new Result<Unit, Text>.Ok(());
}
Result<Path, Text> argumentPath(int index) {
    Text text = Args.get(index)?;
    return Path.fromText(text);
}
Result<Unit, Text> zipExercise() {
    Path roundTrip = argumentPath(0)?;
    Path empty = argumentPath(1)?;

    ZipWriter emptyWriter = ZipWriter.create(empty, 0)?;
    emptyWriter.close()?;
    ZipReader emptyReader = ZipReader.open(empty)?;
    match (emptyReader.next()?) {
        None {}
        Some(entry) { return new Result<Unit, Text>.Err(new Text("empty archive contained an entry")); }
    }
    emptyReader.close()?;

    ZipWriter writer = ZipWriter.create(roundTrip, 0)?;
    ByteBuffer payload = ByteBuffer.withCapacity(0L)?;
    payload.append((byte)65);
    payload.append((byte)0);
    payload.append((byte)255);
    payload.append((byte)66);
    Text filename = new Text("doctor.bin");
    writer.add(filename, payload)?;
    writer.close()?;

    ZipReader reader = ZipReader.open(roundTrip)?;
    match (reader.next()?) {
        None { return new Result<Unit, Text>.Err(new Text("round-trip archive was empty")); }
        Some(entry) {
            borrow Text name = entry.name();
            require(name.equalsString("doctor.bin") && entry.size() == 4L && !entry.isDirectory(), "round-trip entry metadata")?;
            ByteBuffer bytes = reader.readEntry(4L)?;
            require(bytes.length() == 4L && bytes.get(0L) == (byte)65 && bytes.get(1L) == (byte)0
                && bytes.get(2L) == (byte)255 && bytes.get(3L) == (byte)66, "round-trip entry payload")?;
        }
    }
    match (reader.next()?) {
        None {}
        Some(entry) { return new Result<Unit, Text>.Err(new Text("round-trip archive contained an unexpected entry")); }
    }
    reader.close()?;
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (zipExercise()) {
        Ok(unit) { println("ZIP round-trip and empty archive OK"); }
        Err(error) { Console.printText(error); Console.print("\n"); }
    }
}
"#;

const ZIP_GUIDANCE: &str = r#"ZIP diagnostics require Nacre's corrected libarchive build. Run `python3 scripts/build_libarchive.py --prefix "$PWD/target/native-deps"`, then export `CPATH="$PWD/target/native-deps/include${CPATH:+:$CPATH}"`, `LIBRARY_PATH="$PWD/target/native-deps/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"`, and `LD_LIBRARY_PATH="$PWD/target/native-deps/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"`."#;

fn doctor_zip(direct: bool) -> Result<(), String> {
    let program = if direct {
        nacre::compile_llvm_direct_optimized_with_flags(
            ZIP_DOCTOR_SOURCE,
            "-O2",
            &["-DNACRE_MEMORY_STATS"],
        )
    } else {
        let mut generated = compile_to_c(ZIP_DOCTOR_SOURCE)
            .map_err(|diagnostic| diagnostic.render("<doctor ZIP>", ZIP_DOCTOR_SOURCE))?;
        generated.insert_str(0, "#define NACRE_MEMORY_STATS 1\n");
        native::compile_c(&generated)
    }
    .map_err(|error| format!("{error}\n{ZIP_GUIDANCE}"))?;

    let directory = DoctorTempDir::new()?;
    let output = Command::new(program.path())
        .args(["round-trip.zip", "empty.zip"])
        .current_dir(&directory.0)
        .output()
        .map_err(|error| format!("failed to run ZIP doctor program: {error}\n{ZIP_GUIDANCE}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        || output.stdout != b"ZIP round-trip and empty archive OK\n"
        || !memory_counters_balanced(&stderr)
    {
        return Err(format!(
            "ZIP native diagnostic failed: {:?}\n{}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            stderr,
            ZIP_GUIDANCE,
        ));
    }
    println!("OK: ZIP writer/reader round-trip, empty archive ownership, allocations balanced, live=0");
    Ok(())
}
