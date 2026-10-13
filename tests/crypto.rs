use nacre::{compile_to_c, native};
use std::process::Output;

fn execute(source: &str) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("crypto.ncr", source)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_memory_balanced(output: &Output) {
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

const VECTORS: &str = r#"
int hexDigit(char value) {
    if (value >= '0' && value <= '9') { return value - '0'; }
    if (value >= 'a' && value <= 'f') { return value - 'a' + 10; }
    if (value >= 'A' && value <= 'F') { return value - 'A' + 10; }
    return -1;
}

Result<ByteBuffer, Text> fromHex(borrow Text encoded) {
    if (encoded.byteLength() % 2 != 0) {
        return new Result<ByteBuffer, Text>.Err(new Text("odd hexadecimal length"));
    }
    ByteBuffer output = ByteBuffer.withCapacity((long)(encoded.byteLength() / 2))?;
    int index = 0;
    while (index < encoded.charCount()) {
        int high = hexDigit(encoded.charAt(index));
        int low = hexDigit(encoded.charAt(index + 1));
        if (high < 0 || low < 0) {
            return new Result<ByteBuffer, Text>.Err(new Text("invalid hexadecimal input"));
        }
        output.append((byte)(high * 16 + low));
        index = index + 2;
    }
    return new Result<ByteBuffer, Text>.Ok(move output);
}

Text toHex(borrow ByteBuffer bytes) {
    Text digits = new Text("0123456789abcdef");
    Text output = new Text("");
    long index = 0L;
    while (index < bytes.length()) {
        int value = (int)bytes.get(index);
        output.appendChar(digits.charAt(value / 16));
        output.appendChar(digits.charAt(value % 16));
        index = index + 1L;
    }
    return output;
}

void printHex(borrow ByteBuffer bytes) {
    Text encoded = toHex(bytes);
    encoded.print();
}

boolean isError(Result<ByteBuffer, Text> result) {
    match (result) {
        Ok(value) { return false; }
        Err(error) { return true; }
    }
}

Result<Unit, Text> runVectors() {
    Text sha256 = new Text("SHA256");
    ByteBuffer abc = fromHex(new Text("616263"))?;
    ByteBuffer digest = Crypto.digest(sha256, abc)?;
    printHex(digest);
    Text sha384 = new Text("SHA384");
    ByteBuffer digest384 = Crypto.digest(sha384, abc)?;
    printHex(digest384);
    Text sha512 = new Text("SHA512");
    ByteBuffer digest512 = Crypto.digest(sha512, abc)?;
    printHex(digest512);
    Text sha3_256 = new Text("SHA3-256");
    ByteBuffer digest3_256 = Crypto.digest(sha3_256, abc)?;
    printHex(digest3_256);
    Text sha3_384 = new Text("SHA3-384");
    ByteBuffer digest3_384 = Crypto.digest(sha3_384, abc)?;
    printHex(digest3_384);
    Text sha3_512 = new Text("SHA3-512");
    ByteBuffer digest3_512 = Crypto.digest(sha3_512, abc)?;
    printHex(digest3_512);

    Text hmacName = new Text("SHA256");
    ByteBuffer hmacKey = fromHex(new Text("6b6579"))?;
    ByteBuffer hmacData = fromHex(new Text("54686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f67"))?;
    ByteBuffer mac = Crypto.hmac(hmacName, hmacKey, hmacData)?;
    printHex(mac);

    Text pbkdfName = new Text("SHA256");
    ByteBuffer password = fromHex(new Text("70617373776f7264"))?;
    ByteBuffer salt = fromHex(new Text("73616c74"))?;
    ByteBuffer derived = Crypto.pbkdf2(pbkdfName, password, salt, 1, 32L)?;
    printHex(derived);

    Text hkdfName = new Text("SHA256");
    ByteBuffer ikm = fromHex(new Text("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"))?;
    ByteBuffer hkdfSalt = fromHex(new Text("000102030405060708090a0b0c"))?;
    ByteBuffer info = fromHex(new Text("f0f1f2f3f4f5f6f7f8f9"))?;
    ByteBuffer okm = Crypto.hkdf(hkdfName, ikm, hkdfSalt, info, 42L)?;
    printHex(okm);

    ByteBuffer aesKey = fromHex(new Text("00000000000000000000000000000000"))?;
    ByteBuffer nonce = fromHex(new Text("000000000000000000000000"))?;
    ByteBuffer plaintext = fromHex(new Text("00000000000000000000000000000000"))?;
    ByteBuffer aad = fromHex(new Text(""))?;
    ByteBuffer ciphertextAndTag = Crypto.aesGcmEncrypt(aesKey, nonce, plaintext, aad)?;
    printHex(ciphertextAndTag);
    ByteBuffer decrypted = Crypto.aesGcmDecrypt(aesKey, nonce, ciphertextAndTag, aad)?;
    printHex(decrypted);
    println(Crypto.constantTimeEquals(plaintext, decrypted));
    ByteBuffer gcmKey = fromHex(new Text("feffe9928665731c6d6a8f9467308308"))?;
    ByteBuffer gcmNonce = fromHex(new Text("cafebabefacedbaddecaf888"))?;
    ByteBuffer gcmPlaintext = fromHex(new Text("d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a728c3c0c95156809539fcf0e2429a6b525b16aedf5aa0de657ba637b39"))?;
    ByteBuffer gcmAad = fromHex(new Text("feedfacedeadbeeffeedfacedeadbeefabaddad2"))?;
    ByteBuffer gcmCiphertextAndTag = Crypto.aesGcmEncrypt(gcmKey, gcmNonce, gcmPlaintext, gcmAad)?;
    printHex(gcmCiphertextAndTag);
    ByteBuffer gcmRecovered = Crypto.aesGcmDecrypt(gcmKey, gcmNonce, gcmCiphertextAndTag, gcmAad)?;
    printHex(gcmRecovered);
    println(Crypto.constantTimeEquals(gcmPlaintext, gcmRecovered));

    ByteBuffer different = fromHex(new Text("00000000000000000000000000000001"))?;
    println(Crypto.constantTimeEquals(plaintext, different));
    ByteBuffer emptyRandom = Crypto.randomBytes(0L)?;
    ByteBuffer random = Crypto.randomBytes(32L)?;
    println(emptyRandom.length() == 0L);
    println(random.length() == 32L);
    println(Crypto.constantTimeEquals(emptyRandom, random));
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (runVectors()) {
        Ok(value) { }
        Err(error) { println("VECTOR_ERR"); }
    }
}
"#;

const INVALID_INPUTS: &str = r#"
int hexDigit(char value) {
    if (value >= '0' && value <= '9') { return value - '0'; }
    if (value >= 'a' && value <= 'f') { return value - 'a' + 10; }
    return -1;
}

Result<ByteBuffer, Text> fromHex(borrow Text encoded) {
    ByteBuffer output = ByteBuffer.withCapacity((long)(encoded.byteLength() / 2))?;
    int index = 0;
    while (index < encoded.charCount()) {
        int high = hexDigit(encoded.charAt(index));
        int low = hexDigit(encoded.charAt(index + 1));
        output.append((byte)(high * 16 + low));
        index = index + 2;
    }
    return new Result<ByteBuffer, Text>.Ok(move output);
}

boolean isError(Result<ByteBuffer, Text> result) {
    match (result) {
        Ok(value) { return false; }
        Err(error) { return true; }
    }
}

Result<Unit, Text> runInvalid() {
    Text sha256 = new Text("SHA256");
    Text sha1 = new Text("SHA1");
    Text nulName = new Text("SHA256\0");
    ByteBuffer empty = fromHex(new Text(""))?;
    ByteBuffer key = fromHex(new Text("00000000000000000000000000000000"))?;
    ByteBuffer shortKey = fromHex(new Text("000000000000000000000000000000"))?;
    ByteBuffer nonce = fromHex(new Text("000000000000000000000000"))?;
    ByteBuffer shortNonce = fromHex(new Text("0000000000000000000000"))?;
    ByteBuffer shortCiphertext = fromHex(new Text("00"))?;
    ByteBuffer ciphertextAndTag = fromHex(new Text("0388dace60b6a392f328c2b971b2fe78ab6e47d42cec13bdf53a67b21257bddf"))?;
    ByteBuffer aad = fromHex(new Text(""))?;
    ByteBuffer wrongKey = fromHex(new Text("01000000000000000000000000000000"))?;
    ByteBuffer badTag = fromHex(new Text("0388dace60b6a392f328c2b971b2fe78ab6e47d42cec13bdf53a67b21257bdd0"))?;

    println(isError(Crypto.randomBytes(-1L)));
    println(isError(Crypto.randomBytes(2147483648L)));
    println(isError(Crypto.digest(sha1, empty)));
    println(isError(Crypto.digest(nulName, empty)));
    println(isError(Crypto.pbkdf2(sha256, empty, empty, 0, 1L)));
    println(isError(Crypto.hkdf(sha256, empty, empty, empty, 8161L)));
    println(isError(Crypto.aesGcmEncrypt(shortKey, nonce, empty, aad)));
    println(isError(Crypto.aesGcmEncrypt(key, shortNonce, empty, aad)));
    println(isError(Crypto.aesGcmDecrypt(shortKey, nonce, ciphertextAndTag, aad)));
    println(isError(Crypto.aesGcmDecrypt(key, shortNonce, ciphertextAndTag, aad)));
    println(isError(Crypto.aesGcmDecrypt(key, nonce, shortCiphertext, aad)));
    println(isError(Crypto.aesGcmDecrypt(wrongKey, nonce, ciphertextAndTag, aad)));
    println(isError(Crypto.aesGcmDecrypt(key, nonce, badTag, aad)));
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (runInvalid()) {
        Ok(value) { }
        Err(error) { println("INVALID_INPUT_HARNESS_ERR"); }
    }
}
"#;

#[test]
fn crypto_known_answers_and_owned_outputs() {
    let output = execute(VECTORS);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, concat!(
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n",
        "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7\n",
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\n",
        "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532\n",
        "ec01498288516fc926459f58e2c6ad8df9b473cb0fc08c2596da7cf0e49be4b298d88cea927ac7f539f1edf228376d25\n",
        "b751850b1a57168a5693cd924b6b096e08f621827444f70d884f5d0240d2712e10e116e9192af3c91a7ec57647e3934057340b4cf408d5a56592f8274eec53f0\n",
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8\n",
        "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b\n",
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865\n",
        "0388dace60b6a392f328c2b971b2fe78ab6e47d42cec13bdf53a67b21257bddf\n",
        "00000000000000000000000000000000\n",
        "42831ec2217774244b7221b784d0d49ce3aa212f2c02a4e035c17e2329aca12e21d514b25466931c7d8f6a5aac84aa051ba30b396a0aac973d58e0915bc94fbc3221a5db94fae95ae7121a47\n",
        "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a728c3c0c95156809539fcf0e2429a6b525b16aedf5aa0de657ba637b39\n",
        "true\n",
        "true\nfalse\ntrue\ntrue\nfalse\n"
    ).as_bytes());
    assert_memory_balanced(&output);
}

#[test]
fn crypto_rejects_unsupported_algorithms_invalid_boundaries_and_bad_authentication() {
    let output = execute(INVALID_INPUTS);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
    assert_memory_balanced(&output);
}
