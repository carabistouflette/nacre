use nacre::{compile_to_c, native};
use std::process::Output;

fn execute(source: &str) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("uuid.ncr", source)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_clean(output: &Output, expected: &[u8], expected_allocations: Option<usize>) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, expected);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut report = stderr.lines().filter(|line| line.starts_with("NACRE_MEMORY "));
    let line = report.next().unwrap_or_else(|| panic!("missing memory report:\n{stderr}"));
    assert!(report.next().is_none(), "{stderr}");
    let mut fields = line.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations: usize = fields.next().unwrap().strip_prefix("allocations=").unwrap().parse().unwrap();
    let frees: usize = fields.next().unwrap().strip_prefix("frees=").unwrap().parse().unwrap();
    assert_eq!(allocations, frees, "{line}");
    assert_eq!(fields.next(), Some("live=0"), "{line}");
    assert_eq!(fields.next(), None, "{line}");
    if let Some(expected) = expected_allocations {
        assert_eq!(allocations, expected, "{line}");
    }
}

#[test]
fn rfc_namespace_name_md5_and_sha1_vectors_match_text_and_network_bytes() {
    let source = r#"
void main() {
    Text namespaceText = new Text("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
    match (Uuid.parse(namespaceText)) {
        Err(error) { println("namespace parse failed"); }
        Ok(namespace) {
            Text nameText = new Text("www.widgets.com");
            ByteBuffer name = Encoding.utf8Encode(nameText);
            match (Uuid.nameMd5(namespace, name)) {
                Err(error) { println("MD5 failed"); }
                Ok(uuid) {
                    Text canonical = uuid.toText();
                    canonical.print();
                    ByteBuffer bytes = uuid.toBytes();
                    Text networkHex = Encoding.hexEncode(bytes);
                    networkHex.print();
                }
            }
            match (Uuid.nameSha1(namespace, name)) {
                Err(error) { println("SHA-1 failed"); }
                Ok(uuid) {
                    Text canonical = uuid.toText();
                    canonical.print();
                    ByteBuffer bytes = uuid.toBytes();
                    Text networkHex = Encoding.hexEncode(bytes);
                    networkHex.print();
                }
            }
        }
    }
}
"#;
    assert_clean(
        &execute(source),
        b"3d813cbb-47fb-32ba-91df-831e1593ac29\n3d813cbb47fb32ba91df831e1593ac29\n21f7f8de-8051-5b89-8680-0195ef798b6a\n21f7f8de80515b8986800195ef798b6a\n",
        None,
    );
}

#[test]
fn uuid_parse_is_canonical_case_insensitive_and_rejects_malformed_text() {
    let source = r#"
void showParse(String raw) {
    Text input = new Text(raw);
    match (Uuid.parse(input)) {
        Err(error) { println("rejected"); }
        Ok(uuid) {
            Text canonical = uuid.toText();
            canonical.print();
        }
    }
}
void main() {
    showParse("00112233-4455-6677-8899-aabbccddeeff");
    showParse("00112233-4455-6677-8899-AABBCCDDEEFF");
    showParse("");
    showParse("00112233-4455-6677-8899-aabbccddeef");
    showParse("00112233-4455-6677-8899-aabbccddeeff0");
    showParse("00112233.4455-6677-8899-aabbccddeeff");
    showParse("0011223g-4455-6677-8899-aabbccddeeff");
    showParse("00112233\0-4455-6677-8899-aabbccddeeff");
}
"#;
    assert_clean(
        &execute(source),
        b"00112233-4455-6677-8899-aabbccddeeff\n00112233-4455-6677-8899-aabbccddeeff\nrejected\nrejected\nrejected\nrejected\nrejected\nrejected\n",
        None,
    );
}

#[test]
fn uuid_from_bytes_requires_exact_network_order_length() {
    let source = r#"
Result<Uuid, Text> fromHex(String hex) {
    Text input = new Text(hex);
    ByteBuffer bytes = Encoding.hexDecode(input)?;
    return Uuid.fromBytes(bytes);
}
void showBytes(String hex) {
    match (fromHex(hex)) {
        Err(error) { println("rejected"); }
        Ok(uuid) {
            Text canonical = uuid.toText();
            canonical.print();
        }
    }
}
void main() {
    showBytes("00112233445566778899aabbccddeeff");
    showBytes("00112233445566778899aabbccddeeff00");
    showBytes("00112233445566778899aabbccddee");
}
"#;
    assert_clean(
        &execute(source),
        b"00112233-4455-6677-8899-aabbccddeeff\nrejected\nrejected\n",
        None,
    );
}

#[test]
fn uuid_long_bits_clone_order_nil_version_and_rfc_variants_are_preserved() {
    let source = r#"
void main() {
    Uuid high = Uuid.fromLongs(-1L, -9223372036854775808L);
    Text highText = high.toText();
    println(highText.equalsString("ffffffff-ffff-ffff-8000-000000000000"));
    println(high.mostSignificantBits());
    println(high.leastSignificantBits());
    Uuid copied = high.clone();
    println(copied.equals(high));
    println(high.compareTo(copied));
    Uuid zero = Uuid.fromLongs(0L, 0L);
    Text zeroText = zero.toText();
    println(zeroText.equalsString("00000000-0000-0000-0000-000000000000"));
    println(zero.version());
    println(zero.variant());
    println(high.compareTo(zero));

    int version = 0;
    while (version <= 15) {
        Uuid value = Uuid.fromLongs(((long)version) << 12, 0L);
        println(value.version() == version);
        version = version + 1;
    }
    Uuid ncs = Uuid.fromLongs(0L, (long)127 << 56);
    Uuid rfc = Uuid.fromLongs(0L, (long)191 << 56);
    Uuid microsoft = Uuid.fromLongs(0L, (long)223 << 56);
    Uuid future = Uuid.fromLongs(0L, (long)255 << 56);
    println(ncs.variant());
    println(rfc.variant());
    println(microsoft.variant());
    println(future.variant());
}
"#;
    let mut expected = String::from(
        "true\n-1\n-9223372036854775808\ntrue\n0\ntrue\n0\n0\n1\n",
    );
    expected.push_str(&"true\n".repeat(16));
    expected.push_str("0\n2\n6\n7\n");
    assert_clean(&execute(source), expected.as_bytes(), None);
}

#[test]
fn uuid_scalar_queries_do_not_allocate_and_owners_are_dropped() {
    let source = r#"
void main() {
    Uuid first = Uuid.fromLongs(-1L, -9223372036854775808L);
    Uuid second = Uuid.fromLongs(0L, 0L);
    println(first.mostSignificantBits());
    println(first.leastSignificantBits());
    println(first.version());
    println(first.variant());
    println(first.equals(second));
    println(first.compareTo(second));
}
"#;
    assert_clean(
        &execute(source),
        b"-1\n-9223372036854775808\n15\n2\nfalse\n1\n",
        Some(2),
    );
}

#[test]
fn uuid_random_returns_v4_rfc_variant_owner() {
    let source = r#"
void main() {
    match (Uuid.random()) {
        Err(error) { println("first random failed"); }
        Ok(first) {
            println(first.version() == 4);
            println(first.variant() == 2);
        }
    }
}
"#;
    assert_clean(&execute(source), b"true\ntrue\n", None);
}
