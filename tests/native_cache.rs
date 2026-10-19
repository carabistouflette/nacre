#![cfg(unix)]

//! Persistent C-cache regressions requiring a Unix C compiler.
use nacre::native;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = "#include <stdio.h>\n#include \"cache_value.h\"\n#ifndef NACRE_CACHE_OVERRIDE\n#define NACRE_CACHE_OVERRIDE 0\n#endif\nint main(void) { printf(\"%d\\n\", NACRE_CACHE_VALUE + NACRE_CACHE_OVERRIDE); return 0; }\n";
#[cfg(target_os = "linux")]
const LIBRARY_SOURCE: &str = r#"
#include <stdio.h>
extern int nacre_cache_fixture_value(void);
int main(void) { printf("%d\n", nacre_cache_fixture_value()); return 0; }
"#;

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

fn make_compiler_wrapper(temp: &TestDir) -> PathBuf {
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
if [ -n "${MACOSX_DEPLOYMENT_TARGET:-}" ]; then
    set -- "$@" "-DNACRE_CACHE_OVERRIDE=$MACOSX_DEPLOYMENT_TARGET"
fi
exec "$NACRE_FIXTURE_REAL_CC" "$@"
"#,
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    wrapper
}

#[cfg(target_os = "linux")]
fn create_archive_fixture(compiler: &std::ffi::OsStr, directory: &Path, value: i32) {
    let source = directory.join("fixture.c");
    let object = directory.join("fixture.o");
    let archive = directory.join("libnacre_cache_fixture.a");
    fs::write(&source, format!("int nacre_cache_fixture_value(void) {{ return {value}; }}\n")).unwrap();
    let compiled = Command::new(compiler)
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "fixture compiler failed:\n{}", String::from_utf8_lossy(&compiled.stderr));
    let archived = Command::new("ar")
        .arg("rcs")
        .arg(&archive)
        .arg(&object)
        .output()
        .unwrap();
    assert!(archived.status.success(), "fixture archive creation failed:\n{}", String::from_utf8_lossy(&archived.stderr));
}

#[cfg(target_os = "linux")]
fn create_shared_library_fixture(compiler: &std::ffi::OsStr, directory: &Path, value: i32) {
    let source = directory.join("fixture.c");
    let library = directory.join("libnacre_runpath_fixture.so");
    fs::write(&source, format!("int nacre_cache_fixture_value(void) {{ return {value}; }}\n")).unwrap();
    let compiled = Command::new(compiler)
        .args(["-shared", "-fPIC"])
        .arg(&source)
        .arg("-Wl,-soname,libnacre_runpath_fixture.so")
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "fixture shared-library compiler failed:\n{}", String::from_utf8_lossy(&compiled.stderr));
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
        self.run_with_deployment_target(version, expected, None);
    }

    fn run_with_deployment_target(
        &self,
        version: &str,
        expected: &str,
        target: Option<&str>,
    ) {
        let mut command = Command::new(self.executable);
        command.args(["--exact", "c_cache_probe_child", "--nocapture"])
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
            .env_remove("MACOSX_DEPLOYMENT_TARGET");
        if let Some(target) = target {
            command.env("MACOSX_DEPLOYMENT_TARGET", target);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "cache probe failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[cfg(target_os = "linux")]
    fn run_with_linker_search_path(&self, library_path: &Path, expected: &str) {
        let output = Command::new(self.executable)
            .args(["--exact", "c_cache_linker_search_path_probe_child", "--nocapture"])
            .env("NACRE_CACHE_LINKER_SEARCH_PATH_PROBE_CHILD", "1")
            .env("NACRE_CACHE_LINKER_SEARCH_PATH", library_path)
            .env("NACRE_CACHE_EXPECTED", expected)
            .env("NACRE_CACHE_INCLUDE", self.include)
            .env("NACRE_CACHE_DIR", self.cache)
            .env("NACRE_ENABLE_TEST_CACHE", "1")
            .env("NACRE_CC", self.wrapper)
            .env("NACRE_FIXTURE_REAL_CC", self.compiler)
            .env("NACRE_FIXTURE_VERSION", "linker-search-path-v1")
            .env("NACRE_FIXTURE_COMPILE_LOG", self.compile_log)
            .env_remove("LIBRARY_PATH")
            .env_remove("MACOSX_DEPLOYMENT_TARGET")
            .env_remove("NACRE_NO_CACHE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "linker-search-path cache probe failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[cfg(target_os = "linux")]
    fn run_with_linker_runpath(&self, library_path: &Path, runpath: &Path, expected: &str) {
        let output = Command::new(self.executable)
            .args(["--exact", "c_cache_ld_run_path_probe_child", "--nocapture"])
            .env("NACRE_CACHE_LD_RUN_PATH_PROBE_CHILD", "1")
            .env("NACRE_CACHE_EXPECTED", expected)
            .env("NACRE_CACHE_INCLUDE", self.include)
            .env("NACRE_CACHE_DIR", self.cache)
            .env("NACRE_ENABLE_TEST_CACHE", "1")
            .env("NACRE_CC", self.wrapper)
            .env("NACRE_FIXTURE_REAL_CC", self.compiler)
            .env("NACRE_FIXTURE_VERSION", "ld-run-path-v1")
            .env("NACRE_FIXTURE_COMPILE_LOG", self.compile_log)
            .env("LIBRARY_PATH", library_path)
            .env("LD_RUN_PATH", runpath)
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("MACOSX_DEPLOYMENT_TARGET")
            .env_remove("NACRE_NO_CACHE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "LD_RUN_PATH cache probe failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[cfg(target_os = "linux")]
    fn run_with_library_path_presence(&self, library_dir: &Path, empty: bool, expect_link_failure: bool) {
        let mut command = Command::new(self.executable);
        command.args(["--exact", "c_cache_library_path_presence_probe_child", "--nocapture"])
            .env("NACRE_CACHE_LIBRARY_PATH_PRESENCE_PROBE_CHILD", "1")
            .env("NACRE_CACHE_EXPECTED", "10\n")
            .env("NACRE_CACHE_INCLUDE", self.include)
            .env("NACRE_CACHE_DIR", self.cache)
            .env("NACRE_ENABLE_TEST_CACHE", "1")
            .env("NACRE_CC", self.wrapper)
            .env("NACRE_FIXTURE_REAL_CC", self.compiler)
            .env("NACRE_FIXTURE_VERSION", "library-path-presence-v1")
            .env("NACRE_FIXTURE_COMPILE_LOG", self.compile_log)
            .current_dir(library_dir)
            .env_remove("LD_RUN_PATH")
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("MACOSX_DEPLOYMENT_TARGET")
            .env_remove("NACRE_NO_CACHE");
        if expect_link_failure {
            command.env("NACRE_CACHE_EXPECT_LINK_FAILURE", "1");
        } else {
            command.env_remove("NACRE_CACHE_EXPECT_LINK_FAILURE");
        }

        if empty {
            command.env("LIBRARY_PATH", "");
        } else {
            command.env_remove("LIBRARY_PATH");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "library-path presence probe failed\nstdout: {}\nstderr: {}",
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
            .env_remove("MACOSX_DEPLOYMENT_TARGET")
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

#[cfg(target_os = "linux")]
#[test]
fn c_cache_linker_search_path_probe_child() {
    if std::env::var_os("NACRE_CACHE_LINKER_SEARCH_PATH_PROBE_CHILD").is_none() {
        return;
    }
    let search_path = std::env::var_os("NACRE_CACHE_LINKER_SEARCH_PATH").unwrap();
    let search_path = PathBuf::from(search_path);
    let search_path = search_path.to_str().unwrap();
    let program = native::compile_c_optimized(
        LIBRARY_SOURCE,
        "-O0",
        &[
            "-L",
            search_path,
            "-Wl,--whole-archive",
            "-lnacre_cache_fixture",
            "-Wl,--no-whole-archive",
        ],
    )
    .unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected = std::env::var("NACRE_CACHE_EXPECTED").unwrap();
    assert_eq!(output.stdout, expected.as_bytes());
}

#[cfg(target_os = "linux")]
#[test]
fn c_cache_ld_run_path_probe_child() {
    if std::env::var_os("NACRE_CACHE_LD_RUN_PATH_PROBE_CHILD").is_none() { return; }
    let program = native::compile_c_optimized(
        LIBRARY_SOURCE,
        "-O0",
        &["-Wl,--no-as-needed", "-lnacre_runpath_fixture"],
    ).unwrap();
    let output = Command::new(program.path())
        .env_remove("LIBRARY_PATH")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("LD_RUN_PATH")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected = std::env::var("NACRE_CACHE_EXPECTED").unwrap();
    assert_eq!(output.stdout, expected.as_bytes());
}

#[cfg(target_os = "linux")]
#[test]
fn c_cache_library_path_presence_probe_child() {
    if std::env::var_os("NACRE_CACHE_LIBRARY_PATH_PRESENCE_PROBE_CHILD").is_none() { return; }
    let program = native::compile_c_optimized(
        LIBRARY_SOURCE,
        "-O0",
        &["-Wl,--whole-archive", "-lnacre_cache_fixture", "-Wl,--no-whole-archive"],
    );
    if std::env::var_os("NACRE_CACHE_EXPECT_LINK_FAILURE").is_some() {
        assert!(program.is_err(), "unset LIBRARY_PATH must not reuse the cwd-linked executable");
        return;
    }
    let program = program.unwrap();
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
    let wrapper = make_compiler_wrapper(&temp);

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
    probe.run_with_deployment_target("v2", "21\n", Some("1"));
    assert_eq!(fs::read(&compile_log).unwrap().len(), 4, "compiler environment must invalidate the executable");

    probe.run_with_deployment_target("v2", "22\n", Some("2"));
    assert_eq!(fs::read(&compile_log).unwrap().len(), 5, "changed compiler environment must invalidate the executable");

    probe.run_with_deployment_target("v2", "22\n", Some("2"));
    assert_eq!(fs::read(&compile_log).unwrap().len(), 5, "unchanged compiler environment should reuse the executable");
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

#[cfg(target_os = "linux")]
#[test]
fn c_cache_relinks_when_external_library_changes() {
    let temp = TestDir::new();
    let first_library = temp.path("first library");
    let second_library = temp.path("second library");
    fs::create_dir(&first_library).unwrap();
    fs::create_dir(&second_library).unwrap();
    let compiler = native::find_c_compiler().unwrap();
    create_archive_fixture(&compiler, &first_library, 10);
    create_archive_fixture(&compiler, &second_library, 20);
    let include = temp.path("include headers");
    fs::create_dir(&include).unwrap();
    let wrapper = make_compiler_wrapper(&temp);
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

    probe.run_with_linker_search_path(&first_library, "10\n");
    assert_eq!(fs::read(&compile_log).unwrap().len(), 1);
    create_archive_fixture(&compiler, &first_library, 11);
    probe.run_with_linker_search_path(&first_library, "11\n");
    assert_eq!(
        fs::read(&compile_log).unwrap().len(),
        2,
        "changed contents at the same library path must relink the executable",
    );

    probe.run_with_linker_search_path(&second_library, "20\n");
    assert_eq!(
        fs::read(&compile_log).unwrap().len(),
        3,
        "a changed linker search path must relink the executable",
    );
    probe.run_with_linker_search_path(&second_library, "20\n");
    assert_eq!(
        fs::read(&compile_log).unwrap().len(),
        3,
        "an unchanged linker search path should reuse the executable",
    );
}

#[cfg(target_os = "linux")]
#[test]
fn c_cache_bypasses_linker_scripts_with_untracked_inputs() {
    let temp = TestDir::new();
    let library_dir = temp.path("script library");
    fs::create_dir(&library_dir).unwrap();
    let compiler = native::find_c_compiler().unwrap();
    create_archive_fixture(&compiler, &library_dir, 10);
    let archive = library_dir.join("libnacre_cache_fixture_real.a");
    fs::rename(library_dir.join("libnacre_cache_fixture.a"), &archive).unwrap();
    fs::write(
        library_dir.join("libnacre_cache_fixture.so"),
        format!("INPUT ( \"{}\" )\n", archive.display()),
    )
    .unwrap();

    let include = temp.path("include headers");
    fs::create_dir(&include).unwrap();
    let wrapper = make_compiler_wrapper(&temp);
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

    probe.run_with_linker_search_path(&library_dir, "10\n");
    assert_eq!(fs::read(&compile_log).unwrap().len(), 1);
    create_archive_fixture(&compiler, &library_dir, 11);
    fs::rename(library_dir.join("libnacre_cache_fixture.a"), &archive).unwrap();
    probe.run_with_linker_search_path(&library_dir, "11\n");
    assert_eq!(
        fs::read(&compile_log).unwrap().len(),
        2,
        "linker scripts can reference library inputs omitted from dependency scanning",
    );
}

#[cfg(target_os = "linux")]
#[test]
fn c_cache_relinks_external_library_runpath_changes() {
    let temp = TestDir::new();
    let first_library = temp.path("first runpath");
    let second_library = temp.path("second runpath");
    fs::create_dir(&first_library).unwrap();
    fs::create_dir(&second_library).unwrap();
    let compiler = native::find_c_compiler().unwrap();
    create_shared_library_fixture(&compiler, &first_library, 10);
    create_shared_library_fixture(&compiler, &second_library, 20);
    let include = temp.path("include headers");
    fs::create_dir(&include).unwrap();
    let wrapper = make_compiler_wrapper(&temp);
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

    probe.run_with_linker_runpath(&first_library, &first_library, "10\n");
    assert_eq!(fs::read(&compile_log).unwrap().len(), 1);
    probe.run_with_linker_runpath(&first_library, &second_library, "20\n");
    assert_eq!(
        fs::read(&compile_log).unwrap().len(),
        2,
        "a changed linker runpath must relink the executable",
    );
    probe.run_with_linker_runpath(&first_library, &second_library, "20\n");
    assert_eq!(
        fs::read(&compile_log).unwrap().len(),
        2,
        "an unchanged linker runpath should reuse the executable",
    );
}

#[cfg(target_os = "linux")]
#[test]
fn c_cache_distinguishes_empty_and_unset_library_path() {
    let temp = TestDir::new();
    let library_dir = temp.path("working library");
    fs::create_dir(&library_dir).unwrap();
    let compiler = native::find_c_compiler().unwrap();
    create_archive_fixture(&compiler, &library_dir, 10);
    let include = temp.path("include headers");
    fs::create_dir(&include).unwrap();
    let wrapper = make_compiler_wrapper(&temp);
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

    probe.run_with_library_path_presence(&library_dir, true, false);
    assert_eq!(fs::read(&compile_log).unwrap().len(), 1);
    probe.run_with_library_path_presence(&library_dir, false, true);
    assert_eq!(fs::read(&compile_log).unwrap().len(), 2);
    probe.run_with_library_path_presence(&library_dir, true, false);
    assert_eq!(
        fs::read(&compile_log).unwrap().len(),
        2,
        "an unchanged empty library path should reuse the executable",
    );
}

#[test]
fn c_cache_config_probe_child() {
    if std::env::var_os("NACRE_CACHE_CONFIG_PROBE_CHILD").is_none() { return; }
    let config = std::env::var("NACRE_CACHE_CONFIG_FILE").unwrap();
    let config_flag = format!("--config={config}");
    let program = native::compile_c_optimized(
        "#ifndef VALUE\n#define VALUE 0\n#endif\nint main(void) { return VALUE; }\n",
        "-O0",
        &[&config_flag],
    ).unwrap();
    let output = program.output().unwrap();
    let expected = std::env::var("NACRE_CACHE_EXPECTED_EXIT").unwrap().parse::<i32>().unwrap();
    assert_eq!(output.status.code(), Some(expected));
}

#[test]
fn c_cache_invalidates_for_changed_clang_config_file() {
    if !Command::new("clang")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return;
    }
    let temp = TestDir::new();
    let config = temp.path("clang.cfg");
    let cache = temp.path("cache");
    let executable = std::env::current_exe().unwrap();
    let run_probe = |expected: &str| {
        let output = Command::new(&executable)
            .args(["--exact", "c_cache_config_probe_child", "--nocapture"])
            .env("NACRE_CACHE_CONFIG_PROBE_CHILD", "1")
            .env("NACRE_CACHE_CONFIG_FILE", &config)
            .env("NACRE_CACHE_EXPECTED_EXIT", expected)
            .env("NACRE_CACHE_DIR", &cache)
            .env("NACRE_ENABLE_TEST_CACHE", "1")
            .env("NACRE_CC", "clang")
            .env_remove("CLANG_CONFIG_FILE")
            .env_remove("NACRE_NO_CACHE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Clang config cache probe failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    };

    fs::write(&config, "-DVALUE=3\n").unwrap();
    run_probe("3");
    fs::write(&config, "-DVALUE=4\n").unwrap();
    run_probe("4");
}

#[test]
fn c_cache_empty_path_probe_child() {
    if std::env::var_os("NACRE_CACHE_EMPTY_PATH_PROBE_CHILD").is_none() { return; }
    let program = native::compile_c_optimized("int main(void) { return 0; }\n", "-O0", &[]).unwrap();
    let output = program.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn c_cache_ignores_empty_location_environment_variables() {
    let temp = TestDir::new();
    let executable = std::env::current_exe().unwrap();
    let run_probe = |label: &str,
                     cache_override: Option<&str>,
                     xdg_value: &str,
                     home_value: Option<&str>,
                     expected_cache: Option<&Path>| {
        let home = temp.path(&format!("{label}-home"));
        let working = temp.path(&format!("{label}-work"));
        fs::create_dir(&home).unwrap();
        fs::create_dir(&working).unwrap();

        let mut command = Command::new(&executable);
        command.args(["--exact", "c_cache_empty_path_probe_child", "--nocapture"])
            .env("NACRE_CACHE_EMPTY_PATH_PROBE_CHILD", "1")
            .env("NACRE_ENABLE_TEST_CACHE", "1")
            .env("HOME", &home)
            .env("XDG_CACHE_HOME", xdg_value)
            .env_remove("NACRE_NO_CACHE")
            .current_dir(&working);
        if let Some(home_value) = home_value {
            command.env("HOME", home_value);
        }
        if let Some(cache_override) = cache_override {
            command.env("NACRE_CACHE_DIR", cache_override);
        } else {
            command.env_remove("NACRE_CACHE_DIR");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "empty-cache-path probe failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert_eq!(
            fs::read_dir(&working).unwrap().count(),
            0,
            "empty cache settings must not write cache files under the working directory",
        );
        if let Some(expected_cache) = expected_cache {
            assert!(
                expected_cache.is_dir(),
                "expected cache at {}",
                expected_cache.display(),
            );
        }
    };

    let xdg_home = temp.path("empty-xdg-home");
    run_probe(
        "empty-xdg",
        None,
        "",
        None,
        Some(&xdg_home.join(".cache/nacre")),
    );

    let override_home = temp.path("empty-override-home");
    let xdg_home = override_home.join("xdg");
    run_probe(
        "empty-override",
        Some(""),
        xdg_home.to_str().unwrap(),
        None,
        Some(&xdg_home.join("nacre")),
    );

    run_probe("empty-home", None, "", Some(""), None);

}
