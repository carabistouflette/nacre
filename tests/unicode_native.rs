use nacre::{compile_to_c, native};
use std::process::Output;

fn execute(source: &str, flags: &[&str]) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("unicode_native.ncr", source)));
    native::compile_c_with_flags(&c, flags)
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_clean(output: &Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = stderr.lines().find(|line| line.starts_with("NACRE_MEMORY "))
        .unwrap_or_else(|| panic!("missing allocation report: {stderr}"));
    let mut fields = report.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"), "{report}");
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{report}");
    assert_eq!(fields.next(), Some("live=0"), "{report}");
    assert_eq!(fields.next(), None, "{report}");
    assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
    assert!(!stderr.contains("runtime error:"), "{stderr}");
}

const SANITIZERS: &[&str] = &[
    "-DNACRE_MEMORY_STATS",
    "-fsanitize=address,undefined",
    "-fno-omit-frame-pointer",
];

#[test]
fn unicode_transforms_graphemes_properties_and_locale_errors() {
    let output = execute(r#"
void main() {
    Text decomposed = new Text("é");
    Text nfc = new Text("NFC");
    match (Unicode.normalize(decomposed, nfc)) {
        Ok(composed) { println(composed.equalsString("é")); }
        Err(error) { println("NFC_ERR"); drop(error); }
    }

    Text composedInput = new Text("é");
    Text nfd = new Text("NFD");
    match (Unicode.normalize(composedInput, nfd)) {
        Ok(decomposedOutput) { println(decomposedOutput.equals(decomposed)); }
        Err(error) { println("NFD_ERR"); drop(error); }
    }
    Text german = new Text("de-DE");
    Text sharpS = new Text("straße");
    match (Unicode.toUpperCase(sharpS, german)) {
        Ok(uppercase) { uppercase.print(); }
        Err(error) { println("UPPER_ERR"); drop(error); }
    }
    Text turkish = new Text("tr-TR");
    Text dotted = new Text("Iİ");
    match (Unicode.toLowerCase(dotted, turkish)) {
        Ok(lowercase) { lowercase.print(); }
        Err(error) { println("LOWER_ERR"); drop(error); }
    }
    Text caseFoldInput = new Text("Straße");
    match (Unicode.caseFold(caseFoldInput)) {
        Ok(folded) { folded.print(); }
        Err(error) { println("FOLD_ERR"); drop(error); }
    }
    Text umlaut = new Text("ä");
    Text zed = new Text("z");
    match (Unicode.compareLocale(umlaut, zed, german)) {
        Ok(order) { println(order < 0); }
        Err(error) { println("COLLATION_ERR"); drop(error); }
    }

    Text family = new Text("👨‍👩‍👧‍👦");
    match (Unicode.graphemeCount(family)) {
        Ok(count) { println(count); }
        Err(error) { println("COUNT_ERR"); drop(error); }
    }
    match (Unicode.graphemeAt(family, 0)) {
        Ok(grapheme) { grapheme.print(); }
        Err(error) { println("GRAPHEME_ERR"); drop(error); }
    }
    match (Unicode.graphemeAt(family, 1)) {
        Ok(grapheme) { println("BAD_INDEX_ACCEPTED"); drop(grapheme); }
        Err(error) { println("INDEX_ERR"); drop(error); }
    }

    println(Character.isLetter('Ω'));
    println(Character.isDigit('٥'));
    println(Character.isWhitespace('　'));
    println(Character.isUpperCase('Ω'));
    println(Character.isLowerCase('ω'));
    println(Character.toUpperCase('ω') == 'Ω');
    println(Character.toLowerCase('Ω') == 'ω');
    println(Character.category('A') == 1);
    println(Character.numericValue('٥') == 5.0);

    Text nulText = new Text("a\0b");
    Text root = new Text("");
    match (Unicode.toUpperCase(nulText, root)) {
        Ok(mapped) { println(mapped.equalsString("A\0B")); }
        Err(error) { println("NUL_ERR"); drop(error); }
    }
    Text invalidLocale = new Text("zz-ZZ");
    match (Unicode.toUpperCase(sharpS, invalidLocale)) {
        Ok(mapped) { println("UNKNOWN_LOCALE_ACCEPTED"); drop(mapped); }
        Err(error) { println("LOCALE_ERR"); drop(error); }
    }
    Text noncanonicalLocale = new Text("en-us");
    match (Unicode.toUpperCase(sharpS, noncanonicalLocale)) {
        Ok(mapped) { println("NONCANONICAL_LOCALE_ACCEPTED"); drop(mapped); }
        Err(error) { println("CANONICAL_ERR"); drop(error); }
    }
    Text invalidForm = new Text("NFKX");
    match (Unicode.normalize(decomposed, invalidForm)) {
        Ok(mapped) { println("INVALID_FORM_ACCEPTED"); drop(mapped); }
        Err(error) { println("FORM_ERR"); drop(error); }
    }
}
"#, SANITIZERS);
    assert_clean(&output);
    assert_eq!(output.stdout,
        "true\ntrue\nSTRASSE\nıi\nstrasse\ntrue\n1\n👨‍👩‍👧‍👦\nINDEX_ERR\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\nLOCALE_ERR\nCANONICAL_ERR\nFORM_ERR\n".as_bytes());
}

#[test]
fn charset_conversions_are_strict_and_preserve_explicit_endian_data() {
    let output = execute(r#"
void main() {
    Text latinText = new Text("café");
    Text latinName = new Text("iso-8859-1");
    match (Charset.encode(latinText, latinName)) {
        Err(error) { println("LATIN_ENCODE_ERR"); drop(error); }
        Ok(latinBytes) {
            println(latinBytes.length() == 4L);
            println(latinBytes.get(3L) == (byte)233);
            match (Charset.decode(latinBytes, latinName)) {
                Ok(decoded) { println(decoded.equals(latinText)); }
                Err(error) { println("LATIN_DECODE_ERR"); drop(error); }
            }
        }
    }

    Text unicodeText = new Text("A🦀");
    Text littleEndian = new Text("UTF-16LE");
    match (Charset.encode(unicodeText, littleEndian)) {
        Err(error) { println("UTF16LE_ENCODE_ERR"); drop(error); }
        Ok(bytes) {
            println(bytes.length() == 6L);
            match (Charset.decode(bytes, littleEndian)) {
                Ok(decoded) { println(decoded.equals(unicodeText)); }
                Err(error) { println("UTF16LE_DECODE_ERR"); drop(error); }
            }
        }
    }
    Text bigEndian = new Text("UTF-16BE");
    match (Charset.encode(unicodeText, bigEndian)) {
        Err(error) { println("UTF16BE_ENCODE_ERR"); drop(error); }
        Ok(bytes) {
            match (Charset.decode(bytes, bigEndian)) {
                Ok(decoded) { println(decoded.equals(unicodeText)); }
                Err(error) { println("UTF16BE_DECODE_ERR"); drop(error); }
            }
        }
    }

    Text asciiName = new Text("US-ASCII");
    match (Charset.encode(latinText, asciiName)) {
        Ok(bytes) { println("ASCII_ACCEPTED_UNREPRESENTABLE_TEXT"); drop(bytes); }
        Err(error) { println("ASCII_ERR"); drop(error); }
    }
    Text genericUtf16Name = new Text("UTF-16");
    match (Charset.encode(latinText, genericUtf16Name)) {
        Ok(bytes) { println("GENERIC_UTF16_ACCEPTED"); drop(bytes); }
        Err(error) { println("UNKNOWN_CHARSET_ERR"); drop(error); }
    }

    match (ByteBuffer.withCapacity(0L)) {
        Err(error) { println("BOM_BUFFER_ERR"); drop(error); }
        Ok(bomBytes) {
            bomBytes.append((byte)255);
            bomBytes.append((byte)254);
            bomBytes.append((byte)65);
            bomBytes.append((byte)0);
            match (Charset.decode(bomBytes, littleEndian)) {
                Ok(decoded) { println(decoded.charCount() == 2); }
                Err(error) { println("BOM_DECODE_ERR"); drop(error); }
            }
        }
    }
    match (ByteBuffer.withCapacity(0L)) {
        Err(error) { println("SURROGATE_BUFFER_ERR"); drop(error); }
        Ok(invalidUtf16) {
            invalidUtf16.append((byte)0);
            invalidUtf16.append((byte)216);
            match (Charset.decode(invalidUtf16, littleEndian)) {
                Ok(decoded) { println("UNPAIRED_SURROGATE_ACCEPTED"); drop(decoded); }
                Err(error) { println("SURROGATE_ERR"); drop(error); }
            }
        }
    }

    Text nulText = new Text("a\0b");
    Text utf8 = new Text("UTF-8");
    match (Charset.encode(nulText, utf8)) {
        Err(error) { println("UTF8_ENCODE_ERR"); drop(error); }
        Ok(bytes) {
            println(bytes.length() == 3L);
            match (Charset.decode(bytes, utf8)) {
                Ok(decoded) { println(decoded.equals(nulText)); }
                Err(error) { println("UTF8_DECODE_ERR"); drop(error); }
            }
        }
    }
}
"#, SANITIZERS);
    assert_clean(&output);
    assert_eq!(output.stdout,
        "true\ntrue\ntrue\ntrue\ntrue\nASCII_ERR\nUNKNOWN_CHARSET_ERR\ntrue\nSURROGATE_ERR\ntrue\ntrue\n".as_bytes());
}

#[test]
fn locale_number_formatting_preserves_exact_decimals_and_rejects_invalid_input() {
    let output = execute(r#"
void main() {
    List<Text> locales = Locale.available();
    boolean hasGerman = false;
    boolean hasRoot = false;
    int index = 0;
    while (index < locales.size()) {
        borrow Text tag = locales.getRef(index);
        if (tag.equalsString("de-DE")) { hasGerman = true; }
        if (tag.equalsString("und")) { hasRoot = true; }
        index = index + 1;
    }
    println(locales.size() > 0);
    println(hasGerman);
    println(hasRoot);

    Text pattern = new Text("#,##0.00");
    Text german = new Text("de-DE");
    match (Locale.formatNumber(12345.5, pattern, german)) {
        Ok(formatted) { formatted.print(); }
        Err(error) { println("NUMBER_FORMAT_ERR"); drop(error); }
    }
    Text exact = new Text("12345678901234567890.125");
    Text exactPattern = new Text("#,##0.000");
    match (Locale.formatDecimal(exact, exactPattern, german)) {
        Ok(formatted) { formatted.print(); }
        Err(error) { println("EXACT_FORMAT_ERR"); drop(error); }
    }
    Text malformedExact = new Text("123.45x");
    match (Locale.formatDecimal(malformedExact, exactPattern, german)) {
        Ok(formatted) { println("MALFORMED_EXACT_ACCEPTED"); drop(formatted); }
        Err(error) { println("EXACT_DECIMAL_ERR"); drop(error); }
    }
    Text localized = new Text("12.345,5");
    Text oneDecimal = new Text("#,##0.0");
    match (Locale.parseNumber(localized, oneDecimal, german)) {
        Ok(number) { println(number == 12345.5); }
        Err(error) { println("NUMBER_PARSE_ERR"); drop(error); }
    }
    Text currency = new Text("usd");
    Text us = new Text("en-US");
    match (Locale.formatCurrency(12.5, currency, us)) {
        Ok(formatted) { formatted.print(); }
        Err(error) { println("CURRENCY_FORMAT_ERR"); drop(error); }
    }

    Text trailing = new Text("12.345,5 trailing");
    match (Locale.parseNumber(trailing, oneDecimal, german)) {
        Ok(number) { println("TRAILING_INPUT_ACCEPTED"); }
        Err(error) { println("TRAILING_ERR"); drop(error); }
    }
    Text invalidPattern = new Text("#,##0.0.0");
    match (Locale.formatNumber(1.0, invalidPattern, german)) {
        Ok(formatted) { println("INVALID_PATTERN_ACCEPTED"); drop(formatted); }
        Err(error) { println("PATTERN_ERR"); drop(error); }
    }
    Text invalidLocale = new Text("zz-ZZ");
    match (Locale.formatNumber(1.0, pattern, invalidLocale)) {
        Ok(formatted) { println("INVALID_LOCALE_ACCEPTED"); drop(formatted); }
        Err(error) { println("LOCALE_ERR"); drop(error); }
    }
    Text invalidCurrency = new Text("ZZZ");
    match (Locale.formatCurrency(1.0, invalidCurrency, us)) {
        Ok(formatted) { println("INVALID_CURRENCY_ACCEPTED"); drop(formatted); }
        Err(error) { println("CURRENCY_ERR"); drop(error); }
    }
}
"#, SANITIZERS);
    assert_clean(&output);
    assert_eq!(output.stdout,
        "true\ntrue\ntrue\n12.345,50\n12.345.678.901.234.567.890,125\nEXACT_DECIMAL_ERR\ntrue\n$12.50\nTRAILING_ERR\nPATTERN_ERR\nLOCALE_ERR\nCURRENCY_ERR\n".as_bytes());
}

#[test]
fn ordinary_text_programs_remain_usable_without_icu_calls() {
    let output = execute(
        r#"void main() { Text value = new Text("ordinary café text"); println(value.charCount()); value.print(); }"#,
        &["-DNACRE_MEMORY_STATS"],
    );
    assert_clean(&output);
    assert_eq!(output.stdout, "18\nordinary café text\n".as_bytes());
}
