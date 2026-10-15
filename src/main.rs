#![forbid(unsafe_code)]
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use nacre::{compile_to_c, native};
use nacre::project::{self, SourceBundle};

const HELP: &str = "Nacre — experimental native compiler written in Rust

Usage:
  nacre check  source.ncr ... | directory
  nacre test   source.ncr ... | directory [--native] [--lto] [--jemalloc]
  nacre fmt    source.ncr ... | directory [--check]
  nacre run    source.ncr [--memory-stats] [--native] [--lto] [--jemalloc] [-O0|-O1|-O2|-O3|-Os] [-- arguments...]
  nacre build  source.ncr [-o executable] [--memory-stats] [--native] [--lto] [--jemalloc] [-O0|-O1|-O2|-O3|-Os]
  nacre emit-c source.ncr [-o output.c] [--memory-stats]
  nacre emit-flow source.ncr [-o output.flow]
  nacre emit-borrows source.ncr [-o output.borrows]
  nacre emit-types source.ncr [-o output.types]
  nacre emit-llvm source.ncr [-o output.ll] [--memory-stats] [-O0|-O1|-O2|-O3|-Os]
  nacre init new-project-directory
  nacre doctor
  nacre lsp
  nacre --help
  nacre --version

Examples with Cargo:
  cargo run -- check examples/language/hello_world.ncr
  cargo run -- run examples/language/counter.ncr
  cargo run -- build examples/language/counter.ncr -o out/counter
  cargo run -- emit-c examples/language/counter.ncr -o out/counter.c

Multiple files or a .ncr directory are accepted by all commands.
Source files may declare modules and import public names; all requested inputs are compiled together.
--sanitize (run/build): AddressSanitizer + UndefinedBehaviorSanitizer.
--native (run/build): enable host CPU native instructions (-march=native).
--lto (run/build): enable link-time optimization (-flto).
--jemalloc (run/build): link high-performance jemalloc allocator (-ljemalloc).
init: create a new project; refuse an existing path.
doctor: compile and run an end-to-end test with List and Text.
build: default output is out/<source-name>.
emit-c / emit-flow / emit-borrows / emit-types / emit-llvm: write to stdout when -o is omitted.
emit-llvm: textual LLVM via Clang's C frontend (NACRE_CLANG), not a direct backend.
Native and LLVM optimization: -O2 by default.
emit-flow: ownership graph without a C compiler.
--memory-stats: allocation/deallocation counters on the target program's stderr.
CC or NACRE_CC: path to a standalone C11 executable (cc, clang, gcc), without options.
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
    if command == "init" {
        if args.len() != 2 { return Err("usage: nacre init new-project-directory".to_owned()); }
        let path = PathBuf::from(&args[1]);
        project::init(&path)?;
        println!("Project created: {}", path.display());
        return Ok(0);
    }
    if command == "doctor" {
        if args.len() != 1 { return Err("usage: nacre doctor".to_owned()); }
        return doctor();
    }
    if !matches!(command, "check" | "run" | "build" | "emit-c" | "emit-flow" | "emit-borrows" | "emit-types" | "emit-llvm" | "test" | "fmt") {
        return Err(format!("unknown command: {command}\n\n{HELP}"));
    }
    let mut sources = Vec::new();
    let mut output = None;
    let mut memory_stats = false;
    let mut check_only = false;
    let mut sanitize = false;
    let mut native_arch = false;
    let mut lto = false;
    let mut jemalloc = false;
    let mut optimization = None;
    let mut program_args = &args[args.len()..];
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--" && command == "run" {
            program_args = &args[i + 1..];
            break;
        } else if args[i] == "-o" && matches!(command, "build" | "emit-c" | "emit-flow" | "emit-borrows" | "emit-types" | "emit-llvm") && output.is_none() {
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
        } else if args[i] == "--sanitize" && matches!(command, "run" | "build" | "test") && !sanitize {
            sanitize = true; i += 1;
        } else if args[i] == "--native" && matches!(command, "run" | "build" | "test") && !native_arch {
            native_arch = true; i += 1;
        } else if args[i] == "--lto" && matches!(command, "run" | "build" | "test") && !lto {
            lto = true; i += 1;
        } else if args[i] == "--jemalloc" && matches!(command, "run" | "build" | "test") && !jemalloc {
            jemalloc = true; i += 1;
        } else if args[i] == "--check" && command == "fmt" && !check_only {
            check_only = true; i += 1;
        } else if !args[i].to_string_lossy().starts_with('-') {
            sources.push(PathBuf::from(&args[i])); i += 1;
        } else { return Err(format!("invalid arguments for {command}\n\n{HELP}")); }
    }
    let source_path = sources.first().ok_or_else(|| format!("missing source file\n\n{HELP}"))?;
    if command == "test" {
        return nacre::test_runner::run_tests(&sources, native_arch, lto, jemalloc);
    }
    if command == "fmt" {
        return nacre::fmt::format_files(&sources, check_only);
    }
    let bundle = SourceBundle::from_paths(&sources)?;
    if command == "check" {
        bundle.validate().map_err(|d| bundle.render(&d))?;
        println!("OK: {} ({} source file(s))", source_path.display(), bundle.source_count());
        return Ok(0);
    }
    if matches!(command, "emit-flow" | "emit-borrows" | "emit-types") {
        let flow = (if command == "emit-flow" { bundle.emit_flow() } else if command == "emit-types" { bundle.emit_types() } else { bundle.emit_borrows() })
            .map_err(|d| bundle.render(&d))?;
        if let Some(path) = output {
            for source in bundle.paths() { protect_source(source, &path)?; }
            native::create_parent(&path)?;
            fs::write(&path, flow).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
            eprintln!("Analysis {}: {}", if command == "emit-types" { "types" } else { "ownership" }, path.display());
        } else { io::stdout().write_all(flow.as_bytes()).map_err(|e| e.to_string())?; }
        return Ok(0);
    }
    let mut c_source = bundle.compile_to_c().map_err(|d| bundle.render(&d))?;
    if memory_stats { c_source.insert_str(0, "#define NACRE_MEMORY_STATS 1\n"); }
    if command == "emit-c" || command == "emit-llvm" {
        if command == "emit-llvm" { c_source = native::emit_llvm(&c_source, optimization.unwrap_or("-O2"))?; }
        if let Some(path) = output {
            for source in bundle.paths() { protect_source(source, &path)?; }
            native::create_parent(&path)?;
            fs::write(&path, c_source).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
            eprintln!("{} generated: {}", if command == "emit-llvm" { "LLVM" } else { "C" }, path.display());
        } else { io::stdout().write_all(c_source.as_bytes()).map_err(|e| e.to_string())?; }
        return Ok(0);
    }
    let mut flags = Vec::new();
    if sanitize {
        flags.extend(["-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-fno-omit-frame-pointer", "-g"]);
        if cfg!(target_os = "linux") { flags.extend(["-fno-pie", "-no-pie"]); }
    }
    if native_arch { flags.push("-march=native"); }
    if lto { flags.push("-flto"); }
    if jemalloc { flags.push("-ljemalloc"); }
    let program = native::compile_c_optimized(&c_source, optimization.unwrap_or("-O2"), &flags)?;
    if command == "run" { return Ok(program.run(program_args)?.code().unwrap_or(1)); }
    let path = output.unwrap_or_else(|| {
        let name = source_path.file_stem().unwrap_or_else(|| std::ffi::OsStr::new("program"));
        let path = Path::new("out").join(name);
        if cfg!(windows) { path.with_extension("exe") } else { path }
    });
    for source in bundle.paths() { protect_source(source, &path)?; }
    program.save(&path)?;
    eprintln!("Native executable: {}", path.display());
    Ok(0)
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

fn doctor() -> Result<i32, String> {
    let source = "void main() { List<int> xs = new List<int>(); xs.add(42); Text text = new Text(\"test=\"); text.appendInt(xs.pop()); text.print(); }";
    let mut generated = compile_to_c(source).map_err(|d| d.render("<doctor>", source))?;
    println!("OK: frontend, types, ownership, and borrowing");
    generated.insert_str(0, "#define NACRE_MEMORY_STATS 1\n");
    let program = native::compile_c(&generated)?;
    let result = program.output()?;
    let memory_report = String::from_utf8_lossy(&result.stderr);
    let mut fields = memory_report.split_whitespace();
    let memory_balanced = fields.next() == Some("NACRE_MEMORY") && {
        let allocations = fields.next().and_then(|field| field.strip_prefix("allocations="));
        let frees = fields.next().and_then(|field| field.strip_prefix("frees="));
        allocations.is_some()
            && allocations == frees
            && fields.next() == Some("live=0")
            && fields.next().is_none()
    };
    if !result.status.success() || result.stdout != b"test=42\n" || !memory_balanced {
        return Err(format!("native test failed: {:?}\n{}\n{}", result.status,
            String::from_utf8_lossy(&result.stdout), memory_report));
    }
    println!("OK: compilation and native execution");
    println!("OK: List/Text, allocations balanced, live=0");
    Ok(0)
}
