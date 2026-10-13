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
Result<BigInt, Text> parseInteger(Text value, int radix) {
    return BigInt.parse(value, radix);
}
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
    match (parseInteger(new Text("123456789012345678901234567890"), 10)) {
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
    BigInt five = BigInt.fromLong(5L);
    BigInt inverted = five.bitNot();
    showBigInt(anded);
    showBigInt(ored);
    showBigInt(xored);
    showBigInt(inverted);
    showBooleanResult(minusSeven.testBit(0));
    showBooleanResult(minusSeven.testBit(63));
    showBooleanResult(minusSeven.testBit(-1));
    BigInt negated = minusSeven.negate();
    BigInt absolute = minusSeven.abs();
    showBigInt(negated);
    showBigInt(absolute);
    println(minusSeven.signum());
    BigInt minusEight = BigInt.fromLong(-8L);
    showIntResult(minusEight.bitLength());
    showBigIntResult(minusSeven.shiftLeft(4));
    BigInt twelve = BigInt.fromLong(12L);
    showBigIntResult(twelve.pow(10));
    showBigIntResult(twelve.pow(-1));
    match (three.pow(630000)) {
        Ok(value) { showIntResult(value.bitLength()); }
        Err(error) { println("ERR"); }
    }
    showBigIntResult(two.pow(1000000));
    BigInt four = BigInt.fromLong(4L);
    BigInt thirteen = BigInt.fromLong(13L);
    BigInt modulus497 = BigInt.fromLong(497L);
    BigInt minusOne = BigInt.fromLong(-1L);
    BigInt seven = BigInt.fromLong(7L);
    BigInt seventeen = BigInt.fromLong(17L);
    BigInt modulus3120 = BigInt.fromLong(3120L);
    BigInt six = BigInt.fromLong(6L);
    BigInt nine = BigInt.fromLong(9L);
    showBigIntResult(four.modPow(thirteen, modulus497));
    showBigIntResult(four.modPow(minusOne, seven));
    showBigIntResult(seventeen.modInverse(modulus3120));
    showBigIntResult(six.modInverse(nine));
    BigInt minus54 = BigInt.fromLong(-54L);
    BigInt twentyFour = BigInt.fromLong(24L);
    BigInt gcd = minus54.gcd(twentyFour);
    showBigInt(gcd);
    showBigIntResult(minusSeven.shiftLeft(-1));
    BigInt zero = BigInt.fromLong(0L);
    BigInt minusThree = BigInt.fromLong(-3L);
    showBigIntResult(minusSeven.divide(zero));
    showBigIntResult(minusSeven.mod(minusThree));
    showBigIntResult(parseInteger(new Text(" 12"), 10));
    showBigIntResult(parseInteger(new Text("ff"), 1));
    showBigIntResult(parseInteger(new Text("-ff"), 16));
    BigInt byteMax = BigInt.fromLong(255L);
    showTextResult(byteMax.toText(16));
    BigInt longMax = BigInt.fromLong(9223372036854775807L);
    BigInt longMin = BigInt.fromLong(-9223372036854775807L - 1L);
    showLongResult(longMax.toLong());
    showLongResult(longMin.toLong());
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
Result<BigDecimal, Text> parseDecimal(Text value) {
    return BigDecimal.parse(value);
}
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
    BigDecimal parsed = parseDecimal(new Text("1.2300e2"))?;
    println(parsed.scale());
    showDecimal(parsed);
    BigDecimal negativeScale = parseDecimal(new Text("1.2e3"))?;
    println(negativeScale.scale());
    showDecimal(negativeScale);
    showLongResult(negativeScale.toLong());
    BigDecimal a = parseDecimal(new Text("1.20"))?;
    BigDecimal b = parseDecimal(new Text("0.003"))?;
    showDecimalResult(a.add(b));
    BigDecimal first = parseDecimal(new Text("1.0"))?;
    BigDecimal second = parseDecimal(new Text("2.00"))?;
    showDecimalResult(first.subtract(second));
    BigDecimal three = parseDecimal(new Text("3.0"))?;
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
    BigDecimal onePointThreeSevenFive = parseDecimal(new Text("1.375"))?;
    showDecimalResult(onePointThreeSevenFive.rescale(2, new RoundingMode.HalfEven()));
    showDecimalResult(one.divide(eight, 2, new RoundingMode.Unnecessary()));
    BigDecimal two = BigDecimal.fromLong(2L);
    showDecimalResult(one.divide(two, 2, new RoundingMode.Unnecessary()));
    BigDecimal onePointTwoFive = parseDecimal(new Text("1.25"))?;
    showDecimalResult(onePointTwoFive.rescale(1, new RoundingMode.HalfUp()));
    showDecimalResult(onePointTwoFive.rescale(1, new RoundingMode.HalfDown()));
    showDecimalResult(onePointTwoFive.rescale(1, new RoundingMode.HalfEven()));
    BigDecimal negativeOnePointTwoFive = onePointTwoFive.negate();
    showDecimalResult(negativeOnePointTwoFive.rescale(1, new RoundingMode.HalfUp()));
    showDecimalResult(negativeOnePointTwoFive.rescale(1, new RoundingMode.HalfDown()));
    showDecimalResult(negativeOnePointTwoFive.rescale(1, new RoundingMode.HalfEven()));
    BigDecimal onePointZero = parseDecimal(new Text("1.0"))?;
    BigDecimal onePointZeroZero = parseDecimal(new Text("1.00"))?;
    println(onePointZero.compareTo(onePointZeroZero));
    BigDecimal whole = parseDecimal(new Text("1200.00"))?;
    showLongResult(whole.toLong());
    showLongResult(onePointTwoFive.toLong());
    BigDecimal tooLarge = parseDecimal(new Text("9223372036854775808"))?;
    showLongResult(tooLarge.toLong());
    BigDecimal zero = BigDecimal.fromLong(0L);
    showDecimalResult(one.divide(zero, 2, new RoundingMode.Down()));
    showDecimalResult(parseDecimal(new Text("1e100001")));
    showDecimalResult(parseDecimal(new Text("NaN")));
    showDecimalResult(parseDecimal(new Text(" 1")));
    showDecimalResult(parseDecimal(new Text("1.0x")));
    BigDecimal scaleLimit = parseDecimal(new Text("1e100000"))?;
    BigDecimal onePower = parseDecimal(new Text("1e1"))?;
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
        "1.3\n1.2\n1.2\n",
        "-1.3\n-1.2\n-1.2\n0\n1200\nERR\nERR\nERR\nERR\nERR\nERR\nERR\nERR\n",
    ));
}
#[test]
fn bigint_apis_compile_without_bigdecimal_and_accept_both_signs() {
    let source = r#"
void show(Result<BigInt, Text> result) {
    match (result) {
        Ok(value) {
            match (value.toText(10)) {
                Ok(text) { text.print(); }
                Err(error) { println("ERR"); }
            }
        }
        Err(error) { println("ERR"); }
    }
}
void main() {
    Text positive = new Text("+123");
    Text negative = new Text("-123");
    show(BigInt.parse(positive, 10));
    show(BigInt.parse(negative, 10));
}
"#;
    let output = execute(source);
    assert_lines(&output, "123\n-123\n");
}

#[test]
fn bigdecimal_apis_compile_without_bigint_and_keep_signs_and_rounding() {
    let source = r#"
void show(Result<BigDecimal, Text> result) {
    match (result) {
        Ok(value) {
            match (value.toText()) {
                Ok(text) { text.print(); }
                Err(error) { println("ERR"); }
            }
        }
        Err(error) { println("ERR"); }
    }
}
void main() {
    Text positive = new Text("+12.50");
    Text negative = new Text("-12.50");
    show(BigDecimal.parse(positive));
    show(BigDecimal.parse(negative));
    BigDecimal one = BigDecimal.fromLong(1L);
    BigDecimal eight = BigDecimal.fromLong(8L);
    show(one.divide(eight, 2, new RoundingMode.HalfUp()));
}
"#;
    let output = execute(source);
    assert_lines(&output, "12.50\n-12.50\n0.13\n");
}

#[test]
fn numeric_parse_enforces_coefficient_limits_and_strips_leading_zeroes() {
    for (class, digits, radix, expected) in [
        ("BigInt", "f".repeat(250_000), Some(16), "1000000\n"),
        ("BigInt", format!("1{}", "0".repeat(250_000)), Some(16), "ERR\n"),
        ("BigDecimal", format!("1{}", "0".repeat(301_029)), None, "1\n"),
        ("BigDecimal", format!("1{}", "0".repeat(301_030)), None, "ERR\n"),
        ("BigInt", format!("{}1", "0".repeat(400_000)), Some(10), "1\n"),
        ("BigDecimal", format!("{}1", "0".repeat(400_000)), None, "1\n"),
    ] {
        let argument = radix.map_or(String::new(), |radix| format!(", {radix}"));
        let query = if radix.is_some() {
            "match (value.bitLength()) { Ok(bits) { println(bits); } Err(error) { println(\"ERR\"); } }"
        } else {
            "println(value.signum());"
        };
        let source = format!(
            "void main() {{ Text input = new Text(\"{digits}\"); match ({class}.parse(input{argument})) {{ Ok(value) {{ {query} }} Err(error) {{ println(\"ERR\"); }} }} }}"
        );
        assert_lines(&execute(&source), expected);
    }
}

#[test]
fn localized_exact_decimal_composes_with_bigdecimal_parse() {
    let source = r##"
void main() {
    Text localized = new Text("9.007.199.254.740.993,01");
    Text pattern = new Text("#,##0.00");
    Text german = new Text("de-DE");
    match (Locale.parseDecimal(localized, pattern, german)) {
        Ok(exactText) {
            println(exactText.equalsString("9007199254740993.01"));
            match (BigDecimal.parse(exactText)) {
                Ok(value) {
                    match (value.toText()) {
                        Ok(decimalText) { println(decimalText.equalsString("9007199254740993.01")); }
                        Err(error) { println("BIGDECIMAL_TEXT_ERR"); }
                    }
                }
                Err(error) { println("BIGDECIMAL_PARSE_ERR"); }
            }
        }
        Err(error) { println("LOCALE_PARSE_ERR"); }
    }
}
"##;
    let output = execute(source);
    assert_lines(&output, "true\ntrue\n");
}
