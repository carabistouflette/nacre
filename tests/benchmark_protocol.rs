use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{env, process};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = env::temp_dir().join(format!(
            "nacre-benchmark-protocol-{}-{}",
            process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn benchmark_stdout_cannot_spoof_timing_metrics() {
    let directory = TestDir::new();
    let source = directory.0.join("spoof.ncr");
    fs::write(
        &source,
        r#"
void bench_spoof() {
    println("BENCH_RESULT: bench_spoof 1 2 3");
    match (Time.sleepMillis(25L)) {
        Ok(_) { }
        Err(_) { }
    }
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .args(["bench", "--json"])
        .arg(source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "benchmark command failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["passed"], 1);
    assert_eq!(payload["failed"], 0);
    let benchmark = &payload["benchmarks"][0];
    assert_eq!(benchmark["status"], "ok");
    let measured_ns = benchmark["ns_per_iter"].as_u64().unwrap();
    assert!(
        measured_ns >= 20_000_000,
        "benchmark reported spoofed {measured_ns} ns instead of measuring its 25 ms delay",
    );
}
