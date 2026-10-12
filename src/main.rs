#![forbid(unsafe_code)]
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use nacre::{compile_to_c, native};
use nacre::project::{self, SourceBundle};

const HELP: &str = "Nacre — experimental native compiler written in Rust

Utilisation:
  nacre check  source.ncr ... | dossier
  nacre run    fichier.ncr [--memory-stats] [-O0|-O1|-O2|-O3|-Os] [-- arguments...]
  nacre build  fichier.ncr [-o executable] [--memory-stats] [-O0|-O1|-O2|-O3|-Os]
  nacre emit-c fichier.ncr [-o fichier.c] [--memory-stats]
  nacre emit-flow fichier.ncr [-o fichier.flow]
  nacre emit-borrows fichier.ncr [-o fichier.borrows]
  nacre emit-types fichier.ncr [-o fichier.types]
  nacre emit-llvm fichier.ncr [-o fichier.ll] [--memory-stats] [-O0|-O1|-O2|-O3|-Os]
  nacre init nouveau-dossier
  nacre doctor
  nacre lsp
  nacre --help
  nacre --version

Exemples avec Cargo:
  cargo run -- check examples/hello.ncr
  cargo run -- run examples/counter.ncr
  cargo run -- build examples/counter.ncr -o out/counter
  cargo run -- emit-c examples/counter.ncr -o out/counter.c

Plusieurs fichiers ou un dossier .ncr sont acceptes par toutes les commandes.
Un espace de noms partage; pas encore de packages/imports ou compilation separee.
--sanitize (run/build): AddressSanitizer + UndefinedBehaviorSanitizer.
init: cree un projet neuf; refuse un chemin existant.
doctor: compile et execute un test de bout en bout avec List et Text.
build: sortie par defaut dans out/<nom-du-fichier>.
emit-c / emit-flow / emit-borrows / emit-types / emit-llvm: sortie standard si -o est absent.
emit-llvm: LLVM textuel via le frontend C de Clang (NACRE_CLANG), pas un backend direct.
Optimisation native et LLVM: -O2 par defaut.
emit-flow: graphe d'ownership, sans compilateur C.
--memory-stats: compteurs allocation/liberation sur stderr du programme cible.
CC ou NACRE_CC: chemin d'un executable C11 (cc, clang, gcc), sans options.
Les fichiers de sortie existants sont remplaces, sauf le fichier source.
Cibles initiales: Linux et WSL. macOS non teste.
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
    let command = command.to_str().ok_or("commande non UTF-8")?;
    if command == "lsp" {
        if args.len() != 1 { return Err("utilisation: nacre lsp".to_owned()); }
        nacre::lsp::run()?;
        return Ok(0);
    }
    if command == "init" {
        if args.len() != 2 { return Err("utilisation: nacre init nouveau-dossier".to_owned()); }
        let path = PathBuf::from(&args[1]);
        project::init(&path)?;
        println!("Projet cree: {}", path.display());
        return Ok(0);
    }
    if command == "doctor" {
        if args.len() != 1 { return Err("utilisation: nacre doctor".to_owned()); }
        return doctor();
    }
    if !matches!(command, "check" | "run" | "build" | "emit-c" | "emit-flow" | "emit-borrows" | "emit-types" | "emit-llvm") {
        return Err(format!("commande inconnue: {command}\n\n{HELP}"));
    }
    let mut sources = Vec::new();
    let mut output = None;
    let mut memory_stats = false;
    let mut sanitize = false;
    let mut optimization = None;
    let mut program_args = &args[args.len()..];
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--" && command == "run" {
            program_args = &args[i + 1..];
            break;
        } else if args[i] == "-o" && matches!(command, "build" | "emit-c" | "emit-flow" | "emit-borrows" | "emit-types" | "emit-llvm") && output.is_none() {
            let path = args.get(i + 1).ok_or("chemin manquant apres -o")?;
            output = Some(PathBuf::from(path));
            i += 2;
        } else if args[i] == "--memory-stats" && matches!(command, "run" | "build" | "emit-c" | "emit-llvm") && !memory_stats {
            memory_stats = true;
            i += 1;
        } else if matches!(command, "run" | "build" | "emit-llvm") && optimization.is_none()
            && args[i].to_str().is_some_and(|s| matches!(s, "-O0" | "-O1" | "-O2" | "-O3" | "-Os")) {
            optimization = args[i].to_str();
            i += 1;
        } else if args[i] == "--sanitize" && matches!(command, "run" | "build") && !sanitize {
            sanitize = true; i += 1;
        } else if !args[i].to_string_lossy().starts_with('-') {
            sources.push(PathBuf::from(&args[i])); i += 1;
        } else { return Err(format!("arguments invalides pour {command}\n\n{HELP}")); }
    }
    let source_path = sources.first().ok_or_else(|| format!("fichier source manquant\n\n{HELP}"))?;
    let bundle = SourceBundle::from_paths(&sources)?;
    if command == "check" {
        bundle.validate().map_err(|d| bundle.render(&d))?;
        println!("OK: {} ({} fichier(s))", source_path.display(), bundle.source_count());
        return Ok(0);
    }
    if matches!(command, "emit-flow" | "emit-borrows" | "emit-types") {
        let flow = (if command == "emit-flow" { bundle.emit_flow() } else if command == "emit-types" { bundle.emit_types() } else { bundle.emit_borrows() })
            .map_err(|d| bundle.render(&d))?;
        if let Some(path) = output {
            for source in bundle.paths() { protect_source(source, &path)?; }
            native::create_parent(&path)?;
            fs::write(&path, flow).map_err(|e| format!("impossible d'ecrire {}: {e}", path.display()))?;
            eprintln!("Analyse {}: {}", if command == "emit-types" { "des types" } else { "memoire" }, path.display());
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
            fs::write(&path, c_source).map_err(|e| format!("impossible d'ecrire {}: {e}", path.display()))?;
            eprintln!("{} genere: {}", if command == "emit-llvm" { "LLVM" } else { "C" }, path.display());
        } else { io::stdout().write_all(c_source.as_bytes()).map_err(|e| e.to_string())?; }
        return Ok(0);
    }
    let mut flags = Vec::new();
    if sanitize {
        flags.extend(["-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-fno-omit-frame-pointer", "-g"]);
        if cfg!(target_os = "linux") { flags.extend(["-fno-pie", "-no-pie"]); }
    }
    let program = native::compile_c_optimized(&c_source, optimization.unwrap_or("-O2"), &flags)?;
    if command == "run" { return Ok(program.run(program_args)?.code().unwrap_or(1)); }
    let path = output.unwrap_or_else(|| {
        let name = source_path.file_stem().unwrap_or_else(|| std::ffi::OsStr::new("program"));
        let path = Path::new("out").join(name);
        if cfg!(windows) { path.with_extension("exe") } else { path }
    });
    for source in bundle.paths() { protect_source(source, &path)?; }
    program.save(&path)?;
    eprintln!("Executable natif: {}", path.display());
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
        if same { return Err("refus d'ecraser le fichier source avec la sortie du compilateur".to_owned()); }
    }
    Ok(())
}

fn doctor() -> Result<i32, String> {
    let source = "void main() { List<int> xs = new List<int>(); xs.add(42); Text text = new Text(\"test=\"); text.appendInt(xs.pop()); text.print(); }";
    let mut generated = compile_to_c(source).map_err(|d| d.render("<doctor>", source))?;
    println!("OK: frontend, types, ownership et emprunts");
    generated.insert_str(0, "#define NACRE_MEMORY_STATS 1\n");
    let program = native::compile_c(&generated)?;
    let result = program.output()?;
    if !result.status.success() || result.stdout != b"test=42\n" || result.stderr != b"NACRE_MEMORY allocations=4 frees=4 live=0\n" {
        return Err(format!("echec du test natif: {:?}\n{}\n{}", result.status,
            String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr)));
    }
    println!("OK: compilation C11 et execution native");
    println!("OK: List/Text, 4 allocations, 4 liberations, live=0");
    Ok(0)
}
