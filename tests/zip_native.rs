use nacre::{compile_to_c, native};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("nacre-zip-{}-{serial}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn compile(source: &str) -> native::NativeProgram {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("zip_native.ncr", source)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("{error}"))
}

fn assert_live_zero(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "native ownership report did not show zero live objects:\n{stderr}"
    );
}

fn run(program: &native::NativeProgram, args: &[&Path]) -> Output {
    let output = Command::new(program.path()).args(args).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_live_zero(&output);
    output
}

const WRITER_SOURCE: &str = r#"
Result<Unit, Text> require(boolean condition, String message) {
    if (!condition) { return new Result<Unit, Text>.Err(new Text(message)); }
    return new Result<Unit, Text>.Ok(());
}
Result<Path, Text> argumentPath(int index) {
    Text text = Args.get(index)?;
    return Path.fromText(text);
}
Result<Unit, Text> writeEntries(borrow Path path, int level) {
    ZipWriter writer = ZipWriter.create(path, level)?;
    ByteBuffer payload = ByteBuffer.withCapacity(0L)?;
    payload.append((byte)65);
    payload.append((byte)0);
    payload.append((byte)255);
    payload.append((byte)66);
    Text filename = new Text("café-λ.bin");
    Text emptyInvalid = new Text("");
    match (writer.add(emptyInvalid, payload)) {
        Err(error) { println("writer rejects empty entry name"); }
        Ok(unit) { return new Result<Unit, Text>.Err(new Text("writer accepted empty entry name")); }
    }
    Text nulInvalid = new Text("bad\0name");
    match (writer.add(nulInvalid, payload)) {
        Err(error) { println("writer rejects NUL entry name"); }
        Ok(unit) { return new Result<Unit, Text>.Err(new Text("writer accepted NUL entry name")); }
    }
    Text trailingSlash = new Text("payload/");
    match (writer.add(trailingSlash, payload)) {
        Err(error) { println("writer rejects trailing slash file name"); }
        Ok(unit) { return new Result<Unit, Text>.Err(new Text("writer accepted trailing slash file name")); }
    }
    writer.add(filename, payload)?;
    ByteBuffer unread = ByteBuffer.withCapacity(0L)?;
    unread.append((byte)115);
    unread.append((byte)107);
    unread.append((byte)105);
    unread.append((byte)112);
    Text unreadName = new Text("unread.bin");
    writer.add(unreadName, unread)?;
    Text directory = new Text("folder/");
    writer.addDirectory(directory)?;
    ByteBuffer empty = ByteBuffer.withCapacity(0L)?;
    Text emptyName = new Text("empty");
    writer.add(emptyName, empty)?;
    ByteBuffer safe = ByteBuffer.withCapacity(0L)?;
    safe.append((byte)115);
    safe.append((byte)97);
    safe.append((byte)102);
    safe.append((byte)101);
    Text traversalName = new Text("../kept.txt");
    writer.add(traversalName, safe)?;
    writer.close()?;
    match (writer.add(filename, payload)) {
        Err(error) { println("closed writer rejects add"); }
        Ok(unit) { return new Result<Unit, Text>.Err(new Text("closed writer accepted add")); }
    }
    match (writer.close()) {
        Err(error) { println("closed writer rejects close"); }
        Ok(unit) { return new Result<Unit, Text>.Err(new Text("closed writer accepted close")); }
    }
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> dropOpenWriter(borrow Path path) {
    ZipWriter writer = ZipWriter.create(path, 0)?;
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> writeEmpty(borrow Path path) {
    ZipWriter writer = ZipWriter.create(path, 0)?;
    writer.close()?;
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> readWritten(borrow Path path) {
    ZipReader reader = ZipReader.open(path)?;
    boolean done = false;
    int index = 0;
    while (!done) {
        match (reader.next()?) {
            None { done = true; }
            Some(entry) {
                if (index == 0) {
                    require(entry.name().equalsString("café-λ.bin") && entry.size() == 4L && !entry.isDirectory(), "written file metadata")?;
                    ByteBuffer bytes = reader.readEntry(4L)?;
                    require(bytes.length() == 4L && bytes.get(0L) == (byte)65 && bytes.get(1L) == (byte)0
                        && bytes.get(2L) == (byte)255 && bytes.get(3L) == (byte)66, "written binary payload")?;
                    match (reader.readEntry(4L)) {
                        Err(error) { println("reader rejects repeated entry read"); }
                        Ok(repeated) { return new Result<Unit, Text>.Err(new Text("reader repeated entry read")); }
                    }
                } else if (index == 1) {
                    require(entry.name().equalsString("unread.bin"), "unread entry metadata")?;
                } else if (index == 2) {
                    require(entry.name().equalsString("folder/") && entry.isDirectory() && entry.size() == 0L, "directory metadata")?;
                    reader.skipEntry()?;
                } else if (index == 3) {
                    require(entry.name().equalsString("empty") && !entry.isDirectory() && entry.size() == 0L, "empty file metadata")?;
                    ByteBuffer bytes = reader.readEntry(0L)?;
                    require(bytes.length() == 0L, "empty file data")?;
                } else if (index == 4) {
                    require(entry.name().equalsString("../kept.txt"), "preserved traversal-shaped name")?;
                    ByteBuffer bytes = reader.readEntry(4L)?;
                    require(bytes.length() == 4L && bytes.get(0L) == (byte)115, "safe entry data")?;
                }
                index = index + 1;
            }
        }
    }
    require(index == 5, "written entry order")?;
    reader.close()?;
    match (reader.next()) {
        Err(error) { println("closed reader rejects next"); }
        Ok(option) { return new Result<Unit, Text>.Err(new Text("closed reader accepted next")); }
    }
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> writerExercise() {
    Path stored = argumentPath(0)?;
    Path deflated = argumentPath(1)?;
    Path empty = argumentPath(2)?;
    Path dropped = argumentPath(3)?;
    dropOpenWriter(dropped)?;
    match (ZipWriter.create(stored, 10)) {
        Err(error) { println("writer rejects invalid level"); }
        Ok(writer) { return new Result<Unit, Text>.Err(new Text("writer accepted invalid level")); }
    }
    writeEntries(stored, 0)?;
    writeEntries(deflated, 6)?;
    writeEmpty(empty)?;
    readWritten(stored)?;
    readWritten(deflated)?;
    ZipReader emptyReader = ZipReader.open(empty)?;
    match (emptyReader.next()?) {
        None { println("empty archive"); }
        Some(entry) { return new Result<Unit, Text>.Err(new Text("empty archive had an entry")); }
    }
    emptyReader.close()?;
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (writerExercise()) {
        Ok(unit) { println("ZIP_WRITER_OK"); }
        Err(error) { Console.printText(error); Console.print("\n"); }
    }
}
"#;

const READER_SOURCE: &str = r#"
Result<Unit, Text> require(boolean condition, String message) {
    if (!condition) { return new Result<Unit, Text>.Err(new Text(message)); }
    return new Result<Unit, Text>.Ok(());
}
Result<Path, Text> argumentPath(int index) {
    Text text = Args.get(index)?;
    return Path.fromText(text);
}
Result<ByteBuffer, Text> readFile(borrow Path path) {
    File file = File.openRead(path)?;
    ByteBuffer bytes = ByteBuffer.withCapacity(0L)?;
    long count = 1L;
    while (count > 0L) { count = file.readSome(bytes, 65536L)?; }
    file.close()?;
    return new Result<ByteBuffer, Text>.Ok(bytes);
}
Result<Unit, Text> inspect(borrow mut ZipReader reader) {
    boolean done = false;
    int index = 0;
    while (!done) {
        match (reader.next()?) {
            None { done = true; }
            Some(entry) {
                if (index == 0) {
                    require(entry.name().equalsString("stored.bin") && entry.size() == 4L && !entry.isDirectory(), "stored entry metadata")?;
                    match (reader.readEntry(-1L)) {
                        Err(error) { println("reader rejects negative limit"); }
                        Ok(bytes) { return new Result<Unit, Text>.Err(new Text("reader accepted negative limit")); }
                    }
                    ByteBuffer bytes = reader.readEntry(4L)?;
                    require(bytes.length() == 4L && bytes.get(0L) == (byte)65 && bytes.get(1L) == (byte)0
                        && bytes.get(2L) == (byte)255 && bytes.get(3L) == (byte)66, "stored payload")?;
                    match (reader.readEntry(4L)) {
                        Err(error) { println("reader permits one read"); }
                        Ok(repeated) { return new Result<Unit, Text>.Err(new Text("reader permitted repeated read")); }
                    }
                } else if (index == 1) {
                    require(entry.name().equalsString("deflated.txt") && entry.size() == 107L, "deflated entry metadata")?;
                    ByteBuffer bytes = reader.readEntry(107L)?;
                    require(bytes.length() == 107L && bytes.get(0L) == (byte)102 && bytes.get(10L) == (byte)45
                        && bytes.get(11L) == (byte)90 && bytes.get(106L) == (byte)90, "deflated payload")?;
                } else if (index == 2) {
                    require(entry.name().equalsString("empty-dir/") && entry.isDirectory() && entry.size() == 0L, "directory metadata")?;
                    reader.skipEntry()?;
                } else if (index == 3) {
                    require(entry.name().equalsString("empty") && !entry.isDirectory() && entry.size() == 0L, "empty file metadata")?;
                    ByteBuffer bytes = reader.readEntry(0L)?;
                    require(bytes.length() == 0L, "empty file data")?;
                } else if (index == 4) {
                    require(entry.name().equalsString("../kept.txt") && entry.size() == 13L, "preserved unsafe-looking name")?;
                    ByteBuffer bytes = reader.readEntry(13L)?;
                    require(bytes.length() == 13L, "traversal-shaped entry data")?;
                } else if (index == 5) {
                    require(entry.name().equalsString("bomb.bin") && entry.size() == 100000L, "bomb declared size")?;
                    match (reader.readEntry(32L)) {
                        Err(error) { println("reader bounds actual decompressed output"); }
                        Ok(bytes) { return new Result<Unit, Text>.Err(new Text("reader exceeded decompression bound")); }
                    }
                }
                index = index + 1;
            }
        }
    }
    require(index == 6, "fixture entry order")?;
    reader.close()?;
    match (reader.next()) {
        Err(error) { println("closed reader rejects next"); }
        Ok(option) { return new Result<Unit, Text>.Err(new Text("closed reader accepted next")); }
    }
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> dropReaderWithoutClose(borrow Path path) {
    ZipReader reader = ZipReader.open(path)?;
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> dropMemoryReaderWithoutClose(borrow Path path) {
    ByteBuffer source = readFile(path)?;
    ZipReader reader = ZipReader.fromBytes(source)?;
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> inspectFixture() {
    Path path = argumentPath(0)?;
    ZipReader fileReader = ZipReader.open(path)?;
    inspect(fileReader)?;
    ByteBuffer source = readFile(path)?;
    ZipReader memoryReader = ZipReader.fromBytes(source)?;
    source.clear();
    inspect(memoryReader)?;
    dropReaderWithoutClose(path)?;
    dropMemoryReaderWithoutClose(path)?;
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (inspectFixture()) {
        Err(error) { Console.printText(error); Console.print("\n"); }
        Ok(unit) { println("ZIP_READER_OK"); }
    }
}
"#;

const DAMAGE_SOURCE: &str = r#"
Result<Unit, Text> require(boolean condition, String message) {
    if (!condition) { return new Result<Unit, Text>.Err(new Text(message)); }
    return new Result<Unit, Text>.Ok(());
}
Result<Path, Text> argumentPath(int index) {
    Text text = Args.get(index)?;
    return Path.fromText(text);
}
Result<boolean, Text> rejectsDamagedArchive(borrow Path path) {
    boolean rejected = false;
    match (ZipReader.open(path)) {
        Err(error) { rejected = true; }
        Ok(reader) {
            match (reader.next()) {
                Err(error) { rejected = true; }
                Ok(option) {
                    match (option) {
                        None { rejected = true; }
                        Some(entry) {
                            match (reader.readEntry(16L)) {
                                Err(error) { rejected = true; }
                                Ok(bytes) { rejected = false; }
                            }
                        }
                    }
                }
            }
            match (reader.close()) {
                Err(error) { rejected = true; }
                Ok(unit) {}
            }
        }
    }
    return new Result<boolean, Text>.Ok(rejected);
}
Result<Unit, Text> damagedExercise() {
    Path truncated = argumentPath(0)?;
    Path corrupt = argumentPath(1)?;
    Path truncatedDirectory = argumentPath(2)?;
    Path missingUnicode = argumentPath(3)?;
    require(rejectsDamagedArchive(truncated)?, "truncated archive was accepted")?;
    require(rejectsDamagedArchive(corrupt)?, "corrupt CRC archive was accepted")?;
    require(rejectsDamagedArchive(truncatedDirectory)?, "archive with truncated central directory was accepted")?;
    match (ZipReader.open(missingUnicode)) {
        Err(error) { println("ZIP_ERROR_UTF8_OK"); }
        Ok(reader) {
            reader.close()?;
            return new Result<Unit, Text>.Err(new Text("missing archive path was accepted"));
        }
    }
    println("ZIP_DAMAGED_OK");
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (damagedExercise()) {
        Err(error) { Console.printText(error); Console.print("\n"); }
        Ok(unit) { println("ZIP_DAMAGED_OK"); }
    }
}
"#;

// Generated independently with Python's standard-library zipfile implementation.
const ZIP_FIXTURE_HEX: &str = "504b030414000000000073534d5d8a3d1a0904000000040000000a00000073746f7265642e62696e4100ff42504b030414000000080073534d5d505646c0100000006b0000000c0000006465666c617465642e7478744bcbac28292d4ad53db75b378ac60000504b030414000000000073534d5d0000000000000000000000000a000000656d7074792d6469722f504b030414000000080073534d5d00000000020000000000000005000000656d7074790300504b030414000000000073534d5db7abe11b0d0000000d0000000b0000002e2e2f6b6570742e7478746e6f7420657874726163746564504b030414000000080073534d5d7fadbe0a73000000a086010008000000626f6d622e62696eedc13101000000c2a08ceb5fc61a1e4001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000af06504b0102140314000000000073534d5d8a3d1a0904000000040000000a000000000000000000000080010000000073746f7265642e62696e504b0102140314000000080073534d5d505646c0100000006b0000000c000000000000000000000080012c0000006465666c617465642e747874504b0102140314000000000073534d5d0000000000000000000000000a0000000000000000001000fd4166000000656d7074792d6469722f504b0102140314000000080073534d5d00000000020000000000000005000000000000000000000080018e000000656d707479504b0102140314000000000073534d5db7abe11b0d0000000d0000000b00000000000000000000008001b30000002e2e2f6b6570742e747874504b0102140314000000080073534d5d7fadbe0a73000000a08601000800000000000000000000008001e9000000626f6d622e62696e504b050600000000060006004c010000820100000000";

fn fixture_bytes() -> Vec<u8> {
    ZIP_FIXTURE_HEX.as_bytes().chunks_exact(2).map(|pair| {
        let digit = |value: u8| match value {
            b'0'..=b'9' => value - b'0',
            b'a'..=b'f' => value - b'a' + 10,
            _ => panic!("invalid fixture hex"),
        };
        digit(pair[0]) * 16 + digit(pair[1])
    }).collect()
}

#[test]
fn writer_produces_store_deflate_directory_empty_and_sequential_entries() {
    let scratch = Scratch::new();
    let stored = scratch.path("stored.zip");
    let deflated = scratch.path("deflated.zip");
    let empty = scratch.path("empty.zip");
    let dropped = scratch.path("dropped.zip");
    let program = compile(WRITER_SOURCE);
    let output = run(&program, &[&stored, &deflated, &empty, &dropped]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ZIP_WRITER_OK"), "{stdout}");
    assert!(stdout.contains("writer rejects empty entry name"), "{stdout}");
    assert!(stdout.contains("writer rejects NUL entry name"), "{stdout}");
    assert!(stdout.contains("writer rejects trailing slash file name"), "{stdout}");
    assert!(stdout.contains("closed writer rejects add"), "{stdout}");
    assert!(stdout.contains("closed writer rejects close"), "{stdout}");
    assert!(stdout.contains("closed reader rejects next"), "{stdout}");
    assert!(stdout.contains("empty archive"), "{stdout}");

    let check = r#"import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    entries = archive.infolist()
    assert [entry.filename for entry in entries] == ['café-λ.bin', 'unread.bin', 'folder/', 'empty', '../kept.txt']
    assert archive.getinfo('café-λ.bin').compress_type == int(sys.argv[2])
    assert archive.read('café-λ.bin') == bytes([65, 0, 255, 66])
    assert archive.read('unread.bin') == b'skip'
    assert archive.read('empty') == b''
    assert archive.read('../kept.txt') == b'safe'
    assert entries[2].is_dir()
with zipfile.ZipFile(sys.argv[3]) as archive:
    assert archive.namelist() == []
"#;
    for (archive_path, expected_method) in [(&stored, "0"), (&deflated, "8")] {
        let python = Command::new("python3")
            .arg("-c").arg(check)
            .arg(archive_path).arg(expected_method).arg(&empty)
            .output().unwrap();
        assert!(python.status.success(), "{}", String::from_utf8_lossy(&python.stderr));
    }
}

#[test]
fn reader_handles_independent_store_deflate_directories_empty_entries_and_bounds() {
    let scratch = Scratch::new();
    let fixture = scratch.path("independent.zip");
    fs::write(&fixture, fixture_bytes()).unwrap();
    let program = compile(READER_SOURCE);
    let output = run(&program, &[&fixture]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ZIP_READER_OK"), "{stdout}");
    assert!(stdout.contains("reader rejects negative limit"), "{stdout}");
    assert!(stdout.contains("reader bounds actual decompressed output"), "{stdout}");
    assert!(stdout.contains("closed reader rejects next"), "{stdout}");
}

#[test]
fn reader_rejects_truncated_archives_and_preserves_utf8_errors() {
    let scratch = Scratch::new();
    let fixture = fixture_bytes();
    let truncated = scratch.path("truncated.zip");
    fs::write(&truncated, &fixture[..43]).unwrap();
    let corrupt = scratch.path("corrupt-crc.zip");
    let mut corrupt_bytes = fixture.clone();
    corrupt_bytes[40] ^= 1;
    fs::write(&corrupt, corrupt_bytes).unwrap();
    let directory_signature = fixture.windows(4)
        .position(|window| window == b"PK\x01\x02")
        .expect("independent fixture has a central directory");
    let truncated_directory = scratch.path("truncated-directory.zip");
    fs::write(&truncated_directory, &fixture[..directory_signature + 4]).unwrap();
    let mut missing_unicode = scratch.path("missing");
    // Keep the libarchive diagnostic truncation point inside a lambda scalar.
    if (511 - 32 - (missing_unicode.as_os_str().len() + 1)) % 3 == 0 {
        missing_unicode.push("x");
    }
    let component = "λ".repeat(80);
    for _ in 0..3 { missing_unicode.push(&component); }
    let program = compile(DAMAGE_SOURCE);
    let output = run(&program, &[&truncated, &corrupt, &truncated_directory, &missing_unicode]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ZIP_DAMAGED_OK"), "{stdout}");
    assert!(stdout.contains("ZIP_ERROR_UTF8_OK"), "{stdout}");
