use nacre::{compile_to_c, native};
use std::process::Output;

fn execute(source: &str) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("big_numbers.ncr", source)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_success_and_balanced(output: &Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("NACRE_MEMORY allocations="), "{stderr}");
    assert!(stderr.contains("live=0"), "{stderr}");
}

fn assert_lines(output: &Output, expected: &str) {
    assert_success_and_balanced(output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

#[test]
fn bigint_uses_exact_gmp_arithmetic_and_checked_conversions() {
    let source = r#"
void showBigInt(borrow BigInt value) {
    match (value.toText(10)) {
        Ok(text) { text.print(); }
        Err(error) { println("TEXT-ERR"); }
    }
}
void showBigIntResult(Result<BigInt, Text> result) {
    match (result) {
        Ok(value) { showBigInt(value); }
        Err(error) { println("ERR"); }
    }
}
void showTextResult(Result<Text, Text> result) {
    match (result) {
        Ok(value) { value.print(); }
        Err(error) { println("ERR"); }
    }
}
void showIntResult(Result<int, Text> result) {
    match (result) {
        Ok(value) { println(value); }
        Err(error) { println("ERR"); }
    }
}
void showLongResult(Result<long, Text> result) {
    match (result) {
        Ok(value) { println(value); }
        Err(error) { println("ERR"); }
    }
}
void showBooleanResult(Result<boolean, Text> result) {
    match (result) {
        Ok(value) { println(value); }
        Err(error) { println("ERR"); }
    }
}
void main() {
    BigInt two = BigInt.fromLong(2L);
    match (BigInt.parse(new Text("123456789012345678901234567890"), 10)) {
        Err(error) { println("PARSE-ERR"); }
        Ok(large) {
            showBigInt(large);
            BigInt doubled = large.add(large);
            showBigInt(doubled);
            BigInt product = large.multiply(two);
            showBigInt(product);
            BigInt predecessor = large.subtract(two);
            showBigInt(predecessor);
            println(large.compareTo(doubled));
            BigInt copy = large.clone();
            showBigInt(copy);
            showLongResult(large.toLong());
        }
    }
    BigInt minusSeven = BigInt.fromLong(-7L);
    BigInt three = BigInt.fromLong(3L);
    showBigIntResult(minusSeven.divide(three));
    showBigIntResult(minusSeven.remainder(three));
    showBigIntResult(minusSeven.mod(three));
    showBigIntResult(minusSeven.shiftRight(1));
    BigInt anded = minusSeven.bitAnd(three);
    BigInt ored = minusSeven.bitOr(three);
    BigInt xored = minusSeven.bitXor(three);
    BigInt inverted = BigInt.fromLong(5L).bitNot();
    showBigInt(anded);
    showBigInt(ored);
    showBigInt(xored);
    showBigInt(inverted);
    showBooleanResult(minusSeven.testBit(0));
    showBooleanResult(minusSeven.testBit(63));
    showBooleanResult(minusSeven.testBit(-1));
    showBigInt(minusSeven.negate());
    showBigInt(minusSeven.abs());
    println(minusSeven.signum());
    showIntResult(BigInt.fromLong(-8L).bitLength());
    showBigIntResult(minusSeven.shiftLeft(4));
    showBigIntResult(BigInt.fromLong(12L).pow(10));
    showBigIntResult(BigInt.fromLong(12L).pow(-1));
    match (BigInt.fromLong(3L).pow(630000)) {
        Ok(value) { showIntResult(value.bitLength()); }
        Err(error) { println("ERR"); }
    }
    showBigIntResult(BigInt.fromLong(2L).pow(1000000));
    showBigIntResult(BigInt.fromLong(4L).modPow(BigInt.fromLong(13L), BigInt.fromLong(497L)));
    showBigIntResult(BigInt.fromLong(4L).modPow(BigInt.fromLong(-1L), BigInt.fromLong(7L)));
    showBigIntResult(BigInt.fromLong(17L).modInverse(BigInt.fromLong(3120L)));
    showBigIntResult(BigInt.fromLong(6L).modInverse(BigInt.fromLong(9L)));
    BigInt gcd = BigInt.fromLong(-54L).gcd(BigInt.fromLong(24L));
    showBigInt(gcd);
    showBigIntResult(minusSeven.shiftLeft(-1));
    showBigIntResult(minusSeven.divide(BigInt.fromLong(0L)));
    showBigIntResult(minusSeven.mod(BigInt.fromLong(-3L)));
    showBigIntResult(BigInt.parse(new Text(" 12"), 10));
    showBigIntResult(BigInt.parse(new Text("ff"), 1));
    showBigIntResult(BigInt.parse(new Text("-ff"), 16));
    showTextResult(BigInt.fromLong(255L).toText(16));
    showLongResult(BigInt.fromLong(9223372036854775807L).toLong());
    showLongResult(BigInt.fromLong(-9223372036854775807L - 1L).toLong());
}
"#;
    let output = execute(source);
    assert_lines(&output, concat!(
        "123456789012345678901234567890\n",
        "246913578024691357802469135780\n",
        "246913578024691357802469135780\n",
        "123456789012345678901234567888\n",
        "-1\n",
        "123456789012345678901234567890\n",
        "ERR\n",
        "-2\n-1\n2\n-4\n1\n-5\n-6\n-6\ntrue\ntrue\nERR\n",
        "7\n7\n-1\n3\n-112\n",
        "61917364224\nERR\n998527\nERR\n445\nERR\n2753\nERR\n6\nERR\nERR\nERR\nERR\nERR\n",
        "-255\nff\n9223372036854775807\n-9223372036854775808\n",
    ));
}

#[test]
fn bigdecimal_preserves_scale_and_rounds_every_mode_exactly() {
    let source = r#"
void showDecimal(borrow BigDecimal value) {
    match (value.toText()) {
        Ok(text) { text.print(); }
        Err(error) { println("TEXT-ERR"); }
    }
}
void showDecimalResult(Result<BigDecimal, Text> result) {
    match (result) {
        Ok(value) { showDecimal(value); }
        Err(error) { println("ERR"); }
    }
}
void showLongResult(Result<long, Text> result) {
    match (result) {
        Ok(value) { println(value); }
        Err(error) { println("ERR"); }
    }
}
Result<Unit, Text> runDecimalScenarios() {
    BigDecimal parsed = BigDecimal.parse(new Text("1.2300e2"))?;
    println(parsed.scale());
    showDecimal(parsed);
    BigDecimal negativeScale = BigDecimal.parse(new Text("1.2e3"))?;
    println(negativeScale.scale());
    showDecimal(negativeScale);
    showLongResult(negativeScale.toLong());
    BigDecimal a = BigDecimal.parse(new Text("1.20"))?;
    BigDecimal b = BigDecimal.parse(new Text("0.003"))?;
    showDecimalResult(a.add(b));
    BigDecimal first = BigDecimal.parse(new Text("1.0"))?;
    BigDecimal second = BigDecimal.parse(new Text("2.00"))?;
    showDecimalResult(first.subtract(second));
    BigDecimal three = BigDecimal.parse(new Text("3.0"))?;
    showDecimalResult(a.multiply(three));
    BigDecimal one = BigDecimal.fromLong(1L);
    BigDecimal eight = BigDecimal.fromLong(8L);
    BigDecimal minusOne = one.negate();
    println(minusOne.signum());
    BigDecimal absoluteOne = minusOne.abs();
    BigDecimal copiedOne = absoluteOne.clone();
    showDecimal(copiedOne);
    showDecimalResult(one.divide(eight, 2, new RoundingMode.Up()));
    showDecimalResult(one.divide(eight, 2, new RoundingMode.Down()));
    showDecimalResult(one.divide(eight, 2, new RoundingMode.Floor()));
    showDecimalResult(one.divide(eight, 2, new RoundingMode.Ceiling()));
    showDecimalResult(minusOne.divide(eight, 2, new RoundingMode.Floor()));
    showDecimalResult(minusOne.divide(eight, 2, new RoundingMode.Ceiling()));
    showDecimalResult(one.divide(eight, 2, new RoundingMode.HalfUp()));
    showDecimalResult(one.divide(eight, 2, new RoundingMode.HalfDown()));
    showDecimalResult(one.divide(eight, 2, new RoundingMode.HalfEven()));
    BigDecimal onePointThreeSevenFive = BigDecimal.parse(new Text("1.375"))?;
    showDecimalResult(onePointThreeSevenFive.rescale(2, new RoundingMode.HalfEven()));
    showDecimalResult(one.divide(eight, 2, new RoundingMode.Unnecessary()));
    BigDecimal two = BigDecimal.fromLong(2L);
    showDecimalResult(one.divide(two, 2, new RoundingMode.Unnecessary()));
    BigDecimal onePointTwoFive = BigDecimal.parse(new Text("1.25"))?;
    showDecimalResult(onePointTwoFive.rescale(1, new RoundingMode.HalfUp()));
    showDecimalResult(onePointTwoFive.rescale(1, new RoundingMode.HalfDown()));
    showDecimalResult(onePointTwoFive.rescale(1, new RoundingMode.HalfEven()));
    BigDecimal onePointZero = BigDecimal.parse(new Text("1.0"))?;
    BigDecimal onePointZeroZero = BigDecimal.parse(new Text("1.00"))?;
    println(onePointZero.compareTo(onePointZeroZero));
    BigDecimal whole = BigDecimal.parse(new Text("1200.00"))?;
    showLongResult(whole.toLong());
    showLongResult(onePointTwoFive.toLong());
    BigDecimal tooLarge = BigDecimal.parse(new Text("9223372036854775808"))?;
    showLongResult(tooLarge.toLong());
    showDecimalResult(one.divide(BigDecimal.fromLong(0L), 2, new RoundingMode.Down()));
    showDecimalResult(BigDecimal.parse(new Text("1e100001")));
    showDecimalResult(BigDecimal.parse(new Text("NaN")));
    showDecimalResult(BigDecimal.parse(new Text(" 1")));
    showDecimalResult(BigDecimal.parse(new Text("1.0x")));
    BigDecimal scaleLimit = BigDecimal.parse(new Text("1e100000"))?;
    BigDecimal onePower = BigDecimal.parse(new Text("1e1"))?;
    showDecimalResult(scaleLimit.multiply(onePower));
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (runDecimalScenarios()) {
        Ok(success) { }
        Err(error) { println("UNEXPECTED-PARSE-ERROR"); }
    }
}
"#;
    let output = execute(source);
    assert_lines(&output, concat!(
        "2\n123.00\n-2\n1200\n1200\n1.203\n-1.00\n3.600\n-1\n1\n",
        "0.13\n0.12\n0.12\n0.13\n-0.13\n-0.12\n",
        "0.13\n0.12\n0.12\n1.38\nERR\n0.50\n",
        "1.3\n1.2\n1.2\n0\n1200\nERR\nERR\nERR\nERR\nERR\nERR\nERR\nERR\n",
    ));
}
