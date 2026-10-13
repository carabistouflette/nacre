use nacre::{compile_to_c, native};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DATABASE: AtomicUsize = AtomicUsize::new(0);

struct TemporaryDatabase(PathBuf);

impl TemporaryDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nacre-sqlite-{}-{serial}.db",
            std::process::id()
        ));
        Self(path)
    }
}

impl Drop for TemporaryDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn compile(source: &str) -> native::NativeProgram {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("sqlite.ncr", source)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native SQLite compile failed: {error}"))
}

fn assert_memory_balanced(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().find(|line| line.starts_with("NACRE_MEMORY "))
        .unwrap_or_else(|| panic!("missing native allocation report:\n{stderr}"));
    let mut fields = line.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{line}");
    assert_eq!(fields.next(), Some("live=0"), "{line}");
    assert_eq!(fields.next(), None, "{line}");
}

fn run_memory(source: &str) -> Output {
    let output = compile(source).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_memory_balanced(&output);
    output
}

const AFFINE_BINDING_AND_ROWS: &str = r#"
boolean optionLongEquals(Option<long> option, long expected) {
    match (option) { Some(value) { return value == expected; } None { return false; } }
}
boolean optionLongIsNone(Option<long> option) {
    match (option) { None { return true; } Some(value) { return false; } }
}
boolean optionDoubleEquals(Option<double> option, double expected) {
    match (option) { Some(value) { return value == expected; } None { return false; } }
}
Result<boolean, Text> borrowedCountMatches(borrow Database database, int expected) {
    Text name = database.columnName(0)?;
    int count = database.columnCount()?;
    return new Result<boolean, Text>.Ok(count == expected && name.equalsString("id"));
}
Result<int, Text> borrowedColumnType(borrow Database database) {
    return database.columnType(0);
}
Result<Unit, Text> exercise() {
    Text path = new Text(":memory:");
    Database database = Database.open(path)?;
    Text create = new Text("CREATE TABLE sample (id INTEGER PRIMARY KEY, text_value TEXT, blob_value BLOB, nullable INTEGER, huge INTEGER, real_value REAL)");
    database.execute(create)?;

    Text insert = new Text("INSERT INTO sample(text_value, blob_value, nullable, huge, real_value) VALUES(?,?,?,?,?)");
    database.prepare(insert)?;
    Text injected = new Text("x'); DROP TABLE sample;--\0tail");
    ByteBuffer binary = ByteBuffer.withCapacity(0L)?;
    binary.append((byte)0);
    binary.append((byte)255);
    binary.append((byte)65);
    long lowest = -9223372036854775807L - 1L;
    database.bindText(1, injected)?;
    database.bindBlob(2, binary)?;
    database.bindNull(3)?;
    database.bindLong(4, lowest)?;
    database.bindDouble(5, 1.25)?;
    boolean insertHasRow = database.step()?;
    println(!insertHasRow);
    println(database.changes()? == 1L);
    println(database.lastInsertRowId()? == 1L);
    drop(injected);
    drop(binary);

    Text query = new Text("SELECT id AS id, text_value AS literal, blob_value AS payload, nullable, huge, real_value FROM sample");
    database.prepare(query)?;
    println(borrowedCountMatches(database, 6)?);
    println(database.columnName(0)?.equalsString("id"));
    println(database.step()?);
    println(borrowedColumnType(database)?);
    println(database.columnType(1)?);
    println(database.columnType(2)?);
    println(database.columnType(3)?);
    println(database.columnType(5)?);
    println(optionLongEquals(database.columnLong(0)?, 1L));
    Option<Text> copiedText = database.columnText(1)?;
    Option<ByteBuffer> copiedBlob = database.columnBlob(2)?;
    Option<long> copiedNull = database.columnLong(3)?;
    Option<long> copiedLong = database.columnLong(4)?;
    Option<double> copiedDouble = database.columnDouble(5)?;
    boolean queryHasMore = database.step()?;
    println(!queryHasMore);

    database.prepare(new Text("SELECT ?"))?;
    database.bindLong(1, 42L)?;
    println(database.step()?);
    println(optionLongEquals(database.columnLong(0)?, 42L));
    database.reset()?;
    database.clearBindings()?;
    println(database.step()?);
    println(optionLongIsNone(database.columnLong(0)?));
    database.reset()?;
    database.bindLong(1, 43L)?;
    println(database.step()?);
    println(optionLongEquals(database.columnLong(0)?, 43L));

    database.close()?;
    database.close()?;
    drop(database);
    match (copiedText) {
        None { println(false); }
        Some(text) { println(text.equalsString("x'); DROP TABLE sample;--\0tail")); }
    }
    match (copiedBlob) {
        None { println(false); }
        Some(bytes) {
            println(bytes.length());
            println((int)bytes.get(0L));
            println((int)bytes.get(1L));
            println((int)bytes.get(2L));
        }
    }
    println(optionLongIsNone(copiedNull));
    println(optionLongEquals(copiedLong, lowest));
    println(optionDoubleEquals(copiedDouble, 1.25));
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(unit) { }
        Err(error) { Console.printText(error); println(""); }
    }
}
"#;

const TRANSACTIONS_AND_ERRORS: &str = r#"
boolean unitFailed(Result<Unit, Text> result) {
    match (result) { Ok(unit) { return false; } Err(error) { return true; } }
}
boolean intFailed(Result<int, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean longFailed(Result<long, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean textFailed(Result<Text, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean boolFailed(Result<boolean, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean databaseFailed(Result<Database, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean optionLongEquals(Option<long> option, long expected) {
    match (option) { Some(value) { return value == expected; } None { return false; } }
}
Result<Unit, Text> exercise() {
    Text badPath = new Text("bad\0path");
    println(databaseFailed(Database.open(badPath)));
    Text memory = new Text(":memory:");
    Database database = Database.open(memory)?;
    println(unitFailed(database.prepare(new Text("SELECT 1\0"))));
    database.execute(new Text("CREATE TABLE parent (id INTEGER PRIMARY KEY)"))?;
    database.execute(new Text("CREATE TABLE child (parent_id INTEGER NOT NULL REFERENCES parent(id))"))?;
    database.begin()?;
    database.execute(new Text("INSERT INTO parent VALUES(1)"))?;
    database.commit()?;
    database.begin()?;
    database.execute(new Text("INSERT INTO parent VALUES(2)"))?;
    database.rollback()?;
    database.begin()?;
    println(longFailed(database.execute(new Text("INSERT INTO child VALUES(99)"))));
    database.rollback()?;

    database.prepare(new Text("SELECT COUNT(*) FROM parent"))?;
    println(database.step()?);
    Option<long> count = database.columnLong(0)?;
    println(optionLongEquals(count, 1L));

    println(longFailed(database.execute(new Text("SELECT 1"))));
    database.execute(new Text("CREATE TABLE marker (id INTEGER)"))?;
    println(longFailed(database.execute(new Text("INSERT INTO marker VALUES(1); INSERT INTO marker VALUES(2)"))));
    database.prepare(new Text("SELECT COUNT(*) FROM marker"))?;
    println(database.step()?);
    println(optionLongEquals(database.columnLong(0)?, 0L));

    database.prepare(new Text("SELECT 77; -- trailing comment\n"))?;
    println(unitFailed(database.prepare(new Text("SELECT 1; SELECT 2"))));
    println(database.step()?);
    println(optionLongEquals(database.columnLong(0)?, 77L));
    println(textFailed(database.columnText(0)));
    println(optionLongFailed(database.columnLong(1)));
    println(textFailed(database.columnName(1)));
    println(intFailed(database.columnType(1)));

    database.prepare(new Text("SELECT ?"))?;
    println(unitFailed(database.bindNull(0)));
    println(unitFailed(database.bindLong(2, 9L)));
    database.bindLong(1, 9L)?;
    println(database.step()?);
    println(optionLongEquals(database.columnLong(0)?, 9L));
    println(unitFailed(database.setBusyTimeoutMillis(-1L)));
    println(unitFailed(database.setBusyTimeoutMillis(2147483648L)));

    database.close()?;
    database.close()?;
    println(unitFailed(database.prepare(new Text("SELECT 1"))));
    println(boolFailed(database.step()));
    println(intFailed(database.columnCount()));
    println(longFailed(database.execute(new Text("CREATE TABLE closed_test (id INTEGER)"))));
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(unit) { }
        Err(error) { Console.printText(error); println(""); }
    }
}
"#;

const BUSY_TIMEOUT: &str = r#"
boolean longFailed(Result<long, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean optionLongEquals(Option<long> option, long expected) {
    match (option) { Some(value) { return value == expected; } None { return false; } }
}
Result<Unit, Text> abandonTransaction(borrow Text path) {
    Database abandoned = Database.open(path)?;
    abandoned.begin()?;
    abandoned.execute(new Text("INSERT INTO held VALUES(3)"))?;
    abandoned.prepare(new Text("SELECT value FROM held"))?;
    println(abandoned.step()?);
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> exercise(borrow Text path) {
    Database first = Database.open(path)?;
    Database second = Database.open(path)?;
    first.execute(new Text("CREATE TABLE IF NOT EXISTS held (value INTEGER)"))?;
    first.begin()?;
    first.execute(new Text("INSERT INTO held VALUES(1)"))?;
    second.setBusyTimeoutMillis(150L)?;
    long start = Time.monotonicNanos()?;
    boolean busy = longFailed(second.execute(new Text("INSERT INTO held VALUES(2)")));
    long elapsed = Time.monotonicNanos()? - start;
    first.rollback()?;
    println(busy);
    println(elapsed >= 75000000L && elapsed < 5000000000L);
    abandonTransaction(path)?;
    Database verify = Database.open(path)?;
    verify.prepare(new Text("SELECT COUNT(*) FROM held"))?;
    println(verify.step()?);
    println(optionLongEquals(verify.columnLong(0)?, 0L));
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (Args.get(0)) {
        Err(error) { println("ARG_ERROR"); }
        Ok(path) {
            match (exercise(path)) {
                Ok(unit) { }
                Err(error) { Console.printText(error); println(""); }
            }
        }
    }
}
"#;

#[test]
fn sqlite_bindings_types_transactions_and_owned_rows() {
    let output = run_memory(AFFINE_BINDING_AND_ROWS);
    assert_eq!(output.stdout, concat!(
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\n1\n3\n4\n5\n2\n",
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
        "true\n3\n0\n255\n65\ntrue\ntrue\ntrue\n"
    ).as_bytes());
}

#[test]
fn sqlite_transactions_foreign_keys_single_commands_and_errors() {
    let output = run_memory(TRANSACTIONS_AND_ERRORS);
    assert_eq!(output.stdout, "true\n".repeat(26).as_bytes());
}

#[test]
fn sqlite_busy_timeout_is_bounded_between_two_connections() {
    let database = TemporaryDatabase::new();
    let program = compile(BUSY_TIMEOUT);
    let output = Command::new(program.path()).arg(&database.0).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"true\ntrue\ntrue\ntrue\ntrue\n");
    assert_memory_balanced(&output);
}
