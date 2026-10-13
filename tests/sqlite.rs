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
Result<boolean, Text> borrowedCountMatches(borrow SqlStatement statement, int expected) {
    Text name = statement.columnName(0)?;
    int count = statement.columnCount()?;
    return new Result<boolean, Text>.Ok(count == expected && name.equalsString("id"));
}
Result<int, Text> borrowedColumnType(borrow SqlStatement statement) {
    return statement.columnType(0);
}
Result<Unit, Text> exercise() {
    Text path = new Text(":memory:");
    Database database = Database.open(path)?;
    Text create = new Text("CREATE TABLE sample (id INTEGER PRIMARY KEY, text_value TEXT, blob_value BLOB, nullable INTEGER, huge INTEGER, real_value REAL)");
    database.execute(create)?;

    Text insert = new Text("INSERT INTO sample(text_value, blob_value, nullable, huge, real_value) VALUES(?,?,?,?,?)");
    SqlStatement insertStatement = database.prepare(insert)?;
    Text injected = new Text("x'); DROP TABLE sample;--\0tail");
    ByteBuffer binary = ByteBuffer.withCapacity(0L)?;
    binary.append((byte)0);
    binary.append((byte)255);
    binary.append((byte)65);
    long lowest = -9223372036854775807L - 1L;
    insertStatement.bindText(1, injected)?;
    insertStatement.bindBlob(2, binary)?;
    insertStatement.bindNull(3)?;
    insertStatement.bindLong(4, lowest)?;
    insertStatement.bindDouble(5, 1.25)?;
    boolean insertHasRow = insertStatement.step()?;
    println(!insertHasRow);
    println(database.changes()? == 1L);
    println(database.lastInsertRowId()? == 1L);
    println(!insertStatement.step()?);
    println(database.lastInsertRowId()? == 1L);
    drop(injected);
    drop(binary);

    Text query = new Text("SELECT id AS id, text_value AS literal, blob_value AS payload, nullable, huge, real_value FROM sample");
    SqlStatement queryStatement = database.prepare(query)?;
    println(borrowedCountMatches(queryStatement, 6)?);
    Text columnName = queryStatement.columnName(0)?;
    println(columnName.equalsString("id"));
    println(queryStatement.step()?);
    println(borrowedColumnType(queryStatement)?);
    println(queryStatement.columnType(1)?);
    println(queryStatement.columnType(2)?);
    println(queryStatement.columnType(3)?);
    println(queryStatement.columnType(5)?);
    println(optionLongEquals(queryStatement.columnLong(0)?, 1L));
    Option<Text> copiedText = queryStatement.columnText(1)?;
    Option<ByteBuffer> copiedBlob = queryStatement.columnBlob(2)?;
    Option<long> copiedNull = queryStatement.columnLong(3)?;
    Option<long> copiedLong = queryStatement.columnLong(4)?;
    Option<double> copiedDouble = queryStatement.columnDouble(5)?;
    boolean queryHasMore = queryStatement.step()?;
    println(!queryHasMore);
    println(!queryStatement.step()?);
    Text emptyBlobQuery = new Text("SELECT CAST(x'' AS BLOB)");
    SqlStatement emptyBlobStatement = database.prepare(emptyBlobQuery)?;
    println(emptyBlobStatement.step()?);
    match (emptyBlobStatement.columnBlob(0)?) {
        None { println(false); }
        Some(bytes) { println(bytes.length() == 0L); }
    }

    Text parameterQuery = new Text("SELECT ?");
    SqlStatement parameterStatement = database.prepare(parameterQuery)?;
    parameterStatement.bindLong(1, 42L)?;
    println(parameterStatement.step()?);
    println(optionLongEquals(parameterStatement.columnLong(0)?, 42L));
    parameterStatement.reset()?;
    parameterStatement.clearBindings()?;
    println(parameterStatement.step()?);
    println(optionLongIsNone(parameterStatement.columnLong(0)?));
    parameterStatement.reset()?;
    parameterStatement.bindLong(1, 43L)?;
    println(parameterStatement.step()?);
    println(optionLongEquals(parameterStatement.columnLong(0)?, 43L));

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
boolean optionTextFailed(Result<Option<Text>, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean boolFailed(Result<boolean, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean databaseFailed(Result<Database, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean statementFailed(Result<SqlStatement, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean optionLongFailed(Result<Option<long>, Text> result) {
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
    Text nulQuery = new Text("SELECT 1\0");
    println(statementFailed(database.prepare(nulQuery)));
    Text parentSchema = new Text("CREATE TABLE parent (id INTEGER PRIMARY KEY)");
    database.execute(parentSchema)?;
    Text childSchema = new Text("CREATE TABLE child (parent_id INTEGER NOT NULL REFERENCES parent(id))");
    database.execute(childSchema)?;
    Text primaryPragma = new Text("PRAGMA foreign_keys=OFF; SELECT 1");
    Text tailPragma = new Text("SELECT 1; PRAGMA foreign_keys=OFF");
    Text invalidChild = new Text("INSERT INTO child VALUES(98)");
    println(statementFailed(database.prepare(primaryPragma)));
    println(longFailed(database.execute(invalidChild)));
    println(statementFailed(database.prepare(tailPragma)));
    println(longFailed(database.execute(invalidChild)));
    println(longFailed(database.execute(primaryPragma)));
    println(longFailed(database.execute(invalidChild)));
    println(longFailed(database.execute(tailPragma)));
    println(longFailed(database.execute(invalidChild)));
    Text eventSourceTable = new Text("CREATE TABLE event_source (payload TEXT)");
    Text eventLogTable = new Text("CREATE TABLE event_log (payload TEXT)");
    database.execute(eventSourceTable)?;
    database.execute(eventLogTable)?;
    Text eventTrigger = new Text("CREATE TRIGGER capture_event AFTER INSERT ON event_source BEGIN INSERT INTO event_log(payload) VALUES(CASE WHEN NEW.payload = 'source;value' THEN 'trigger;case' ELSE NEW.payload END); INSERT INTO event_log(payload) VALUES('second;trigger'); END;");
    database.execute(eventTrigger)?;
    Text addEvent = new Text("INSERT INTO event_source(payload) VALUES('source;value')");
    database.execute(addEvent)?;
    Text eventCount = new Text("SELECT COUNT(*) FROM event_log");
    SqlStatement eventCountStatement = database.prepare(eventCount)?;
    println(eventCountStatement.step()?);
    println(optionLongEquals(eventCountStatement.columnLong(0)?, 2L));
    eventCountStatement.step()?;
    eventCountStatement.close()?;
    Text disableForeignKeys = new Text("PRAGMA foreign_keys=OFF");
    Text enableForeignKeys = new Text("PRAGMA foreign_keys=ON");
    SqlStatement disableStatement = database.prepare(disableForeignKeys)?;
    disableStatement.step()?;
    disableStatement.close()?;
    SqlStatement enableStatement = database.prepare(enableForeignKeys)?;
    enableStatement.step()?;
    enableStatement.close()?;
    database.begin()?;
    Text insertParent1 = new Text("INSERT INTO parent VALUES(1)");
    database.execute(insertParent1)?;
    database.commit()?;
    database.begin()?;
    Text insertParent2 = new Text("INSERT INTO parent VALUES(2)");
    database.execute(insertParent2)?;
    database.rollback()?;
    database.begin()?;
    Text invalidChild99 = new Text("INSERT INTO child VALUES(99)");
    println(longFailed(database.execute(invalidChild99)));
    database.rollback()?;

    Text countParents = new Text("SELECT COUNT(*) FROM parent");
    SqlStatement countParentsStatement = database.prepare(countParents)?;
    println(countParentsStatement.step()?);
    Option<long> count = countParentsStatement.columnLong(0)?;
    println(optionLongEquals(count, 1L));
    countParentsStatement.close()?;

    Text rowQuery = new Text("SELECT 1");
    println(longFailed(database.execute(rowQuery)));
    Text markerSchema = new Text("CREATE TABLE marker (id INTEGER)");
    database.execute(markerSchema)?;
    Text multipleInserts = new Text("INSERT INTO marker VALUES(1); INSERT INTO marker VALUES(2)");
    println(longFailed(database.execute(multipleInserts)));
    Text countMarkers = new Text("SELECT COUNT(*) FROM marker");
    SqlStatement countMarkersStatement = database.prepare(countMarkers)?;
    println(countMarkersStatement.step()?);
    println(optionLongEquals(countMarkersStatement.columnLong(0)?, 0L));
    countMarkersStatement.close()?;

    Text commentedQuery = new Text("SELECT 77; /* block comment */ -- trailing comment\n");
    SqlStatement commentedStatement = database.prepare(commentedQuery)?;
    Text multipleQueries = new Text("SELECT 1; SELECT 2");
    println(statementFailed(database.prepare(multipleQueries)));
    println(commentedStatement.step()?);
    println(optionLongEquals(commentedStatement.columnLong(0)?, 77L));
    println(optionTextFailed(commentedStatement.columnText(0)));
    println(optionLongFailed(commentedStatement.columnLong(1)));
    println(textFailed(commentedStatement.columnName(1)));
    println(intFailed(commentedStatement.columnType(1)));
    commentedStatement.close()?;

    Text parameterQuery = new Text("SELECT ?");
    SqlStatement parameterStatement = database.prepare(parameterQuery)?;
    println(unitFailed(parameterStatement.bindNull(0)));
    println(unitFailed(parameterStatement.bindLong(2, 9L)));
    parameterStatement.bindLong(1, 9L)?;
    println(parameterStatement.step()?);
    println(optionLongEquals(parameterStatement.columnLong(0)?, 9L));
    println(unitFailed(database.setBusyTimeoutMillis(-1L)));
    println(unitFailed(database.setBusyTimeoutMillis(2147483648L)));

    database.close()?;
    database.close()?;
    println(statementFailed(database.prepare(rowQuery)));
    println(boolFailed(parameterStatement.step()));
    println(intFailed(parameterStatement.columnCount()));
    println(unitFailed(parameterStatement.reset()));
    println(unitFailed(parameterStatement.clearBindings()));
    println(unitFailed(parameterStatement.bindLong(1, 10L)));
    println(optionLongFailed(parameterStatement.columnLong(0)));
    parameterStatement.close()?;
    Text closedSchema = new Text("CREATE TABLE closed_test (id INTEGER)");
    println(longFailed(database.execute(closedSchema)));
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
    Text insert = new Text("INSERT INTO held VALUES(3)");
    abandoned.execute(insert)?;
    Text select = new Text("SELECT value FROM held");
    SqlStatement statement = abandoned.prepare(select)?;
    println(statement.step()?);
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> exercise(borrow Text path) {
    Database first = Database.open(path)?;
    Database second = Database.open(path)?;
    Text schema = new Text("CREATE TABLE IF NOT EXISTS held (value INTEGER)");
    first.execute(schema)?;
    first.begin()?;
    Text firstInsert = new Text("INSERT INTO held VALUES(1)");
    first.execute(firstInsert)?;
    second.setBusyTimeoutMillis(150L)?;
    long start = Time.monotonicNanos()?;
    Text secondInsert = new Text("INSERT INTO held VALUES(2)");
    boolean busy = longFailed(second.execute(secondInsert));
    long elapsed = Time.monotonicNanos()? - start;
    first.rollback()?;
    println(busy);
    println(elapsed >= 75000000L && elapsed < 5000000000L);
    abandonTransaction(path)?;
    Database verify = Database.open(path)?;
    Text count = new Text("SELECT COUNT(*) FROM held");
    SqlStatement countStatement = verify.prepare(count)?;
    println(countStatement.step()?);
    println(optionLongEquals(countStatement.columnLong(0)?, 0L));
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
fn sqlite_transactions_foreign_keys_single_commands_and_errors() {
    let output = run_memory(TRANSACTIONS_AND_ERRORS);
    assert_eq!(output.stdout, "true\n".repeat(40).as_bytes());
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

const INDEPENDENT_STATEMENTS: &str = r#"
boolean boolFailed(Result<boolean, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean intFailed(Result<int, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean unitFailed(Result<Unit, Text> result) {
    match (result) { Ok(unit) { return false; } Err(error) { return true; } }
}
boolean statementFailed(Result<SqlStatement, Text> result) {
    match (result) { Ok(statement) { return false; } Err(error) { return true; } }
}
boolean optionLongFailed(Result<Option<long>, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean optionLongEquals(Option<long> option, long expected) {
    match (option) { Some(value) { return value == expected; } None { return false; } }
}
Result<SqlStatement, Text> statementAfterDatabaseDrop() {
    Text memory = new Text(":memory:");
    Database database = Database.open(memory)?;
    Text sql = new Text("SELECT 73");
    SqlStatement statement = database.prepare(sql)?;
    return new Result<SqlStatement, Text>.Ok(statement);
}
Result<Unit, Text> discardStatement(SqlStatement statement) {
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> exercise() {
    Text memory = new Text(":memory:");
    Database database = Database.open(memory)?;
    Text schema = new Text("CREATE TABLE items(value INTEGER)");
    database.execute(schema)?;
    Text seed = new Text("INSERT INTO items VALUES(10),(20),(30)");
    database.execute(seed)?;
    Text auditSchema = new Text("CREATE TABLE audit(value INTEGER)");
    database.execute(auditSchema)?;

    database.begin()?;
    Text transactionInsert = new Text("INSERT INTO items VALUES(40)");
    database.execute(transactionInsert)?;
    Text transactionSql = new Text("SELECT value FROM items WHERE value=40");
    SqlStatement transactionStatement = database.prepare(transactionSql)?;
    println(transactionStatement.step()?);
    println(optionLongEquals(transactionStatement.columnLong(0)?, 40L));
    transactionStatement.close()?;
    database.rollback()?;

    Text leftSql = new Text("SELECT value FROM items WHERE value >= ? ORDER BY value");
    Text rightSql = new Text("SELECT value FROM items WHERE value <= ? ORDER BY value");
    SqlStatement left = database.prepare(leftSql)?;
    SqlStatement right = database.prepare(rightSql)?;
    left.bindLong(1, 10L)?;
    right.bindLong(1, 30L)?;
    println(left.step()?);
    println(optionLongEquals(left.columnLong(0)?, 10L));
    println(right.step()?);
    println(optionLongEquals(right.columnLong(0)?, 10L));

    Text auditInsert = new Text("INSERT INTO audit VALUES(1)");
    database.execute(auditInsert)?;
    Text multiple = new Text("SELECT 1; SELECT 2");
    println(statementFailed(database.prepare(multiple)));
    println(optionLongEquals(right.columnLong(0)?, 10L));
    Text independentSql = new Text("SELECT 999");
    SqlStatement independent = database.prepare(independentSql)?;
    println(independent.step()?);
    println(optionLongEquals(independent.columnLong(0)?, 999L));
    independent.close()?;

    left.reset()?;
    left.clearBindings()?;
    left.bindLong(1, 30L)?;
    println(right.step()?);
    println(optionLongEquals(right.columnLong(0)?, 20L));
    println(left.step()?);
    println(optionLongEquals(left.columnLong(0)?, 30L));
    println(right.step()?);
    println(optionLongEquals(right.columnLong(0)?, 30L));
    println(!left.step()?);
    println(!right.step()?);

    Text dropSql = new Text("SELECT value FROM items");
    SqlStatement dropped = database.prepare(dropSql)?;
    println(dropped.step()?);
    discardStatement(dropped)?;
    Text afterDrop = new Text("INSERT INTO audit VALUES(2)");
    database.execute(afterDrop)?;

    database.close()?;
    println(boolFailed(left.step()));
    println(boolFailed(right.step()));
    println(intFailed(left.columnCount()));
    println(unitFailed(right.reset()));
    println(unitFailed(left.clearBindings()));
    println(unitFailed(right.bindLong(1, 50L)));
    println(optionLongFailed(left.columnLong(0)));
    left.close()?;
    right.close()?;

    match (statementAfterDatabaseDrop()) {
        Err(error) { println(false); println(false); }
        Ok(orphan) {
            println(boolFailed(orphan.step()));
            println(!unitFailed(orphan.close()));
        }
    }
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(unit) { }
        Err(error) { Console.printText(error); println(""); }
    }
}
"#;

#[test]
fn sqlite_prepared_statements_are_independent_and_close_v2_defers_connections() {
    let output = run_memory(INDEPENDENT_STATEMENTS);
    assert_eq!(output.stdout, "true\n".repeat(28).as_bytes());
}

#[test]
fn sqlite_bom_triggers_and_leading_empty_commands_preserve_sqlite_grammar() {
    let source = r#"
Result<Unit, Text> exercise() {
    Text memory = new Text(":memory:");
    Database database = Database.open(memory)?;
    Text table = new Text("CREATE TABLE t(x INTEGER)");
    Text audit = new Text("CREATE TABLE audit(payload TEXT)");
    database.execute(table)?;
    database.execute(audit)?;
    Text leadingQuery = new Text("; /* empty commands */ ; SELECT 73");
    SqlStatement leadingStatement = database.prepare(leadingQuery)?;
    println(leadingStatement.step()?);
    Option<long> result = leadingStatement.columnLong(0)?;
    match (result) { None { println(false); } Some(value) { println(value == 73L); } }
    leadingStatement.step()?;
    leadingStatement.close()?;
    Text leadingInsert = new Text("; ; INSERT INTO t VALUES(11)");
    database.execute(leadingInsert)?;
    // Both SQL inputs below begin with a literal UTF-8 BOM.
    Text executedTrigger = new Text("﻿CREATE TRIGGER executed AFTER INSERT ON t BEGIN INSERT INTO audit VALUES('exec;value'); END;");
    database.execute(executedTrigger)?;
    Text preparedTrigger = new Text("﻿CREATE TEMP TRIGGER prepared AFTER INSERT ON t BEGIN INSERT INTO audit VALUES(CASE WHEN NEW.x = 22 THEN 'prepare;value' ELSE 'wrong' END); END;");
    SqlStatement preparedTriggerStatement = database.prepare(preparedTrigger)?;
    preparedTriggerStatement.step()?;
    preparedTriggerStatement.close()?;
    Text insert = new Text("INSERT INTO t VALUES(22)");
    database.execute(insert)?;
    Text query = new Text("SELECT payload FROM audit ORDER BY payload");
    SqlStatement queryStatement = database.prepare(query)?;
    println(queryStatement.step()?);
    Option<Text> first = queryStatement.columnText(0)?;
    match (first) { None { println(false); } Some(value) { println(value.equalsString("exec;value")); } }
    println(queryStatement.step()?);
    Option<Text> second = queryStatement.columnText(0)?;
    match (second) { None { println(false); } Some(value) { println(value.equalsString("prepare;value")); } }
    println(!queryStatement.step()?);
    queryStatement.close()?;
    Text values = new Text("SELECT SUM(x) FROM t");
    SqlStatement valuesStatement = database.prepare(values)?;
    println(valuesStatement.step()?);
    Option<long> total = valuesStatement.columnLong(0)?;
    match (total) { None { println(false); } Some(value) { println(value == 33L); } }
    valuesStatement.close()?;
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(value) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = run_memory(source);
    assert_eq!(output.stdout, "true\n".repeat(9).as_bytes());
}

#[test]
fn sqlite_statements_interleave_and_finalize_in_any_owner_order() {
    let source = r#"
boolean optionLongEquals(Option<long> option, long expected) {
    match (option) { Some(value) { return value == expected; } None { return false; } }
}
boolean optionLongIsNone(Option<long> option) {
    match (option) { None { return true; } Some(value) { return false; } }
}
boolean boolFailed(Result<boolean, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean unitFailed(Result<Unit, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
Result<Unit, Text> exercise() {
    Text memory = new Text(":memory:");
    Database database = Database.open(memory)?;
    Text sink = new Text("CREATE TABLE sink (value INTEGER)");
    database.execute(sink)?;
    Text leftSql = new Text("SELECT ? UNION ALL SELECT ?");
    Text rightSql = new Text("SELECT ? UNION ALL SELECT ?");
    SqlStatement left = database.prepare(leftSql)?;
    SqlStatement right = database.prepare(rightSql)?;
    left.bindLong(1, 11L)?;
    left.bindLong(2, 12L)?;
    right.bindLong(1, 21L)?;
    right.bindLong(2, 22L)?;
    println(left.step()?);
    println(optionLongEquals(left.columnLong(0)?, 11L));
    println(right.step()?);
    println(optionLongEquals(right.columnLong(0)?, 21L));
    Text insert = new Text("INSERT INTO sink VALUES(1)");
    database.execute(insert)?;
    left.reset()?;
    left.bindLong(1, 31L)?;
    left.bindLong(2, 32L)?;
    println(left.step()?);
    println(optionLongEquals(left.columnLong(0)?, 31L));
    println(right.step()?);
    println(optionLongEquals(right.columnLong(0)?, 22L));
    println(left.step()?);
    println(optionLongEquals(left.columnLong(0)?, 32L));
    println(!right.step()?);
    println(!left.step()?);
    left.reset()?;
    left.clearBindings()?;
    println(left.step()?);
    println(optionLongIsNone(left.columnLong(0)?));
    right.reset()?;
    println(right.step()?);
    println(optionLongEquals(right.columnLong(0)?, 21L));
    left.close()?;
    println(boolFailed(left.step()));
    database.close()?;
    println(boolFailed(right.step()));
    println(!unitFailed(right.close()));
    println(!unitFailed(right.close()));
    drop(database);
    drop(left);
    drop(right);

    Database abandoned = Database.open(memory)?;
    SqlStatement orphan = abandoned.prepare(leftSql)?;
    drop(abandoned);
    println(boolFailed(orphan.step()));
    println(!unitFailed(orphan.close()));
    drop(orphan);
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(unit) { }
        Err(error) { Console.printText(error); println(""); }
    }
}
"#;
    let output = run_memory(source);
    assert_eq!(output.stdout, "true\n".repeat(22).as_bytes());
}

#[test]
fn sqlite_bindings_types_transactions_and_owned_rows() {
    let output = run_memory(AFFINE_BINDING_AND_ROWS);
    assert_eq!(output.stdout, concat!(
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n1\n3\n4\n5\n2\n",
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
        "true\n3\n0\n255\n65\ntrue\ntrue\ntrue\n"
    ).as_bytes());
}

#[test]
fn sqlite_close_and_drop_roll_back_before_retained_statements_are_finalized() {
    let source = r#"
boolean insertSucceeds(borrow mut Database database, borrow Text command) {
    match (database.execute(command)) {
        Ok(changes) { return changes == 1L; }
        Err(error) { return false; }
    }
}
boolean longEquals(Option<long> value, long expected) {
    match (value) { Some(actual) { return actual == expected; } None { return false; } }
}
Result<Unit, Text> exercise() {
    Text path = Args.get(0)?;
    Database first = Database.open(path)?;
    Text schema = new Text("CREATE TABLE items(value INTEGER)");
    first.execute(schema)?;
    Database second = Database.open(path)?;
    second.setBusyTimeoutMillis(0L)?;
    Text query = new Text("SELECT value FROM items");
    Text pendingInsert = new Text("INSERT INTO items VALUES(10)");
    Text committedInsert = new Text("INSERT INTO items VALUES(20)");
    first.begin()?;
    first.execute(pendingInsert)?;
    SqlStatement retained = first.prepare(query)?;
    first.close()?;
    println(insertSucceeds(second, committedInsert));
    retained.close()?;
    Database abandoned = Database.open(path)?;
    abandoned.begin()?;
    abandoned.execute(pendingInsert)?;
    SqlStatement orphan = abandoned.prepare(query)?;
    drop(abandoned);
    println(insertSucceeds(second, committedInsert));
    orphan.close()?;
    Text aggregate = new Text("SELECT COUNT(*), SUM(value) FROM items");
    SqlStatement rows = second.prepare(aggregate)?;
    println(rows.step()? && longEquals(rows.columnLong(0)?, 2L)
        && longEquals(rows.columnLong(1)?, 40L));
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) { Ok(unit) {} Err(error) { error.print(); } }
}
"#;
    let database = TemporaryDatabase::new();
    let program = compile(source);
    let output = Command::new(program.path()).arg(&database.0).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"true\ntrue\ntrue\n");
    assert_memory_balanced(&output);
}
