use nacre::{compile_to_c, native};
use std::process::Output;

const HELPERS: &str = r#"
boolean unitOk(Result<Unit, Text> result) {
    match (result) { Ok(value) { return true; } Err(error) { return false; } }
}
boolean unitError(Result<Unit, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean byteBufferError(Result<ByteBuffer, Text> result) {
    match (result) { Ok(value) { return false; } Err(error) { return true; } }
}
boolean byteBufferHex(Result<ByteBuffer, Text> result, String expected) {
    match (result) {
        Err(error) { return false; }
        Ok(value) { return Encoding.hexEncode(value).equalsString(expected); }
    }
}
boolean booleanResultIs(Result<boolean, Text> result, boolean expected) {
    match (result) { Err(error) { return false; } Ok(value) { return value == expected; } }
}
boolean textError(Result<Text, Text> result) {
    match (result) { Err(error) { return true; } Ok(value) { return false; } }
}
boolean textResultEquals(Result<Text, Text> result, String expected) {
    match (result) { Err(error) { return false; } Ok(value) { return value.equalsString(expected); } }
}
boolean textResultNonempty(Result<Text, Text> result) {
    match (result) { Err(error) { return false; } Ok(value) { return !value.isEmpty(); } }
}
boolean keyError(Result<CryptoKey, Text> result) {
    match (result) { Err(error) { return true; } Ok(value) { return false; } }
}
boolean keySuccess(Result<CryptoKey, Text> result) {
    match (result) { Err(error) { return false; } Ok(value) { return true; } }
}
boolean digestError(Result<CryptoDigest, Text> result) {
    match (result) { Err(error) { return true; } Ok(value) { return false; } }
}
boolean digestSuccess(Result<CryptoDigest, Text> result) {
    match (result) {
        Err(error) { return false; }
        Ok(digest) { return !byteBufferError(digest.finish()); }
    }
}
boolean macError(Result<CryptoMac, Text> result) {
    match (result) { Err(error) { return true; } Ok(value) { return false; } }
}
boolean macSuccess(Result<CryptoMac, Text> result) {
    match (result) {
        Err(error) { return false; }
        Ok(mac) { return !byteBufferError(mac.finish()); }
    }
}
long longResultValue(Result<long, Text> result) {
    match (result) { Err(error) { return -1L; } Ok(value) { return value; } }
}
boolean certificateError(Result<Certificate, Text> result) {
    match (result) { Err(error) { return true; } Ok(value) { return false; } }
}
boolean byteBufferResultsEqual(Result<ByteBuffer, Text> first, Result<ByteBuffer, Text> second) {
    match (first) {
        Err(error) { return false; }
        Ok(a) {
            match (second) {
                Err(error) { return false; }
                Ok(b) { return Encoding.hexEncode(a).equals(Encoding.hexEncode(b)); }
            }
        }
    }
}
boolean publicPemRoundTrips(borrow CryptoKey key) {
    match (key.publicPem()) {
        Err(error) { return false; }
        Ok(pem) {
            match (CryptoKey.parsePublicPem(pem)) {
                Err(error) { return false; }
                Ok(parsed) { return textResultNonempty(parsed.publicPem()); }
            }
        }
    }
}
boolean privatePemRoundTrips(borrow CryptoKey key) {
    match (key.privatePem()) {
        Err(error) { return false; }
        Ok(pem) {
            match (CryptoKey.parsePrivatePem(pem)) {
                Err(error) { return false; }
                Ok(parsed) { return textResultNonempty(parsed.privatePem()); }
            }
        }
    }
}
"#;

fn execute(source: &str) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("crypto_keys.ncr", source)));
    native::compile_c_with_flags(
        &c,
        &[
            "-DNACRE_MEMORY_STATS",
            "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer",
        ],
    )
    .unwrap_or_else(|error| panic!("native compile failed: {error}"))
    .output()
    .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_clean(output: &Output, expected: &[u8]) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, expected);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .find(|line| line.starts_with("NACRE_MEMORY "))
        .unwrap_or_else(|| panic!("missing memory report:\n{stderr}"));
    let mut fields = line.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{line}");
    assert_eq!(fields.next(), Some("live=0"), "{line}");
    assert_eq!(fields.next(), None, "{line}");
    assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
    assert!(!stderr.contains("LeakSanitizer"), "{stderr}");
    assert!(!stderr.contains("runtime error:"), "{stderr}");
}

fn assert_native(source: &str, expected: &[u8]) {
    assert_clean(&execute(source), expected);
}

fn nacre_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for character in value.chars() {
        match character {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            '\0' => literal.push_str("\\0"),
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

#[test]
fn digest_and_hmac_streams_match_vectors_and_are_terminal() {
    let source = format!(
        r#"{HELPERS}
void main() {{
    ByteBuffer first = Encoding.utf8Encode(new Text("a"));
    ByteBuffer second = Encoding.utf8Encode(new Text("bc"));
    match (CryptoDigest.create(new Text("SHA256"))) {{
        Err(error) {{ println(false); }}
        Ok(digest) {{
            boolean firstUpdate = unitOk(digest.update(first));
            boolean secondUpdate = unitOk(digest.update(second));
            boolean vector = byteBufferHex(digest.finish(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
            boolean terminalUpdate = unitError(digest.update(first));
            boolean terminalFinish = byteBufferError(digest.finish());
            println(firstUpdate && secondUpdate && vector && terminalUpdate && terminalFinish);
        }}
    }}

    ByteBuffer key = Encoding.utf8Encode(new Text("key"));
    ByteBuffer messageFirst = Encoding.utf8Encode(new Text("The quick brown "));
    ByteBuffer messageSecond = Encoding.utf8Encode(new Text("fox jumps over the lazy dog"));
    match (CryptoMac.create(new Text("SHA256"), key)) {{
        Err(error) {{ println(false); }}
        Ok(mac) {{
            boolean firstUpdate = unitOk(mac.update(messageFirst));
            boolean secondUpdate = unitOk(mac.update(messageSecond));
            boolean vector = byteBufferHex(mac.finish(), "f7bc83f430538424b13298eaa6fb143ef4d59a14946175997479dbc2d1a3cd8");
            boolean terminalUpdate = unitError(mac.update(messageFirst));
            boolean terminalFinish = byteBufferError(mac.finish());
            println(firstUpdate && secondUpdate && vector && terminalUpdate && terminalFinish);
        }}
    }}
    boolean digestNames =
        digestSuccess(CryptoDigest.create(new Text("SHA384"))) &&
        digestSuccess(CryptoDigest.create(new Text("SHA512"))) &&
        digestSuccess(CryptoDigest.create(new Text("SHA3-256"))) &&
        digestSuccess(CryptoDigest.create(new Text("SHA3-384"))) &&
        digestSuccess(CryptoDigest.create(new Text("SHA3-512")));
    boolean macNames =
        macSuccess(CryptoMac.create(new Text("SHA384"), key)) &&
        macSuccess(CryptoMac.create(new Text("SHA512"), key)) &&
        macSuccess(CryptoMac.create(new Text("SHA3-256"), key)) &&
        macSuccess(CryptoMac.create(new Text("SHA3-384"), key)) &&
        macSuccess(CryptoMac.create(new Text("SHA3-512"), key));
    println(digestNames && macNames);
    println(digestError(CryptoDigest.create(new Text("SHA-1"))));
    println(macError(CryptoMac.create(new Text("SHA-1"), key)));
    println(digestError(CryptoDigest.create(new Text("SHA256\0"))));
}}
"#
    );
    assert_native(&source, b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn rfc8032_ed25519_key_pem_signing_and_role_checks() {
    const PRIVATE_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIJ1hsZ3v/VpguoRK9JLsLMREScVpezJpGXA7rAMcrn9g\n-----END PRIVATE KEY-----\n";
    const PUBLIC_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=\n-----END PUBLIC KEY-----\n";
    const SIGNATURE_HEX: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";
    let source = format!(
        r#"{HELPERS}
void main() {{
    match (CryptoKey.parsePrivatePem(new Text({}))) {{
        Err(error) {{ println(false); }}
        Ok(privateKey) {{
            match (CryptoKey.parsePublicPem(new Text({}))) {{
                Err(error) {{ println(false); }}
                Ok(publicKey) {{
                    ByteBuffer empty = Encoding.utf8Encode(new Text(""));
                    boolean exports = privatePemRoundTrips(privateKey) && publicPemRoundTrips(publicKey);
                    boolean publicCannotSign = byteBufferError(publicKey.sign(new Text(""), empty));
                    boolean publicHasNoPrivateExport = textError(publicKey.privatePem());
                    println(exports && publicCannotSign && publicHasNoPrivateExport);
                    match (privateKey.sign(new Text(""), empty)) {{
                        Err(error) {{ println(false); println(false); }}
                        Ok(signature) {{
                            println(Encoding.hexEncode(signature).equalsString("{}"));
                            println(booleanResultIs(publicKey.verify(new Text(""), empty, signature), true));
                            signature.set(0L, (byte)(signature.get(0L) + 1));
                            println(booleanResultIs(publicKey.verify(new Text(""), empty, signature), false));
                        }}
                    }}
                }}
            }}
        }}
    }}
    Text encrypted = new Text("-----BEGIN ENCRYPTED PRIVATE KEY-----\nAA==\n-----END ENCRYPTED PRIVATE KEY-----\n");
    println(keyError(CryptoKey.parsePrivatePem(encrypted)));
    Text trailing = new Text({});
    trailing.append("trailing");
    println(keyError(CryptoKey.parsePublicPem(trailing)));
}}
"#,
        nacre_literal(PRIVATE_PEM),
        nacre_literal(PUBLIC_PEM),
        SIGNATURE_HEX,
        nacre_literal(PUBLIC_PEM),
    );
    assert_native(&source, b"true\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn generated_rsa_signatures_reject_wrong_keys_and_invalid_policy() {
    let source = format!(
        r#"{HELPERS}
void main() {{
    Text digest = new Text("SHA256");
    ByteBuffer data = Encoding.utf8Encode(new Text("RSA-PSS payload"));
    match (CryptoKey.generate(new Text("RSA"), 2048)) {{
        Err(error) {{ println(false); }}
        Ok(key) {{
            match (CryptoKey.generate(new Text("RSA"), 2048)) {{
                Err(error) {{ println(false); }}
                Ok(otherKey) {{
                    match (key.sign(digest, data)) {{
                        Err(error) {{ println(false); println(false); println(false); }}
                        Ok(signature) {{
                            boolean correctKey = booleanResultIs(key.verify(digest, data, signature), true);
                            boolean wrongKey = booleanResultIs(otherKey.verify(digest, data, signature), false);
                            boolean publicRoundTrip = false;
                            match (key.publicPem()) {{
                                Err(error) {{ }}
                                Ok(publicPem) {{
                                    match (CryptoKey.parsePublicPem(publicPem)) {{
                                        Err(error) {{ }}
                                        Ok(publicKey) {{
                                            publicRoundTrip = booleanResultIs(publicKey.verify(digest, data, signature), true)
                                                && byteBufferError(publicKey.sign(digest, data))
                                                && textError(publicKey.privatePem());
                                        }}
                                    }}
                                }}
                            }}
                            boolean privateRoundTrip = privatePemRoundTrips(key);
                            signature.set(0L, (byte)(signature.get(0L) + 1));
                            boolean tampered = booleanResultIs(key.verify(digest, data, signature), false);
                            println(correctKey && wrongKey && publicRoundTrip && privateRoundTrip && tampered);
                        }}
                    }}
                }}
            }}
    }}
    println(keyError(CryptoKey.generate(new Text("RSA"), 1024)));
    println(keyError(CryptoKey.generate(new Text("RSA"), 8448)));
    println(keyError(CryptoKey.generate(new Text("RSA\0"), 2048)));
    println(keyError(CryptoKey.generate(new Text("Ed25519"), 1)));
    println(keyError(CryptoKey.generate(new Text("EC-P256"), 256)));
    println(keySuccess(CryptoKey.generate(new Text("Ed25519"), 0)));
}}
"#
    );
    assert_native(&source, b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn x25519_and_ec_keys_agree_and_ec_keys_sign() {
    let source = format!(
        r#"{HELPERS}
boolean signsWith(borrow CryptoKey key) {{
    ByteBuffer data = Encoding.utf8Encode(new Text("curve signature"));
    match (key.sign(new Text("SHA256"), data)) {{
        Err(error) {{ return false; }}
        Ok(signature) {{ return booleanResultIs(key.verify(new Text("SHA256"), data, signature), true); }}
    }}
}}
boolean agreementMatches(String algorithm) {{
    Text name = new Text(algorithm);
    match (CryptoKey.generate(name, 0)) {{
        Err(error) {{ return false; }}
        Ok(first) {{
            match (CryptoKey.generate(name, 0)) {{
                Err(error) {{ return false; }}
                Ok(second) {{ return byteBufferResultsEqual(first.agree(second), second.agree(first)) && signsWith(first); }}
            }}
        }}
    }}
}}
boolean crossGroupRejected() {{
    match (CryptoKey.generate(new Text("X25519"), 0)) {{
        Err(error) {{ return false; }}
        Ok(x25519) {{
            match (CryptoKey.generate(new Text("EC-P256"), 0)) {{
                Err(error) {{ return false; }}
                Ok(ec) {{ return byteBufferError(x25519.agree(ec)); }}
            }}
        }}
    }}
}}
void main() {{
    println(agreementMatches("X25519"));
    println(agreementMatches("EC-P256"));
    println(agreementMatches("EC-P384"));
    println(crossGroupRejected());
}}
"#
    );
    assert_native(&source, b"true\ntrue\ntrue\ntrue\n");
}

#[test]
fn certificates_enforce_pem_chain_time_and_san_host_policy() {
    const LEAF: &str = include_str!("fixtures/crypto_leaf.pem");
    const INTERMEDIATE: &str = include_str!("fixtures/crypto_intermediate.pem");
    let chain = format!("{LEAF}{INTERMEDIATE}");
    let ca_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/crypto_root.pem");
    let source = format!(
        r#"{HELPERS}
void main() {{
    Text leafPem = new Text({});
    match (Certificate.parsePem(leafPem)) {{
        Err(error) {{ println(false); }}
        Ok(leaf) {{
            println(textResultEquals(leaf.subject(), "CN=wrong.example"));
            println(textResultEquals(leaf.issuer(), "CN=Nacre Test Intermediate"));
            println(textResultNonempty(leaf.serialText()));
            long start = longResultValue(leaf.notBeforeUnixSeconds());
            long end = longResultValue(leaf.notAfterUnixSeconds());
            println(start >= 0L && end > start);
            println(booleanResultIs(leaf.matchesHost(new Text("good.example")), true));
            println(booleanResultIs(leaf.matchesHost(new Text("wrong.example")), false));
            match (Path.fromText(new Text({}))) {{
                Err(error) {{ println(false); println(false); println(false); println(false); }}
                Ok(caFile) {{
                    println(unitError(leaf.verify(new Text("good.example"), caFile, start)));
                    match (Certificate.parsePemChain(new Text({}))) {{
                        Err(error) {{ println(false); println(false); println(false); }}
                        Ok(chain) {{
                            println(unitOk(chain.verify(new Text("good.example"), caFile, start)));
                            println(unitError(chain.verify(new Text("wrong.example"), caFile, start)));
                            println(unitError(chain.verify(new Text("good.example"), caFile, end + 1L)));
                        }}
                    }}
                }}
            }}
        }}
    }}
    println(certificateError(Certificate.parsePem(new Text("not a certificate"))));
    leafPem.append("trailing text");
    println(certificateError(Certificate.parsePem(leafPem)));
}}
"#,
        nacre_literal(LEAF),
        nacre_literal(ca_path),
        nacre_literal(&chain),
    );
    assert_native(
        &source,
        b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
    );
}
