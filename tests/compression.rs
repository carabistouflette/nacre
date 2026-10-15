mod support;

use std::process::Output;

fn execute(source: &str) -> Output {
    support::compile(
        source,
        &["-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"],
    )
    .unwrap_or_else(|error| panic!("native compile failed: {error}"))
    .output()
    .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_clean(output: &Output, expected: &[u8]) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, expected);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")), "{stderr}");
    assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
    assert!(!stderr.contains("runtime error:"), "{stderr}");
}

fn append_bytes(variable: &str, bytes: &[u8]) -> String {
    let mut source = String::new();
    for byte in bytes {
        source.push_str(&format!("        {variable}.append((byte){byte});\n"));
    }
    source
}

fn case_source(name: &str, bytes: &[u8], call: &str) -> String {
    let mut source = String::from(
        "        match (makeBuffer()) {\n            Err(error) { println(\"factory-error\"); }\n            Ok(__BUFFER__) {\n__APPENDS____CALL__\n            }\n        }\n",
    );
    source = source.replace("__BUFFER__", name);
    source = source.replace("__APPENDS__", &append_bytes(name, bytes));
    source.replace("__CALL__", &format!("                {call}\n"))
}

#[test]
fn checksums_and_external_zlib_and_rfc1952_vectors_are_exact() {
    const ZLIB_HELLO_WORLD: &[u8] = &[
        0x78, 0xda, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0x28, 0xcf,
        0x2f, 0xca, 0x49, 0x01, 0x00, 0x1a, 0x0b, 0x04, 0x5d,
    ];
    const GZIP_HELLO_WORLD: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03,
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0x28, 0xcf, 0x2f, 0xca,
        0x49, 0x01, 0x00, 0x85, 0x11, 0x4a, 0x0d, 0x0b, 0x00, 0x00, 0x00,
    ];
    const GZIP_HELLO: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03,
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0x00, 0x00, 0xf6, 0xf9,
        0x81, 0xed, 0x06, 0x00, 0x00, 0x00,
    ];
    const GZIP_WORLD: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03,
        0x2b, 0xcf, 0x2f, 0xca, 0x49, 0x01, 0x00, 0x43, 0x11, 0x77,
        0x3a, 0x05, 0x00, 0x00, 0x00,
    ];
    let source = String::from(
        r#"
Result<ByteBuffer, Text> makeBuffer() { return ByteBuffer.withCapacity(0L); }
boolean isHelloWorld(borrow ByteBuffer value) {
    return value.length() == 11L
        && value.get(0L) == (byte)104 && value.get(1L) == (byte)101
        && value.get(2L) == (byte)108 && value.get(3L) == (byte)108
        && value.get(4L) == (byte)111 && value.get(5L) == (byte)32
        && value.get(6L) == (byte)119 && value.get(7L) == (byte)111
        && value.get(8L) == (byte)114 && value.get(9L) == (byte)108
        && value.get(10L) == (byte)100;
}
void checkEmptyDeflate(borrow ByteBuffer input) {
    match (Compression.deflate(input, -1)) {
        Err(error) { println("empty-deflate-error"); }
        Ok(encoded) {
            match (Compression.inflate(encoded, 0L)) {
                Err(error) { println("empty-inflate-error"); }
                Ok(decoded) { println(decoded.length() == 0L); }
            }
        }
    }
}
void checkEmptyGzip(borrow ByteBuffer input) {
    match (Compression.gzip(input, 9)) {
        Err(error) { println("empty-gzip-error"); }
        Ok(encoded) {
            match (Compression.gunzip(encoded, 0L)) {
                Err(error) { println("empty-gunzip-error"); }
                Ok(decoded) { println(decoded.length() == 0L); }
            }
        }
    }
}
void main() {
    match (makeBuffer()) {
        Err(error) { println("factory-error"); }
        Ok(checks) {
/* CHECKSUM_BYTES */
            println(Checksums.crc32(checks) == 3421780262L);
            println(Checksums.adler32(checks) == 152961502L);
/* ZLIB_VECTOR */
/* GZIP_VECTOR */
/* GZIP_CONCAT */
            match (makeBuffer()) {
                Err(error) { println("factory-error"); }
                Ok(empty) {
                    println(Checksums.crc32(empty) == 0L);
                    println(Checksums.adler32(empty) == 1L);
                    checkEmptyDeflate(empty);
                    checkEmptyGzip(empty);
                }
            }
        }
    }
}
"#,
    );
    let source = source
        .replace("/* CHECKSUM_BYTES */", &append_bytes("checks", b"123456789"))
        .replace("/* ZLIB_VECTOR */", &case_source("zlibVector", ZLIB_HELLO_WORLD, r#"match (Compression.inflate(zlibVector, 11L)) { Err(error) { println("zlib-vector-error"); } Ok(decoded) { println(isHelloWorld(decoded)); } }"#))
        .replace("/* GZIP_VECTOR */", &case_source("gzipVector", GZIP_HELLO_WORLD, r#"match (Compression.gunzip(gzipVector, 11L)) { Err(error) { println("gzip-vector-error"); } Ok(decoded) { println(isHelloWorld(decoded)); } }"#))
        .replace("/* GZIP_CONCAT */", &case_source("gzipConcat", &[GZIP_HELLO, GZIP_WORLD].concat(), r#"match (Compression.gunzip(gzipConcat, 11L)) { Err(error) { println("gzip-concat-error"); } Ok(decoded) { println(isHelloWorld(decoded)); } }"#));
    let output = execute(&source);
    assert_clean(&output, b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn large_binary_payload_round_trips_through_wrapped_deflate_and_gzip() {
    let source = r#"
Result<ByteBuffer, Text> makeBuffer() { return ByteBuffer.withCapacity(0L); }
boolean sameBuffer(borrow ByteBuffer left, borrow ByteBuffer right) {
    if (left.length() != right.length()) { return false; }
    long index = 0L;
    while (index < left.length()) {
        if (left.get(index) != right.get(index)) { return false; }
        index = index + 1L;
    }
    return true;
}
void roundTripDeflate(borrow ByteBuffer input, int level) {
    match (Compression.deflate(input, level)) {
        Err(error) { println("deflate-error"); }
        Ok(encoded) {
            match (Compression.inflate(encoded, input.length())) {
                Err(error) { println("inflate-error"); }
                Ok(decoded) { println(sameBuffer(input, decoded)); }
            }
        }
    }
}
void roundTripGzip(borrow ByteBuffer input, int level) {
    match (Compression.gzip(input, level)) {
        Err(error) { println("gzip-error"); }
        Ok(encoded) {
            match (Compression.gunzip(encoded, input.length())) {
                Err(error) { println("gunzip-error"); }
                Ok(decoded) { println(sameBuffer(input, decoded)); }
            }
        }
    }
}
void main() {
    match (makeBuffer()) {
        Err(error) { println("factory-error"); }
        Ok(input) {
            long index = 0L;
            while (index < 262147L) {
                input.append((byte)(index % 256L));
                index = index + 1L;
            }
            roundTripDeflate(input, -1);
            roundTripDeflate(input, 0);
            roundTripGzip(input, 9);
        }
    }
}
"#;
    let output = execute(source);
    assert_clean(&output, b"true\ntrue\ntrue\n");
}

#[test]
fn decompression_errors_reject_bounds_corruption_truncation_and_trailing_bytes() {
    const ZLIB_HELLO_WORLD: &[u8] = &[
        0x78, 0xda, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0x28, 0xcf,
        0x2f, 0xca, 0x49, 0x01, 0x00, 0x1a, 0x0b, 0x04, 0x5d,
    ];
    const GZIP_HELLO_WORLD: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03,
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0x28, 0xcf, 0x2f, 0xca,
        0x49, 0x01, 0x00, 0x85, 0x11, 0x4a, 0x0d, 0x0b, 0x00, 0x00, 0x00,
    ];
    let mut corrupt_gzip = GZIP_HELLO_WORLD.to_vec();
    corrupt_gzip[23] ^= 0x80;
    let mut truncated_zlib = ZLIB_HELLO_WORLD.to_vec();
    truncated_zlib.pop();
    let mut trailing_zlib = ZLIB_HELLO_WORLD.to_vec();
    trailing_zlib.push(0);
    let mut truncated_gzip = GZIP_HELLO_WORLD.to_vec();
    truncated_gzip.pop();
    let mut trailing_gzip = GZIP_HELLO_WORLD.to_vec();
    trailing_gzip.push(0);

    let mut source = String::from(
        r#"
Result<ByteBuffer, Text> makeBuffer() { return ByteBuffer.withCapacity(0L); }
void expectInflateError(borrow ByteBuffer input, long maximum) {
    match (Compression.inflate(input, maximum)) {
        Err(error) { println("inflate-error"); }
        Ok(output) { println("unexpected-inflate-success"); }
    }
}
void expectGunzipError(borrow ByteBuffer input, long maximum) {
    match (Compression.gunzip(input, maximum)) {
        Err(error) { println("gunzip-error"); }
        Ok(output) { println("unexpected-gunzip-success"); }
    }
}
void expectDeflateError(borrow ByteBuffer input, int level) {
    match (Compression.deflate(input, level)) {
        Err(error) { println("deflate-error"); }
        Ok(output) { println("unexpected-deflate-success"); }
    }
}
void expectGzipError(borrow ByteBuffer input, int level) {
    match (Compression.gzip(input, level)) {
        Err(error) { println("gzip-error"); }
        Ok(output) { println("unexpected-gzip-success"); }
    }
}
void checkBombBound() {
    match (makeBuffer()) {
        Err(error) { println("factory-error"); }
        Ok(bomb) {
            long index = 0L;
            while (index < 131072L) {
                bomb.append((byte)0);
                index = index + 1L;
            }
            match (Compression.deflate(bomb, 9)) {
                Err(error) { println("bomb-deflate-error"); }
                Ok(encoded) { expectInflateError(encoded, 16L); }
            }
            match (Compression.gzip(bomb, 9)) {
                Err(error) { println("bomb-gzip-error"); }
                Ok(encoded) { expectGunzipError(encoded, 16L); }
            }
        }
    }
}
void main() {
/* ZLIB_NEGATIVE */
/* ZLIB_ZERO */
/* ZLIB_SMALL */
/* ZLIB_CORRUPT */
/* ZLIB_TRUNCATED */
/* ZLIB_TRAILING */
/* GZIP_NEGATIVE */
/* GZIP_ZERO */
/* GZIP_SMALL */
/* GZIP_CORRUPT */
/* GZIP_TRUNCATED */
/* GZIP_TRAILING */
/* INVALID_LEVELS */
checkBombBound();
}
"#,
    );
    for (marker, name, bytes, call) in [
        ("/* ZLIB_NEGATIVE */", "zlibNegative", ZLIB_HELLO_WORLD, "expectInflateError(zlibNegative, -1L);"),
        ("/* ZLIB_ZERO */", "zlibZero", ZLIB_HELLO_WORLD, "expectInflateError(zlibZero, 0L);"),
        ("/* ZLIB_SMALL */", "zlibSmall", ZLIB_HELLO_WORLD, "expectInflateError(zlibSmall, 10L);"),
        ("/* ZLIB_CORRUPT */", "zlibCorrupt", &[1u8, 2, 3][..], "expectInflateError(zlibCorrupt, 64L);"),
        ("/* ZLIB_TRUNCATED */", "zlibTruncated", truncated_zlib.as_slice(), "expectInflateError(zlibTruncated, 64L);"),
        ("/* ZLIB_TRAILING */", "zlibTrailing", trailing_zlib.as_slice(), "expectInflateError(zlibTrailing, 64L);"),
        ("/* GZIP_NEGATIVE */", "gzipNegative", GZIP_HELLO_WORLD, "expectGunzipError(gzipNegative, -1L);"),
        ("/* GZIP_ZERO */", "gzipZero", GZIP_HELLO_WORLD, "expectGunzipError(gzipZero, 0L);"),
        ("/* GZIP_SMALL */", "gzipSmall", GZIP_HELLO_WORLD, "expectGunzipError(gzipSmall, 10L);"),
        ("/* GZIP_CORRUPT */", "gzipCorrupt", corrupt_gzip.as_slice(), "expectGunzipError(gzipCorrupt, 64L);"),
        ("/* GZIP_TRUNCATED */", "gzipTruncated", truncated_gzip.as_slice(), "expectGunzipError(gzipTruncated, 64L);"),
        ("/* GZIP_TRAILING */", "gzipTrailing", trailing_gzip.as_slice(), "expectGunzipError(gzipTrailing, 64L);"),
        ("/* INVALID_LEVELS */", "empty", &[] as &[u8], "expectDeflateError(empty, -2); expectGzipError(empty, 10);"),
    ] {
        source = source.replace(marker, &case_source(name, bytes, call));
    }
    let output = execute(&source);
    assert_clean(
        &output,
        b"inflate-error\ninflate-error\ninflate-error\ninflate-error\ninflate-error\ninflate-error\ngunzip-error\ngunzip-error\ngunzip-error\ngunzip-error\ngunzip-error\ngunzip-error\ndeflate-error\ngzip-error\ninflate-error\ngunzip-error\n",
    );
}
