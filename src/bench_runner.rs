//! Built-in benchmark runner for Nacre (`nacre bench`).
//! Discovers parameterless void functions matching `bench_*` or `bench*` and measures throughput.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use crate::ir::Type;
use crate::native;
use crate::project::SourceBundle;

pub fn run_benchmarks(
    sources: &[PathBuf],
    filter: Option<&str>,
    native_arch: bool,
    lto: bool,
    jemalloc: bool,
    json_output: bool,
) -> Result<i32, String> {
    let bundle = SourceBundle::from_test_paths(sources)?;
    let typed = bundle.typed().map_err(|d| bundle.render(&d))?;

    // Discover benchmark functions: void functions with no parameters starting with bench
    let mut all_benches = Vec::new();
    for function in &typed.functions {
        if function.receiver.is_none()
            && function.params.is_empty()
            && function.return_type == Type::Void
            && (function.name.starts_with("bench_") || (function.name.starts_with("bench") && function.name != "bench"))
        {
            all_benches.push((function.name.clone(), function.id));
        }
    }

    let (bench_cases, filtered_out): (Vec<_>, Vec<_>) = all_benches
        .into_iter()
        .partition(|(name, _)| filter.is_none_or(|f| name.contains(f)));

    let filtered_count = filtered_out.len();
    if !json_output {
        println!("\nrunning {} benchmark(s)", bench_cases.len());
    }
    if bench_cases.is_empty() {
        if json_output {
            let payload = serde_json::json!({
                "type": "bench",
                "benchmarks": [],
                "passed": 0,
                "failed": 0,
                "filtered_out": filtered_count,
                "duration_seconds": 0.0
            });
            println!("{}", serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?);
        } else {
            println!("\ntest result: ok. 0 passed; 0 failed; {filtered_count} filtered out; finished in 0.00s\n");
        }
        return Ok(0);
    }

    // Compile bundle to C
    let mut c_source = bundle.compile_to_c().map_err(|d| bundle.render(&d))?;
    c_source.insert_str(0, "#define NACRE_MEMORY_STATS 1\n#include <time.h>\n");

    // Replace the default main() with our benchmark dispatcher main()
    let main_start = c_source.rfind("int main(int argc, char **argv) {").ok_or("missing main in generated C")?;
    c_source.truncate(main_start);

    for (_name, id) in &bench_cases {
        c_source.push_str(&format!(r#"
static void ncr_run_bench_{id}(const char *name) {{
    for (int w = 0; w < 3; ++w) {{
        ncr_fn_{id}();
    }}
    uint64_t iters = 1;
    struct timespec start, end;
    while (1) {{
        clock_gettime(CLOCK_MONOTONIC, &start);
        for (uint64_t i = 0; i < iters; ++i) {{
            ncr_fn_{id}();
        }}
        clock_gettime(CLOCK_MONOTONIC, &end);
        uint64_t elapsed_ns = (end.tv_sec - start.tv_sec) * 1000000000ULL + (end.tv_nsec - start.tv_nsec);
        if (elapsed_ns >= 20000000ULL || iters >= 100000000ULL) {{
            break;
        }}
        iters *= 10;
    }}
    uint64_t samples[5];
    for (int b = 0; b < 5; ++b) {{
        clock_gettime(CLOCK_MONOTONIC, &start);
        for (uint64_t i = 0; i < iters; ++i) {{
            ncr_fn_{id}();
        }}
        clock_gettime(CLOCK_MONOTONIC, &end);
        uint64_t total_ns = (end.tv_sec - start.tv_sec) * 1000000000ULL + (end.tv_nsec - start.tv_nsec);
        samples[b] = total_ns / iters;
    }}
    uint64_t sum = 0;
    for (int b = 0; b < 5; ++b) sum += samples[b];
    uint64_t mean_ns = sum / 5;
    uint64_t max_diff = 0;
    for (int b = 0; b < 5; ++b) {{
        uint64_t diff = samples[b] > mean_ns ? samples[b] - mean_ns : mean_ns - samples[b];
        if (diff > max_diff) max_diff = diff;
    }}
    uint64_t ops_sec = mean_ns > 0 ? 1000000000ULL / mean_ns : 0;
    printf("BENCH_RESULT: %s %llu %llu %llu\n", name, (unsigned long long)mean_ns, (unsigned long long)max_diff, (unsigned long long)ops_sec);
}}
"#));
    }

    c_source.push_str("int main(int argc, char **argv) {\n");
    c_source.push_str("    ncr_argc = argc;\n    ncr_argv = argv;\n");
    c_source.push_str("    if (argc > 1) {\n");
    c_source.push_str("        const char *t = argv[1];\n");

    for (name, id) in &bench_cases {
        c_source.push_str(&format!(
            "        if (strcmp(t, \"{name}\") == 0) {{ ncr_run_bench_{id}(\"{name}\"); return 0; }}\n"
        ));
    }
    c_source.push_str("        fprintf(stderr, \"unknown benchmark: %s\\n\", t);\n        return 2;\n");
    c_source.push_str("    }\n    return 0;\n}\n");

    let mut flags = vec!["-DNACRE_MEMORY_STATS"];
    if native_arch { flags.push("-march=native"); }
    if lto { flags.push("-flto"); }
    if jemalloc { flags.push("-ljemalloc"); }

    let program = native::compile_c_optimized(&c_source, "-O3", &flags)?;

    let mut passed = 0;
    let mut failed = 0;
    let mut bench_results = Vec::new();
    let suite_start = Instant::now();

    for (name, _) in &bench_cases {
        if !json_output {
            print!("test {name} ... ");
        }
        let output = Command::new(program.path()).arg(name).output()
            .map_err(|e| format!("failed to execute benchmark process: {e}"))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut reported = false;
            for line in stdout.lines() {
                if let Some(rest) = line.strip_prefix("BENCH_RESULT: ") {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 4 {
                        let ns: u64 = parts[1].parse().unwrap_or(0);
                        let diff: u64 = parts[2].parse().unwrap_or(0);
                        let ops: u64 = parts[3].parse().unwrap_or(0);
                        if json_output {
                            bench_results.push(serde_json::json!({
                                "name": name,
                                "ns_per_iter": ns,
                                "max_diff_ns": diff,
                                "ops_per_sec": ops,
                                "status": "ok"
                            }));
                        } else {
                            let formatted_ops = format_with_commas(parts[3]);
                            println!("bench: {:>8} ns/iter (+/- {:>4}) ({} ops/sec)", ns, diff, formatted_ops);
                        }
                        reported = true;
                        break;
                    }
                }
            }
            if !reported {
                if json_output {
                    bench_results.push(serde_json::json!({ "name": name, "status": "ok" }));
                } else {
                    println!("ok");
                }
            }
            passed += 1;
        } else {
            if json_output {
                bench_results.push(serde_json::json!({ "name": name, "status": "failed" }));
            } else {
                println!("FAILED");
                let stderr = String::from_utf8_lossy(&output.stderr);
                for line in stderr.lines() {
                    eprintln!("    {line}");
                }
            }
            failed += 1;
        }
    }

    let suite_duration = suite_start.elapsed();
    if json_output {
        let payload = serde_json::json!({
            "type": "bench",
            "benchmarks": bench_results,
            "passed": passed,
            "failed": failed,
            "filtered_out": filtered_count,
            "duration_seconds": suite_duration.as_secs_f64()
        });
        println!("{}", serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?);
    } else {
        let verdict = if failed == 0 { "ok" } else { "FAILED" };
        println!(
            "\ntest result: {verdict}. {passed} passed; {failed} failed; {filtered_count} filtered out; finished in {:.2?}\n",
            suite_duration
        );
    }

    if failed > 0 {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn format_with_commas(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    for (i, c) in chars.into_iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}
