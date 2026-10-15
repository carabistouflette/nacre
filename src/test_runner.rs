//! Built-in test runner for Nacre (`nacre test`).
//! Discovers parameterless void functions matching `test_*` or `test*` and runs them in isolation.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use crate::ir::Type;
use crate::native;
use crate::project::SourceBundle;

pub fn run_tests(
    sources: &[PathBuf],
    native_arch: bool,
    lto: bool,
    jemalloc: bool,
) -> Result<i32, String> {
    let mut bundle_sources = Vec::new();
    let mut explicit_main = false;

    for path in sources {
        if path.is_file() {
            let content = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            if content.contains("void main()") {
                explicit_main = true;
            }
            bundle_sources.push((path.display().to_string(), content));
        } else if path.is_dir() {
            let bundle = SourceBundle::from_paths(&[path.clone()])?;
            for file in bundle.paths() {
                let content = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
                if content.contains("void main()") {
                    explicit_main = true;
                }
                bundle_sources.push((file.display().to_string(), content));
            }
        }
    }

    if bundle_sources.is_empty() {
        return Err("no source files found".to_owned());
    }

    // If no main() is defined in the test sources, append a dummy main to pass validation
    if !explicit_main {
        bundle_sources.push(("__ncr_synthetic_main.ncr".to_owned(), "void main() {}\n".to_owned()));
    }

    let bundle = SourceBundle::from_sources(bundle_sources)?;
    let typed = bundle.typed().map_err(|d| bundle.render(&d))?;

    // Discover test functions: void functions with no parameters starting with test
    let mut test_cases = Vec::new();
    for function in &typed.functions {
        if function.receiver.is_none()
            && function.params.is_empty()
            && function.return_type == Type::Void
            && (function.name.starts_with("test_") || (function.name.starts_with("test") && function.name != "test"))
        {
            test_cases.push((function.name.clone(), function.id));
        }
    }

    println!("\nrunning {} test(s)", test_cases.len());
    if test_cases.is_empty() {
        println!("\ntest result: ok. 0 passed; 0 failed; finished in 0.00s\n");
        return Ok(0);
    }

    // Compile bundle to C
    let mut c_source = bundle.compile_to_c().map_err(|d| bundle.render(&d))?;
    c_source.insert_str(0, "#define NACRE_MEMORY_STATS 1\n");

    // Replace the default main() with our test dispatcher main()
    let main_start = c_source.rfind("int main(int argc, char **argv) {").ok_or("missing main in generated C")?;
    c_source.truncate(main_start);

    c_source.push_str("int main(int argc, char **argv) {\n");
    c_source.push_str("    ncr_argc = argc;\n    ncr_argv = argv;\n");
    c_source.push_str("    if (argc > 1) {\n");
    c_source.push_str("        const char *t = argv[1];\n");

    for (name, id) in &test_cases {
        c_source.push_str(&format!(
            "        if (strcmp(t, \"{name}\") == 0) {{ ncr_fn_{id}(); ncr_memory_report(); return 0; }}\n"
        ));
    }
    c_source.push_str("        fprintf(stderr, \"unknown test: %s\\n\", t);\n        return 2;\n");
    c_source.push_str("    }\n    return 0;\n}\n");

    // Compile test runner binary
    let mut flags = vec!["-DNACRE_MEMORY_STATS"];
    if native_arch { flags.push("-march=native"); }
    if lto { flags.push("-flto"); }
    if jemalloc { flags.push("-ljemalloc"); }

    let program = native::compile_c_optimized(&c_source, "-O2", &flags)?;

    let mut passed = 0;
    let mut failed = 0;
    let suite_start = Instant::now();

    for (name, _) in &test_cases {
        print!("test {name} ... ");
        let test_start = Instant::now();
        let output = Command::new(program.path()).arg(name).output()
            .map_err(|e| format!("failed to execute test process: {e}"))?;
        let test_duration = test_start.elapsed();

        if output.status.success() {
            println!("ok ({:.2?})", test_duration);
            passed += 1;
        } else {
            println!("FAILED");
            failed += 1;
            let stderr = String::from_utf8_lossy(&output.stderr);
            for line in stderr.lines() {
                if !line.starts_with("NACRE_MEMORY") {
                    eprintln!("    {line}");
                }
            }
        }
    }

    let suite_duration = suite_start.elapsed();
    let verdict = if failed == 0 { "ok" } else { "FAILED" };
    println!("\ntest result: {verdict}. {passed} passed; {failed} failed; finished in {:.2?}\n", suite_duration);

    if failed > 0 {
        Ok(1)
    } else {
        Ok(0)
    }
}
