use nacre::validate;
mod support;
use std::process::{Command, Output};

const EXAMPLE: &str = include_str!("../examples/language/error_propagation.ncr");

fn execute(source: &str) -> Output {
    let program = support::compile(
        source,
        &[
            "-DNACRE_MEMORY_STATS",
            "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer",
        ],
    )
    .unwrap();
    let output = Command::new(program.path()).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_live_zero(&output);
    output
}

fn assert_live_zero(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "{stderr}"
    );
}

fn assert_rejected(source: &str) {
    assert!(validate(source).is_err(), "invalid propagation program was accepted");
}

#[test]
fn postfix_try_moves_success_values_and_propagates_owned_errors() {
    let output = execute(EXAMPLE);
    let actual = std::str::from_utf8(&output.stdout).unwrap();
    let values = actual.lines().filter(|line| !line.starts_with("Text.parseInt:"));
    let expected =
        concat!(
            "TEXT OK\ncopied text\n",
            "PENDING ARG OK\n41\n",
            "NESTED CONSTRUCTOR OK\n41\n",
            "OBJECT VALUE OK\n43\n",
            "ENUM VALUE OK\nenum payload\n",
            "INTERFACE VALUE OK\n61\n",
            "OBJECT ERROR\n71\n",
            "ENUM ERROR\nenum failure\n",
            "INTERFACE ERROR\n72\n",
            "PENDING ARG ERROR\n",
            "NESTED CONSTRUCTOR ERROR\n",
            "SHORT-CIRCUIT SKIP OK\n1\n",
            "SHORT-CIRCUIT ERROR\nshort-circuit failure\n",
            "NESTED LOOP ERROR\ninner failure\n",
            "LOOP CONDITION ERROR\ncondition failure\n",
        );
    assert!(values.eq(expected.lines()), "{actual}");
}


#[test]
fn postfix_try_rejects_invalid_contexts_and_carriers() {
    assert_rejected(
        "int outside() { return new Result<int, Text>.Ok(1)?; } void main() {}",
    );
    assert_rejected(
        "Result<int, Text> fromText() { return new Result<int, Text>.Ok(1); } class Box { public Box() {} } Result<int, Box> wrongError() { return fromText()?; } void main() {}",
    );
    assert_rejected(
        "enum Pretend { Ok(int); Err(Text); } Result<int, Text> spoof() { int value = new Pretend.Ok(1)?; return new Result<int, Text>.Ok(value); } void main() {}",
    );
    assert_rejected(
        "Result<int, Text> notResult() { int value = 1?; return new Result<int, Text>.Ok(value); } void main() {}",
    );
    assert_rejected(
        "Result<int, Text> borrowed(borrow Result<int, Text> value) { return new Result<int, Text>.Ok(value?); } void main() {}",
    );
    assert_rejected(
        "class Factory { public Factory() { int value = new Result<int, Text>.Ok(1)?; } } void main() {}",
    );
}

#[test]
fn standalone_propagation_drops_ignored_success_owners_and_preserves_errors() {
    let source = r#"
Result<Text, Text> value(boolean successful) {
    if (successful) { return new Result<Text, Text>.Ok(new Text("discarded owner")); }
    return new Result<Text, Text>.Err(new Text("failure"));
}
Result<int, Text> number() { return new Result<int, Text>.Ok(42); }
Result<Unit, Text> unit() { return new Result<Unit, Text>.Ok(()); }
Result<Unit, Text> exercise(boolean fail) {
    Text local = new Text("live local");
    unit()?;
    number()?;
    value(true)?;
    if (fail) { value(false)?; }
    println("continued");
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise(false)) { Ok(done) { println(done); } Err(error) { println("unexpected"); } }
    match (exercise(true)) { Ok(done) { println("unexpected"); } Err(error) { println(error.equalsString("failure")); } }
}
"#;
    assert_eq!(execute(source).stdout, b"continued\n()\ntrue\n");
}
