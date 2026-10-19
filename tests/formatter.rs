mod support;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEMP_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let serial = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nacre-formatter-{}-{serial}",
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

#[test]
fn formatter_preserves_shift_operators_and_nested_generic_closers() {
    let source = r#"
void main() {
    List<List<int>> nested = new List<List<int>>();
    int left = 1 << 3;
    int right = 16 >> 2;
    println(left);
    println(right);
}
"#;

    let formatted = nacre::fmt::format_source(source).expect("valid source formats");
    let program = support::compile(&formatted, &[]).expect("formatted source compiles");
    let output = program.output().expect("formatted program runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"8\n4\n");
}

#[test]
fn invalid_later_file_does_not_reformat_earlier_files() {
    let scratch = Scratch::new();
    let first = scratch.0.join("a.ncr");
    let invalid = scratch.0.join("z.ncr");
    let original_first = "void main(){println(1);}";
    let original_invalid = "void main(){";
    fs::write(&first, original_first).unwrap();
    fs::write(&invalid, original_invalid).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg("fmt")
        .arg(&scratch.0)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("formatter safety check failed"), "{stderr}");
    assert_eq!(fs::read_to_string(first).unwrap(), original_first);
    assert_eq!(fs::read_to_string(invalid).unwrap(), original_invalid);
}
