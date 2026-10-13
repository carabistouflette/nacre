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
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("binary_streams.ncr", source)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"]).unwrap()
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
    writer.writeUtf8(new Text("xy"))?;
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
    long copied = Streams.copy(input, output, 100L, 7L)?;
    input.close()?;
    output.close()?;
    println(copied == 100L);

    File copiedFile = File.openRead(copyPath)?;
    ByteBuffer copiedBytes = Streams.readAll(copiedFile, 100L, 9L)?;
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
    long boundedCount = Streams.copy(boundedInput, boundedOutput, 13L, 5L)?;
    boundedInput.close()?;
    boundedOutput.close()?;
    println(boundedCount == 13L);
    File boundedFile = File.openRead(shortPath)?;
    ByteBuffer boundedBytes = Streams.readAll(boundedFile, 100L, 4L)?;
    boundedFile.close()?;
    println(boundedBytes.length() == 13L && boundedBytes.get(0L) == (byte)0
        && boundedBytes.get(12L) == (byte)12);

    File exactBoundFile = File.openRead(shortPath)?;
    ByteBuffer exactBoundBytes = Streams.readAll(exactBoundFile, 13L, 4L)?;
    exactBoundFile.close()?;
    println(exactBoundBytes.length() == 13L && exactBoundBytes.get(0L) == (byte)0
        && exactBoundBytes.get(12L) == (byte)12);

    File shortOutput = File.create(writeAllPath, 384)?;
    ThreeByteFileSink shortSink = new ThreeByteFileSink(shortOutput);
    Streams.writeAll(shortSink, payload)?;
    shortSink.close()?;
    File allFile = File.openRead(writeAllPath)?;
    ByteBuffer allBytes = Streams.readAll(allFile, 100L, 8L)?;
    allFile.close()?;
    boolean allMatch = allBytes.length() == 100L;
    index = 0;
    while (index < 100) {
        if (allBytes.get((long)index) != (byte)index) { allMatch = false; }
        index = index + 1;
    }
    println(allMatch);

    File prefixInput = File.openRead(sourcePath)?;
    ByteBuffer prefix = Streams.readUpTo(prefixInput, 13L, 4L)?;
    ByteBuffer afterPrefix = ByteBuffer.withCapacity(0L)?;
    long prefixCursorCount = prefixInput.read(afterPrefix, 1L)?;
    prefixInput.close()?;
    println(prefix.length() == 13L && prefix.get(0L) == (byte)0 && prefix.get(12L) == (byte)12
        && prefixCursorCount == 1L && afterPrefix.get(0L) == (byte)13);
    File emptyInput = File.openRead(emptyPath)?;
    ByteBuffer emptyRead = Streams.readAll(emptyInput, 0L, 4L)?;
    emptyInput.close()?;
    println(emptyRead.length() == 0L);

    File zeroLimitInput = File.openRead(sourcePath)?;
    match (Streams.readAll(zeroLimitInput, 0L, 4L)) {
        Err(error) { println("readAll max-zero rejected"); drop(error); }
        Ok(value) { println("readAll max-zero accepted"); }
    }
    ByteBuffer afterZeroProbe = ByteBuffer.withCapacity(0L)?;
    long zeroProbeCount = zeroLimitInput.read(afterZeroProbe, 1L)?;
    zeroLimitInput.close()?;
    println(zeroProbeCount == 1L && afterZeroProbe.get(0L) == (byte)1);

    File excessInput = File.openRead(sourcePath)?;
    match (Streams.readAll(excessInput, 13L, 4L)) {
        Err(error) { println("readAll bound rejected"); drop(error); }
        Ok(value) { println("readAll bound accepted"); }
    }
    ByteBuffer afterLimitProbe = ByteBuffer.withCapacity(0L)?;
    long limitProbeCount = excessInput.read(afterLimitProbe, 1L)?;
    excessInput.close()?;
    println(limitProbeCount == 1L && afterLimitProbe.get(0L) == (byte)14);

    File shortCopyInput = File.openRead(sourcePath)?;
    File shortCopyFile = File.create(copyPath, 384)?;
    ThreeByteFileSink shortCopySink = new ThreeByteFileSink(shortCopyFile);
    long shortCopied = Streams.copy(shortCopyInput, shortCopySink, 100L, 11L)?;
    shortCopyInput.close()?;
    shortCopySink.close()?;
    println(shortCopied == 100L);
    File shortCopiedFile = File.openRead(copyPath)?;
    ByteBuffer shortCopiedBytes = Streams.readAll(shortCopiedFile, 100L, 6L)?;
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
        b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\nreadAll max-zero rejected\ntrue\nreadAll bound rejected\ntrue\ntrue\ntrue\n"
    );

    let bytes = fs::read(copied).unwrap();
    assert_eq!(bytes.len(), 100);
    assert_eq!(bytes, (0_u8..100).collect::<Vec<_>>());
    assert_eq!(fs::read(short).unwrap(), (0_u8..13).collect::<Vec<_>>());
    assert_eq!(fs::read(write_all).unwrap(), (0_u8..100).collect::<Vec<_>>());
}
