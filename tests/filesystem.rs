mod support;

use nacre::{compile_to_c, native};
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nacre-filesystem-{}-{serial}",
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

fn compile_with_flags(source: &str, flags: &[&str]) -> native::NativeProgram {
    support::compile(source, flags).unwrap_or_else(|error| panic!("{error}"))
}

fn compile(source: &str) -> native::NativeProgram {
    compile_with_flags(source, &["-DNACRE_MEMORY_STATS"])
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

fn run_source(source: &str, args: &[&Path]) -> Output {
    let program = compile(source);
    run(&program, args)
}

fn take_line(reader: &mut BufReader<std::process::ChildStdout>) -> String {
    let mut line = String::new();
    let count = reader.read_line(&mut line).unwrap();
    assert_ne!(count, 0, "child ended before emitting its synchronization state");
    line.trim_end_matches(['\r', '\n']).to_owned()
}

fn send_line(child: &mut Child) {
    child.stdin.as_mut().unwrap().write_all(b"continue\n").unwrap();
    child.stdin.as_mut().unwrap().flush().unwrap();
}

fn finish_child(child: Child, mut reader: BufReader<std::process::ChildStdout>) -> Output {
    let mut stdout = Vec::new();
    reader.read_to_end(&mut stdout).unwrap();
    let mut output = child.wait_with_output().unwrap();
    output.stdout = stdout;
    output
}

fn wait_for_fifo_readers_to_close(path: &Path) {
    use std::os::unix::fs::OpenOptionsExt;
    const O_NONBLOCK: i32 = 0o4000;
    const ENXIO: i32 = 6;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    // Concurrent forks can retain a CLOEXEC reader until exec; observe the kernel state.
    loop {
        match fs::OpenOptions::new().write(true).custom_flags(O_NONBLOCK).open(path) {
            Err(error) if error.raw_os_error() == Some(ENXIO) => return,
            Err(error) => panic!("probing FIFO readers failed: {error}"),
            Ok(probe) => drop(probe),
        }
        assert!(std::time::Instant::now() < deadline, "FIFO still has a reader");
        std::thread::yield_now();
    }
}

fn assert_proc_target(pid: u32, target: &Path, expected: bool) {
    let directory = PathBuf::from(format!("/proc/{pid}/fd"));
    let matches = fs::read_dir(directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .any(|path| path == target);
    assert_eq!(matches, expected, "descriptor target {target:?}, expected open={expected}");
}

const STREAM_SOURCE: &str = r#"
Result<Unit,Text> exercise(borrow Path path) {
    File writer = File.create(path, 384)?;
    ByteBuffer source = ByteBuffer.withCapacity(0L)?;
    source.append((byte)65);
    source.append((byte)0);
    source.append((byte)66);
    source.append((byte)67);
    long partial = writer.writeSome(source, 1L, 2L)?;
    println(partial);
    match (writer.writeSome(source, source.length(), 1L)) {
        Err(error) { println("range rejected"); }
        Ok(count) { println("invalid range accepted"); }
    }
    writer.writeAll(source)?;
    writer.flush()?;
    writer.close()?;
    match (writer.flush()) {
        Err(error) { println("closed flush rejected"); }
        Ok(unit) { println("closed flush accepted"); }
    }
    match (writer.close()) {
        Err(error) { println("second close rejected"); }
        Ok(unit) { println("second close accepted"); }
    }
    match (writer.writeSome(source, 0L, 1L)) {
        Err(error) { println("closed writeSome rejected"); }
        Ok(count) { println("closed writeSome accepted"); }
    }
    match (writer.writeAll(source)) {
        Err(error) { println("closed writeAll rejected"); }
        Ok(unit) { println("closed writeAll accepted"); }
    }

    File reader = File.openRead(path)?;
    ByteBuffer destination = ByteBuffer.withCapacity(0L)?;
    destination.append((byte)90);
    match (reader.readSome(destination, -1L)) {
        Err(error) { println("negative read rejected"); }
        Ok(count) { println("negative read accepted"); }
    }
    long total = 0L;
    boolean eof = false;
    while (!eof) {
        long count = reader.readSome(destination, 2L)?;
        if (count == 0L) { eof = true; }
        else { total = total + count; }
    }
    println(total);
    println(reader.readSome(destination, 0L)?);
    reader.close()?;
    match (reader.readSome(destination, 1L)) {
        Err(error) { println("closed read rejected"); }
        Ok(count) { println("closed read accepted"); }
    }
    println(destination.length());
    println(destination.get(0L));
    boolean contentsMatch = true;
    long cursor = 0L;
    while (cursor < partial) {
        if (destination.get(cursor + 1L) != source.get(cursor + 1L)) { contentsMatch = false; }
        cursor = cursor + 1L;
    }
    int sourceIndex = 0;
    while (sourceIndex < 4) {
        if (destination.get(partial + 1L + (long)sourceIndex) != source.get((long)sourceIndex)) { contentsMatch = false; }
        sourceIndex = sourceIndex + 1;
    }
    println(contentsMatch);

    File appender = File.openAppend(path, 384)?;
    ByteBuffer suffix = ByteBuffer.withCapacity(0L)?;
    suffix.append((byte)88);
    appender.writeAll(suffix)?;
    appender.close()?;
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(path) {
                    match (exercise(path)) {
                        Err(error) { Console.printText(error); println(""); }
                        Ok(unit) { println("DONE"); }
                    }
                }
            }
        }
    }
}
"#;

#[test]
fn path_is_immutable_joins_without_normalizing_and_rejects_nul() {
    let source = r#"
void main() {
    match (Path.fromText(new Text("root/"))) {
        Err(error) { println("BASE_ERROR"); }
        Ok(base) {
            match (Path.fromText(new Text("child"))) {
                Err(error) { println("CHILD_ERROR"); }
                Ok(child) {
                    match (Path.join(base, child)) {
                        Err(error) { println("JOIN_ERROR"); }
                        Ok(joined) {
                            Console.printText(base.asText()); println("");
                            Console.printText(joined.asText()); println("");
                            println(joined.isAbsolute());
                            Path copy = joined.clone();
                            {
                                borrow Text copyText = copy.asText();
                                borrow Text joinedText = joined.asText();
                                println(copyText.equals(joinedText));
                            }
                            drop(joined);
                            Console.printText(copy.asText()); println("");
                        }
                    }
                }
            }
        }
    }
    match (Path.fromText(new Text("/absolute"))) {
        Err(error) { println("ABSOLUTE_ERROR"); }
        Ok(absolute) {
            println(absolute.isAbsolute());
            match (Path.fromText(new Text("base"))) {
                Err(error) { println("BASE_ERROR"); }
                Ok(base) {
                    match (Path.join(base, absolute)) {
                        Err(error) { println("ABSOLUTE_CHILD_REJECTED"); }
                        Ok(joined) { println("ABSOLUTE_CHILD_ACCEPTED"); }
                    }
                }
            }
        }
    }
    match (Path.fromText(new Text(""))) {
        Err(error) { println("EMPTY_REJECTED"); }
        Ok(path) { println("EMPTY_ACCEPTED"); }
    }
    match (Path.fromText(new Text("bad\0path"))) {
        Err(error) { println("NUL_REJECTED"); }
        Ok(path) { println("NUL_ACCEPTED"); }
    }
    match (Path.fromText(new Text("root/."))) {
        Err(error) { println("BASE_ERROR"); }
        Ok(base) {
            match (Path.fromText(new Text(".."))) {
                Err(error) { println("CHILD_ERROR"); }
                Ok(child) {
                    match (Path.join(base, child)) {
                        Err(error) { println("JOIN_ERROR"); }
                        Ok(joined) { Console.printText(joined.asText()); println(""); }
                    }
                }
            }
        }
    }
}
"#;
    let output = run_source(source, &[]);
    assert_eq!(
        output.stdout,
        b"root/\nroot/child\nfalse\ntrue\nroot/child\ntrue\nABSOLUTE_CHILD_REJECTED\nEMPTY_REJECTED\nNUL_REJECTED\nroot/./..\n"
    );
}

#[test]
fn native_resources_have_no_public_constructors_and_path_borrows_are_readonly() {
    for constructor in [
        "void main() { Path path = new Path(); }",
        "void main() { File file = new File(); }",
        "void main() { Directory directory = new Directory(); }",
        "void main() { FileInfo info = new FileInfo(); }",
    ] {
        assert!(compile_to_c(constructor).is_err(), "{constructor}");
    }
    assert!(compile_to_c("void main() { Map<Path,int> values = new Map<Path,int>(); }").is_err());

    let mutation = r#"
void main() {
    match (Path.fromText(new Text("immutable"))) {
        Err(error) { }
        Ok(path) { path.asText().append(" changed"); }
    }
}
"#;
    assert!(compile_to_c(mutation).is_err());
}

#[test]
fn file_streams_preserve_binary_bytes_truncate_append_and_report_closed_state() {
    let scratch = Scratch::new();
    let path = scratch.0.join("stream.bin");
    fs::write(&path, b"a much longer old file that must be truncated").unwrap();
    let output = run_source(STREAM_SOURCE, &[path.as_path()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<_> = stdout.lines().collect();
    let prefix: usize = lines[0].parse().unwrap();
    assert!(prefix <= 2);
    assert_eq!(lines[1], "range rejected");
    assert_eq!(lines[2], "closed flush rejected");
    assert_eq!(lines[3], "second close rejected");
    assert_eq!(lines[4], "closed writeSome rejected");
    assert_eq!(lines[5], "closed writeAll rejected");
    assert_eq!(lines[6], "negative read rejected");
    let total: usize = lines[7].parse().unwrap();
    assert_eq!(total, prefix + 4);
    assert_eq!(lines[8], "0");
    assert_eq!(lines[9], "closed read rejected");
    assert_eq!(lines[10].parse::<usize>().unwrap(), total + 1);
    assert_eq!(lines[11], "90");
    assert_eq!(lines[12], "true");
    assert_eq!(lines[13], "DONE");
    let original = [65, 0, 66, 67];
    let mut expected = original[1..1 + prefix].to_vec();
    expected.extend_from_slice(&original);
    expected.push(88);
    assert_eq!(fs::read(path).unwrap(), expected);
}

#[test]
fn filesystem_path_text_conveniences_preserve_nul_and_reject_bad_os_paths() {
    let scratch = Scratch::new();
    let path = scratch.0.join("text.txt");
    let source = r#"
Result<Unit,Text> exercise(borrow Text path) {
    Text value = new Text("A\0é");
    Unit success = Files.writeText(path, value)?;
    Text read = Files.readText(path)?;
    println(success);
    println(read.equals(value));
    Text invalid = path.clone();
    invalid.append("\0suffix");
    match (Files.readText(invalid)) {
        Err(error) { println("NUL_PATH_REJECTED"); }
        Ok(unexpected) { println("NUL_PATH_ACCEPTED"); }
    }
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(path) {
            match (exercise(path)) {
                Err(error) { Console.printText(error); println(""); }
                Ok(unit) { println("DONE"); }
            }
        }
    }
}
"#;
    let output = run_source(source, &[path.as_path()]);
    assert_eq!(output.stdout, b"()\ntrue\nNUL_PATH_REJECTED\nDONE\n");
    assert_eq!(fs::read(path).unwrap(), "A\0é".as_bytes());
}

#[test]
fn files_read_text_rejects_invalid_utf8_instead_of_replacing_bytes() {
    let scratch = Scratch::new();
    let path = scratch.0.join("invalid-utf8.txt");
    fs::write(&path, [0xff, 0xfe]).unwrap();
    let source = r#"
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(path) {
            match (Files.readText(path)) {
                Err(error) { println("INVALID_UTF8_REJECTED"); }
                Ok(value) { println("INVALID_UTF8_ACCEPTED"); }
            }
        }
    }
}
"#;
    let output = run_source(source, &[path.as_path()]);
    assert_eq!(output.stdout, b"INVALID_UTF8_REJECTED\n");
}

#[test]
fn files_read_text_preserves_utf8_and_nul_across_partial_buffer_tails() {
    let scratch = Scratch::new();
    let input = scratch.0.join("input.txt");
    let copied = scratch.0.join("copied.txt");
    let source = r#"
Result<Unit,Text> exercise() {
    Text input = Args.get(0)?;
    Text output = Args.get(1)?;
    Text content = Files.readText(input)?;
    Files.writeText(output, content)?;
    println(content.byteLength());
    println(content.charCount());
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(unit) { println("DONE"); }
        Err(error) { println("READ_ERROR"); }
    }
}
"#;
    let program = compile(source);
    let token = "Ω\0🙂e\u{301}".as_bytes();
    for size in [0, 1, 65535, 65536, 65537, 131071, 131072, 131073, 463347] {
        let mut bytes = vec![b'a'; size];
        for boundary in [16usize, 65536, 131072] {
            let start = boundary - 4;
            if start + token.len() <= bytes.len() {
                bytes[start..start + token.len()].copy_from_slice(token);
            }
        }
        fs::write(&input, &bytes).unwrap();
        fs::write(&copied, b"unchanged").unwrap();
        let output = run(&program, &[&input, &copied]);
        let scalars = std::str::from_utf8(&bytes).unwrap().chars().count();
        assert_eq!(output.stdout, format!("{size}\n{scalars}\nDONE\n").as_bytes());
        assert_eq!(fs::read(&copied).unwrap(), bytes);
    }
    for (prefix, invalid) in [
        (65535, b"\xf0\x9f".as_slice()),
        (65535, b"\xe2x\x80".as_slice()),
        (131071, b"\xed\xa0\x80".as_slice()),
    ] {
        let mut bytes = vec![b'a'; prefix];
        bytes.extend_from_slice(invalid);
        fs::write(&input, bytes).unwrap();
        fs::write(&copied, b"unchanged").unwrap();
        let output = run(&program, &[&input, &copied]);
        assert_eq!(output.stdout, b"READ_ERROR\n");
        assert_eq!(fs::read(&copied).unwrap(), b"unchanged");
    }
}

#[test]
fn directory_names_are_owned_utf8_components_and_closed_handles_fail() {
    let scratch = Scratch::new();
    let unicode_name = "entrée-🙂.txt";
    fs::write(scratch.0.join(unicode_name), b"x").unwrap();
    let source = r#"
Result<Text,Text> firstName(borrow mut Directory directory) {
    Option<Text> option = directory.nextName()?;
    match (option) {
        Some(name) { return new Result<Text,Text>.Ok(name); }
        None { return new Result<Text,Text>.Err(new Text("directory is empty")); }
    }
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(rootText) {
            match (Path.fromText(rootText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(root) {
                    match (Directory.open(root)) {
                        Err(error) { println("OPEN_ERROR"); }
                        Ok(directory) {
                            match (firstName(directory)) {
                                Err(error) { println("FIRST_ERROR"); }
                                Ok(saved) {
                                    match (directory.nextName()) {
                                        Err(error) { println("READ_ERROR"); }
                                        Ok(option) {
                                            Console.printText(saved); println("");
                                            match (option) {
                                                Some(name) { Console.printText(name); println(""); }
                                                None { println("EOF"); }
                                            }
                                        }
                                    }
                                    match (directory.close()) {
                                        Err(error) { println("CLOSE_ERROR"); }
                                        Ok(unit) { println("CLOSED"); }
                                    }
                                    match (directory.close()) {
                                        Err(error) { println("CLOSED_CLOSE_REJECTED"); }
                                        Ok(unit) { println("CLOSED_CLOSE_ACCEPTED"); }
                                    }
                                    match (directory.nextName()) {
                                        Err(error) { println("CLOSED_NEXT_REJECTED"); }
                                        Ok(option) { println("CLOSED_NEXT_ACCEPTED"); }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let output = run_source(source, &[scratch.0.as_path()]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let names: Vec<_> = stdout.lines().collect();
    assert_eq!(names, [unicode_name, "EOF", "CLOSED", "CLOSED_CLOSE_REJECTED", "CLOSED_NEXT_REJECTED"]);
}

#[cfg(unix)]
#[test]
fn directory_invalid_utf8_entry_is_an_error() {
    use std::os::unix::ffi::OsStringExt;

    let scratch = Scratch::new();
    let invalid_name = OsString::from_vec(vec![0xff, b'x']);
    fs::write(scratch.0.join(invalid_name), b"x").unwrap();
    let source = r#"
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(path) {
                    match (Directory.open(path)) {
                        Err(error) { println("OPEN_ERROR"); }
                        Ok(directory) {
                            match (directory.nextName()) {
                                Err(error) { println("INVALID_UTF8_REJECTED"); }
                                Ok(option) { println("INVALID_UTF8_ACCEPTED"); }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let output = run_source(source, &[scratch.0.as_path()]);
    assert_eq!(output.stdout, b"INVALID_UTF8_REJECTED\n");
}

#[test]
fn lstat_metadata_and_bounded_create_rename_remove_operations_are_observable() {
    use std::os::unix::fs::symlink;

    let scratch = Scratch::new();
    let source_path = scratch.0.join("source.txt");
    let link_path = scratch.0.join("symbolic-link");
    fs::write(&source_path, b"data").unwrap();
    symlink("source.txt", &link_path).unwrap();
    let source = r#"
Result<Unit,Text> operations(borrow Path root, borrow Path source) {
    match (Files.metadata(root)) {
        Err(error) { println("ROOT_METADATA_ERROR"); }
        Ok(info) { println(info.isDirectory()); println(info.isFile()); println(info.isSymlink()); }
    }
    match (Files.metadata(source)) {
        Err(error) { println("FILE_METADATA_ERROR"); }
        Ok(info) { println(info.size()); println(info.isFile()); println(info.isDirectory()); println(info.isSymlink()); }
    }
    Path linkPart = Path.fromText(new Text("symbolic-link"))?;
    Path link = Path.join(root, linkPart)?;
    match (Files.metadata(link)) {
        Err(error) { println("LINK_METADATA_ERROR"); }
        Ok(info) { println(info.isFile()); println(info.isSymlink()); }
    }
    Path targetPart = Path.fromText(new Text("renamed.txt"))?;
    Path target = Path.join(root, targetPart)?;
    Files.rename(source, target)?;
    Files.removeFile(target)?;
    Path directoryPart = Path.fromText(new Text("created-directory"))?;
    Path directory = Path.join(root, directoryPart)?;
    Files.createDirectory(directory, 448)?;
    Files.removeDirectory(directory)?;
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(rootText) {
            match (Path.fromText(rootText)) {
                Err(error) { println("ROOT_PATH_ERROR"); }
                Ok(root) {
                    match (Args.get(1)) {
                        Err(error) { println("ARG_ERROR"); }
                        Ok(sourceText) {
                            match (Path.fromText(sourceText)) {
                                Err(error) { println("SOURCE_PATH_ERROR"); }
                                Ok(source) {
                                    match (operations(root, source)) {
                                        Err(error) { Console.printText(error); println(""); }
                                        Ok(unit) { println("OPERATIONS_DONE"); }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let output = run_source(source, &[scratch.0.as_path(), source_path.as_path()]);
    assert_eq!(
        output.stdout,
        b"true\nfalse\nfalse\n4\ntrue\nfalse\nfalse\nfalse\ntrue\nOPERATIONS_DONE\n"
    );
    assert!(!source_path.exists());
    assert!(!scratch.0.join("renamed.txt").exists());
    assert!(!scratch.0.join("created-directory").exists());
    assert!(fs::symlink_metadata(&link_path).is_ok());
}

#[test]
fn remove_directory_does_not_recurse_or_remove_contents() {
    let scratch = Scratch::new();
    let nonempty = scratch.0.join("nonempty");
    fs::create_dir(&nonempty).unwrap();
    fs::write(nonempty.join("marker"), b"keep").unwrap();
    let source = r#"
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(path) {
                    match (Files.removeDirectory(path)) {
                        Err(error) { println("NONEMPTY_REJECTED"); }
                        Ok(unit) { println("NONEMPTY_REMOVED"); }
                    }
                }
            }
        }
    }
}
"#;
    let output = run_source(source, &[nonempty.as_path()]);
    assert_eq!(output.stdout, b"NONEMPTY_REJECTED\n");
    assert_eq!(fs::read(nonempty.join("marker")).unwrap(), b"keep");
}

#[test]
fn file_and_directory_close_and_drop_release_real_proc_descriptors() {
    let scratch = Scratch::new();
    let file = scratch.0.join("held.txt");
    let missing = scratch.0.join("does-not-exist");
    fs::write(&file, b"held").unwrap();
    let source = r#"
void printLine(String value) { Console.print(value); Console.print("\n"); }
void pauseUntilReleased() {
    match (Console.readLine()) {
        Err(error) { drop(error); }
        Ok(option) { drop(option); }
    }
}
void inspect(borrow Path filePath, borrow Path missingPath, borrow Path directoryPath) {
    match (File.openRead(missingPath)) {
        Err(error) { printLine("FAILED_OPEN_DONE"); }
        Ok(file) { printLine("FAILED_OPEN_UNEXPECTED"); }
    }
    match (File.openRead(filePath)) {
        Err(error) { printLine("FILE_OPEN_ERROR"); }
        Ok(file) {
            printLine("FILE_OPEN"); pauseUntilReleased();
            match (file.close()) {
                Err(error) { printLine("FILE_CLOSE_ERROR"); }
                Ok(unit) { printLine("FILE_CLOSED"); }
            }
            pauseUntilReleased();
            {
                match (File.openRead(filePath)) {
                    Err(error) { printLine("FILE_DROP_OPEN_ERROR"); }
                    Ok(transient) { printLine("FILE_DROP_OPEN"); pauseUntilReleased(); }
                }
            }
            printLine("FILE_DROPPED"); pauseUntilReleased();
        }
    }
    match (Directory.open(directoryPath)) {
        Err(error) { printLine("DIRECTORY_OPEN_ERROR"); }
        Ok(directory) {
            printLine("DIRECTORY_OPEN"); pauseUntilReleased();
            match (directory.close()) {
                Err(error) { printLine("DIRECTORY_CLOSE_ERROR"); }
                Ok(unit) { printLine("DIRECTORY_CLOSED"); }
            }
            pauseUntilReleased();
            {
                match (Directory.open(directoryPath)) {
                    Err(error) { printLine("DIRECTORY_DROP_OPEN_ERROR"); }
                    Ok(transient) { printLine("DIRECTORY_DROP_OPEN"); pauseUntilReleased(); }
                }
            }
            printLine("DIRECTORY_DROPPED"); pauseUntilReleased();
        }
    }
}
void main() {
    match (Args.get(0)) {
        Err(error) { printLine("ARG_ERROR"); }
        Ok(fileText) {
            match (Path.fromText(fileText)) {
                Err(error) { printLine("FILE_PATH_ERROR"); }
                Ok(filePath) {
                    match (Args.get(1)) {
                        Err(error) { printLine("ARG_ERROR"); }
                        Ok(missingText) {
                            match (Path.fromText(missingText)) {
                                Err(error) { printLine("MISSING_PATH_ERROR"); }
                                Ok(missingPath) {
                                    match (Args.get(2)) {
                                        Err(error) { printLine("ARG_ERROR"); }
                                        Ok(directoryText) {
                                            match (Path.fromText(directoryText)) {
                                                Err(error) { printLine("DIRECTORY_PATH_ERROR"); }
                                                Ok(directoryPath) { inspect(filePath, missingPath, directoryPath); }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let program = compile(source);
    let mut child = Command::new(program.path())
        .args([&file, &missing, &scratch.0])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let mut output = BufReader::new(child.stdout.take().unwrap());

    assert_eq!(take_line(&mut output), "FAILED_OPEN_DONE");
    assert_proc_target(pid, &missing, false);
    assert_eq!(take_line(&mut output), "FILE_OPEN");
    assert_proc_target(pid, &file, true);
    send_line(&mut child);
    assert_eq!(take_line(&mut output), "FILE_CLOSED");
    assert_proc_target(pid, &file, false);
    send_line(&mut child);
    assert_eq!(take_line(&mut output), "FILE_DROP_OPEN");
    assert_proc_target(pid, &file, true);
    send_line(&mut child);
    assert_eq!(take_line(&mut output), "FILE_DROPPED");
    assert_proc_target(pid, &file, false);
    send_line(&mut child);
    assert_eq!(take_line(&mut output), "DIRECTORY_OPEN");
    assert_proc_target(pid, &scratch.0, true);
    send_line(&mut child);
    assert_eq!(take_line(&mut output), "DIRECTORY_CLOSED");
    assert_proc_target(pid, &scratch.0, false);
    send_line(&mut child);
    assert_eq!(take_line(&mut output), "DIRECTORY_DROP_OPEN");
    assert_proc_target(pid, &scratch.0, true);
    send_line(&mut child);
    assert_eq!(take_line(&mut output), "DIRECTORY_DROPPED");
    assert_proc_target(pid, &scratch.0, false);
    send_line(&mut child);
    drop(output);
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_live_zero(&result);
}

#[cfg(target_os = "linux")]
#[test]
fn closed_fifo_write_returns_err_without_delivering_sigpipe_to_the_process() {
    let scratch = Scratch::new();
    let fifo = scratch.0.join("writer.fifo");
    let created = Command::new("mkfifo").arg(&fifo).status().unwrap();
    assert!(created.success());
    let source = r#"
void printLine(String value) { Console.print(value); Console.print("\n"); }
void pauseUntilReleased() {
    match (Console.readLine()) {
        Err(error) { drop(error); }
        Ok(option) { drop(option); }
    }
}
Result<Unit,Text> writeAfterRelease(borrow mut File file) {
    ByteBuffer payload = ByteBuffer.withCapacity(0L)?;
    payload.append((byte)88);
    return file.writeAll(payload);
}
void main() {
    match (Args.get(0)) {
        Err(error) { printLine("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { printLine("PATH_ERROR"); }
                Ok(path) {
                    match (File.openAppend(path, 384)) {
                        Err(error) { printLine("OPEN_ERROR"); }
                        Ok(file) {
                            printLine("OPEN"); pauseUntilReleased();
                            match (writeAfterRelease(file)) {
                                Err(error) { printLine("WRITE_ERR"); }
                                Ok(unit) { printLine("WRITE_OK"); }
                            }
                            printLine("ALIVE");
                            match (file.close()) {
                                Err(error) { printLine("CLOSE_ERR"); }
                                Ok(unit) { printLine("CLOSED"); }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
    for (blocked, pending) in [(false, false), (true, false), (true, true)] {
        let mut c = compile_to_c(source).unwrap();
        let signal_state = r#"
static void verify_caller_signal_state(void) {
    sigset_t mask, pending;
    if (pthread_sigmask(SIG_SETMASK, NULL, &mask) != 0 || sigpending(&pending) != 0) { _Exit(90); }
    if (sigismember(&mask, SIGPIPE) != @BLOCKED@ || sigismember(&pending, SIGPIPE) != @PENDING@) { _Exit(91); }
}
__attribute__((constructor)) static void establish_caller_signal_state(void) {
    struct sigaction action = {0};
    action.sa_handler = SIG_DFL;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGPIPE, &action, NULL) != 0) { _Exit(92); }
    sigset_t pipe;
    sigemptyset(&pipe);
    sigaddset(&pipe, SIGPIPE);
    if (pthread_sigmask(@BLOCKED@ ? SIG_BLOCK : SIG_UNBLOCK, &pipe, NULL) != 0) { _Exit(93); }
    if (@PENDING@ && pthread_kill(pthread_self(), SIGPIPE) != 0) { _Exit(94); }
    if (atexit(verify_caller_signal_state) != 0) { _Exit(95); }
}
"#.replace("@BLOCKED@", if blocked { "1" } else { "0" })
  .replace("@PENDING@", if pending { "1" } else { "0" });
        c.push_str(&signal_state);
        let program = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let read_end = fs::OpenOptions::new().read(true).write(true).open(&fifo).unwrap();
    let mut child = Command::new(program.path())
        .arg(&fifo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    assert_eq!(take_line(&mut output), "OPEN");
    drop(read_end);
    wait_for_fifo_readers_to_close(&fifo);
    send_line(&mut child);
    assert_eq!(take_line(&mut output), "WRITE_ERR");
    assert_eq!(take_line(&mut output), "ALIVE");
    assert_eq!(take_line(&mut output), "CLOSED");
    drop(output);
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_live_zero(&result);
    }
}

#[test]
fn filesystem_example_runs_as_a_native_program_and_cleans_its_workspace() {
    let source = include_str!("../examples/io/filesystem.ncr");
    let program = compile(source);
    let scratch = Scratch::new();
    let output = run(&program, &[scratch.0.as_path()]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.lines().any(|line| line == "4"));
    assert!(stdout.lines().any(|line| line == "payload.bin"));
    assert!(stdout.lines().any(|line| line == "note-é.txt"));
    assert!(stdout.lines().any(|line| line == "cleaned"));
    assert!(stdout.lines().any(|line| line == "filesystem complete"));
}

#[test]
fn filesystem_binary_streams_run_under_address_and_undefined_sanitizers() {
    let scratch = Scratch::new();
    let path = scratch.0.join("sanitized.bin");
    let program = compile_with_flags(
        STREAM_SOURCE,
        &[
            "-DNACRE_MEMORY_STATS",
            "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer",
        ],
    );
    let output = run(&program, &[path.as_path()]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("DONE"));
    let lines: Vec<_> = String::from_utf8_lossy(&output.stdout).lines().map(str::to_owned).collect();
    let prefix: usize = lines[0].parse().unwrap();
    let original = [65, 0, 66, 67];
    let mut expected = original[1..1 + prefix].to_vec();
    expected.extend_from_slice(&original);
    expected.push(88);
    assert_eq!(fs::read(path).unwrap(), expected);
}
#[test]
fn positioned_file_io_preserves_the_cursor_and_metadata_is_observable() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let scratch = Scratch::new();
    let file = scratch.0.join("positioned.bin");
    let created = scratch.0.join("created.bin");
    fs::write(&file, b"abcdefghij").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    let source = r#"
Result<Unit,Text> exercise(borrow Path path, borrow Path createdPath) {
    File file = File.openReadWrite(path, 384, false)?;
    println(file.position()?);
    ByteBuffer destination = ByteBuffer.withCapacity(0L)?;
    println(file.readAt(destination, 3L, 4L)?);
    println(file.position()?);
    ByteBuffer replacement = ByteBuffer.withCapacity(0L)?;
    replacement.append((byte)88);
    replacement.append((byte)89);
    println(file.writeAt(replacement, 0L, 6L, 2L)?);
    println(file.position()?);
    match (file.readAt(destination, -1L, 1L)) {
        Err(error) { println("READAT_NEGATIVE_OFFSET_REJECTED"); }
        Ok(count) { println("READAT_NEGATIVE_OFFSET_ACCEPTED"); }
    }
    match (file.writeAt(replacement, 1L, 0L, 2L)) {
        Err(error) { println("WRITEAT_INVALID_RANGE_REJECTED"); }
        Ok(count) { println("WRITEAT_INVALID_RANGE_ACCEPTED"); }
    }
    println(file.position()?);
    println(file.seek(3L, 0)?);
    match (file.seek(-5L, 1)) {
        Err(error) { println("SEEK_NEGATIVE_REJECTED"); }
        Ok(position) { println("SEEK_NEGATIVE_ACCEPTED"); }
    }
    println(file.position()?);
    println(file.seek(5L, 0)?);
    file.truncate(4L)?;
    println(file.position()?);
    FileInfo info = Files.metadata(path)?;
    println(info.size());
    println(info.modifiedMillis());
    println(info.accessedMillis());
    println(info.changedMillis());
    println(info.mode());
    file.close()?;

    File append = File.openAppend(path, 384)?;
    match (append.writeAt(replacement, 0L, 0L, 1L)) {
        Err(error) { println("APPEND_PWRITE_REJECTED"); }
        Ok(count) { println("APPEND_PWRITE_ACCEPTED"); }
    }
    append.close()?;
    match (File.openReadWrite(createdPath, 384, false)) {
        Err(error) { println("NO_CREATE_REJECTED"); }
        Ok(unexpected) { unexpected.close()?; println("NO_CREATE_ACCEPTED"); }
    }
    File created = File.openReadWrite(createdPath, 384, true)?;
    created.close()?;
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("FILE_ARG_ERROR"); }
        Ok(fileText) {
            match (Path.fromText(fileText)) {
                Err(error) { println("FILE_PATH_ERROR"); }
                Ok(filePath) {
                    match (Args.get(1)) {
                        Err(error) { println("CREATED_ARG_ERROR"); }
                        Ok(createdText) {
                            match (Path.fromText(createdText)) {
                                Err(error) { println("CREATED_PATH_ERROR"); }
                                Ok(createdPath) {
                                    match (exercise(filePath, createdPath)) {
                                        Err(error) { Console.printText(error); println(""); }
                                        Ok(unit) { println("COMPLETE"); }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let output = run_source(source, &[file.as_path(), created.as_path()]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(
        &lines[..13],
        [
            "0",
            "4",
            "0",
            "2",
            "0",
            "READAT_NEGATIVE_OFFSET_REJECTED",
            "WRITEAT_INVALID_RANGE_REJECTED",
            "0",
            "3",
            "SEEK_NEGATIVE_REJECTED",
            "3",
            "5",
            "5"
        ]
    );
    assert_eq!(lines[13], "4");
    assert_eq!(lines[18..], ["APPEND_PWRITE_REJECTED", "NO_CREATE_REJECTED", "COMPLETE"]);
    assert_eq!(fs::metadata(&created).unwrap().len(), 0);
    let metadata = fs::metadata(&file).unwrap();
    let millis = |seconds: i64, nanos: i64| seconds * 1000 + nanos / 1_000_000;
    assert_eq!(lines[14].parse::<i64>().unwrap(), millis(metadata.mtime(), metadata.mtime_nsec()));
    assert_eq!(lines[15].parse::<i64>().unwrap(), millis(metadata.atime(), metadata.atime_nsec()));
    assert_eq!(lines[16].parse::<i64>().unwrap(), millis(metadata.ctime(), metadata.ctime_nsec()));
    assert_eq!(lines[17].parse::<i32>().unwrap(), metadata.mode() as i32);
    assert_eq!(fs::read(&file).unwrap(), b"abcd");
}

#[test]
fn files_copy_links_directories_listing_and_canonical_paths_use_real_state() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let scratch = Scratch::new();
    let source_file = scratch.0.join("source.bin");
    let target_file = scratch.0.join("target.bin");
    let link_file = scratch.0.join("link.bin");
    let rejected_file = scratch.0.join("rejected.bin");
    let nested = scratch.0.join("nested").join("deeper");
    let payload = vec![b'x'; 200_000];
    fs::write(&source_file, &payload).unwrap();
    let source = r#"
Result<Unit,Text> exercise(borrow Path root, borrow Path source, borrow Path target, borrow Path link, borrow Path rejected) {
    match (Files.copy(source, target, false)) {
        Err(error) { println("COPY_NEW_ERROR"); }
        Ok(unit) { println("COPY_NEW_OK"); }
    }
    match (Files.copy(source, target, false)) {
        Err(error) { println("COPY_EXISTING_REJECTED"); }
        Ok(unit) { println("COPY_EXISTING_ACCEPTED"); }
    }
    Files.copy(source, target, true)?;
    Files.symlink(source, link)?;
    match (Files.copy(link, rejected, false)) {
        Err(error) { println("COPY_SYMLINK_REJECTED"); }
        Ok(unit) { println("COPY_SYMLINK_ACCEPTED"); }
    }
    Files.createDirectories(rejected, 448)?;
    Path nestedPart = Path.fromText(new Text("nested/deeper"))?;
    Path nested = Path.join(root, nestedPart)?;
    Files.createDirectories(nested, 448)?;
    Path linkTarget = Files.readLink(link)?;
    Console.printText(linkTarget.asText()); println("");
    Files.chmod(target, 416)?;
    FileInfo info = Files.metadata(target)?;
    println(info.size());
    println(info.mode());
    List<Text> names = Files.list(root)?;
    println(names.size());
    Path canonical = Files.canonical(source)?;
    Console.printText(canonical.asText()); println("");
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ROOT_ARG_ERROR"); }
        Ok(rootText) {
            match (Path.fromText(rootText)) {
                Err(error) { println("ROOT_PATH_ERROR"); }
                Ok(root) {
                    match (Args.get(1)) {
                        Err(error) { println("SOURCE_ARG_ERROR"); }
                        Ok(sourceText) {
                            match (Path.fromText(sourceText)) {
                                Err(error) { println("SOURCE_PATH_ERROR"); }
                                Ok(source) {
                                    match (Args.get(2)) {
                                        Err(error) { println("TARGET_ARG_ERROR"); }
                                        Ok(targetText) {
                                            match (Path.fromText(targetText)) {
                                                Err(error) { println("TARGET_PATH_ERROR"); }
                                                Ok(target) {
                                                    match (Args.get(3)) {
                                                        Err(error) { println("LINK_ARG_ERROR"); }
                                                        Ok(linkText) {
                                                            match (Path.fromText(linkText)) {
                                                                Err(error) { println("LINK_PATH_ERROR"); }
                                                                Ok(link) {
                                                                    match (Args.get(4)) {
                                                                        Err(error) { println("REJECTED_ARG_ERROR"); }
                                                                        Ok(rejectedText) {
                                                                            match (Path.fromText(rejectedText)) {
                                                                                Err(error) { println("REJECTED_PATH_ERROR"); }
                                                                                Ok(rejected) {
                                                                                    match (exercise(root, source, target, link, rejected)) {
                                                                                        Err(error) { Console.printText(error); println(""); }
                                                                                        Ok(unit) { println("COMPLETE"); }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let source_path = fs::canonicalize(&source_file).unwrap();
    let output = run_source(
        source,
        &[
            scratch.0.as_path(),
            source_file.as_path(),
            target_file.as_path(),
            link_file.as_path(),
            rejected_file.as_path(),
        ],
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert!(lines.contains(&"COPY_NEW_OK"));
    assert!(lines.contains(&"COPY_EXISTING_REJECTED"));
    assert!(lines.contains(&"COPY_SYMLINK_REJECTED"));
    assert!(lines.contains(&"COMPLETE"));
    assert_eq!(lines[3], source_file.to_str().unwrap());
    assert_eq!(lines[4], "200000");
    assert_eq!(lines[5].parse::<u32>().unwrap(), fs::metadata(&target_file).unwrap().mode() as u32);
    assert_eq!(lines[6], "5");
    assert_eq!(lines[7], source_path.to_str().unwrap());
    assert_eq!(fs::read(&target_file).unwrap(), payload);
    assert_eq!(fs::read_link(&link_file).unwrap(), source_file);
    assert!(fs::symlink_metadata(&rejected_file).unwrap().is_dir());
    assert!(nested.is_dir());
    assert_eq!(
        fs::metadata(&target_file).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn files_read_link_grows_for_long_targets() {
    use std::os::unix::fs::symlink;

    let scratch = Scratch::new();
    let target = format!("{}target", "segment/".repeat(40));
    let link = scratch.0.join("long-link");
    symlink(&target, &link).unwrap();

    let source = r#"
Result<Unit,Text> printLink(borrow Path link) {
    Path target = Files.readLink(link)?;
    Console.printText(target.asText());
    println("");
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(linkText) {
            match (Path.fromText(linkText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(link) {
                    match (printLink(link)) {
                        Err(error) { Console.printText(error); println(""); }
                        Ok(unit) { println("OK"); }
                    }
                }
            }
        }
    }
}
"#;
    let output = run_source(source, &[link.as_path()]);
    assert_eq!(output.stdout, format!("{target}\nOK\n").as_bytes());
}

#[cfg(target_os = "linux")]
#[test]
fn canonical_rejects_invalid_utf8_in_resolved_paths() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;

    let scratch = Scratch::new();
    let invalid_target = scratch.0.join(OsString::from_vec(vec![b't', 0xff]));
    let link = scratch.0.join("valid-link");
    fs::write(&invalid_target, b"payload").unwrap();
    symlink(&invalid_target, &link).unwrap();
    let source = r#"
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(path) {
                    match (Files.canonical(path)) {
                        Err(error) { Console.printText(error); println(""); }
                        Ok(canonical) { println("CANONICAL_INVALID_UTF8_ACCEPTED"); }
                    }
                }
            }
        }
    }
}
"#;
    let output = run_source(source, &[link.as_path()]);
    assert_eq!(
        output.stdout,
        b"Files.canonical target is not valid UTF-8 failed (errno=84)\n"
    );
}

fn spawn_watcher(
    program: &native::NativeProgram,
    path: &Path,
) -> (Child, BufReader<std::process::ChildStdout>) {
    let mut child = Command::new(program.path())
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    assert_eq!(take_line(&mut output), "WATCH_READY");
    (child, output)
}

#[cfg(target_os = "linux")]
#[test]
fn watcher_observes_create_write_rename_delete_after_a_readiness_barrier() {
    let scratch = Scratch::new();
    let entry = scratch.0.join("entry");
    let renamed = scratch.0.join("renamed");
    let source = r#"
void printEvent(borrow WatchEvent event) {
    Console.printText(event.name());
    Console.print(" ");
    match (event.kind()) {
        Created { Console.print("Created"); }
        Modified { Console.print("Modified"); }
        AttributesChanged { Console.print("AttributesChanged"); }
        ClosedForWrite { Console.print("ClosedForWrite"); }
        Deleted { Console.print("Deleted"); }
        MovedFrom { Console.print("MovedFrom"); }
        MovedTo { Console.print("MovedTo"); }
        MovedSelf { Console.print("MovedSelf"); }
        DeletedSelf { Console.print("DeletedSelf"); }
        Ignored { Console.print("Ignored"); }
        Overflow { Console.print("Overflow"); }
    }
    Console.print(" ");
    println(event.cookie());
}
Result<Unit,Text> watch(borrow Path path) {
    Watcher watcher = Watcher.open(path)?;
    match (watcher.next(0L)?) {
        None { println("NONBLOCK_EMPTY"); }
        Some(event) { println("NONBLOCK_UNEXPECTED_EVENT"); }
    }
    Console.print("WATCH_READY\n");
    match (Console.readLine()) {
        Err(error) { return new Result<Unit,Text>.Err(error); }
        Ok(option) { drop(option); }
    }
    boolean finished = false;
    while (!finished) {
        match (watcher.next(200L)?) {
            None { finished = true; }
            Some(event) { printEvent(event); }
        }
    }
    watcher.close()?;
    watcher.close()?;
    println("CLOSE_IDEMPOTENT");
    match (watcher.next(0L)) {
        Err(error) { println("CLOSED_NEXT_REJECTED"); }
        Ok(option) { println("CLOSED_NEXT_ACCEPTED"); }
    }
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(path) {
                    match (watch(path)) {
                        Err(error) { Console.printText(error); println(""); }
                        Ok(unit) { println("WATCH_DONE"); }
                    }
                }
            }
        }
    }
}
"#;
    let program = compile(source);
    let mut child = Command::new(program.path())
        .arg(&scratch.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    assert_eq!(take_line(&mut output), "NONBLOCK_EMPTY");
    assert_eq!(take_line(&mut output), "WATCH_READY");

    fs::write(&entry, b"payload").unwrap();
    fs::rename(&entry, &renamed).unwrap();
    fs::remove_file(&renamed).unwrap();
    send_line(&mut child);
    let result = finish_child(child, output);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_live_zero(&result);
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("entry Created "));
    assert!(stdout.contains("entry Modified "));
    assert!(stdout.contains("entry ClosedForWrite "));
    assert!(stdout.contains("entry MovedFrom "));
    assert!(stdout.contains("renamed MovedTo "));
    assert!(stdout.contains("renamed Deleted "));
    assert!(stdout.contains("CLOSE_IDEMPOTENT\n"));
    assert!(stdout.contains("CLOSED_NEXT_REJECTED\n"));
    assert!(stdout.contains("WATCH_DONE\n"));
    let from_cookie = stdout
        .lines()
        .find(|line| line.starts_with("entry MovedFrom "))
        .unwrap()
        .split_whitespace()
        .nth(2)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let to_cookie = stdout
        .lines()
        .find(|line| line.starts_with("renamed MovedTo "))
        .unwrap()
        .split_whitespace()
        .nth(2)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert_ne!(from_cookie, 0);
    assert_eq!(from_cookie, to_cookie);
    assert!(!entry.exists());
    assert!(!renamed.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn watcher_reports_real_inotify_queue_overflow() {
    let scratch = Scratch::new();
    let source = r#"
Result<Unit,Text> drain(borrow Path path) {
    Watcher watcher = Watcher.open(path)?;
    Console.print("WATCH_READY\n");
    match (Console.readLine()) {
        Err(error) { return new Result<Unit,Text>.Err(error); }
        Ok(option) { drop(option); }
    }
    boolean finished = false;
    boolean overflow = false;
    boolean overflowKind = false;
    while (!finished) {
        match (watcher.next(0L)?) {
            None { finished = true; }
            Some(event) {
                if (event.overflow()) {
                    overflow = true;
                    match (event.kind()) {
                        Created { }
                        Modified { }
                        AttributesChanged { }
                        ClosedForWrite { }
                        Deleted { }
                        MovedFrom { }
                        MovedTo { }
                        MovedSelf { }
                        DeletedSelf { }
                        Ignored { }
                        Overflow { overflowKind = true; }
                    }
                }
            }
        }
    }
    if (overflow && overflowKind) { println("OVERFLOW_KIND_SEEN"); }
    else { println("OVERFLOW_KIND_MISSING"); }
    watcher.close()?;
    watcher.close()?;
    println("CLOSE_IDEMPOTENT");
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(path) {
                    match (drain(path)) {
                        Err(error) { Console.printText(error); println(""); }
                        Ok(unit) { println("WATCH_DONE"); }
                    }
                }
            }
        }
    }
}
"#;
    let queue_limit = fs::read_to_string("/proc/sys/fs/inotify/max_queued_events")
        .unwrap()
        .trim()
        .parse::<usize>()
        .unwrap();
    let program = compile(source);
    let (mut child, output) = spawn_watcher(&program, &scratch.0);

    for index in 0..=queue_limit {
        let path = scratch.0.join(format!("overflow-{index:08x}"));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        drop(file);
    }
    send_line(&mut child);
    let result = finish_child(child, output);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_live_zero(&result);
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("OVERFLOW_KIND_SEEN\n"), "{stdout}");
    assert!(stdout.contains("CLOSE_IDEMPOTENT\n"));
    assert!(stdout.contains("WATCH_DONE\n"));
}

#[cfg(target_os = "linux")]
#[test]
fn watcher_reports_self_deletion_invalidation_and_close_is_idempotent() {
    let scratch = Scratch::new();
    let watched_directory = scratch.0.join("watched");
    fs::create_dir(&watched_directory).unwrap();
    let source = r#"
Result<Unit,Text> watch(borrow Path path) {
    Watcher watcher = Watcher.open(path)?;
    Console.print("WATCH_READY\n");
    match (Console.readLine()) {
        Err(error) { return new Result<Unit,Text>.Err(error); }
        Ok(option) { drop(option); }
    }
    boolean sawDeletedSelf = false;
    boolean finished = false;
    while (!finished) {
        match (watcher.next(2000L)) {
            Err(error) {
                if (sawDeletedSelf) { println("INVALIDATED_AFTER_DELETE_SELF"); }
                else { println("INVALIDATED_WITHOUT_DELETE_SELF"); }
                finished = true;
            }
            Ok(option) {
                match (option) {
                    None { println("UNEXPECTED_TIMEOUT"); finished = true; }
                    Some(event) {
                        match (event.kind()) {
                            Created { }
                            Modified { }
                            AttributesChanged { }
                            ClosedForWrite { }
                            Deleted { }
                            MovedFrom { }
                            MovedTo { }
                            MovedSelf { }
                            DeletedSelf { sawDeletedSelf = true; println("DELETE_SELF_EVENT"); }
                            Ignored { }
                            Overflow { }
                        }
                    }
                }
            }
        }
    }
    watcher.close()?;
    watcher.close()?;
    println("CLOSE_IDEMPOTENT");
    match (watcher.next(0L)) {
        Err(error) { println("CLOSED_NEXT_REJECTED"); }
        Ok(option) { println("CLOSED_NEXT_ACCEPTED"); }
    }
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(path) {
                    match (watch(path)) {
                        Err(error) { Console.printText(error); println(""); }
                        Ok(unit) { println("WATCH_DONE"); }
                    }
                }
            }
        }
    }
}
"#;
    let program = compile(source);
    let (mut child, output) = spawn_watcher(&program, &watched_directory);

    fs::remove_dir(&watched_directory).unwrap();
    send_line(&mut child);
    let result = finish_child(child, output);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_live_zero(&result);
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("DELETE_SELF_EVENT\n"));
    assert!(stdout.contains("INVALIDATED_AFTER_DELETE_SELF\n"));
    assert!(stdout.contains("CLOSE_IDEMPOTENT\n"));
    assert!(stdout.contains("CLOSED_NEXT_REJECTED\n"));
    assert!(stdout.contains("WATCH_DONE\n"));
}

#[cfg(target_os = "linux")]
#[test]
fn watcher_remains_usable_after_non_utf8_entry_name() {
    use std::os::unix::ffi::OsStringExt;

    let scratch = Scratch::new();
    let watched = scratch.0.join("watched");
    let source_dir = scratch.0.join("source");
    fs::create_dir(&watched).unwrap();
    fs::create_dir(&source_dir).unwrap();
    let invalid_name = OsString::from_vec(vec![0xff]);
    let source_path = source_dir.join(&invalid_name);
    let target_path = watched.join(invalid_name);
    fs::write(&source_path, b"payload").unwrap();
    let source = r#"
Result<Unit,Text> watch(borrow Path path) {
    Watcher watcher = Watcher.open(path)?;
    Console.print("WATCH_READY\n");
    match (Console.readLine()) {
        Err(error) { return new Result<Unit,Text>.Err(error); }
        Ok(option) { drop(option); }
    }
    match (watcher.next(2000L)) {
        Err(error) {
            if (error.equalsString("Watcher.next event name is not valid UTF-8")) { println("NON_UTF8_NAME_REPORTED"); }
            else { println("UNEXPECTED_INITIAL_ERROR"); }
            drop(error);
        }
        Ok(option) { println("NON_UTF8_NAME_NOT_REPORTED"); drop(option); }
    }
    Console.print("VALID_EVENT_READY\n");
    match (Console.readLine()) {
        Err(error) { return new Result<Unit,Text>.Err(error); }
        Ok(option) { drop(option); }
    }
    boolean done = false;
    while (!done) {
        match (watcher.next(2000L)) {
            Err(error) {
                if (error.equalsString("Watcher.next event name is not valid UTF-8")) { println("NON_UTF8_NAME_REPORTED_AGAIN"); }
                else if (error.equalsString("Watcher.next: directory watch is invalidated")) {
                    println("WATCHER_INVALIDATED");
                    done = true;
                } else {
                    println("UNEXPECTED_ERROR");
                    done = true;
                }
                drop(error);
            }
            Ok(option) {
                match (option) {
                    None { println("VALID_EVENT_TIMED_OUT"); done = true; }
                    Some(event) {
                        borrow Text name = event.name();
                        if (name.equalsString("valid")) {
                            println("VALID_EVENT_RECEIVED");
                            done = true;
                        }
                    }
                }
            }
        }
    }
    watcher.close()?;
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(path) {
                    match (watch(path)) {
                        Err(error) { Console.printText(error); println(""); }
                        Ok(unit) { println("WATCH_DONE"); }
                    }
                }
            }
        }
    }
}
"#;
    let program = compile(source);
    let (mut child, mut output) = spawn_watcher(&program, &watched);
    fs::rename(&source_path, &target_path).unwrap();
    send_line(&mut child);
    assert_eq!(take_line(&mut output), "NON_UTF8_NAME_REPORTED");
    assert_eq!(take_line(&mut output), "VALID_EVENT_READY");
    fs::create_dir(watched.join("valid")).unwrap();
    send_line(&mut child);
    let result = finish_child(child, output);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_live_zero(&result);
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("VALID_EVENT_RECEIVED\n"), "{stdout}");
    assert!(!stdout.contains("WATCHER_INVALIDATED\n"), "{stdout}");
}
#[cfg(target_os = "linux")]
#[test]
fn watcher_accepts_nameless_attribute_event_for_watched_directory() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = Scratch::new();
    let watched_directory = scratch.0.join("watched");
    fs::create_dir(&watched_directory).unwrap();
    let source = r#"
Result<Unit,Text> watch(borrow Path path) {
    Watcher watcher = Watcher.open(path)?;
    Console.print("WATCH_READY\n");
    match (Console.readLine()) {
        Err(error) { return new Result<Unit,Text>.Err(error); }
        Ok(option) { drop(option); }
    }
    match (watcher.next(2000L)?) {
        None { println("ATTRIBUTE_EVENT_MISSING"); }
        Some(event) {
            borrow Text eventName = event.name();
            if (eventName.byteLength() == 0) {
                match (event.kind()) {
                    Created { println("UNEXPECTED_KIND"); }
                    Modified { println("UNEXPECTED_KIND"); }
                    AttributesChanged { println("SELF_ATTRIBUTES_CHANGED"); }
                    ClosedForWrite { println("UNEXPECTED_KIND"); }
                    Deleted { println("UNEXPECTED_KIND"); }
                    MovedFrom { println("UNEXPECTED_KIND"); }
                    MovedTo { println("UNEXPECTED_KIND"); }
                    MovedSelf { println("UNEXPECTED_KIND"); }
                    DeletedSelf { println("UNEXPECTED_KIND"); }
                    Ignored { println("UNEXPECTED_KIND"); }
                    Overflow { println("UNEXPECTED_KIND"); }
                }
            } else { println("ATTRIBUTE_EVENT_HAD_NAME"); }
        }
    }
    match (watcher.next(0L)?) {
        None { println("WATCH_REMAINS_VALID"); }
        Some(event) { println("UNEXPECTED_SECOND_EVENT"); }
    }
    watcher.close()?;
    return new Result<Unit,Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(pathText) {
            match (Path.fromText(pathText)) {
                Err(error) { println("PATH_ERROR"); }
                Ok(path) {
                    match (watch(path)) {
                        Err(error) { Console.printText(error); println(""); }
                        Ok(unit) { println("WATCH_DONE"); }
                    }
                }
            }
        }
    }
}
"#;
    let program = compile(source);
    let (mut child, output) = spawn_watcher(&program, &watched_directory);
    let current_mode = fs::metadata(&watched_directory).unwrap().permissions().mode() & 0o777;
    fs::set_permissions(
        &watched_directory,
        fs::Permissions::from_mode(current_mode ^ 0o001),
    )
    .unwrap();
    send_line(&mut child);
    let result = finish_child(child, output);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_live_zero(&result);
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("SELF_ATTRIBUTES_CHANGED\n"), "{stdout}");
    assert!(stdout.contains("WATCH_REMAINS_VALID\n"), "{stdout}");
    assert!(stdout.contains("WATCH_DONE\n"), "{stdout}");
}

#[test]
fn invalid_permission_modes_reject_before_filesystem_side_effects() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new();
    let existing = scratch.0.join("existing");
    let missing = scratch.0.join("missing");
    let directory = scratch.0.join("parent/child");
    fs::write(&existing, b"preserve this content").unwrap();
    fs::set_permissions(&existing, fs::Permissions::from_mode(0o600)).unwrap();
    let source = format!(r#"
Result<Unit,Text> exercise(int mode) {{
    Path existing = Path.fromText(new Text({existing:?}))?;
    Path missing = Path.fromText(new Text({missing:?}))?;
    Path directory = Path.fromText(new Text({directory:?}))?;
    match (Files.chmod(existing, mode)) {{ Err(error) {{ println(true); }} Ok(unit) {{ println(false); }} }}
    match (File.create(existing, mode)) {{ Err(error) {{ println(true); }} Ok(file) {{ println(false); }} }}
    match (File.create(missing, mode)) {{ Err(error) {{ println(true); }} Ok(file) {{ println(false); }} }}
    match (File.openAppend(missing, mode)) {{ Err(error) {{ println(true); }} Ok(file) {{ println(false); }} }}
    match (File.openReadWrite(missing, mode, true)) {{ Err(error) {{ println(true); }} Ok(file) {{ println(false); }} }}
    match (File.openReadWrite(existing, mode, false)) {{ Err(error) {{ println(true); }} Ok(file) {{ println(false); }} }}
    match (Files.createDirectory(directory, mode)) {{ Err(error) {{ println(true); }} Ok(unit) {{ println(false); }} }}
    match (Files.createDirectories(directory, mode)) {{ Err(error) {{ println(true); }} Ok(unit) {{ println(false); }} }}
    return new Result<Unit,Text>.Ok(());
}}
Result<Unit,Text> boundaries() {{
    Path existing = Path.fromText(new Text({existing:?}))?;
    Files.chmod(existing, 0)?;
    FileInfo zero = Files.metadata(existing)?;
    println(zero.mode() % 4096);
    Files.chmod(existing, 4095)?;
    FileInfo special = Files.metadata(existing)?;
    println(special.mode() % 4096);
    Files.chmod(existing, 384)?;
    return new Result<Unit,Text>.Ok(());
}}
void main() {{
    match (exercise(-1)) {{ Err(error) {{ println("unexpected error"); }} Ok(unit) {{ }} }}
    match (exercise(4096)) {{ Err(error) {{ println("unexpected error"); }} Ok(unit) {{ }} }}
    match (exercise(2147483647)) {{ Err(error) {{ println("unexpected error"); }} Ok(unit) {{ }} }}
    match (boundaries()) {{ Err(error) {{ println("unexpected boundary error"); }} Ok(unit) {{ }} }}
}}
"#, existing = existing.to_str().unwrap(), missing = missing.to_str().unwrap(),
        directory = directory.to_str().unwrap());
    let output = run_source(&source, &[]);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), format!("{}0\n4095\n", "true\n".repeat(24)));
    assert_eq!(fs::read(&existing).unwrap(), b"preserve this content");
    assert_eq!(fs::metadata(&existing).unwrap().permissions().mode() & 0o7777, 0o600);
    assert!(!missing.exists());
    assert!(!scratch.0.join("parent").exists());
}
