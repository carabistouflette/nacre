use nacre::compile_to_c;
mod support;
use std::process::Output;

const NUMERIC_EXAMPLE: &str = include_str!("../examples/language/numeric_types.ncr");
// U+1F9EA is the Unicode test data in this expected output.
const EXPECTED_OUTPUT: &str = "255\n65\n🧪\n🧪\n3\n-2147483648\n-9223372036854775808\n-9223372036854775808\n0\nfalse\ntrue\n-1\n-3\n-9223372036854775808\n3\n15\n4\n-1\n297\nfalse\ntrue\ntrue\n78\n-9223372036854775808\n()\n1\n255|🧪|-9223372036854775808|3.25|inf\n-9223372036854775808\n[E6111]\n312.5\ntrue\n[E6112]\n()\n";

fn assert_numeric_output(bytes: &[u8]) {
    let text = std::str::from_utf8(bytes).unwrap();
    let values = text.lines().map(|line| {
        if line.starts_with("[E6111]") { "[E6111]" }
        else if line.starts_with("[E6112]") { "[E6112]" }
        else { line }
    });
    assert!(values.eq(EXPECTED_OUTPUT.lines()), "{text}");
}

fn execute(source: &str, flags: &[&str]) -> Output {
    support::compile(source, flags)
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

#[test]
fn numeric_example_covers_native_values_and_collections() {
    let output = execute(NUMERIC_EXAMPLE, &["-DNACRE_MEMORY_STATS"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_numeric_output(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("NACRE_MEMORY allocations="), "{stderr}");
    assert!(stderr.contains("live=0"), "{stderr}");
}

#[test]
fn numeric_example_passes_native_sanitizers() {
    let output = execute(NUMERIC_EXAMPLE, &["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_numeric_output(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
    assert!(!stderr.contains("runtime error:"), "{stderr}");
}

#[test]
fn checked_numeric_failures_report_stable_codes_and_locations() {
    for (source, code) in [
        ("void main() { byte value = (byte)256; println(value); }", "[E6102]"),
        ("void main() { long number = 2147483648L; int value = (int)number; println(value); }", "[E6102]"),
        ("void main() { char value = (char)55296; println(value); }", "[E6102]"),
        ("void main() { double nan = 0.0 / 0.0; int value = (int)nan; println(value); }", "[E6102]"),
        ("void main() { int value = 1 << 32; println(value); }", "[E6104]"),
        ("void main() { int value = 1 << -1; println(value); }", "[E6104]"),
        ("void main() { long value = 1L / 0L; println(value); }", "[E6103]"),
    ] {
        let output = execute(source, &[]);
        assert_eq!(output.status.code(), Some(70), "{}", String::from_utf8_lossy(&output.stderr));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("[1:"), "{stderr}");
        assert!(stderr.contains(code), "{stderr}");
    }
}

#[test]
fn numeric_literal_bounds_and_operator_types_are_checked() {
    for (source, code) in [
        ("void main() { int value = 2147483648; }", "[E6001]"),
        ("void main() { long value = 9223372036854775808L; }", "[E6101]"),
        ("void main() { double value = 1e9999; }", "[E6101]"),
    ] {
        let error = compile_to_c(source).unwrap_err();
        assert!(error.message.contains(code), "{}", error.render("numeric_types.ncr", source));
    }

    let error = compile_to_c("void main() { int value = 1 + true; }").unwrap_err();
    assert!(error.message.contains("[E6105]"), "{}", error.message);

    assert!(compile_to_c("void main() { byte value = 255; }").is_err());
}
