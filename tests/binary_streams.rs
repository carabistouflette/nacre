mod support;

use nacre::{compile_to_c, native};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nacre-binary-streams-{}-{serial}",
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

fn run(program: &native::NativeProgram, args: &[&OsStr]) -> Output {
    let output = Command::new(program.path()).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "{stderr}"
    );
    output
}

const BINARY_SOURCE: &str = r#"
Result<Unit, Text> exercise() {
    ByteBuffer littleBytes = ByteBuffer.withCapacity(0L)?;
    littleBytes.append((byte)254);
    littleBytes.append((byte)0);
    littleBytes.append((byte)128);
    littleBytes.append((byte)120);
    littleBytes.append((byte)86);
    littleBytes.append((byte)52);
    littleBytes.append((byte)242);
    littleBytes.append((byte)0);
    littleBytes.append((byte)0);
    littleBytes.append((byte)0);
    littleBytes.append((byte)0);
    littleBytes.append((byte)0);
    littleBytes.append((byte)0);
    littleBytes.append((byte)0);
    littleBytes.append((byte)128);
    BinaryReader little = new BinaryReader(littleBytes, true);
    println(little.readByte()?);
    println(little.readInt16()?);
    println(little.readInt32()?);
    println(little.readInt64()?);
    println(little.position() == 15L && little.remaining() == 0L);

    ByteBuffer bigBytes = ByteBuffer.withCapacity(0L)?;
    bigBytes.append((byte)255);
    bigBytes.append((byte)254);
    bigBytes.append((byte)128);
    bigBytes.append((byte)0);
    bigBytes.append((byte)0);
    bigBytes.append((byte)1);
    bigBytes.append((byte)128);
    bigBytes.append((byte)0);
    bigBytes.append((byte)0);
    bigBytes.append((byte)0);
    bigBytes.append((byte)0);
    bigBytes.append((byte)0);
    bigBytes.append((byte)0);
    bigBytes.append((byte)1);
    BinaryReader big = new BinaryReader(bigBytes, false);
    println(big.readInt16()?);
    println(big.readInt32()?);
    println(big.readInt64()?);
    big.seek(0L)?;
    println(big.position() == 0L && big.remaining() == 14L);

    BinaryWriter bigWriter = new BinaryWriter(0L, false);
    bigWriter.writeInt16(-2)?;
    bigWriter.writeInt32(-2147483647)?;
    bigWriter.writeInt64(-9223372036854775807L)?;
    ByteBuffer bigOutput = BinaryWriter.finish(move(bigWriter));
    println(bigOutput.length() == 14L && bigOutput.get(0L) == (byte)255
        && bigOutput.get(1L) == (byte)254 && bigOutput.get(2L) == (byte)128
        && bigOutput.get(3L) == (byte)0 && bigOutput.get(4L) == (byte)0
        && bigOutput.get(5L) == (byte)1 && bigOutput.get(6L) == (byte)128
        && bigOutput.get(7L) == (byte)0 && bigOutput.get(8L) == (byte)0
        && bigOutput.get(9L) == (byte)0 && bigOutput.get(10L) == (byte)0
        && bigOutput.get(11L) == (byte)0 && bigOutput.get(12L) == (byte)0
        && bigOutput.get(13L) == (byte)1);
    ByteBuffer textBytes = ByteBuffer.withCapacity(0L)?;
    textBytes.append((byte)65);
    textBytes.append((byte)0);
    textBytes.append((byte)195);
    textBytes.append((byte)169);
    BinaryReader textReader = new BinaryReader(textBytes, true);
    Text decoded = textReader.readUtf8(4L)?;
    println(decoded.byteLength() == 4 && decoded.byteAt(0) == 65
        && decoded.byteAt(1) == 0 && decoded.byteAt(2) == 195 && decoded.byteAt(3) == 169);

    ByteBuffer sliceBytes = ByteBuffer.withCapacity(0L)?;
    sliceBytes.append((byte)11);
    sliceBytes.append((byte)22);
    sliceBytes.append((byte)33);
    BinaryReader sliceReader = new BinaryReader(sliceBytes, true);
    ByteBuffer slice = sliceReader.readBytes(2L)?;
    println(slice.length() == 2L && slice.get(0L) == 11 && slice.get(1L) == 22
        && sliceReader.position() == 2L && sliceReader.readByte()? == (byte)33);

    ByteBuffer shortBytes = ByteBuffer.withCapacity(0L)?;
    shortBytes.append((byte)77);
    BinaryReader shortReader = new BinaryReader(shortBytes, true);
    match (shortReader.readInt32()) {
        Err(error) { println("incomplete field rejected"); drop(error); }
        Ok(value) { println("incomplete field accepted"); }
    }
    println(shortReader.position() == 0L);
    println(shortReader.readByte()?);

    ByteBuffer invalidBytes = ByteBuffer.withCapacity(0L)?;
    invalidBytes.append((byte)192);
    invalidBytes.append((byte)128);
    BinaryReader invalidReader = new BinaryReader(invalidBytes, true);
    match (invalidReader.readUtf8(2L)) {
        Err(error) { println("invalid UTF-8 rejected"); drop(error); }
        Ok(value) { println("invalid UTF-8 accepted"); }
    }
    println(invalidReader.position() == 0L);
    println(invalidReader.readByte()?);

    BinaryReader seekReader = new BinaryReader(ByteBuffer.withCapacity(0L)?, true);
    match (seekReader.seek(-1L)) {
        Err(error) { println("invalid seek rejected"); drop(error); }
        Ok(unit) { println("invalid seek accepted"); }
    }
    println(seekReader.position() == 0L);

    ByteBuffer floatBytes = ByteBuffer.withCapacity(0L)?;
    floatBytes.append((byte)0);
    floatBytes.append((byte)0);
    floatBytes.append((byte)0);
    floatBytes.append((byte)0);
    floatBytes.append((byte)0);
    floatBytes.append((byte)0);
    floatBytes.append((byte)0);
    floatBytes.append((byte)128);
    floatBytes.append((byte)52);
    floatBytes.append((byte)18);
    floatBytes.append((byte)0);
    floatBytes.append((byte)0);
    floatBytes.append((byte)0);
    floatBytes.append((byte)0);
    floatBytes.append((byte)248);
    floatBytes.append((byte)127);
    BinaryReader floatReader = new BinaryReader(floatBytes, true);
    double negativeZero = floatReader.readDouble()?;
    double payloadNaN = floatReader.readDouble()?;
    BinaryWriter floatWriter = new BinaryWriter(0L, true);
    floatWriter.writeDouble(negativeZero)?;
    floatWriter.writeDouble(payloadNaN)?;
    ByteBuffer floatRoundTrip = BinaryWriter.finish(move(floatWriter));
    boolean negativeZeroMatches = true;
    int bitIndex = 0;
    while (bitIndex < 8) {
        byte expected = (byte)0;
        if (bitIndex == 7) { expected = (byte)128; }
        if (floatRoundTrip.get((long)bitIndex) != expected) { negativeZeroMatches = false; }
        bitIndex = bitIndex + 1;
    }
    println(negativeZeroMatches);
    boolean nanPayloadMatches = true;
    bitIndex = 0;
    while (bitIndex < 8) {
        byte expected = (byte)0;
        if (bitIndex == 0) { expected = (byte)52; }
        if (bitIndex == 1) { expected = (byte)18; }
        if (bitIndex == 6) { expected = (byte)248; }
        if (bitIndex == 7) { expected = (byte)127; }
        if (floatRoundTrip.get((long)(bitIndex + 8)) != expected) { nanPayloadMatches = false; }
        bitIndex = bitIndex + 1;
    }
    println(nanPayloadMatches);

    BinaryWriter writer = new BinaryWriter(1L, true);
    match (writer.writeInt16(32768)) {
        Err(error) { println("int16 range rejected"); drop(error); }
        Ok(unit) { println("int16 range accepted"); }
    }
    println(writer.position() == 0L);
    writer.writeByte((byte)254)?;
    writer.writeInt16(-32768)?;
    writer.writeInt32(-231451016)?;
    writer.writeInt64(-9223372036854775807L)?;
    writer.writeDouble(-0.0)?;
    ByteBuffer payload = ByteBuffer.withCapacity(0L)?;
    payload.append((byte)1);
    payload.append((byte)2);
    writer.writeBytes(payload)?;
    payload.set(0L, (byte)99);
    Text suffix = new Text("xy");
    writer.writeUtf8(suffix)?;
    {
        borrow ByteBuffer view = writer.bytes();
        println(view.length() == 27L);
    }
    ByteBuffer finished = BinaryWriter.finish(move(writer));
    println(finished.length() == 27L);
    println(finished.get(0L) == (byte)254 && finished.get(1L) == (byte)0
        && finished.get(2L) == (byte)128);
    println(finished.get(3L) == (byte)120 && finished.get(4L) == (byte)86
        && finished.get(5L) == (byte)52 && finished.get(6L) == (byte)242);
    println(finished.get(7L) == (byte)1 && finished.get(14L) == (byte)128);
    println(finished.get(15L) == (byte)0 && finished.get(22L) == (byte)128);
    println(finished.get(23L) == (byte)1 && finished.get(24L) == (byte)2
        && finished.get(25L) == (byte)120 && finished.get(26L) == (byte)121);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;

#[test]
fn binary_reader_writer_known_bytes_cursor_and_finish_ownership() {
    let program = compile(BINARY_SOURCE);
    let output = run(&program, &[]);
    assert_eq!(
        output.stdout,
        b"254\n-32768\n-231451016\n-9223372036854775808\ntrue\n-2\n-2147483647\n-9223372036854775807\ntrue\ntrue\ntrue\ntrue\nincomplete field rejected\ntrue\n77\ninvalid UTF-8 rejected\ntrue\n192\ninvalid seek rejected\ntrue\ntrue\ntrue\nint16 range rejected\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n"
    );
}

const FLOAT32_SOURCE: &str = r#"
Result<Unit, Text> exercise() {
    ByteBuffer little = ByteBuffer.withCapacity(0L)?;
    little.append((byte)0);
    little.append((byte)0);
    little.append((byte)192);
    little.append((byte)63);
    little.append((byte)0);
    little.append((byte)0);
    little.append((byte)0);
    little.append((byte)128);
    BinaryReader littleReader = new BinaryReader(little, true);
    double oneAndHalf = littleReader.readFloat32()?;
    double negativeZero = littleReader.readFloat32()?;
    println(oneAndHalf == 1.5);
    println(negativeZero == 0.0);

    BinaryWriter writer = new BinaryWriter(0L, true);
    writer.writeFloat32(oneAndHalf)?;
    writer.writeFloat32(negativeZero)?;
    match (writer.writeFloat32(1e300)) {
        Err(error) { println("float32 range rejected"); drop(error); }
        Ok(unit) { println("float32 range accepted"); }
    }
    println(writer.position() == 8L);
    ByteBuffer output = BinaryWriter.finish(move(writer));
    println(output.length() == 8L
        && output.get(0L) == (byte)0 && output.get(1L) == (byte)0
        && output.get(2L) == (byte)192 && output.get(3L) == (byte)63
        && output.get(4L) == (byte)0 && output.get(5L) == (byte)0
        && output.get(6L) == (byte)0 && output.get(7L) == (byte)128);

    ByteBuffer big = ByteBuffer.withCapacity(0L)?;
    big.append((byte)63);
    big.append((byte)192);
    big.append((byte)0);
    big.append((byte)0);
    BinaryReader bigReader = new BinaryReader(big, false);
    println(bigReader.readFloat32()? == 1.5);

    ByteBuffer shortBytes = ByteBuffer.withCapacity(0L)?;
    shortBytes.append((byte)0);
    shortBytes.append((byte)0);
    shortBytes.append((byte)192);
    BinaryReader shortReader = new BinaryReader(shortBytes, true);
    match (shortReader.readFloat32()) {
        Err(error) { println("incomplete float32 rejected"); drop(error); }
        Ok(value) { println("incomplete float32 accepted"); }
    }
    println(shortReader.position() == 0L);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;

#[test]
fn binary32_reader_writer_endian_narrowing_and_atomic_range_failure() {
    let program = compile(FLOAT32_SOURCE);
    let output = run(&program, &[]);
    assert_eq!(
        output.stdout,
        b"true\ntrue\nfloat32 range rejected\ntrue\ntrue\ntrue\nincomplete float32 rejected\ntrue\n"
    );
}

const STREAMS_SOURCE: &str = r#"
class ThreeByteFileSink implements ByteSink {
    private File target;
    public ThreeByteFileSink(File target) { this.target = target; }
    public Result<long, Text> write(borrow ByteBuffer source, long offset, long maxBytes) {
        long amount = maxBytes;
        if (amount > 3L) { amount = 3L; }
        return this.target.write(source, offset, amount);
    }
    public Result<Unit, Text> flush() { return this.target.flush(); }
    public Result<Unit, Text> close() { return this.target.close(); }
}

Result<Unit, Text> exercise() {
    Text sourceName = Args.get(0)?;
    Text copyName = Args.get(1)?;
    Text shortName = Args.get(2)?;
    Text writeAllName = Args.get(3)?;
    Text emptyName = Args.get(4)?;
    Path sourcePath = Path.fromText(sourceName)?;
    Path copyPath = Path.fromText(copyName)?;
    Path shortPath = Path.fromText(shortName)?;
    Path writeAllPath = Path.fromText(writeAllName)?;
    Path emptyPath = Path.fromText(emptyName)?;

    ByteBuffer payload = ByteBuffer.withCapacity(0L)?;
    int index = 0;
    while (index < 100) {
        payload.append((byte)index);
        index = index + 1;
    }
    File initial = File.create(sourcePath, 384)?;
    initial.writeAll(payload)?;
    initial.close()?;
    File emptyCreated = File.create(emptyPath, 384)?;
    emptyCreated.close()?;

    File input = File.openRead(sourcePath)?;
    File output = File.create(copyPath, 384)?;
    long copied = 0L;
    match (Streams.copy(input, output, 100L, 7L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { copied = value; }
    }
    input.close()?;
    output.close()?;
    println(copied == 100L);

    File copiedFile = File.openRead(copyPath)?;
    ByteBuffer copiedBytes;
    match (Streams.readAll(copiedFile, 100L, 9L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { copiedBytes = value; }
    }
    copiedFile.close()?;
    boolean contentsMatch = copiedBytes.length() == 100L;
    index = 0;
    while (index < 100) {
        if (copiedBytes.get((long)index) != (byte)index) { contentsMatch = false; }
        index = index + 1;
    }
    println(contentsMatch);

    File boundedInput = File.openRead(sourcePath)?;
    File boundedOutput = File.create(shortPath, 384)?;
    long boundedCount = 0L;
    match (Streams.copy(boundedInput, boundedOutput, 13L, 5L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { boundedCount = value; }
    }
    boundedInput.close()?;
    boundedOutput.close()?;
    println(boundedCount == 13L);
    File boundedFile = File.openRead(shortPath)?;
    ByteBuffer boundedBytes;
    match (Streams.readAll(boundedFile, 100L, 4L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { boundedBytes = value; }
    }
    boundedFile.close()?;
    println(boundedBytes.length() == 13L && boundedBytes.get(0L) == (byte)0
        && boundedBytes.get(12L) == (byte)12);

    File exactBoundFile = File.openRead(shortPath)?;
    ByteBuffer exactBoundBytes;
    match (Streams.readAll(exactBoundFile, 13L, 4L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { exactBoundBytes = value; }
    }
    exactBoundFile.close()?;
    println(exactBoundBytes.length() == 13L && exactBoundBytes.get(0L) == (byte)0
        && exactBoundBytes.get(12L) == (byte)12);

    File shortOutput = File.create(writeAllPath, 384)?;
    ThreeByteFileSink shortSink = new ThreeByteFileSink(shortOutput);
    match (Streams.writeAll(shortSink, payload)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(unit) {}
    }
    shortSink.close()?;
    File allFile = File.openRead(writeAllPath)?;
    ByteBuffer allBytes;
    match (Streams.readAll(allFile, 100L, 8L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { allBytes = value; }
    }
    allFile.close()?;
    boolean allMatch = allBytes.length() == 100L;
    index = 0;
    while (index < 100) {
        if (allBytes.get((long)index) != (byte)index) { allMatch = false; }
        index = index + 1;
    }
    println(allMatch);

    File prefixInput = File.openRead(sourcePath)?;
    ByteBuffer prefix;
    match (Streams.readUpTo(prefixInput, 13L, 4L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { prefix = value; }
    }
    ByteBuffer afterPrefix = ByteBuffer.withCapacity(0L)?;
    long prefixCursorCount = prefixInput.read(afterPrefix, 1L)?;
    prefixInput.close()?;
    println(prefix.length() == 13L && prefix.get(0L) == (byte)0 && prefix.get(12L) == (byte)12
        && prefixCursorCount == 1L && afterPrefix.get(0L) == (byte)13);
    File emptyInput = File.openRead(emptyPath)?;
    ByteBuffer emptyRead;
    match (Streams.readAll(emptyInput, 0L, 4L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { emptyRead = value; }
    }
    emptyInput.close()?;
    println(emptyRead.length() == 0L);

    File zeroLimitInput = File.openRead(sourcePath)?;
    boolean zeroLimitErrorRetained = false;
    match (Streams.readAll(zeroLimitInput, 0L, 4L)) {
        Err(error) {
            long sourceBytesRead = error.sourceBytesRead();
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            match (retained) {
                None {}
                Some(bytes) {
                    zeroLimitErrorRetained = sourceBytesRead == 1L && bytes.length() == 1L
                        && bytes.get(0L) == (byte)0;
                }
            }
        }
        Ok(value) { drop(value); }
    }
    ByteBuffer afterZeroProbe = ByteBuffer.withCapacity(0L)?;
    long zeroProbeCount = zeroLimitInput.read(afterZeroProbe, 1L)?;
    zeroLimitInput.close()?;
    println(zeroLimitErrorRetained && zeroProbeCount == 1L && afterZeroProbe.get(0L) == (byte)1);

    File excessInput = File.openRead(sourcePath)?;
    boolean excessErrorRetainedProbe = false;
    match (Streams.readAll(excessInput, 13L, 4L)) {
        Err(error) {
            long sourceBytesRead = error.sourceBytesRead();
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            match (retained) {
                None {}
                Some(bytes) {
                    excessErrorRetainedProbe = sourceBytesRead == 14L && bytes.length() == 14L
                        && bytes.get(12L) == (byte)12 && bytes.get(13L) == (byte)13;
                }
            }
        }
        Ok(value) { drop(value); }
    }
    ByteBuffer afterLimitProbe = ByteBuffer.withCapacity(0L)?;
    long limitProbeCount = excessInput.read(afterLimitProbe, 1L)?;
    excessInput.close()?;
    println(excessErrorRetainedProbe && limitProbeCount == 1L && afterLimitProbe.get(0L) == (byte)14);

    File shortCopyInput = File.openRead(sourcePath)?;
    File shortCopyFile = File.create(copyPath, 384)?;
    ThreeByteFileSink shortCopySink = new ThreeByteFileSink(shortCopyFile);
    long shortCopied = 0L;
    match (Streams.copy(shortCopyInput, shortCopySink, 100L, 11L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { shortCopied = value; }
    }
    shortCopyInput.close()?;
    shortCopySink.close()?;
    println(shortCopied == 100L);
    File shortCopiedFile = File.openRead(copyPath)?;
    ByteBuffer shortCopiedBytes;
    match (Streams.readAll(shortCopiedFile, 100L, 6L)) {
        Err(error) { borrow Text message = error.message(); return new Result<Unit, Text>.Err(message.clone()); }
        Ok(value) { shortCopiedBytes = value; }
    }
    shortCopiedFile.close()?;
    boolean shortCopyMatches = shortCopiedBytes.length() == 100L;
    index = 0;
    while (index < 100) {
        if (shortCopiedBytes.get((long)index) != (byte)index) { shortCopyMatches = false; }
        index = index + 1;
    }
    println(shortCopyMatches);
    return new Result<Unit, Text>.Ok(());
}


void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;

#[test]
fn streams_copy_file_interfaces_bounds_and_short_write_recovery() {
    let program = compile(STREAMS_SOURCE);
    let scratch = Scratch::new();
    let source = scratch.0.join("source.bin");
    let copied = scratch.0.join("copied.bin");
    let short = scratch.0.join("short.bin");
    let empty = scratch.0.join("empty.bin");
    let write_all = scratch.0.join("write-all.bin");
    let output = run(
        &program,
        &[
            source.as_os_str(),
            copied.as_os_str(),
            short.as_os_str(),
            write_all.as_os_str(),
            empty.as_os_str(),
        ],
    );
    assert_eq!(
        output.stdout,
        b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n"
    );

    let bytes = fs::read(copied).unwrap();
    assert_eq!(bytes.len(), 100);
    assert_eq!(bytes, (0_u8..100).collect::<Vec<_>>());
    assert_eq!(fs::read(short).unwrap(), (0_u8..13).collect::<Vec<_>>());
    assert_eq!(fs::read(write_all).unwrap(), (0_u8..100).collect::<Vec<_>>());
}

const STREAM_PROGRESS_SOURCE: &str = r#"
class PlannedSource implements ByteSource {
    private int mode;
    private long next;
    private int calls;
    public PlannedSource(int mode) {
        this.mode = mode;
        this.next = 0L;
        this.calls = 0;
    }
    public Result<long, Text> read(borrow mut ByteBuffer destination, long maxBytes) {
        if (maxBytes == 0L) { return new Result<long, Text>.Ok(0L); }
        if (this.mode == 1) {
            if (this.calls == 0) {
                destination.append((byte)50);
                destination.append((byte)51);
                this.calls = this.calls + 1;
                return new Result<long, Text>.Ok(2L);
            }
            if (this.calls == 1) {
                destination.append((byte)52);
                this.calls = this.calls + 1;
                return new Result<long, Text>.Err(new Text("source failure"));
            }
            return new Result<long, Text>.Ok(0L);
        }
        if (this.mode == 3) {
            destination.append((byte)70);
            return new Result<long, Text>.Err(new Text("source failure"));
        }
        if (this.mode == 4) {
            destination.append((byte)80);
            return new Result<long, Text>.Ok(2L);
        }

        long limit = 4L;
        long base = 10L;
        if (this.mode == 5) { limit = 3L; base = 60L; }
        long amount = maxBytes;
        long remaining = limit - this.next;
        if (amount > remaining) { amount = remaining; }
        long index = 0L;
        while (index < amount) {
            destination.append((byte)(base + this.next));
            this.next = this.next + 1L;
            index = index + 1L;
        }
        return new Result<long, Text>.Ok(amount);
    }
}

class PlannedSink implements ByteSink {
    private int mode;
    private int calls;
    public PlannedSink(int mode) {
        this.mode = mode;
        this.calls = 0;
    }
    public Result<long, Text> write(borrow ByteBuffer source, long offset, long maxBytes) {
        if (maxBytes == 0L) { return new Result<long, Text>.Ok(0L); }
        if (this.mode == 0) {
            if (this.calls == 0) {
                this.calls = this.calls + 1;
                if (maxBytes > 2L) { return new Result<long, Text>.Ok(2L); }
                return new Result<long, Text>.Ok(maxBytes);
            }
            return new Result<long, Text>.Err(new Text("sink failure"));
        }
        if (this.mode == 1) { return new Result<long, Text>.Ok(maxBytes); }
        if (this.mode == 2) { return new Result<long, Text>.Ok(0L); }
        return new Result<long, Text>.Ok(maxBytes + 1L);
    }
    public Result<Unit, Text> flush() {
        if (this.mode == 1) { return new Result<Unit, Text>.Err(new Text("flush failure")); }
        return new Result<Unit, Text>.Ok(());
    }
}

boolean copySinkFailureRetainsPendingPacket() {
    PlannedSource source = new PlannedSource(0);
    PlannedSink sink = new PlannedSink(0);
    match (Streams.copy(source, sink, 4L, 4L)) {
        Ok(count) { return false; }
        Err(error) {
            boolean progress = error.sourceBytesRead() == 4L
                && error.sinkBytesWritten() == 2L && error.confirmedOffset() == 2L;
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            boolean packetMatches = false;
            match (retained) {
                None {}
                Some(packet) {
                    packetMatches = packet.length() == 4L && packet.get(0L) == (byte)10
                        && packet.get(1L) == (byte)11 && packet.get(2L) == (byte)12
                        && packet.get(3L) == (byte)13;
                }
            }
            return progress && packetMatches;
        }
    }
}

boolean copySourceFailureRetainsStagedBytes() {
    PlannedSource source = new PlannedSource(3);
    PlannedSink sink = new PlannedSink(1);
    match (Streams.copy(source, sink, 4L, 4L)) {
        Ok(count) { return false; }
        Err(error) {
            boolean progress = error.sourceBytesRead() == 1L
                && error.sinkBytesWritten() == 0L && error.confirmedOffset() == 0L;
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            boolean staged = false;
            match (retained) {
                None {}
                Some(bytes) {
                    staged = bytes.length() == 1L && bytes.get(0L) == (byte)70;
                }
            }
            return progress && staged;
        }
    }
}

boolean readUpToFailureRetainsAppendedPrefix() {
    PlannedSource source = new PlannedSource(1);
    match (Streams.readUpTo(source, 4L, 2L)) {
        Ok(bytes) { drop(bytes); return false; }
        Err(error) {
            boolean progress = error.sourceBytesRead() == 3L;
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            boolean prefixMatches = false;
            match (retained) {
                None {}
                Some(bytes) {
                    prefixMatches = bytes.length() == 3L && bytes.get(0L) == (byte)50
                        && bytes.get(1L) == (byte)51 && bytes.get(2L) == (byte)52;
                }
            }
            return progress && prefixMatches;
        }
    }
}

boolean readAllFailureRetainsLimitProbe() {
    PlannedSource source = new PlannedSource(5);
    match (Streams.readAll(source, 2L, 2L)) {
        Ok(bytes) { drop(bytes); return false; }
        Err(error) {
            boolean progress = error.sourceBytesRead() == 3L;
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            boolean prefixMatches = false;
            match (retained) {
                None {}
                Some(bytes) {
                    prefixMatches = bytes.length() == 3L && bytes.get(0L) == (byte)60
                        && bytes.get(1L) == (byte)61 && bytes.get(2L) == (byte)62;
                }
            }
            return progress && prefixMatches;
        }
    }
}

boolean writeAllFailureLeavesSourceUsable() {
    ByteBuffer source;
    match (ByteBuffer.withCapacity(0L)) {
        Err(error) { return false; }
        Ok(value) { source = value; }
    }
    source.append((byte)10);
    source.append((byte)11);
    source.append((byte)12);
    source.append((byte)13);
    PlannedSink sink = new PlannedSink(0);
    match (Streams.writeAll(sink, source)) {
        Ok(unit) { return false; }
        Err(error) {
            boolean progress = error.sinkBytesWritten() == 2L
                && error.confirmedOffset() == 2L;
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            boolean noCopy = false;
            match (retained) {
                None { noCopy = true; }
                Some(bytes) {}
            }
            return progress && noCopy && source.length() == 4L
                && source.get(0L) == (byte)10 && source.get(3L) == (byte)13;
        }
    }
}

boolean copyFlushFailureRetainsConfirmedCounts() {
    PlannedSource source = new PlannedSource(0);
    PlannedSink sink = new PlannedSink(1);
    match (Streams.copy(source, sink, 10L, 4L)) {
        Ok(count) { return false; }
        Err(error) {
            boolean progress = error.sourceBytesRead() == 4L
                && error.sinkBytesWritten() == 4L && error.confirmedOffset() == 0L;
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            boolean noPendingPacket = false;
            match (retained) {
                None { noPendingPacket = true; }
                Some(bytes) {}
            }
            return progress && noPendingPacket;
        }
    }
}

boolean malformedReadCountRetainsObservableBytes() {
    PlannedSource source = new PlannedSource(4);
    PlannedSink sink = new PlannedSink(1);
    match (Streams.copy(source, sink, 1L, 1L)) {
        Ok(count) { return false; }
        Err(error) {
            boolean noFalseCount = error.sourceBytesRead() == 0L
                && error.sinkBytesWritten() == 0L;
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            boolean staged = false;
            match (retained) {
                None {}
                Some(bytes) { staged = bytes.length() == 1L && bytes.get(0L) == (byte)80; }
            }
            return noFalseCount && staged;
        }
    }
}

boolean noProgressWriteRetainsPacket() {
    PlannedSource source = new PlannedSource(0);
    PlannedSink sink = new PlannedSink(2);
    match (Streams.copy(source, sink, 4L, 4L)) {
        Ok(count) { return false; }
        Err(error) {
            boolean progress = error.sourceBytesRead() == 4L
                && error.sinkBytesWritten() == 0L && error.confirmedOffset() == 0L;
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            boolean packetMatches = false;
            match (retained) {
                None {}
                Some(bytes) { packetMatches = bytes.length() == 4L; }
            }
            return progress && packetMatches;
        }
    }
}

boolean malformedWriteCountRetainsPacket() {
    PlannedSource source = new PlannedSource(0);
    PlannedSink sink = new PlannedSink(3);
    match (Streams.copy(source, sink, 4L, 4L)) {
        Ok(count) { return false; }
        Err(error) {
            boolean progress = error.sourceBytesRead() == 4L
                && error.sinkBytesWritten() == 0L && error.confirmedOffset() == 0L;
            borrow Option<ByteBuffer> retained = error.retainedBytes();
            boolean packetMatches = false;
            match (retained) {
                None {}
                Some(bytes) { packetMatches = bytes.length() == 4L; }
            }
            return progress && packetMatches;
        }
    }
}

void main() {
    println(copySinkFailureRetainsPendingPacket());
    println(copySourceFailureRetainsStagedBytes());
    println(readUpToFailureRetainsAppendedPrefix());
    println(readAllFailureRetainsLimitProbe());
    println(writeAllFailureLeavesSourceUsable());
    println(copyFlushFailureRetainsConfirmedCounts());
    println(malformedReadCountRetainsObservableBytes());
    println(noProgressWriteRetainsPacket());
    println(malformedWriteCountRetainsPacket());
}
"#;

#[test]
fn stream_errors_preserve_recoverable_progress_and_owned_data() {
    let program = compile(STREAM_PROGRESS_SOURCE);
    let output = run(&program, &[]);
    assert_eq!(
        output.stdout,
        b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n"
    );
}

const SERIALIZATION_SOURCE: &str = r#"
class Child {
    private Text label;
    public Child(Text label) { this.label = label; }
    public readonly borrow Text label() { return this.label; }
}

class Parent {
    private Text title;
    private List<Child> children;
    public Parent(Text title, List<Child> children) {
        this.title = title;
        this.children = children;
    }
    public readonly borrow Text title() { return this.title; }
    public readonly int childCount() { return this.children.size(); }
    public readonly borrow Child child(int index) { return this.children.getRef(index); }
}

class ChildCodec implements Codec<Child> {
    private Codec<Text> textCodec;
    public ChildCodec(Codec<Text> textCodec) { this.textCodec = textCodec; }
    public Result<Unit, Text> encode(borrow Child value, borrow mut BinaryWriter writer) {
        return this.textCodec.encode(value.label(), writer);
    }
    public Result<Child, Text> decode(borrow mut BinaryReader reader) {
        Text label = this.textCodec.decode(reader)?;
        return new Result<Child, Text>.Ok(new Child(label));
    }
}

class ParentCodec implements Codec<Parent> {
    private Codec<Text> textCodec;
    private Codec<Child> childCodec;
    public ParentCodec(Codec<Text> textCodec, Codec<Child> childCodec) {
        this.textCodec = textCodec;
        this.childCodec = childCodec;
    }
    public Result<Unit, Text> encode(borrow Parent value, borrow mut BinaryWriter writer) {
        this.textCodec.encode(value.title(), writer)?;
        int count = value.childCount();
        writer.writeInt32(count)?;
        int index = 0;
        while (index < count) {
            this.childCodec.encode(value.child(index), writer)?;
            index = index + 1;
        }
        return new Result<Unit, Text>.Ok(());
    }
    public Result<Parent, Text> decode(borrow mut BinaryReader reader) {
        Text title = this.textCodec.decode(reader)?;
        int count = reader.readInt32()?;
        if (count < 0 || (long)count > reader.remaining() / 4L) {
            return new Result<Parent, Text>.Err(new Text("invalid child count"));
        }
        List<Child> children = new List<Child>();
        int index = 0;
        while (index < count) {
            Child child = this.childCodec.decode(reader)?;
            children.add(child);
            index = index + 1;
        }
        return new Result<Parent, Text>.Ok(new Parent(title, children));
    }
}

class ProbeCodec implements Codec<int> {
    private int decodes;
    public ProbeCodec() { this.decodes = 0; }
    public Result<Unit, Text> encode(int value, borrow mut BinaryWriter writer) {
        return writer.writeInt32(value);
    }
    public Result<int, Text> decode(borrow mut BinaryReader reader) {
        this.decodes = this.decodes + 1;
        return reader.readInt32();
    }
    public readonly int decodeCount() { return this.decodes; }
}

Parent makeParent() {
    List<Child> children = new List<Child>();
    children.add(new Child(new Text("north")));
    children.add(new Child(new Text("south")));
    return new Parent(new Text("catalogue"), children);
}

Result<ByteBuffer, Text> encodeParent() {
    Parent parent = makeParent();
    ParentCodec codec = new ParentCodec(new TextCodec(), new ChildCodec(new TextCodec()));
    ByteBuffer bytes = Serialization<Parent>.encode(parent, codec)?;
    drop(parent);
    drop(codec);
    return new Result<ByteBuffer, Text>.Ok(bytes);
}


Result<Parent, Text> decodeParent() {
    ByteBuffer bytes = encodeParent()?;
    ParentCodec codec = new ParentCodec(new TextCodec(), new ChildCodec(new TextCodec()));
    Parent parent = Serialization<Parent>.decode(move(bytes), codec)?;
    drop(codec);
    return new Result<Parent, Text>.Ok(parent);
}

Result<ByteBuffer, Text> encodeInt(int value) {
    IntCodec codec = new IntCodec();
    ByteBuffer bytes = Serialization<int>.encode(value, codec)?;
    drop(codec);
    return new Result<ByteBuffer, Text>.Ok(bytes);
}


Result<ByteBuffer, Text> invalidLengthFrame(int length) {
    BinaryWriter writer = new BinaryWriter(0L, false);
    writer.writeByte((byte)78)?;
    writer.writeByte((byte)67)?;
    writer.writeByte((byte)82)?;
    writer.writeByte((byte)83)?;
    writer.writeByte((byte)1)?;
    writer.writeInt32(length)?;
    return new Result<ByteBuffer, Text>.Ok(BinaryWriter.finish(move(writer)));
}


boolean rejectedBeforeDecode(ByteBuffer bytes, borrow mut ProbeCodec codec) {
    boolean rejected = false;
    match (Serialization<int>.decode(move(bytes), codec)) {
        Err(error) { rejected = true; drop(error); }
        Ok(value) { }
    }
    return rejected && codec.decodeCount() == 0;
}

boolean rejectParent(ByteBuffer bytes) {
    ParentCodec codec = new ParentCodec(new TextCodec(), new ChildCodec(new TextCodec()));
    boolean rejected = false;
    match (Serialization<Parent>.decode(move(bytes), codec)) {
        Err(error) { rejected = true; drop(error); }
        Ok(value) { drop(value); }
    }
    drop(codec);
    return rejected;
}

Result<ByteBuffer, Text> tagFrame(byte tag) {
    BinaryWriter writer = new BinaryWriter(0L, false);
    writer.writeByte((byte)78)?;
    writer.writeByte((byte)67)?;
    writer.writeByte((byte)82)?;
    writer.writeByte((byte)83)?;
    writer.writeByte((byte)1)?;
    writer.writeByte(tag)?;
    return new Result<ByteBuffer, Text>.Ok(BinaryWriter.finish(move(writer)));
}

Result<ByteBuffer, Text> truncatedTextListFrame() {
    BinaryWriter writer = new BinaryWriter(0L, false);
    writer.writeByte((byte)78)?;
    writer.writeByte((byte)67)?;
    writer.writeByte((byte)82)?;
    writer.writeByte((byte)83)?;
    writer.writeByte((byte)1)?;
    writer.writeInt32(2)?;
    TextCodec textCodec = new TextCodec();
    Text first = new Text("kept");
    textCodec.encode(first, writer)?;
    drop(first);
    writer.writeInt32(2)?;
    writer.writeByte((byte)65)?;
    drop(textCodec);
    return new Result<ByteBuffer, Text>.Ok(BinaryWriter.finish(move(writer)));
}

boolean nestedOptionsMatch(borrow List<Option<Text>> values) {
    if (values.size() != 3) { return false; }
    {
        borrow Option<Text> first = values.getRef(0);
        match (first) {
            None { }
            Some(text) { return false; }
        }
    }
    {
        borrow Option<Text> second = values.getRef(1);
        match (second) {
            None { return false; }
            Some(text) { if (!text.equalsString("alpha")) { return false; } }
        }
    }
    {
        borrow Option<Text> third = values.getRef(2);
        match (third) {
            None { return false; }
            Some(text) { if (!text.equalsString("")) { return false; } }
        }
    }
    return true;
}

boolean resultOkIs(borrow Result<int, Text> value, int expected) {
    match (value) {
        Ok(payload) { return payload == expected; }
        Err(message) { return false; }
    }
}

boolean resultErrIs(borrow Result<int, Text> value, borrow Text expected) {
    match (value) {
        Ok(payload) { return false; }
        Err(message) { return message.equals(expected); }
    }
}

boolean rejectIntList(ByteBuffer bytes, int maxElements) {
    ListCodec<int> codec = new ListCodec<int>(new IntCodec(), maxElements);
    boolean rejected = false;
    match (Serialization<List<int>>.decode(move(bytes), codec)) {
        Err(error) { rejected = true; drop(error); }
        Ok(values) { drop(values); }
    }
    drop(codec);
    return rejected;
}

boolean rejectTextList(ByteBuffer bytes, int maxElements) {
    ListCodec<Text> codec = new ListCodec<Text>(new TextCodec(), maxElements);
    boolean rejected = false;
    match (Serialization<List<Text>>.decode(move(bytes), codec)) {
        Err(error) { rejected = true; drop(error); }
        Ok(values) { drop(values); }
    }
    drop(codec);
    return rejected;
}

boolean rejectNestedOptions(ByteBuffer bytes, int maxElements) {
    ListCodec<Option<Text>> codec = new ListCodec<Option<Text>>(
        new OptionCodec<Text>(new TextCodec()), maxElements);
    boolean rejected = false;
    match (Serialization<List<Option<Text>>>.decode(move(bytes), codec)) {
        Err(error) { rejected = true; drop(error); }
        Ok(values) { drop(values); }
    }
    drop(codec);
    return rejected;
}

boolean rejectOptionTag(ByteBuffer bytes) {
    OptionCodec<Text> codec = new OptionCodec<Text>(new TextCodec());
    boolean rejected = false;
    match (Serialization<Option<Text>>.decode(move(bytes), codec)) {
        Err(error) { rejected = true; drop(error); }
        Ok(value) { drop(value); }
    }
    drop(codec);
    return rejected;
}

boolean rejectResultTag(ByteBuffer bytes) {
    ResultCodec<int, Text> codec = new ResultCodec<int, Text>(new IntCodec(), new TextCodec());
    boolean rejected = false;
    match (Serialization<Result<int, Text>>.decode(move(bytes), codec)) {
        Err(error) { rejected = true; drop(error); }
        Ok(value) { drop(value); }
    }
    drop(codec);
    return rejected;
}

Result<Unit, Text> exercise() {
    ByteBuffer integerFrame = encodeInt(16909060)?;
    boolean canonicalIntegerFrame = integerFrame.length() == 9L
        && integerFrame.get(0L) == (byte)78 && integerFrame.get(1L) == (byte)67
        && integerFrame.get(2L) == (byte)82 && integerFrame.get(3L) == (byte)83
        && integerFrame.get(4L) == (byte)1 && integerFrame.get(5L) == (byte)1
        && integerFrame.get(6L) == (byte)2 && integerFrame.get(7L) == (byte)3
        && integerFrame.get(8L) == (byte)4;
    IntCodec intCodec = new IntCodec();
    int decodedInteger = Serialization<int>.decode(move(integerFrame), intCodec)?;
    drop(intCodec);
    println(canonicalIntegerFrame && decodedInteger == 16909060);

    Parent decoded = decodeParent()?;
    {
        borrow Text title = decoded.title();
        println(title.equalsString("catalogue"));
    }
    {
        borrow Child first = decoded.child(0);
        borrow Text label = first.label();
        println(label.equalsString("north"));
    }
    {
        borrow Child second = decoded.child(1);
        borrow Text label = second.label();
        println(label.equalsString("south"));
    }
    println(decoded.childCount() == 2);
    drop(decoded);

    Text text = new Text("UTF-8 α\0z");
    TextCodec textEncoder = new TextCodec();
    ByteBuffer textFrame = Serialization<Text>.encode(text, textEncoder)?;
    drop(text);
    drop(textEncoder);
    TextCodec textDecoder = new TextCodec();
    Text recoveredText = Serialization<Text>.decode(move(textFrame), textDecoder)?;
    drop(textDecoder);
    println(recoveredText.equalsString("UTF-8 α\0z"));
    drop(recoveredText);

    ByteBuffer raw = ByteBuffer.withCapacity(0L)?;
    raw.append((byte)0);
    raw.append((byte)255);
    raw.append((byte)17);
    BytesCodec bytesEncoder = new BytesCodec();
    ByteBuffer bytesFrame = Serialization<ByteBuffer>.encode(raw, bytesEncoder)?;
    drop(raw);
    drop(bytesEncoder);
    BytesCodec bytesDecoder = new BytesCodec();
    ByteBuffer recoveredBytes = Serialization<ByteBuffer>.decode(move(bytesFrame), bytesDecoder)?;
    drop(bytesDecoder);
    println(recoveredBytes.length() == 3L && recoveredBytes.get(0L) == (byte)0
        && recoveredBytes.get(1L) == (byte)255 && recoveredBytes.get(2L) == (byte)17);
    drop(recoveredBytes);

    ProbeCodec probe = new ProbeCodec();
    ByteBuffer badMagic = encodeInt(7)?;
    badMagic.set(0L, (byte)0);
    println(rejectedBeforeDecode(move(badMagic), probe));
    ByteBuffer badVersion = encodeInt(7)?;
    badVersion.set(4L, (byte)2);
    println(rejectedBeforeDecode(move(badVersion), probe));
    ByteBuffer shortHeader = encodeInt(7)?;
    shortHeader.truncate(4L)?;
    println(rejectedBeforeDecode(move(shortHeader), probe));
    drop(probe);

    ByteBuffer trailing = encodeParent()?;
    trailing.append((byte)99);
    println(rejectParent(move(trailing)));
    ByteBuffer truncatedValue = encodeParent()?;
    truncatedValue.truncate(truncatedValue.length() - 1L)?;
    println(rejectParent(move(truncatedValue)));

    TextCodec invalidTextCodec = new TextCodec();
    boolean negativeTextLengthRejected = false;
    match (Serialization<Text>.decode(invalidLengthFrame(-1)?, invalidTextCodec)) {
        Err(error) { negativeTextLengthRejected = true; drop(error); }
        Ok(value) { drop(value); }
    }
    drop(invalidTextCodec);
    println(negativeTextLengthRejected);

    BytesCodec truncatedBytesCodec = new BytesCodec();
    boolean truncatedBytesLengthRejected = false;
    match (Serialization<ByteBuffer>.decode(invalidLengthFrame(1)?, truncatedBytesCodec)) {
        Err(error) { truncatedBytesLengthRejected = true; drop(error); }
        Ok(value) { drop(value); }
    }
    drop(truncatedBytesCodec);
    println(truncatedBytesLengthRejected);
    List<Option<Text>> nested = new List<Option<Text>>();
    nested.add(new Option<Text>.None());
    nested.add(new Option<Text>.Some(new Text("alpha")));
    nested.add(new Option<Text>.Some(new Text("")));
    ListCodec<Option<Text>> directEncoder = new ListCodec<Option<Text>>(
        new OptionCodec<Text>(new TextCodec()), 3);
    BinaryWriter directWriter = new BinaryWriter(0L, false);
    directEncoder.encode(nested, directWriter)?;
    ByteBuffer directPayload = BinaryWriter.finish(move(directWriter));
    boolean canonicalNestedTags = directPayload.length() == 20L
        && directPayload.get(0L) == (byte)0 && directPayload.get(1L) == (byte)0
        && directPayload.get(2L) == (byte)0 && directPayload.get(3L) == (byte)3
        && directPayload.get(4L) == (byte)0 && directPayload.get(5L) == (byte)1
        && directPayload.get(15L) == (byte)1;
    BinaryReader directReader = new BinaryReader(move(directPayload), false);
    ListCodec<Option<Text>> directDecoder = new ListCodec<Option<Text>>(
        new OptionCodec<Text>(new TextCodec()), 3);
    List<Option<Text>> directDecoded = directDecoder.decode(directReader)?;
    println(canonicalNestedTags && directReader.remaining() == 0L
        && nestedOptionsMatch(directDecoded) && nestedOptionsMatch(nested));
    drop(directDecoded);
    drop(directDecoder);
    drop(directEncoder);

    ListCodec<Option<Text>> framedEncoder = new ListCodec<Option<Text>>(
        new OptionCodec<Text>(new TextCodec()), 3);
    ByteBuffer nestedFrame = Serialization<List<Option<Text>>>.encode(nested, framedEncoder)?;
    boolean nestedInputSurvivedEncoding = nestedOptionsMatch(nested);
    ListCodec<Option<Text>> framedDecoder = new ListCodec<Option<Text>>(
        new OptionCodec<Text>(new TextCodec()), 3);
    List<Option<Text>> framedDecoded =
        Serialization<List<Option<Text>>>.decode(move(nestedFrame), framedDecoder)?;
    println(nestedInputSurvivedEncoding && nestedOptionsMatch(framedDecoded));
    drop(framedDecoded);
    drop(framedDecoder);
    drop(framedEncoder);
    drop(nested);

    List<Option<Text>> emptyNested = new List<Option<Text>>();
    ListCodec<Option<Text>> emptyEncoder = new ListCodec<Option<Text>>(
        new OptionCodec<Text>(new TextCodec()), 0);
    ByteBuffer emptyFrame = Serialization<List<Option<Text>>>.encode(emptyNested, emptyEncoder)?;
    ListCodec<Option<Text>> emptyDecoder = new ListCodec<Option<Text>>(
        new OptionCodec<Text>(new TextCodec()), 0);
    List<Option<Text>> emptyDecoded =
        Serialization<List<Option<Text>>>.decode(move(emptyFrame), emptyDecoder)?;
    println(emptyNested.isEmpty() && emptyDecoded.isEmpty());
    drop(emptyDecoded);
    drop(emptyDecoder);
    drop(emptyEncoder);
    drop(emptyNested);

    List<int> numbers = new List<int>();
    numbers.add(17);
    numbers.add(-9);
    ListCodec<int> numbersEncoder = new ListCodec<int>(new IntCodec(), 2);
    ByteBuffer numbersFrame = Serialization<List<int>>.encode(numbers, numbersEncoder)?;
    ListCodec<int> numbersDecoder = new ListCodec<int>(new IntCodec(), 2);
    List<int> decodedNumbers =
        Serialization<List<int>>.decode(move(numbersFrame), numbersDecoder)?;
    println(decodedNumbers.size() == 2 && decodedNumbers.get(0) == 17
        && decodedNumbers.get(1) == -9 && numbers.get(0) == 17);
    drop(decodedNumbers);
    drop(numbersDecoder);
    drop(numbersEncoder);
    drop(numbers);

    Option<int> primitiveOption = new Option<int>.Some(91);
    OptionCodec<int> primitiveOptionEncoder = new OptionCodec<int>(new IntCodec());
    ByteBuffer primitiveOptionFrame =
        Serialization<Option<int>>.encode(primitiveOption, primitiveOptionEncoder)?;
    OptionCodec<int> primitiveOptionDecoder = new OptionCodec<int>(new IntCodec());
    Option<int> decodedPrimitiveOption =
        Serialization<Option<int>>.decode(move(primitiveOptionFrame), primitiveOptionDecoder)?;
    boolean primitiveOptionRoundTrip = false;
    match (borrow decodedPrimitiveOption) {
        None { }
        Some(value) { primitiveOptionRoundTrip = value == 91; }
    }
    println(primitiveOptionRoundTrip);
    drop(decodedPrimitiveOption);
    drop(primitiveOptionDecoder);
    drop(primitiveOptionEncoder);
    drop(primitiveOption);

    Result<int, Text> successValue = new Result<int, Text>.Ok(73);
    Text expectedFailure = new Text("rejected");
    Result<int, Text> failureValue = new Result<int, Text>.Err(new Text("rejected"));
    ResultCodec<int, Text> resultEncoder =
        new ResultCodec<int, Text>(new IntCodec(), new TextCodec());
    ByteBuffer successFrame = Serialization<Result<int, Text>>.encode(successValue, resultEncoder)?;
    ByteBuffer failureFrame = Serialization<Result<int, Text>>.encode(failureValue, resultEncoder)?;
    boolean canonicalResultTags = successFrame.get(5L) == (byte)0
        && failureFrame.get(5L) == (byte)1;
    ResultCodec<int, Text> resultDecoder =
        new ResultCodec<int, Text>(new IntCodec(), new TextCodec());
    Result<int, Text> decodedSuccess =
        Serialization<Result<int, Text>>.decode(move(successFrame), resultDecoder)?;
    Result<int, Text> decodedFailure =
        Serialization<Result<int, Text>>.decode(move(failureFrame), resultDecoder)?;
    println(canonicalResultTags && resultOkIs(decodedSuccess, 73)
        && resultErrIs(decodedFailure, expectedFailure)
        && resultOkIs(successValue, 73) && resultErrIs(failureValue, expectedFailure));
    drop(decodedSuccess);
    drop(decodedFailure);
    drop(resultDecoder);
    drop(resultEncoder);
    drop(successValue);
    drop(failureValue);
    drop(expectedFailure);

    Text knownUuidText = new Text("00112233-4455-6677-8899-aabbccddeeff");
    Uuid knownUuid = Uuid.parse(knownUuidText)?;
    drop(knownUuidText);
    UuidCodec uuidEncoder = new UuidCodec();
    ByteBuffer uuidFrame = Serialization<Uuid>.encode(knownUuid, uuidEncoder)?;
    boolean uuidWireCanonical = uuidFrame.length() == 21L
        && uuidFrame.get(0L) == (byte)78 && uuidFrame.get(1L) == (byte)67
        && uuidFrame.get(2L) == (byte)82 && uuidFrame.get(3L) == (byte)83
        && uuidFrame.get(4L) == (byte)1 && uuidFrame.get(5L) == (byte)0
        && uuidFrame.get(6L) == (byte)17 && uuidFrame.get(7L) == (byte)34
        && uuidFrame.get(8L) == (byte)51 && uuidFrame.get(9L) == (byte)68
        && uuidFrame.get(10L) == (byte)85 && uuidFrame.get(11L) == (byte)102
        && uuidFrame.get(12L) == (byte)119 && uuidFrame.get(13L) == (byte)136
        && uuidFrame.get(14L) == (byte)153 && uuidFrame.get(15L) == (byte)170
        && uuidFrame.get(16L) == (byte)187 && uuidFrame.get(17L) == (byte)204
        && uuidFrame.get(18L) == (byte)221 && uuidFrame.get(19L) == (byte)238
        && uuidFrame.get(20L) == (byte)255;
    UuidCodec uuidDecoder = new UuidCodec();
    Uuid decodedUuid = Serialization<Uuid>.decode(move(uuidFrame), uuidDecoder)?;
    println(uuidWireCanonical && knownUuid.equals(decodedUuid));
    drop(decodedUuid);
    drop(uuidDecoder);
    drop(uuidEncoder);
    drop(knownUuid);

    List<int> tooMany = new List<int>();
    tooMany.add(7);
    tooMany.add(8);
    ListCodec<int> limitedEncoder = new ListCodec<int>(new IntCodec(), 1);
    BinaryWriter boundedWriter = new BinaryWriter(0L, false);
    boolean encodeLimitRejected = false;
    match (limitedEncoder.encode(tooMany, boundedWriter)) {
        Err(error) { encodeLimitRejected = true; drop(error); }
        Ok(unit) { }
    }
    println(encodeLimitRejected && boundedWriter.position() == 0L
        && tooMany.size() == 2 && tooMany.get(1) == 8);
    drop(limitedEncoder);
    drop(tooMany);

    boolean invalidTagsRejected = rejectOptionTag(tagFrame((byte)2)?)
        && rejectResultTag(tagFrame((byte)255)?);
    boolean truncatedVariantPayloadsRejected = rejectOptionTag(tagFrame((byte)1)?)
        && rejectResultTag(tagFrame((byte)0)?);
    println(invalidTagsRejected && truncatedVariantPayloadsRejected);

    boolean invalidListCountsRejected = rejectIntList(invalidLengthFrame(-1)?, 4)
        && rejectIntList(invalidLengthFrame(-2147483648)?, 4)
        && rejectIntList(invalidLengthFrame(2147483647)?, 4)
        && rejectIntList(invalidLengthFrame(3)?, 2);
    println(invalidListCountsRejected);
    println(rejectTextList(truncatedTextListFrame()?, 4));

    List<int> trailingNumbers = new List<int>();
    trailingNumbers.add(12);
    ListCodec<int> trailingEncoder = new ListCodec<int>(new IntCodec(), 1);
    ByteBuffer aggregateTrailing =
        Serialization<List<int>>.encode(trailingNumbers, trailingEncoder)?;
    aggregateTrailing.append((byte)99);
    println(rejectIntList(move(aggregateTrailing), 1));
    drop(trailingEncoder);
    drop(trailingNumbers);

    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;

#[test]
fn serialization_codecs_frame_compose_and_return_independent_owned_values() {
    let program = compile(SERIALIZATION_SOURCE);
    let output = run(&program, &[]);
    assert_eq!(
        output.stdout,
        b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn mutable_generic_borrows_still_reject_copy_primitives() {
    let source = r#"
interface MutableBorrow<T> {
    void consume(borrow mut T value);
}
void accept(MutableBorrow<int> value) {}
void main() {}
"#;
    assert!(compile_to_c(source).is_err());
}
