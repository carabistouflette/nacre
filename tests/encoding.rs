mod support;
use std::process::Output;

const EXAMPLE: &str = include_str!("../examples/stdlib/encoding.ncr");

fn execute(source: &str) -> Output {
    support::compile(source, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_clean(output: &Output, expected: &[u8]) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, expected);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().find(|line| line.starts_with("NACRE_MEMORY "))
        .unwrap_or_else(|| panic!("missing memory report:\n{stderr}"));
    let mut fields = line.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{line}");
    assert_eq!(fields.next(), Some("live=0"), "{line}");
    assert_eq!(fields.next(), None, "{line}");
}

#[test]
fn stdlib_example_runs_all_encoding_owners_to_completion() {
    assert_clean(&execute(EXAMPLE), b"666f6f\nZm9v\nZm9v\nfoo\n");
}

#[test]
fn rfc4648_base64_vectors_round_trip_padded_and_unpadded_forms() {
    let source = r#"
void checkVector(String raw) {
    Text original = new Text(raw);
    ByteBuffer bytes = Encoding.utf8Encode(original);
    Text basic = Encoding.base64Encode(bytes);
    Text url = Encoding.base64UrlEncode(bytes);
    basic.print();
    url.print();
    match (Encoding.base64Decode(basic)) {
        Err(error) { println(false); }
        Ok(decoded) {
            match (Encoding.utf8Decode(decoded)) {
                Err(error) { println(false); }
                Ok(roundTrip) { println(roundTrip.equals(original)); }
            }
        }
    }
    match (Encoding.base64UrlDecode(url)) {
        Err(error) { println(false); }
        Ok(decoded) {
            match (Encoding.utf8Decode(decoded)) {
                Err(error) { println(false); }
                Ok(roundTrip) { println(roundTrip.equals(original)); }
            }
        }
    }
}
void main() {
    checkVector("");
    checkVector("f");
    checkVector("fo");
    checkVector("foo");
    checkVector("foob");
    checkVector("fooba");
    checkVector("foobar");
}
"#;
    assert_clean(
        &execute(source),
        b"\n\ntrue\ntrue\nZg==\nZg\ntrue\ntrue\nZm8=\nZm8\ntrue\ntrue\nZm9v\nZm9v\ntrue\ntrue\nZm9vYg==\nZm9vYg\ntrue\ntrue\nZm9vYmE=\nZm9vYmE\ntrue\ntrue\nZm9vYmFy\nZm9vYmFy\ntrue\ntrue\n",
    );
}

#[test]
fn hex_and_base64_preserve_binary_nul_and_high_bytes_and_utf8_round_trips() {
    let source = r#"
boolean bytesEqual(borrow ByteBuffer left, borrow ByteBuffer right) {
    if (left.length() != right.length()) { return false; }
    long index = 0L;
    while (index < left.length()) {
        if (left.get(index) != right.get(index)) { return false; }
        index = index + 1L;
    }
    return true;
}
void main() {
    Text original = new Text("A\0é🚀");
    ByteBuffer utf8 = Encoding.utf8Encode(original);
    println(utf8.length());
    println(utf8.get(0L));
    println(utf8.get(1L));
    println(utf8.get(2L));
    println(utf8.get(3L));
    println(utf8.get(4L));
    println(utf8.get(7L));
    match (Encoding.utf8Decode(utf8)) {
        Err(error) { println(false); }
        Ok(roundTrip) { println(roundTrip.equals(original)); }
    }
    Text emptyText = new Text("");
    ByteBuffer emptyBytes = Encoding.utf8Encode(emptyText);
    Text emptyHex = Encoding.hexEncode(emptyBytes);
    emptyHex.print();
    match (Encoding.hexDecode(emptyHex)) {
        Err(error) { println(-1L); }
        Ok(decoded) { println(decoded.length()); }
    }

    match (ByteBuffer.withCapacity(0L)) {
        Err(error) { println("buffer creation failed"); }
        Ok(binary) {
            binary.append((byte)0);
            binary.append((byte)128);
            binary.append((byte)255);
            binary.append((byte)13);
            binary.append((byte)10);
            Text hex = Encoding.hexEncode(binary);
            Text basic = Encoding.base64Encode(binary);
            Text url = Encoding.base64UrlEncode(binary);
            hex.print();
            basic.print();
            url.print();
            match (Encoding.hexDecode(hex)) {
                Err(error) { println(false); }
                Ok(decoded) { println(bytesEqual(binary, decoded)); }
            }
            match (Encoding.base64Decode(basic)) {
                Err(error) { println(false); }
                Ok(decoded) { println(bytesEqual(binary, decoded)); }
            }
            match (Encoding.base64UrlDecode(url)) {
                Err(error) { println(false); }
                Ok(decoded) { println(bytesEqual(binary, decoded)); }
            }
            Text paddedUrl = new Text("AID_DQo=");
            match (Encoding.base64UrlDecode(paddedUrl)) {
                Err(error) { println(false); }
                Ok(decoded) { println(bytesEqual(binary, decoded)); }
            }
        }
    }
}
"#;
    assert_clean(
        &execute(source),
        b"8\n65\n0\n195\n169\n240\n128\ntrue\n\n0\n0080ff0d0a\nAID/DQo=\nAID_DQo\ntrue\ntrue\ntrue\ntrue\n",
    );
}

#[test]
fn decoders_accept_hex_case_and_reject_noncanonical_or_invalid_data() {
    let source = r#"
void expectHexError(String raw) {
    Text input = new Text(raw);
    match (Encoding.hexDecode(input)) {
        Err(error) { println("hex rejected"); }
        Ok(decoded) { println("hex accepted"); }
    }
}
void expectBase64Error(String raw) {
    Text input = new Text(raw);
    match (Encoding.base64Decode(input)) {
        Err(error) { println("Base64 rejected"); }
        Ok(decoded) { println("Base64 accepted"); }
    }
}
void expectUrlError(String raw) {
    Text input = new Text(raw);
    match (Encoding.base64UrlDecode(input)) {
        Err(error) { println("Base64URL rejected"); }
        Ok(decoded) { println("Base64URL accepted"); }
    }
}
void expectUtf8Error(int first, int second, int third, int fourth, int count) {
    match (ByteBuffer.withCapacity(0L)) {
        Err(error) { println("buffer creation failed"); }
        Ok(bytes) {
            bytes.append((byte)first);
            if (count > 1) { bytes.append((byte)second); }
            if (count > 2) { bytes.append((byte)third); }
            if (count > 3) { bytes.append((byte)fourth); }
            match (Encoding.utf8Decode(bytes)) {
                Err(error) { println("invalid UTF-8 rejected"); }
                Ok(decoded) { println("invalid UTF-8 accepted"); }
            }
        }
    }
}
void main() {
    Text upper = new Text("00aF80fF");
    match (Encoding.hexDecode(upper)) {
        Err(error) { println("uppercase hex rejected"); }
        Ok(bytes) {
            println(bytes.get(0L));
            println(bytes.get(1L));
            println(bytes.get(2L));
            println(bytes.get(3L));
        }
    }
    expectHexError("0");
    expectHexError("0g");

    expectBase64Error("Zg");
    expectBase64Error("Zg===");
    expectBase64Error("_w==");
    expectBase64Error("=Zg=");
    expectBase64Error("Z g==");
    expectBase64Error("Zh==");
    expectBase64Error("Zm9=");
    expectBase64Error("A");

    expectUrlError("/w==");
    expectUrlError("_ w");
    expectUrlError("_x");
    expectUrlError("A");
    expectUrlError("w=");
    expectUrlError("__===");
    expectUrlError("_=w=");

    expectUtf8Error(192, 175, 0, 0, 2);
    expectUtf8Error(226, 130, 0, 0, 2);
    expectUtf8Error(237, 160, 128, 0, 3);
    expectUtf8Error(244, 144, 128, 128, 4);
    expectUtf8Error(128, 0, 0, 0, 1);
}
"#;
    let expected = b"0\n175\n128\n255\nhex rejected\nhex rejected\nBase64 rejected\nBase64 rejected\nBase64 rejected\nBase64 rejected\nBase64 rejected\nBase64 rejected\nBase64 rejected\nBase64 rejected\nBase64URL rejected\nBase64URL rejected\nBase64URL rejected\nBase64URL rejected\nBase64URL rejected\nBase64URL rejected\nBase64URL rejected\ninvalid UTF-8 rejected\ninvalid UTF-8 rejected\ninvalid UTF-8 rejected\ninvalid UTF-8 rejected\ninvalid UTF-8 rejected\n";
    assert_clean(&execute(source), expected);
}
