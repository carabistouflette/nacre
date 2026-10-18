#![cfg(unix)]

//! Persistent C-cache regressions requiring a Unix C compiler.
use nacre::native;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = "#include <stdio.h>\n#include \"cache_value.h\"\nint main(void) { printf(\"%d\\n\", NACRE_CACHE_VALUE); return 0; }\n";

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nacre-c-cache-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf { self.0.join(name) }
}

impl Drop for TestDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

struct CacheProbe<'a> {
    executable: &'a Path,
    compiler: &'a std::ffi::OsStr,
    wrapper: &'a Path,
    cache: &'a Path,
    include: &'a Path,
    compile_log: &'a Path,
}

impl CacheProbe<'_> {
    fn run(&self, version: &str, expected: &str) {
        let output = Command::new(self.executable)
            .args(["--exact", "c_cache_probe_child", "--nocapture"])
            .env("NACRE_CACHE_PROBE_CHILD", "1")
            .env("NACRE_CACHE_EXPECTED", expected)
            .env("NACRE_CACHE_INCLUDE", self.include)
            .env("NACRE_CACHE_DIR", self.cache)
            .env("NACRE_ENABLE_TEST_CACHE", "1")
            .env("NACRE_CC", self.wrapper)
            .env("NACRE_FIXTURE_REAL_CC", self.compiler)
            .env("NACRE_FIXTURE_VERSION", version)
            .env("NACRE_FIXTURE_COMPILE_LOG", self.compile_log)
            .env_remove("NACRE_NO_CACHE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cache probe failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    fn run_concurrently(&self, barrier: &Path) {
        let output = Command::new(self.executable)
            .args(["--exact", "c_cache_concurrent_probe_child", "--nocapture"])
            .env("NACRE_CACHE_PROBE_CONCURRENT_CHILD", "1")
            .env("NACRE_CACHE_EXPECTED", "20\n")
            .env("NACRE_CACHE_INCLUDE", self.include)
            .env("NACRE_CACHE_DIR", self.cache)
            .env("NACRE_ENABLE_TEST_CACHE", "1")
            .env("NACRE_CC", self.wrapper)
            .env("NACRE_FIXTURE_REAL_CC", self.compiler)
            .env("NACRE_FIXTURE_VERSION", "v3")
            .env("NACRE_FIXTURE_COMPILE_LOG", self.compile_log)
            .env("NACRE_FIXTURE_BARRIER_DIR", barrier)
            .env("NACRE_FIXTURE_BARRIER_TARGET", "8")
            .env_remove("NACRE_NO_CACHE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "concurrent cache probe failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[test]
fn c_cache_probe_child() {
    if std::env::var_os("NACRE_CACHE_PROBE_CHILD").is_none() { return; }
    let include = std::env::var_os("NACRE_CACHE_INCLUDE").unwrap();
    let include = PathBuf::from(include);
    let include = include.to_str().unwrap();
    let program = native::compile_c_optimized(SOURCE, "-O0", &["-I", include]).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected = std::env::var("NACRE_CACHE_EXPECTED").unwrap();
    assert_eq!(output.stdout, expected.as_bytes());
}

#[test]
fn c_cache_concurrent_probe_child() {
    if std::env::var_os("NACRE_CACHE_PROBE_CONCURRENT_CHILD").is_none() { return; }
    let include = PathBuf::from(std::env::var_os("NACRE_CACHE_INCLUDE").unwrap());
    let include = include.to_str().unwrap().to_owned();
    let barrier = Arc::new(Barrier::new(8));
    let mut threads = Vec::with_capacity(8);
    for _ in 0..8 {
        let barrier = Arc::clone(&barrier);
        let include = include.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            let program = native::compile_c_optimized(SOURCE, "-O0", &["-I", &include]).unwrap();
            let output = program.output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let expected = std::env::var("NACRE_CACHE_EXPECTED").unwrap();
            assert_eq!(output.stdout, expected.as_bytes());
        }));
    }
    for thread in threads { thread.join().unwrap(); }
    let cache = PathBuf::from(std::env::var_os("NACRE_CACHE_DIR").unwrap());
    let staging_files = fs::read_dir(cache).unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
        .count();
    assert_eq!(staging_files, 0, "cache writes must clean up staging files");
}

#[test]
fn c_cache_invalidates_for_header_and_compiler_changes() {
    let temp = TestDir::new();
    let include = temp.path("include headers");
    fs::create_dir(&include).unwrap();
    let header = include.join("cache_value.h");
    fs::write(&header, "#define NACRE_CACHE_VALUE 10\n").unwrap();
    let wrapper = temp.path("compiler-wrapper");
    fs::write(
        &wrapper,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf 'fixture compiler %s\n' "$NACRE_FIXTURE_VERSION"; exit 0; fi
if [ "$1" = "-dumpmachine" ]; then printf '%s\n' fixture-target; exit 0; fi
for argument do
    if [ "$argument" = "-o" ]; then
        printf x >> "$NACRE_FIXTURE_COMPILE_LOG"
        if [ -n "${NACRE_FIXTURE_BARRIER_TARGET:-}" ]; then
            : > "$NACRE_FIXTURE_BARRIER_DIR/$$"
            attempts=0
            while [ "$attempts" -lt 500 ]; do
                count=0
                for marker in "$NACRE_FIXTURE_BARRIER_DIR"/*; do
                    [ -e "$marker" ] && count=$((count + 1))
                done
                [ "$count" -ge "$NACRE_FIXTURE_BARRIER_TARGET" ] && break
                sleep 0.01
                attempts=$((attempts + 1))
            done
        fi
        break
    fi
done
exec "$NACRE_FIXTURE_REAL_CC" "$@"
"#,
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();

    let compiler = native::find_c_compiler().unwrap();
    let cache = temp.path("cache");
    let compile_log = temp.path("actual-compiles");
    let executable = std::env::current_exe().unwrap();

    let probe = CacheProbe {
        executable: &executable,
        compiler: &compiler,
        wrapper: &wrapper,
        cache: &cache,
        include: &include,
        compile_log: &compile_log,
    };
    probe.run("v1", "10\n");
    assert_eq!(fs::read(&compile_log).unwrap().len(), 1);

    fs::write(&header, "#define NACRE_CACHE_VALUE 20\n").unwrap();
    probe.run("v1", "20\n");
    assert_eq!(fs::read(&compile_log).unwrap().len(), 2, "header content must invalidate the executable");

    probe.run("v2", "20\n");
    assert_eq!(fs::read(&compile_log).unwrap().len(), 3, "compiler version must invalidate the executable");

    probe.run("v2", "20\n");
    assert_eq!(fs::read(&compile_log).unwrap().len(), 3, "unchanged inputs should reuse the executable");
    let concurrent_cache = temp.path("concurrent-cache");
    let barrier = temp.path("compiler-barrier");
    fs::create_dir(&barrier).unwrap();
    let concurrent_log = temp.path("concurrent-compiles");
    let concurrent_probe = CacheProbe {
        executable: &executable,
        compiler: &compiler,
        wrapper: &wrapper,
        cache: &concurrent_cache,
        include: &include,
        compile_log: &concurrent_log,
    };
    concurrent_probe.run_concurrently(&barrier);
    assert_eq!(fs::read(&concurrent_log).unwrap().len(), 8);
    concurrent_probe.run("v3", "20\n");
    assert_eq!(fs::read(&concurrent_log).unwrap().len(), 8, "concurrent cache writes must leave a reusable executable");
}
