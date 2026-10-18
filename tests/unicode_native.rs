mod support;
use std::process::Output;

fn execute(source: &str, flags: &[&str]) -> Output {
    support::compile(source, flags)
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
fn unicode_graphemes_cover_empty_combining_and_index_boundaries() {
    let output = execute(r#"
void main() {
    Text empty = new Text("");
    match (Unicode.graphemeCount(empty)) {
        Ok(count) { println(count == 0); }
        Err(error) { println("EMPTY_COUNT_ERR"); drop(error); }
    }
    match (Unicode.graphemeAt(empty, 0)) {
        Ok(grapheme) { println("EMPTY_INDEX_ACCEPTED"); drop(grapheme); }
        Err(error) { println("EMPTY_INDEX_ERR"); drop(error); }
    }

    Text clusters = new Text("éx");
    match (Unicode.graphemeCount(clusters)) {
        Ok(count) { println(count == 2); }
        Err(error) { println("CLUSTER_COUNT_ERR"); drop(error); }
    }
    match (Unicode.graphemeAt(clusters, 0)) {
        Ok(grapheme) { println(grapheme.equalsString("é")); }
        Err(error) { println("FIRST_CLUSTER_ERR"); drop(error); }
    }
    match (Unicode.graphemeAt(clusters, 1)) {
        Ok(grapheme) { println(grapheme.equalsString("x")); }
        Err(error) { println("LAST_CLUSTER_ERR"); drop(error); }
    }
    match (Unicode.graphemeAt(clusters, 2)) {
        Ok(grapheme) { println("END_INDEX_ACCEPTED"); drop(grapheme); }
        Err(error) { println("END_INDEX_ERR"); drop(error); }
    }
    match (Unicode.graphemeAt(clusters, -1)) {
        Ok(grapheme) { println("NEGATIVE_INDEX_ACCEPTED"); drop(grapheme); }
        Err(error) { println("NEGATIVE_INDEX_ERR"); drop(error); }
    }
}
"#, SANITIZERS);
    assert_clean(&output);
    assert_eq!(output.stdout, b"true\nEMPTY_INDEX_ERR\ntrue\ntrue\ntrue\nEND_INDEX_ERR\nNEGATIVE_INDEX_ERR\n");
}

#[test]
fn charset_conversions_are_strict_and_preserve_explicit_endian_data() {
    let output = execute(r#"
void main() {
    List<Text> available = Charset.available();
    Text emptyText = new Text("");
    boolean everyAvailableNameOpens = available.size() > 0;
    int availableIndex = 0;
    while (availableIndex < available.size()) {
        borrow Text name = available.getRef(availableIndex);
        match (Charset.encode(emptyText, name)) {
            Ok(bytes) { drop(bytes); }
            Err(error) { everyAvailableNameOpens = false; drop(error); }
        }
        availableIndex = availableIndex + 1;
    }
    println(everyAvailableNameOpens);

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

    Text euro = new Text("€");
    Text windows1252 = new Text("windows-1252");
    match (Charset.encode(euro, windows1252)) {
        Err(error) { println("WINDOWS1252_ENCODE_ERR"); drop(error); }
        Ok(bytes) {
            println(bytes.length() == 1L && bytes.get(0L) == (byte)128);
            match (Charset.decode(bytes, windows1252)) {
                Ok(decoded) { println(decoded.equals(euro)); }
                Err(error) { println("WINDOWS1252_DECODE_ERR"); drop(error); }
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

    Text japanese = new Text("日本語");
    Text statefulName = new Text("ISO-2022-JP");
    match (Charset.encode(japanese, statefulName)) {
        Err(error) { println("STATEFUL_ENCODE_ERR"); drop(error); }
        Ok(encoded) {
            println(encoded.length() > 3L);
            drop(japanese);
            match (Charset.decode(encoded, statefulName)) {
                Ok(decoded) {
                    drop(encoded);
                    println(decoded.equalsString("日本語"));
                }
                Err(error) { println("STATEFUL_DECODE_ERR"); drop(error); }
            }
        }
    }
    Text unrepresentable = new Text("🦀");
    match (Charset.encode(unrepresentable, statefulName)) {
        Ok(bytes) { println("STATEFUL_UNREPRESENTABLE_ACCEPTED"); drop(bytes); }
        Err(error) { println("STATEFUL_UNREPRESENTABLE_ERR"); drop(error); }
    }

    Text asciiName = new Text("US-ASCII");
    match (Charset.encode(euro, asciiName)) {
        Ok(bytes) { println("ASCII_ACCEPTED_UNREPRESENTABLE_TEXT"); drop(bytes); }
        Err(error) { println("ASCII_ERR"); drop(error); }
    }
    Text emptyCharsetName = new Text("");
    match (Charset.encode(euro, emptyCharsetName)) {
        Ok(bytes) { println("EMPTY_CHARSET_ACCEPTED"); drop(bytes); }
        Err(error) { println("EMPTY_CHARSET_ERR"); drop(error); }
    }
    Text defaultCharsetName = new Text("default");
    match (Charset.encode(euro, defaultCharsetName)) {
        Ok(bytes) { println("DEFAULT_CHARSET_ACCEPTED"); drop(bytes); }
        Err(error) { println("DEFAULT_CHARSET_ERR"); drop(error); }
    }
    Text nulCharsetName = new Text("UTF-8\0alias");
    match (Charset.encode(euro, nulCharsetName)) {
        Ok(bytes) { println("NUL_CHARSET_ACCEPTED"); drop(bytes); }
        Err(error) { println("NUL_CHARSET_ERR"); drop(error); }
    }
    Text unknownCharsetName = new Text("not-a-real-encoding");
    match (Charset.encode(euro, unknownCharsetName)) {
        Ok(bytes) { println("UNKNOWN_CHARSET_ACCEPTED"); drop(bytes); }
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
        Err(error) { println("BE_BOM_BUFFER_ERR"); drop(error); }
        Ok(bomBytes) {
            bomBytes.append((byte)254);
            bomBytes.append((byte)255);
            bomBytes.append((byte)0);
            bomBytes.append((byte)65);
            match (Charset.decode(bomBytes, bigEndian)) {
                Ok(decoded) { println(decoded.charCount() == 2); }
                Err(error) { println("BE_BOM_DECODE_ERR"); drop(error); }
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
    match (ByteBuffer.withCapacity(0L)) {
        Err(error) { println("STATEFUL_BUFFER_ERR"); drop(error); }
        Ok(invalidStateful) {
            invalidStateful.append((byte)27);
            invalidStateful.append((byte)36);
            match (Charset.decode(invalidStateful, statefulName)) {
                Ok(decoded) { println("STATEFUL_MALFORMED_ACCEPTED"); drop(decoded); }
                Err(error) { println("STATEFUL_MALFORMED_ERR"); drop(error); }
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
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\nSTATEFUL_UNREPRESENTABLE_ERR\nASCII_ERR\nEMPTY_CHARSET_ERR\nDEFAULT_CHARSET_ERR\nNUL_CHARSET_ERR\nUNKNOWN_CHARSET_ERR\ntrue\ntrue\nSURROGATE_ERR\nSTATEFUL_MALFORMED_ERR\ntrue\ntrue\n".as_bytes());
}

#[test]
fn locale_number_formatting_preserves_exact_decimals_and_rejects_invalid_input() {
    let output = execute(r##"
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
    println(hasGerman);
    println(hasRoot);

    Text pattern = new Text("#,##0.00");
    Text german = new Text("de-DE");
    Text us = new Text("en-US");
    match (Locale.formatNumber(12345.5, pattern, german)) {
        Ok(formatted) { formatted.print(); }
        Err(error) { println("NUMBER_FORMAT_ERR"); drop(error); }
    }
    Text exact = new Text("12345678901234567890.125");
    Text exactPattern = new Text("#,##0.000");
    Text exactLocalized = new Text("9.007.199.254.740.993,01");
    Text exactLocalizedPattern = new Text("#,##0.00");
    match (Locale.parseDecimal(exactLocalized, exactLocalizedPattern, german)) {
        Ok(parsedExact) {
            match (Locale.formatDecimal(parsedExact, exactLocalizedPattern, german)) {
                Ok(formatted) { println(formatted.equalsString("9.007.199.254.740.993,01")); }
                Err(error) { println("EXACT_PARSE_FORMAT_ERR"); drop(error); }
            }
            Text usd = new Text("USD");
            match (Locale.formatCurrencyDecimal(parsedExact, usd, us)) {
                Ok(formatted) { println(formatted.equalsString("$9,007,199,254,740,993.01")); }
                Err(error) { println("EXACT_CURRENCY_ERR"); drop(error); }
            }
        }
        Err(error) { println("EXACT_PARSE_ERR"); drop(error); }
    }
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
    match (Locale.formatCurrency(12.5, currency, us)) {
        Ok(formatted) { formatted.print(); }
        Err(error) { println("CURRENCY_FORMAT_ERR"); drop(error); }
    }

    Text trailing = new Text("12.345,5 trailing");
    match (Locale.parseNumber(trailing, oneDecimal, german)) {
        Ok(number) { println("TRAILING_INPUT_ACCEPTED"); }
        Err(error) { println("TRAILING_ERR"); drop(error); }
    }
    match (Locale.parseDecimal(trailing, oneDecimal, german)) {
        Ok(parsed) { println("EXACT_TRAILING_INPUT_ACCEPTED"); drop(parsed); }
        Err(error) { println("EXACT_TRAILING_ERR"); drop(error); }
    }
    Text invalidPattern = new Text("#,##0.0.0");
    match (Locale.formatNumber(1.0, invalidPattern, german)) {
        Ok(formatted) { println("INVALID_PATTERN_ACCEPTED"); drop(formatted); }
        Err(error) { println("PATTERN_ERR"); drop(error); }
    }
    match (Locale.parseDecimal(localized, invalidPattern, german)) {
        Ok(parsed) { println("INVALID_PARSE_PATTERN_ACCEPTED"); drop(parsed); }
        Err(error) { println("EXACT_PATTERN_ERR"); drop(error); }
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
    match (Locale.formatCurrencyDecimal(exact, invalidCurrency, us)) {
        Ok(formatted) { println("INVALID_EXACT_CURRENCY_ACCEPTED"); drop(formatted); }
        Err(error) { println("EXACT_CURRENCY_ERR"); drop(error); }
    }
}
"##, SANITIZERS);
    assert_clean(&output);
    assert_eq!(output.stdout,
        "true\ntrue\n12.345,50\ntrue\ntrue\n12.345.678.901.234.567.890,125\nEXACT_DECIMAL_ERR\ntrue\n$12.50\nTRAILING_ERR\nEXACT_TRAILING_ERR\nPATTERN_ERR\nEXACT_PATTERN_ERR\nLOCALE_ERR\nCURRENCY_ERR\nEXACT_CURRENCY_ERR\n".as_bytes());
}

#[test]
fn character_native_use_does_not_require_text_or_buffer_calls() {
    let output = execute(
        r#"void main() { println(Character.isLetter('A')); }"#,
        SANITIZERS,
    );
    assert_clean(&output);
    assert_eq!(output.stdout, b"true\n");
}

#[test]
fn unicode_native_use_does_not_require_charset_calls() {
    let output = execute(r#"
void main() {
    Text input = new Text("Straße");
    match (Unicode.caseFold(input)) {
        Ok(folded) { println(folded.equalsString("strasse")); }
        Err(error) { println("UNICODE_ERR"); drop(error); }
    }
}
"#, SANITIZERS);
    assert_clean(&output);
    assert_eq!(output.stdout, b"true\n");
}

#[test]
fn charset_native_use_does_not_require_unicode_calls() {
    let output = execute(r#"
void main() {
    Text input = new Text("A");
    Text charset = new Text("UTF-8");
    match (Charset.encode(input, charset)) {
        Ok(bytes) { println(bytes.length() == 1L); }
        Err(error) { println("CHARSET_ERR"); drop(error); }
    }
}
"#, SANITIZERS);
    assert_clean(&output);
    assert_eq!(output.stdout, b"true\n");
}

#[test]
fn locale_inventory_factory_works_without_text_or_number_format_calls() {
    let output = execute(r#"
void main() {
    List<Text> locales = Locale.available();
    boolean hasRoot = false;
    int index = 0;
    while (index < locales.size()) {
        borrow Text tag = locales.getRef(index);
        if (tag.equalsString("und")) { hasRoot = true; }
        index = index + 1;
    }
    println(hasRoot);
}
"#, SANITIZERS);
    assert_clean(&output);
    assert_eq!(output.stdout, b"true\n");
}

#[test]
fn canonical_locale_extensions_configure_services_and_reject_unknown_values() {
    let output = execute(r##"
void main() {
    Text umlaut = new Text("ä");
    Text digraph = new Text("ae");
    Text phonebook = new Text("de-DE-u-co-phonebk");
    match (Unicode.compareLocale(umlaut, digraph, phonebook)) {
        Ok(order) { println(order > 0); }
        Err(error) { println("PHONEBOOK_ERR"); drop(error); }
    }

    Text pattern = new Text("#,##0");
    Text arabicNumbers = new Text("en-US-u-nu-arab");
    match (Locale.formatNumber(1234.0, pattern, arabicNumbers)) {
        Ok(formatted) { println(formatted.charAt(0) == '١'); }
        Err(error) { println("NUMBERING_EXTENSION_ERR"); drop(error); }
    }

    Text unknownCollation = new Text("de-DE-u-co-unknown");
    match (Unicode.compareLocale(umlaut, digraph, unknownCollation)) {
        Ok(order) { println("UNKNOWN_COLLATION_ACCEPTED"); }
        Err(error) { println("UNKNOWN_COLLATION_ERR"); drop(error); }
    }
    Text unknownNumbering = new Text("en-US-u-nu-unknown");
    match (Locale.formatNumber(1.0, pattern, unknownNumbering)) {
        Ok(formatted) { println("UNKNOWN_NUMBERING_ACCEPTED"); drop(formatted); }
        Err(error) { println("UNKNOWN_NUMBERING_ERR"); drop(error); }
    }
}
"##, SANITIZERS);
    assert_clean(&output);
    assert_eq!(output.stdout,
        b"true\ntrue\nUNKNOWN_COLLATION_ERR\nUNKNOWN_NUMBERING_ERR\n");
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
