//! Native tests for atomic filesystem replacement.
mod support;

use nacre::native;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nacre-atomic-{}-{serial}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn compile(source: &str) -> native::NativeProgram {
    support::compile(source, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("{error}"))
}

fn assert_live_zero(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "{stderr}"
    );
}

fn run(program: &native::NativeProgram, args: &[&Path]) -> Output {
    let output = Command::new(program.path()).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_live_zero(&output);
    output
}

fn directory_entries(path: &Path) -> Vec<OsString> {
    let mut entries: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    entries.sort();
    entries
}

#[test]
fn atomic_write_preserves_exact_bytes_and_replaces_repeatedly() {
    let scratch = Scratch::new();
    let target = scratch.0.join("board.dat");
    fs::write(&target, b"previous destination").unwrap();
    let source = r#"
void main() {
    match (Args.get(0)) {
        Ok(path) {
            Text first = new Text("A\0é🙂");
            match (Files.writeTextAtomic(path, first)) {
                Ok(unit) { println(unit); }
                Err(error) { println("WRITE_ERROR"); }
            }
            match (Files.readText(path)) {
                Ok(actual) { println(actual.equals(first)); }
                Err(error) { println("READ_ERROR"); }
            }
            Text second = new Text("B\0世界");
            match (Files.writeTextAtomic(path, second)) {
                Ok(unit) { println(unit); }
                Err(error) { println("WRITE_ERROR"); }
            }
        }
        Err(error) { println("ARG_ERROR"); }
    }
}
"#;
    let program = compile(source);
    let output = run(&program, &[target.as_path()]);
    assert_eq!(output.stdout, b"()\ntrue\n()\n");
    assert_eq!(fs::read(&target).unwrap(), "B\0世界".as_bytes());
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o077, 0);
    }
    assert_eq!(directory_entries(&scratch.0), vec![OsString::from("board.dat")]);
}

#[test]
fn invalid_atomic_paths_do_not_touch_the_destination() {
    let scratch = Scratch::new();
    let target = scratch.0.join("board.dat");
    fs::write(&target, b"unchanged").unwrap();
    let source = r#"
void report(borrow Text path, borrow Text content) {
    match (Files.writeTextAtomic(path, content)) {
        Ok(success) { println("OK"); }
        Err(error) { println("ERR"); }
    }
}
void main() {
    match (Args.get(0)) {
        Ok(path) {
            Text empty = new Text("");
            Text nul_path = path.clone();
            nul_path.append("\0suffix");
            Text content = new Text("replacement");
            report(empty, content);
            report(nul_path, content);
        }
        Err(error) { println("ARG_ERROR"); }
    }
}
"#;
    let program = compile(source);
    let output = run(&program, &[target.as_path()]);
    assert_eq!(output.stdout, b"ERR\nERR\n");
    assert_eq!(fs::read(&target).unwrap(), b"unchanged");
    assert_eq!(directory_entries(&scratch.0), vec![OsString::from("board.dat")]);
}

#[cfg(target_os = "linux")]
#[test]
fn temporary_creation_failure_preserves_the_existing_proc_file() {
    let source = r#"
void main() {
    Text path = new Text("/proc/self/comm");
    match (Files.readText(path)) {
        Ok(before) {
            Text content = new Text("replacement");
            match (Files.writeTextAtomic(path, content)) {
                Ok(success) { println("UNEXPECTED_OK"); }
                Err(error) { println("ERR"); }
            }
            match (Files.readText(path)) {
                Ok(after) { println(before.equals(after)); }
                Err(error) { println("READ_ERROR"); }
            }
        }
        Err(error) { println("READ_ERROR"); }
    }
}
"#;
    let program = compile(source);
    let output = run(&program, &[]);
    assert_eq!(output.stdout, b"ERR\ntrue\n");
}

#[test]
fn rename_failure_preserves_destination_and_removes_temporary_file() {
    let scratch = Scratch::new();
    let target = scratch.0.join("destination");
    fs::create_dir(&target).unwrap();
    let marker = target.join("marker");
    fs::write(&marker, b"directory remains").unwrap();
    let source = r#"
void main() {
    match (Args.get(0)) {
        Ok(path) {
            Text content = new Text("replacement");
            match (Files.writeTextAtomic(path, content)) {
                Ok(success) { println("UNEXPECTED_OK"); }
                Err(error) { println("ERR"); }
            }
        }
        Err(error) { println("ARG_ERROR"); }
    }
}
"#;
    let program = compile(source);
    let output = run(&program, &[target.as_path()]);
    assert_eq!(output.stdout, b"ERR\n");
    assert_eq!(fs::read(&marker).unwrap(), b"directory remains");
    assert_eq!(directory_entries(&scratch.0), vec![OsString::from("destination")]);
    assert_eq!(directory_entries(&target), vec![OsString::from("marker")]);
}

#[cfg(target_os = "linux")]
#[test]
fn write_failure_preserves_destination_and_removes_temporary_file() {
    let scratch = Scratch::new();
    let target = scratch.0.join("limited.dat");
    fs::write(&target, b"old contents").unwrap();
    let payload = "x".repeat(4096);
    let source = format!(
        r#"
void main() {{
    match (Args.get(0)) {{
        Ok(path) {{
            Text content = new Text("{payload}");
            match (Files.writeTextAtomic(path, content)) {{
                Ok(success) {{ println("UNEXPECTED_OK"); }}
                Err(error) {{ println("ERR"); }}
            }}
        }}
        Err(error) {{ println("ARG_ERROR"); }}
    }}
}}
"#
    );
    let program = compile(&source);
    let output = Command::new("sh")
        .arg("-c")
        .arg("trap '' XFSZ; ulimit -f 1 || exit 77; exec \"$1\" \"$2\"")
        .arg("atomic-files-test")
        .arg(program.path())
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"ERR\n");
    assert_live_zero(&output);
    assert_eq!(fs::read(&target).unwrap(), b"old contents");
    assert_eq!(directory_entries(&scratch.0), vec![OsString::from("limited.dat")]);
}

#[cfg(unix)]
#[test]
fn invalid_utf8_argument_cannot_be_used_as_a_filesystem_path() {
    use std::os::unix::ffi::OsStringExt;

    let bad_path = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff]);
    let source = r#"
void main() {
    match (Args.get(0)) {
        Ok(path) { println("UNEXPECTED_OK"); }
        Err(error) { println("ERR"); }
    }
}
"#;
    let program = compile(source);
    let output = run(&program, &[Path::new(&bad_path)]);
    assert_eq!(output.stdout, b"ERR\n");
}
