mod support;
use std::ffi::OsString;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_PATH: AtomicUsize = AtomicUsize::new(0);

fn execute(source: &str, input: &[u8], args: &[OsString]) -> Output {
    let program = support::compile(source, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let mut child = Command::new(program.path())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_live_zero(&output);
    output
}

fn assert_live_zero(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")), "{stderr}");
}

fn assert_stdout(output: &Output, expected: &[u8]) {
    assert_eq!(output.stdout, expected);
}

#[test]
fn console_print_methods_preserve_nul_and_add_no_newline() {
    let source = r#"
void main() {
    Console.print("A\0");
    Text value = new Text("B\0C");
    Console.printText(value);
}
"#;
    assert_stdout(&execute(source, b"", &[]), b"A\0B\0C");
}

fn temp_path(label: &str) -> std::path::PathBuf {
    let unique = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("nacre-v6-{label}-{}-{unique}", std::process::id()))
}

#[test]
fn console_read_line_newline_crlf_partial_eof_and_prompt() {
    let source = r#"
void main() {
    Console.print("prompt:");
    match (Console.readLine()) {
        Ok(option) { match (option) { Some(line) { line.print(); } None { println("EOF"); } } }
        Err(error) { error.print(); }
    }
    match (Console.readLine()) {
        Ok(option) { match (option) { Some(line) { line.print(); } None { println("EOF"); } } }
        Err(error) { error.print(); }
    }
    match (Console.readLine()) {
        Ok(option) { match (option) { Some(line) { line.print(); } None { println("EOF"); } } }
        Err(error) { error.print(); }
    }
    match (Console.readLine()) {
        Ok(option) { match (option) { Some(line) { line.print(); } None { println("EOF"); } } }
        Err(error) { error.print(); }
    }
    match (Console.readLine()) {
        Ok(option) { match (option) { Some(line) { line.print(); } None { println("EOF"); } } }
        Err(error) { error.print(); }
    }
    match (Console.readLine()) {
        Ok(option) { match (option) { Some(line) { line.print(); } None { println("EOF"); } } }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, b"\nfirst\r\n0123456789abcdef\nthis line is longer than sixteen bytes\nlast", &[]);
    assert_stdout(&output, b"prompt:\nfirst\n0123456789abcdef\nthis line is longer than sixteen bytes\nlast\nEOF\n");
}

#[test]
fn console_read_line_immediate_eof_is_none() {
    let source = r#"
void main() {
    match (Console.readLine()) {
        Ok(option) { match (option) { Some(line) { line.print(); } None { println("EOF"); } } }
        Err(error) { error.print(); }
    }
}
"#;
    assert_stdout(&execute(source, b"", &[]), b"EOF\n");
}

#[test]
fn console_read_line_rejects_invalid_utf8_without_leaking() {
    let source = r#"
void main() {
    match (Console.readLine()) {
        Ok(option) { match (option) { Some(line) { line.print(); } None { println("EOF"); } } }
        Err(error) { error.print(); }
    }
    match (Console.readLine()) {
        Ok(option) { match (option) { Some(line) { line.print(); } None { println("EOF"); } } }
        Err(error) { error.print(); }
    }
}
"#;
    assert_stdout(&execute(source, &[0xff, b'\n', b'o', b'k', b'\n'], &[]), b"Console.readLine: invalid UTF-8\nok\n");
}

#[test]
fn text_parse_int_is_strict_and_checks_int32_boundaries() {
    let source = r#"
void check(borrow Text value) {
    match (value.parseInt()) {
        Ok(number) { println(number); }
        Err(error) { println("ERR"); }
    }
}
void main() {
    Text max = new Text("2147483647"); check(max);
    Text min = new Text("-2147483648"); check(min);
    Text plus = new Text("+2147483647"); check(plus);
    Text zero = new Text("-0"); check(zero);
    Text too_large = new Text("2147483648"); check(too_large);
    Text too_small = new Text("-2147483649"); check(too_small);
    Text empty = new Text(""); check(empty);
    Text sign = new Text("+"); check(sign);
    Text leading_space = new Text(" 1"); check(leading_space);
    Text trailing_space = new Text("1 "); check(trailing_space);
    Text junk = new Text("12x"); check(junk);
    Text non_ascii = new Text("١"); check(non_ascii);
}
"#;
    assert_stdout(
        &execute(source, b"", &[]),
        b"2147483647\n-2147483648\n2147483647\n0\nERR\nERR\nERR\nERR\nERR\nERR\nERR\nERR\n",
    );
}

#[test]
fn text_slice_bytes_respects_multibyte_utf8_boundaries() {
    let source = r#"
void show(borrow Text value, int start, int end) {
    match (value.sliceBytes(start, end)) {
        Ok(part) { Console.printText(part); Console.print("\n"); }
        Err(error) { println("ERR"); }
    }
}
void main() {
    Text value = new Text("Aé🧭Z");
    show(value, 1, 3);
    show(value, 3, 7);
    show(value, 0, 8);
    show(value, 2, 3);
    show(value, 4, 7);
    show(value, -1, 1);
    show(value, 2, 1);
    show(value, 0, 9);
}
"#;
    assert_stdout(
        &execute(source, b"", &[]),
        "é\n🧭\nAé🧭Z\nERR\nERR\nERR\nERR\nERR\n".as_bytes(),
    );
}

#[test]
fn files_round_trip_embedded_nul_and_utf8() {
    let path = temp_path("roundtrip");
    let args = [path.as_os_str().to_os_string()];
    let source = r#"
void main() {
    match (Args.get(0)) {
        Ok(path) {
            Text content = new Text("A\0Bé");
            match (Files.writeText(path, content)) {
                Ok(unit) { println(unit); }
                Err(error) { println("WRITE_ERROR"); }
            }
            match (Files.readText(path)) {
                Ok(value) { Console.printText(value); Console.print("\n"); }
                Err(error) { println("READ_ERROR"); }
            }
        }
        Err(error) { println("ARG_ERROR"); }
    }
}
"#;
    let output = execute(source, b"", &args);
    assert_stdout(&output, "()\nA\0Bé\n".as_bytes());
    assert_eq!(std::fs::read(&path).unwrap(), "A\0Bé".as_bytes());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn files_missing_unwritable_and_nul_paths_are_recoverable() {
    let missing = temp_path("missing");
    let directory = std::env::temp_dir();
    let args = [missing.as_os_str().to_os_string(), directory.as_os_str().to_os_string()];
    let source = r#"
void readStatus(borrow Text path) {
    match (Files.readText(path)) {
        Ok(value) { println("OK"); }
        Err(error) { println("ERR"); }
    }
}
void writeStatus(borrow Text path) {
    Text content = new Text("content");
    match (Files.writeText(path, content)) {
        Ok(success) { println("OK"); }
        Err(error) { println("ERR"); }
    }
}
void main() {
    match (Args.get(0)) { Ok(path) { readStatus(path); } Err(error) { println("ARG_ERROR"); } }
    match (Args.get(1)) { Ok(path) { writeStatus(path); } Err(error) { println("ARG_ERROR"); } }
    Text nul_path = new Text("bad\0path");
    readStatus(nul_path);
    writeStatus(nul_path);
}
"#;
    assert_stdout(&execute(source, b"", &args), b"ERR\nERR\nERR\nERR\n");
}

#[test]
fn files_read_rejects_invalid_utf8_content_recoverably() {
    let path = temp_path("invalid-utf8");
    std::fs::write(&path, [0xff, b'x']).unwrap();
    let args = [path.as_os_str().to_os_string()];
    let source = r#"
void main() {
    match (Args.get(0)) {
        Ok(path) {
            match (Files.readText(path)) {
                Ok(value) { println("OK"); }
                Err(error) { println("ERR"); }
            }
        }
        Err(error) { println("ARG_ERROR"); }
    }
}
"#;
    assert_stdout(&execute(source, b"", &args), b"ERR\n");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn args_count_and_get_preserve_contents_and_check_bounds() {
    let args = ["first", "with spaces", "--leading", "λ"]
        .into_iter().map(OsString::from).collect::<Vec<_>>();
    let source = r#"
void show(int index) {
    match (Args.get(index)) {
        Ok(value) { value.print(); }
        Err(error) { println("ERR"); }
    }
}
void main() {
    println(Args.count());
    show(0); show(1); show(2); show(3); show(-1); show(4);
}
"#;
    assert_stdout(&execute(source, b"", &args), "4\nfirst\nwith spaces\n--leading\nλ\nERR\nERR\n".as_bytes());
}

#[test]
fn args_empty_and_out_of_range_are_recoverable() {
    let source = r#"
void main() {
    println(Args.count());
    match (Args.get(0)) {
        Ok(value) { value.print(); }
        Err(error) { println("ERR"); }
    }
}
"#;
    assert_stdout(&execute(source, b"", &[]), b"0\nERR\n");
}

#[cfg(unix)]
#[test]
fn args_get_rejects_invalid_utf8_without_leaking() {
    use std::os::unix::ffi::OsStringExt;
    let args = [OsString::from_vec(vec![0xff])];
    let source = r#"
void main() {
    match (Args.get(0)) {
        Ok(value) { value.print(); }
        Err(error) { println("ERR"); }
    }
}
"#;
    assert_stdout(&execute(source, b"", &args), b"ERR\n");
}

#[test]
fn read_text_transfers_owned_buffers_at_growth_boundaries() {
    let source = r#"
void main() {
    match (Args.get(0)) {
        Ok(path) {
            match (Files.readText(path)) {
                Ok(value) { value.append("!"); Console.printText(value); }
                Err(error) { println("READ_ERROR"); }
            }
        }
        Err(error) { println("ARG_ERROR"); }
    }
}
"#;
    for content in ["a".repeat(4096), format!("{}é", "a".repeat(4095))] {
        let path = temp_path("buffer-boundary");
        std::fs::write(&path, content.as_bytes()).unwrap();
        let args = [path.as_os_str().to_owned()];
        let mut expected = content.into_bytes();
        expected.push(b'!');
        assert_stdout(&execute(source, b"", &args), &expected);
        std::fs::remove_file(path).unwrap();
    }
}
