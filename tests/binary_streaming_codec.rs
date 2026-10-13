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
            "nacre-binary-streaming-codec-{}-{serial}",
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
        .unwrap_or_else(|error| panic!("{}", error.render("binary_streaming_codec.ncr", source)));
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

const STREAM_RECOVERY_SOURCE: &str = r#"
class TraceSource implements ByteSource {
    private ByteBuffer data;
    private long position;
    private long chunkLimit;
    private boolean failNext;

    public TraceSource(ByteBuffer data, long chunkLimit, boolean failNext) {
        this.data = data;
        this.position = 0L;
        this.chunkLimit = chunkLimit;
        this.failNext = failNext;
    }

    public Result<long, Text> read(borrow mut ByteBuffer destination, long maxBytes) {
        println("source read");
        if (maxBytes == 0L) {
            println("zero read");
            return new Result<long, Text>.Ok(0L);
        }
        long amount = maxBytes;
        if (amount > this.chunkLimit) { amount = this.chunkLimit; }
        long available = this.data.length() - this.position;
        if (amount > available) { amount = available; }
        long copied = 0L;
        while (copied < amount) {
            destination.append(this.data.get(this.position));
            this.position = this.position + 1L;
            copied = copied + 1L;
        }
        if (this.failNext) {
            this.failNext = false;
            return new Result<long, Text>.Err(new Text("source failed after a prefix"));
        }
        return new Result<long, Text>.Ok(copied);
    }
}

class RetrySink implements ByteSink {
    private ByteBuffer output;
    private boolean failed;

    public RetrySink(ByteBuffer output) {
        this.output = output;
        this.failed = false;
    }

    public Result<long, Text> write(borrow ByteBuffer source, long offset, long maxBytes) {
        println("sink write");
        println(offset);
        println(maxBytes);
        if (!this.failed && offset > 0L) {
            this.failed = true;
            return new Result<long, Text>.Err(new Text("sink failed after a confirmed prefix"));
        }
        long amount = maxBytes;
        if (amount > 2L) { amount = 2L; }
        long copied = 0L;
        while (copied < amount) {
            this.output.append(source.get(offset + copied));
            copied = copied + 1L;
        }
        return new Result<long, Text>.Ok(copied);
    }

    public Result<Unit, Text> flush() {
        println("sink flush");
        println(this.output.length() == 4L
            && this.output.get(0L) == (byte)4 && this.output.get(1L) == (byte)3
            && this.output.get(2L) == (byte)2 && this.output.get(3L) == (byte)1);
        return new Result<Unit, Text>.Ok(());
    }
}

Result<Unit, Text> exercise() {
    ByteBuffer partial = ByteBuffer.withCapacity(0L)?;
    partial.append((byte)52);
    partial.append((byte)18);
    partial.append((byte)120);
    partial.append((byte)86);
    BinaryReader reader = BinaryReader.fromSource(new TraceSource(partial, 2L, true), true, 4L)?;
    match (reader.readInt32()) {
        Err(error) { println("source error retained"); drop(error); }
        Ok(value) { println("source error lost"); }
    }
    println(reader.position() == 0L && reader.remaining() == 4L);
    println(reader.readInt16()? == 4660);
    println(reader.readByte()? == (byte)120);
    println(reader.readByte()? == (byte)86);
    match (reader.readByte()) {
        Err(error) { println("exact source limit rejected"); drop(error); }
        Ok(value) { println("source limit exceeded"); }
    }
    match (reader.seek(1L)) {
        Err(error) { println("source seek rejected"); drop(error); }
        Ok(unit) { println("source seek accepted"); }
    }
    reader.seek(reader.position())?;

    ByteBuffer one = ByteBuffer.withCapacity(0L)?;
    one.append((byte)77);
    BinaryReader eof = BinaryReader.fromSource(new TraceSource(one, 2L, false), true, 5L)?;
    match (eof.readInt32()) {
        Err(error) { println("source EOF retained"); drop(error); }
        Ok(value) { println("source EOF accepted"); }
    }
    println(eof.position() == 0L);
    println(eof.readByte()? == (byte)77);
    match (eof.readByte()) {
        Err(error) { println("source EOF stayed final"); drop(error); }
        Ok(value) { println("source EOF repeated"); }
    }

    ByteBuffer invalid = ByteBuffer.withCapacity(0L)?;
    invalid.append((byte)192);
    invalid.append((byte)128);
    BinaryReader invalidReader = BinaryReader.fromSource(new TraceSource(invalid, 2L, false), true, 2L)?;
    match (invalidReader.readUtf8(2L)) {
        Err(error) { println("invalid UTF-8 retained"); drop(error); }
        Ok(value) { println("invalid UTF-8 accepted"); }
    }
    println(invalidReader.position() == 0L);
    println(invalidReader.readByte()? == (byte)192);
    println(invalidReader.readByte()? == (byte)128);

    ByteBuffer invalidFactorySource = ByteBuffer.withCapacity(0L)?;
    match (BinaryReader.fromSource(new TraceSource(invalidFactorySource, 2L, false), true, -1L)) {
        Err(error) { println("negative reader limit rejected"); drop(error); }
        Ok(value) { println("negative reader limit accepted"); }
    }
    ByteBuffer invalidFactorySink = ByteBuffer.withCapacity(0L)?;
    match (BinaryWriter.toSink(new RetrySink(invalidFactorySink), true, -1L)) {
        Err(error) { println("negative writer limit rejected"); drop(error); }
        Ok(value) { println("negative writer limit accepted"); }
    }

    BinaryWriter bufferWriter = new BinaryWriter(0L, true);
    bufferWriter.writeInt16(258)?;
    println(bufferWriter.position() == 2L && bufferWriter.pendingOffset() == 0L);
    bufferWriter.flush()?;
    ByteBuffer bufferOutput = BinaryWriter.finish(move(bufferWriter));
    println(bufferOutput.length() == 2L && bufferOutput.get(0L) == (byte)2
        && bufferOutput.get(1L) == (byte)1);

    ByteBuffer received = ByteBuffer.withCapacity(0L)?;
    BinaryWriter writer = BinaryWriter.toSink(new RetrySink(received), true, 5L)?;
    match (writer.writeInt32(16909060)) {
        Err(error) { println("writer prefix failure retained"); drop(error); }
        Ok(unit) { println("writer prefix failure missed"); }
    }
    println(writer.position() == 2L && writer.pendingOffset() == 2L);
    {
        borrow ByteBuffer packet = writer.bytes();
        println(packet.length() == 4L && packet.get(0L) == (byte)4
            && packet.get(1L) == (byte)3 && packet.get(2L) == (byte)2
            && packet.get(3L) == (byte)1);
    }
    match (writer.writeByte((byte)99)) {
        Err(error) { println("pending packet blocked new write"); drop(error); }
        Ok(unit) { println("pending packet duplicated"); }
    }
    println(writer.position() == 2L && writer.pendingOffset() == 2L);
    writer.flush()?;
    println(writer.position() == 4L && writer.pendingOffset() == 0L);
    {
        borrow ByteBuffer packet = writer.bytes();
        println(packet.length() == 0L);
    }
    writer.writeByte((byte)85)?;
    match (writer.writeByte((byte)86)) {
        Err(error) { println("writer exact limit rejected"); drop(error); }
        Ok(unit) { println("writer limit exceeded"); }
    }
    ByteBuffer sentSuffix = BinaryWriter.finish(move(writer));
    println(sentSuffix.length() == 0L);

    ByteBuffer finishReceived = ByteBuffer.withCapacity(0L)?;
    BinaryWriter finishing = BinaryWriter.toSink(new RetrySink(finishReceived), true, 4L)?;
    match (finishing.writeInt32(16909060)) {
        Err(error) { println("finish packet left pending"); drop(error); }
        Ok(unit) { println("finish packet unexpectedly complete"); }
    }
    ByteBuffer unwritten = BinaryWriter.finish(move(finishing));
    println(unwritten.length() == 2L && unwritten.get(0L) == (byte)2
        && unwritten.get(1L) == (byte)1);
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
fn binary_streaming_reader_retry_and_writer_resume_preserve_progress() {
    let program = compile(STREAM_RECOVERY_SOURCE);
    let output = run(&program, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("source error retained\n"));
    assert!(stdout.contains("source EOF retained\n"));
    assert!(stdout.contains("invalid UTF-8 retained\n"));
    assert!(stdout.contains("pending packet blocked new write\n"));
    assert!(stdout.contains("writer exact limit rejected\n"));
    assert!(stdout.contains("negative reader limit rejected\n"));
    assert!(stdout.contains("negative writer limit rejected\n"));
    assert!(!stdout.contains("zero read\n"));
    assert_eq!(stdout.matches("source read\n").count(), 6, "{stdout}");
    assert_eq!(stdout.matches("sink write\n").count(), 6, "{stdout}");
    assert_eq!(stdout.matches("sink flush\n").count(), 1, "{stdout}");
}

const STREAM_CODEC_SOURCE: &str = r#"
class ChunkSource implements ByteSource {
    private File input;
    public ChunkSource(File input) { this.input = input; }
    public Result<long, Text> read(borrow mut ByteBuffer destination, long maxBytes) {
        long amount = maxBytes;
        if (amount > 3L) { amount = 3L; }
        return this.input.read(destination, amount);
    }
}

class ChunkSink implements ByteSink {
    private File output;
    public ChunkSink(File output) { this.output = output; }
    public Result<long, Text> write(borrow ByteBuffer source, long offset, long maxBytes) {
        long amount = maxBytes;
        if (amount > 2L) { amount = 2L; }
        return this.output.write(source, offset, amount);
    }
    public Result<Unit, Text> flush() { return this.output.flush(); }
}

class BinaryRecord {
    private int number;
    private long wide;
    private double precise;
    private double narrow;
    private Text name;
    private ByteBuffer blob;

    public BinaryRecord(int number, long wide, double precise, double narrow, Text name, ByteBuffer blob) {
        this.number = number;
        this.wide = wide;
        this.precise = precise;
        this.narrow = narrow;
        this.name = name;
        this.blob = blob;
    }
    public readonly int number() { return this.number; }
    public readonly long wide() { return this.wide; }
    public readonly double precise() { return this.precise; }
    public readonly double narrow() { return this.narrow; }
    public readonly borrow Text name() { return this.name; }
    public readonly borrow ByteBuffer blob() { return this.blob; }
}

class BinaryRecordCodec implements Codec<BinaryRecord> {
    public Result<Unit, Text> encode(borrow BinaryRecord value, borrow mut BinaryWriter writer) {
        writer.writeInt32(value.number())?;
        writer.writeInt64(value.wide())?;
        writer.writeDouble(value.precise())?;
        writer.writeFloat32(value.narrow())?;
        borrow Text name = value.name();
        borrow ByteBuffer blob = value.blob();
        writer.writeInt32(name.byteLength())?;
        writer.writeUtf8(name)?;
        writer.writeInt32((int)blob.length())?;
        writer.writeBytes(blob)?;
        return new Result<Unit, Text>.Ok(());
    }

    public Result<BinaryRecord, Text> decode(borrow mut BinaryReader reader) {
        int number = reader.readInt32()?;
        long wide = reader.readInt64()?;
        double precise = reader.readDouble()?;
        double narrow = reader.readFloat32()?;
        int nameLength = reader.readInt32()?;
        Text name = reader.readUtf8((long)nameLength)?;
        int blobLength = reader.readInt32()?;
        ByteBuffer blob = reader.readBytes((long)blobLength)?;
        return new Result<BinaryRecord, Text>.Ok(
            new BinaryRecord(number, wide, precise, narrow, name, blob));
    }
}

Result<boolean, Text> roundTrip(borrow Text pathName, boolean littleEndian) {
    Path path = Path.fromText(pathName.clone())?;
    File output = File.create(path, 384)?;
    BinaryWriter writer = BinaryWriter.toSink(new ChunkSink(output), littleEndian, 39L)?;
    ByteBuffer payload = ByteBuffer.withCapacity(0L)?;
    payload.append((byte)0);
    payload.append((byte)255);
    payload.append((byte)17);
    BinaryRecord value = new BinaryRecord(-16909061, -72623859790382856L,
        -123.5, 1.5, new Text("A\0é"), payload);
    BinaryRecordCodec codec = new BinaryRecordCodec();
    codec.encode(value, writer)?;
    boolean writerState = writer.position() == 39L && writer.pendingOffset() == 0L;
    {
        borrow ByteBuffer staged = writer.bytes();
        writerState = writerState && staged.length() == 0L;
    }
    writer.flush()?;
    ByteBuffer writerRemainder = BinaryWriter.finish(move(writer));
    if (writerRemainder.length() != 0L) {
        return new Result<boolean, Text>.Err(new Text("successful writer retained bytes"));
    }

    File input = File.openRead(path)?;
    BinaryReader reader = BinaryReader.fromSource(new ChunkSource(input), littleEndian, 39L)?;
    boolean readerBudget = reader.remaining() == 39L;
    BinaryRecord decoded = codec.decode(reader)?;
    borrow Text decodedName = decoded.name();
    borrow ByteBuffer decodedBlob = decoded.blob();
    boolean valuesMatch = decoded.number() == -16909061
        && decoded.wide() == -72623859790382856L
        && decoded.precise() == -123.5 && decoded.narrow() == 1.5
        && decodedName.equalsString("A\0é")
        && decodedBlob.length() == 3L
        && decodedBlob.get(0L) == (byte)0
        && decodedBlob.get(1L) == (byte)255
        && decodedBlob.get(2L) == (byte)17;
    boolean cursor = reader.position() == 39L && reader.remaining() == 0L;
    match (reader.readByte()) {
        Err(error) { drop(error); }
        Ok(unexpectedByte) { cursor = false; }
    }
    return new Result<boolean, Text>.Ok(writerState && readerBudget && valuesMatch && cursor);
}

Result<Unit, Text> exercise() {
    Text littlePath = Args.get(0)?;
    Text bigPath = Args.get(1)?;
    println(roundTrip(littlePath, true)?);
    println(roundTrip(bigPath, false)?);
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
fn generic_codecs_compose_over_source_and_sink_binary_streams_in_both_orders() {
    let program = compile(STREAM_CODEC_SOURCE);
    let scratch = Scratch::new();
    let little = scratch.0.join("little.bin");
    let big = scratch.0.join("big.bin");
    let output = run(
        &program,
        &[little.as_os_str(), big.as_os_str()],
    );
    assert_eq!(output.stdout, b"true\ntrue\n");
}
