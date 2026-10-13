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
Result<ByteBuffer, Text> hexBuffer(String value) {
    return Encoding.hexDecode(new Text(value));
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
boolean certificateTimesMatch(borrow Certificate certificate, long expectedBefore, long expectedAfter) {
    match (certificate.notBeforeUnixSeconds()) {
        Err(error) { return false; }
        Ok(before) {
            match (certificate.notAfterUnixSeconds()) {
                Err(error) { return false; }
                Ok(after) { return before == expectedBefore && after == expectedAfter; }
            }
        }
    }
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
boolean publishedAgreementMatches(String firstPem, String secondPem, String expected) {
    match (CryptoKey.parsePrivatePem(new Text(firstPem))) {
        Err(error) { return false; }
        Ok(first) {
            match (CryptoKey.parsePrivatePem(new Text(secondPem))) {
                Err(error) { return false; }
                Ok(second) {
                    return byteBufferHex(first.agree(second), expected)
                        && byteBufferHex(second.agree(first), expected);
                }
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
                Ok(second) {{ return byteBufferResultsEqual(first.agree(second), second.agree(first)); }}
            }}
        }}
    }}
}}
boolean ecSigningMatches(String algorithm) {{
    match (CryptoKey.generate(new Text(algorithm), 0)) {{
        Err(error) {{ return false; }}
        Ok(key) {{ return signsWith(key); }}
    }}
}}
boolean x25519SigningUnsupported() {{
    match (CryptoKey.generate(new Text("X25519"), 0)) {{
        Err(error) {{ return false; }}
        Ok(key) {{
            ByteBuffer data = Encoding.utf8Encode(new Text("curve signature"));
            return byteBufferError(key.sign(new Text("SHA256"), data));
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
    println(ecSigningMatches("EC-P256"));
    println(ecSigningMatches("EC-P384"));
    println(x25519SigningUnsupported());
    println(crossGroupRejected());
}}
"#
    );
    assert_native(&source, b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn nist_rsa_pss_verification_fixes_digest_mgf1_and_salt_length() {
    const PUBLIC_PEM: &str = include_str!("fixtures/crypto_rsapss_nist.pub.pem");
    const MESSAGE_HEX: &str = "e002377affb04f0fe4598de9d92d31d6c786040d5776976556a2cfc55e54a1dcb3cb1b126bd6a4bed2a184990ccea773fcc79d246553e6c64f686d21ad4152673cafec22aeb40f6a084e8a5b4991f4c64cf8a927effd0fd775e71e8329e41fdd4457b3911173187b4f09a817d79ea2397fc12dfe3d9c9a0290c8ead31b6690a6";
    const SIGNATURE_HEX: &str = "4f9b425c2058460e4ab2f5c96384da2327fd29150f01955a76b4efe956af06dc08779a374ee4607eab61a93adc5608f4ec36e47f2a0f754e8ff839a8a19b1db1e884ea4cf348cd455069eb87afd53645b44e28a0a56808f5031da5ba9112768dfbfca44ebe63a0c0572b731d66122fb71609be1480faa4e4f75e43955159d70f081e2a32fbb19a48b9f162cf6b2fb445d2d6994bc58910a26b5943477803cdaaa1bd74b0da0a5d053d8b1dc593091db5388383c26079f344e2aea600d0e324164b450f7b9b465111b7265f3b1b063089ae7e2623fc0fda8052cf4bf3379102fbf71d7c98e8258664ceed637d20f95ff0111881e650ce61f251d9c3a629ef222d";
    const WRONG_MGF1_HEX: &str = include_str!("fixtures/crypto_rsapss_mgf1_sha384.sig.hex").trim();
    const WRONG_SALT_HEX: &str = include_str!("fixtures/crypto_rsapss_salt20.sig.hex").trim();
    // NIST CAVP SigVerPSS_186-3.rsp: 2048-bit RSA, SHA-256, MGF1-SHA256, and 32-byte salt.
    // Negative controls use the same published private key with MGF1-SHA384 or a 20-byte salt.
    let source = format!(
        r#"{HELPERS}
boolean rsaPssVectorChecks(borrow CryptoKey key, String messageHex, String signatureHex,
    String wrongMgfHex, String wrongSaltHex) {{
    match (hexBuffer(messageHex)) {{
        Err(error) {{ return false; }}
        Ok(message) {{
            match (hexBuffer(signatureHex)) {{
                Err(error) {{ return false; }}
                Ok(signature) {{
                    match (hexBuffer(wrongMgfHex)) {{
                        Err(error) {{ return false; }}
                        Ok(wrongMgf) {{
                            match (hexBuffer(wrongSaltHex)) {{
                                Err(error) {{ return false; }}
                                Ok(wrongSalt) {{
                                    boolean knownAnswer = booleanResultIs(key.verify(new Text("SHA256"), message, signature), true);
                                    boolean wrongDigest = booleanResultIs(key.verify(new Text("SHA384"), message, signature), false);
                                    boolean wrongMgfParameters = booleanResultIs(key.verify(new Text("SHA256"), message, wrongMgf), false);
                                    boolean wrongSaltParameters = booleanResultIs(key.verify(new Text("SHA256"), message, wrongSalt), false);
                                    message.set(0L, (byte)(message.get(0L) + 1));
                                    boolean changedMessage = booleanResultIs(key.verify(new Text("SHA256"), message, signature), false);
                                    return knownAnswer && wrongDigest && wrongMgfParameters
                                        && wrongSaltParameters && changedMessage;
                                }}
                            }}
                        }}
                    }}
                }}
            }}
        }}
    }}
}}
void main() {{
    match (CryptoKey.parsePublicPem(new Text({}))) {{
        Err(error) {{ println(false); }}
        Ok(key) {{
            println(rsaPssVectorChecks(key, {}, {}, {}, {}));
        }}
    }}
}}
"#,
        nacre_literal(PUBLIC_PEM),
        nacre_literal(MESSAGE_HEX),
        nacre_literal(SIGNATURE_HEX),
        nacre_literal(WRONG_MGF1_HEX),
        nacre_literal(WRONG_SALT_HEX),
    );
    assert_native(&source, b"true\n");
}

#[test]
fn nist_ecdsa_verification_accepts_binary_p256_vector() {
    const PUBLIC_PEM: &str = include_str!("fixtures/crypto_ecdsa_nist.pub.pem");
    const MESSAGE_HEX: &str = "73c5f6a67456ae48209b5f85d1e7de7758bf235300c6ae2bdceb1dcb27a7730fb68c950b7fcada0ecc4661d3578230f225a875e69aaa17f1e71c6be5c831f22663bac63d0c7a9635edb0043ff8c6f26470f02a7bc56556f1437f06dfa27b487a6c4290d8bad38d4879b334e341ba092dde4e4ae694a9c09302e2dbf443581c08";
    const SIGNATURE_HEX: &str = "304502201d75830cd36f4c9aa181b2c4221e87f176b7f05b7c87824e82e396c88315c407022100cb2acb01dac96efc53a32d4a0d85d0c2e48955214783ecf50a4f0414a319c05a";
    // NIST CAVP FIPS 186-4 SigVer.rsp, P-256/SHA-256; the 128-byte message contains NUL and non-text bytes.
    let source = format!(
        r#"{HELPERS}
boolean ecdsaVectorChecks(borrow CryptoKey key, String messageHex, String signatureHex) {{
    match (hexBuffer(messageHex)) {{
        Err(error) {{ return false; }}
        Ok(message) {{
            match (hexBuffer(signatureHex)) {{
                Err(error) {{ return false; }}
                Ok(signature) {{
                    boolean knownAnswer = booleanResultIs(key.verify(new Text("SHA256"), message, signature), true);
                    boolean wrongDigest = booleanResultIs(key.verify(new Text("SHA384"), message, signature), false);
                    message.set(0L, (byte)(message.get(0L) + 1));
                    boolean changedMessage = booleanResultIs(key.verify(new Text("SHA256"), message, signature), false);
                    return knownAnswer && wrongDigest && changedMessage;
                }}
            }}
        }}
    }}
}}
void main() {{
    match (CryptoKey.parsePublicPem(new Text({}))) {{
        Err(error) {{ println(false); }}
        Ok(key) {{ println(ecdsaVectorChecks(key, {}, {})); }}
    }}
}}
"#,
        nacre_literal(PUBLIC_PEM),
        nacre_literal(MESSAGE_HEX),
        nacre_literal(SIGNATURE_HEX),
    );
    assert_native(&source, b"true\n");
}

#[test]
fn rfc7748_and_rfc5903_agreement_match_known_secrets() {
    const X25519_ALICE: &str = include_str!("fixtures/crypto_x25519_alice.private.pem");
    const X25519_BOB: &str = include_str!("fixtures/crypto_x25519_bob.private.pem");
    const P256_INITIATOR: &str = include_str!("fixtures/crypto_ecdh_rfc5903_initiator.private.pem");
    const P256_RESPONDER: &str = include_str!("fixtures/crypto_ecdh_rfc5903_responder.private.pem");
    // RFC 7748 Section 5.2 and RFC 5903 Section 8.1 independent shared-secret answers.
    let source = format!(
        r#"{HELPERS}
void main() {{
    println(publishedAgreementMatches({}, {}, "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742"));
    println(publishedAgreementMatches({}, {}, "d6840f6b42f6edafd13116e0e12565202fef8e9ece7dce03812464d04b9442de"));
}}
"#,
        nacre_literal(X25519_ALICE),
        nacre_literal(X25519_BOB),
        nacre_literal(P256_INITIATOR),
        nacre_literal(P256_RESPONDER),
    );
    assert_native(&source, b"true\ntrue\n");
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
            println(certificateTimesMatch(leaf, 1791881336L, 2107241336L));
            println(booleanResultIs(leaf.matchesHost(new Text("good.example")), true));
            println(booleanResultIs(leaf.matchesHost(new Text("wrong.example")), false));
            match (Path.fromText(new Text({}))) {{
                Err(error) {{ println(false); println(false); println(false); println(false); }}
                Ok(caFile) {{
                    println(unitError(leaf.verify(new Text("good.example"), caFile, 1791881337L)));
                    match (Certificate.parsePemChain(new Text({}))) {{
                        Err(error) {{ println(false); println(false); println(false); }}
                        Ok(chain) {{
                            println(unitOk(chain.verify(new Text("good.example"), caFile, 1791881337L)));
                            println(unitError(chain.verify(new Text("wrong.example"), caFile, 1791881337L)));
                            println(unitError(chain.verify(new Text("good.example"), caFile, 2107241337L)));
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
#[test]
fn certificate_times_preserve_pre_epoch_and_reject_invalid_intervals() {
    // Parser-only leaf copies have changed validity bytes and are not used for signature verification.
    const PRE_EPOCH: &str = include_str!("fixtures/crypto_leaf_pre_epoch.pem");
    const MALFORMED: &str = include_str!("fixtures/crypto_leaf_malformed_time.pem");
    const REVERSED: &str = include_str!("fixtures/crypto_leaf_reversed_time.pem");
    let source = format!(
        r#"{HELPERS}
void main() {{
    match (Certificate.parsePem(new Text({}))) {{
        Err(error) {{ println(false); }}
        Ok(certificate) {{ println(certificateTimesMatch(certificate, -1L, 1L)); }}
    }}
    println(certificateError(Certificate.parsePem(new Text({}))));
    println(certificateError(Certificate.parsePem(new Text({}))));
}}
"#,
        nacre_literal(PRE_EPOCH),
        nacre_literal(MALFORMED),
        nacre_literal(REVERSED),
    );
    assert_native(&source, b"true\ntrue\ntrue\n");
}
