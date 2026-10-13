use nacre::{compile_to_c, native};
use std::ffi::OsString;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);

fn compile_native(source: &str) -> native::NativeProgram {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("text_regex.ncr", source)));
    native::compile_c_with_flags(
        &c,
        &["-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"],
    ).unwrap_or_else(|error| panic!("native compile failed: {error}"))
}

fn run(program: &native::NativeProgram, args: &[OsString]) -> Output {
    Command::new(program.path()).args(args).output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_clean_memory(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = stderr.lines().find(|line| line.starts_with("NACRE_MEMORY "))
        .unwrap_or_else(|| panic!("missing allocation report: {stderr}"));
    assert!(report.contains("live=0"), "{report}");
    assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
    assert!(!stderr.contains("runtime error:"), "{stderr}");
}

fn execute(source: &str) -> Output {
    let program = compile_native(source);
    let output = run(&program, &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_clean_memory(&output);
    output
}

#[test]
fn text_uses_utf8_scalars_and_exact_bytewise_queries() {
    let output = execute(r#"
void main() {
    Text value = new Text("Aé😀Z\0Q");
    Text emojiNeedle = new Text("😀");
    Text nulNeedle = new Text("\0");
    Text missingNeedle = new Text("missing");
    Text emptyNeedle = new Text("");
    Text prefix = new Text("Aé");
    Text suffix = new Text("Q");
    println(value.byteLength());
    println(value.charCount());
    println(value.charAt(1));
    println(value.charAt(2));
    println((int)value.charAt(4));
    println(value.indexOf(emojiNeedle));
    println(value.indexOf(nulNeedle));
    println(value.indexOf(missingNeedle));
    println(value.indexOf(emptyNeedle));
    println(value.startsWith(prefix));
    println(value.startsWith(emptyNeedle));
    println(value.endsWith(suffix));
    println(value.endsWith(emptyNeedle));

    Text composed = new Text("é");
    Text decomposed = new Text("é");
    println(composed.equals(decomposed));
    println(composed.compareTo(decomposed));

    match (value.slice(1, 3)) {
        Err(error) { println("invalid scalar slice"); drop(error); }
        Ok(slice) { slice.print(); drop(slice); }
    }
    match (value.slice(2, 2)) {
        Err(error) { println("invalid empty slice"); drop(error); }
        Ok(slice) { println(slice.byteLength()); drop(slice); }
    }
    match (value.slice(3, 6)) {
        Err(error) { println("invalid NUL slice"); drop(error); }
        Ok(slice) { println(slice.byteLength()); println(slice.charCount()); println((int)slice.charAt(1)); drop(slice); }
    }
    match (value.slice(-1, 2)) {
        Err(error) { println("range rejected"); drop(error); }
        Ok(slice) { println("range accepted"); drop(slice); }
    }
    match (value.sliceBytes(1, 3)) {
        Err(error) { println("invalid byte slice"); drop(error); }
        Ok(slice) { slice.print(); drop(slice); }
    }
    match (value.sliceBytes(2, 3)) {
        Err(error) { println("split scalar rejected"); drop(error); }
        Ok(slice) { println("split scalar accepted"); drop(slice); }
    }

    Text spaced = new Text(" \t　hé😀 \n");
    Text trimmed = spaced.trim();
    trimmed.print();
    drop(trimmed);

    Text replaceInput = new Text("aé😀a");
    Text accentNeedle = new Text("é");
    Text omega = new Text("Ω");
    Text replaced = replaceInput.replace(accentNeedle, omega);
    replaced.print();
    Text insertionInput = new Text("é😀");
    Text dot = new Text(".");
    Text inserted = insertionInput.replace(emptyNeedle, dot);
    inserted.print();
    Text emptyReplaceInput = new Text("");
    Text emptyReplacement = emptyReplaceInput.replace(emptyNeedle, dot);
    emptyReplacement.print();
    Text overlapInput = new Text("aaa");
    Text overlapNeedle = new Text("aa");
    Text x = new Text("X");
    Text overlap = overlapInput.replace(overlapNeedle, x);
    overlap.print();

    Text delimited = new Text("α::😀::::終::");
    Text delimiter = new Text("::");
    List<Text> pieces = delimited.split(delimiter);
    println(pieces.size());
    int index = 0;
    while (index < pieces.size()) {
        borrow Text piece = pieces.getRef(index);
        piece.print();
        index = index + 1;
    }
    Text scalarInput = new Text("é😀");
    List<Text> scalars = scalarInput.split(emptyNeedle);
    println(scalars.size());
    index = 0;
    while (index < scalars.size()) {
        borrow Text scalar = scalars.getRef(index);
        scalar.print();
        index = index + 1;
    }
    Text emptyInput = new Text("");
    List<Text> emptyParts = emptyInput.split(emptyNeedle);
    println(emptyParts.size());
    List<Text> emptyDelimitedParts = emptyInput.split(delimiter);
    println(emptyDelimitedParts.size());
    borrow Text emptyField = emptyDelimitedParts.getRef(0);
    emptyField.print();
    Text nulInput = new Text("a\0b");
    Text nulDelimiter = new Text("\0");
    List<Text> nulPieces = nulInput.split(nulDelimiter);
    println(nulPieces.size());
    borrow Text beforeNul = nulPieces.getRef(0);
    beforeNul.print();
    borrow Text afterNul = nulPieces.getRef(1);
    afterNul.print();

    Text formatted = new Text("");
    formatted.appendByte((byte)255);
    formatted.append("|");
    formatted.appendChar('🧪');
    formatted.append("|");
    formatted.appendInt(-3);
    formatted.append("|");
    formatted.appendLong(-1234567890123L);
    formatted.append("|");
    formatted.appendDouble(3.5);
    formatted.append("|");
    formatted.appendBoolean(true);
    formatted.append("|");
    Text tail = new Text("tail");
    formatted.appendText(tail);
    formatted.print();
}
"#);
    assert_eq!(output.stdout,
        "10\n6\né\n😀\n0\n3\n8\n-1\n0\ntrue\ntrue\ntrue\ntrue\nfalse\n1\né😀\n0\n3\n3\n0\nrange rejected\né\nsplit scalar rejected\nhé😀\naΩ😀a\n.é.😀.\n.\nXa\n5\nα\n😀\n\n終\n\n2\né\n😀\n0\n1\n\n2\na\nb\n255|🧪|-3|-1234567890123|3.5|true|tail\n".as_bytes());
}

#[test]
fn text_inline_and_heap_storage_transitions_preserve_ownership() {
    let output = execute(r#"
void main() {
    Text empty = new Text("");
    println(empty.isEmpty());
    empty.append("small");
    empty.print();

    Text exact = new Text("0123456789abcdef");
    Text inlineCopy = exact.clone();
    println(exact.byteLength());
    exact.append("g");
    exact.append("0123456789abcdef");
    Text heapCopy = exact.clone();
    exact.append("!");
    heapCopy.append("?");
    println(inlineCopy.equalsString("0123456789abcdef"));
    println(exact.equalsString("0123456789abcdefg0123456789abcdef!"));
    println(heapCopy.equalsString("0123456789abcdefg0123456789abcdef?"));
    inlineCopy.print();
    exact.print();
    heapCopy.print();

    Text smallInput = new Text("a-a-a");
    Text a = new Text("a");
    Text xy = new Text("xy");
    Text smallOwnedTake = smallInput.replace(a, xy);
    smallOwnedTake.print();

    Text largeInput = new Text("aaaaaaaaaaaaaaaaaaaa");
    Text largeOwnedTake = largeInput.replace(a, xy);
    println(largeOwnedTake.byteLength());
    println(largeOwnedTake.byteAt(39));

    Text mixed = new Text("é\0😀");
    println(mixed.byteLength());
    println(mixed.charCount());
    println(mixed.byteAt(2));
    mixed.print();
}
"#);
    assert_eq!(
        output.stdout,
        "true\nsmall\n16\ntrue\ntrue\ntrue\n0123456789abcdef\n0123456789abcdefg0123456789abcdef!\n0123456789abcdefg0123456789abcdef?\nxy-xy-xy\n40\n121\n7\n3\n0\né\0😀\n"
            .as_bytes()
    );
}

#[test]
fn regex_uses_utf8_pcre2_and_owned_capture_text_with_byte_offsets() {
    let output = execute(r#"
void main() {
    Text dotPattern = new Text(".");
    match (Regex.compile(dotPattern)) {
        Err(error) { println("dot pattern rejected"); drop(error); }
        Ok(dot) {
            Text emoji = new Text("😀");
            match (dot.matches(emoji)) {
                Err(error) { println("dot match errored"); drop(error); }
                Ok(value) { println(value); }
            }
            Text subject = new Text("😀y");
            match (dot.search(subject)) {
                Err(error) { println("dot search errored"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("dot not found"); }
                        Some(capture) {
                            println(capture.groupCount());
                            println(capture.start(0));
                            println(capture.end(0));
                            borrow Text matched = capture.group(0);
                            matched.print();
                        }
                    }
                }
            }
            drop(dot);
        }
    }

    Text capturesPattern = new Text("(é)(😀)-([0-9]+)");
    match (Regex.compile(capturesPattern)) {
        Err(error) { println("capture pattern rejected"); drop(error); }
        Ok(regex) {
            Text exact = new Text("é😀-42");
            match (regex.matches(exact)) {
                Err(error) { println("full match errored"); drop(error); }
                Ok(value) { println(value); }
            }
            Text prefixed = new Text("prefé😀-42");
            Option<RegexMatch> saved = new Option<RegexMatch>.None();
            {
                Text subject = prefixed;
                match (regex.search(subject)) {
                    Err(error) { println("capture search errored"); drop(error); }
                    Ok(found) {
                        match (found) {
                            None { println("capture not found"); }
                            Some(capture) {
                                println(capture.groupCount());
                                println(capture.start(0));
                                println(capture.end(0));
                                saved = new Option<RegexMatch>.Some(capture);
                            }
                        }
                    }
                }
            }
            match (saved) {
                None { println("capture was not retained"); }
                Some(capture) {
                    println(capture.hasGroup(2));
                    borrow Text emojiGroup = capture.group(2);
                    emojiGroup.print();
                    println(capture.start(2));
                    println(capture.end(2));
                    borrow Text numberGroup = capture.group(3);
                    numberGroup.print();
                    println(capture.start(3));
                    println(capture.end(3));
                }
            }
            Text absent = new Text("nothing to match");
            match (regex.search(absent)) {
                Err(error) { println("no-match search errored"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("not found"); }
                        Some(capture) { println("unexpected capture"); drop(capture); }
                    }
                }
            }
            Text optionalPattern = new Text("(a)?b");
            match (Regex.compile(optionalPattern)) {
                Err(error) { println("optional pattern rejected"); drop(error); }
                Ok(optionalRegex) {
                    Text justB = new Text("b");
                    match (optionalRegex.search(justB)) {
                        Err(error) { println("optional search errored"); drop(error); }
                        Ok(found) {
                            match (found) {
                                None { println("optional match missing"); }
                                Some(capture) {
                                    println(capture.groupCount());
                                    println(capture.hasGroup(0));
                                    println(capture.hasGroup(1));
                                    println(capture.start(0));
                                    println(capture.end(0));
                                }
                            }
                        }
                    }
                    drop(optionalRegex);
                }
            }

            drop(regex);
        }
    }

    Text emptyPattern = new Text("");
    match (Regex.compile(emptyPattern)) {
        Err(error) { println("empty pattern rejected"); drop(error); }
        Ok(emptyRegex) {
            Text emptySubject = new Text("");
            match (emptyRegex.matches(emptySubject)) {
                Err(error) { println("empty match errored"); drop(error); }
                Ok(value) { println(value); }
            }
            match (emptyRegex.search(emptySubject)) {
                Err(error) { println("empty search errored"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("empty match missing"); }
                        Some(capture) {
                            println(capture.groupCount());
                            println(capture.start(0));
                            println(capture.end(0));
                            borrow Text emptyGroup = capture.group(0);
                            println(emptyGroup.byteLength());
                        }
                    }
                }
            }
            drop(emptyRegex);
        }
    }
}
"#);
    assert_eq!(output.stdout,
        "true\n1\n0\n4\n😀\ntrue\n4\n4\n13\ntrue\n😀\n6\n10\n42\n11\n13\nnot found\n2\ntrue\nfalse\n0\n1\ntrue\n1\n0\n0\n0\n".as_bytes());
}

#[test]
fn regex_supports_modern_pcre2_syntax_full_matches_and_embedded_nul() {
    let output = execute(r#"
void main() {
    Text modernPattern = new Text("(?i)(?=(\\p{L}+)-)\\1-\\p{L}+");
    match (Regex.compile(modernPattern)) {
        Err(error) { println("modern PCRE2 pattern rejected"); drop(error); }
        Ok(regex) {
            Text exact = new Text("Äbc-äBC");
            match (regex.matches(exact)) {
                Err(error) { println("Unicode full match errored"); drop(error); }
                Ok(value) { println(value); }
            }
            Text partial = new Text("1Äbc-äBC");
            match (regex.matches(partial)) {
                Err(error) { println("Unicode partial match errored"); drop(error); }
                Ok(value) { println(value); }
            }
            Text searched = new Text("1Äbc-äBC2");
            match (regex.search(searched)) {
                Err(error) { println("Unicode search errored"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("Unicode match missing"); }
                        Some(capture) {
                            println(capture.groupCount());
                            println(capture.start(0));
                            println(capture.end(0));
                            borrow Text whole = capture.group(0);
                            whole.print();
                            println(capture.start(1));
                            println(capture.end(1));
                            borrow Text letters = capture.group(1);
                            letters.print();
                        }
                    }
                }
            }
            drop(regex);
        }
    }

    Text alternativePattern = new Text("a|ab");
    match (Regex.compile(alternativePattern)) {
        Err(error) { println("alternative pattern rejected"); drop(error); }
        Ok(regex) {
            Text subject = new Text("ab");
            match (regex.matches(subject)) {
                Err(error) { println("alternative full match errored"); drop(error); }
                Ok(value) { println(value); }
            }
            match (regex.search(subject)) {
                Err(error) { println("alternative search errored"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("alternative match missing"); }
                        Some(capture) {
                            borrow Text first = capture.group(0);
                            first.print();
                            println(capture.start(0));
                            println(capture.end(0));
                        }
                    }
                }
            }
            drop(regex);
        }
    }

    Text resetPattern = new Text("a\\Kb");
    match (Regex.compile(resetPattern)) {
        Err(error) { println("reset-start pattern rejected"); drop(error); }
        Ok(regex) {
            Text subject = new Text("ab");
            match (regex.matches(subject)) {
                Err(error) { println("reset-start full match errored"); drop(error); }
                Ok(value) { println(value); }
            }
            match (regex.search(subject)) {
                Err(error) { println("reset-start search errored"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("reset-start match missing"); }
                        Some(capture) {
                            println(capture.start(0));
                            println(capture.end(0));
                            borrow Text group = capture.group(0);
                            group.print();
                        }
                    }
                }
            }
            drop(regex);
        }
    }

    Text nulPattern = new Text("a\0b");
    match (Regex.compile(nulPattern)) {
        Err(error) { println("embedded NUL pattern rejected"); drop(error); }
        Ok(regex) {
            Text nulExact = new Text("a\0b");
            match (regex.matches(nulExact)) {
                Err(error) { println("embedded NUL full match errored"); drop(error); }
                Ok(value) { println(value); }
            }
            Text nulSubject = new Text("x\0a\0by");
            match (regex.search(nulSubject)) {
                Err(error) { println("embedded NUL search errored"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("embedded NUL match missing"); }
                        Some(capture) {
                            println(capture.start(0));
                            println(capture.end(0));
                            borrow Text matched = capture.group(0);
                            matched.print();
                        }
                    }
                }
            }
            drop(regex);
        }
    }

    Text invalid = new Text("[");
    match (Regex.compile(invalid)) {
        Err(error) { println("invalid pattern rejected"); drop(error); }
        Ok(regex) { println("invalid pattern accepted"); drop(regex); }
    }
}
"#);
    assert_eq!(
        output.stdout,
        "true\nfalse\n2\n1\n10\nÄbc-äBC\n1\n5\nÄbc\ntrue\na\n0\n1\ntrue\n1\n2\nb\ntrue\n2\n5\na\0b\ninvalid pattern rejected\n"
            .as_bytes()
    );
}

#[test]
fn regex_match_owns_captures_after_subject_and_regex_drop() {
    let output = execute(r#"
void main() {
    Option<RegexMatch> saved = new Option<RegexMatch>.None();
    {
        Text pattern = new Text("(é+)");
        match (Regex.compile(pattern)) {
            Err(error) { println("pattern rejected"); drop(error); }
            Ok(regex) {
                {
                    Text subject = new Text("xéé!");
                    match (regex.search(subject)) {
                        Err(error) { println("search errored"); drop(error); }
                        Ok(found) {
                            match (found) {
                                None { println("match missing"); }
                                Some(capture) { saved = new Option<RegexMatch>.Some(capture); }
                            }
                        }
                    }
                }
                drop(regex);
            }
        }
    }
    match (saved) {
        None { println("owned match was not retained"); }
        Some(capture) {
            println(capture.groupCount());
            println(capture.start(0));
            println(capture.end(0));
            borrow Text whole = capture.group(0);
            whole.print();
            borrow Text group = capture.group(1);
            group.print();
        }
    }
}
"#);
    assert_eq!(output.stdout, "2\n1\n5\néé\néé\n".as_bytes());
}

#[test]
fn regex_distinguishes_empty_and_unmatched_capture_groups() {
    let output = execute(r#"
void main() {
    Text pattern = new Text("(a*)(b)?");
    match (Regex.compile(pattern)) {
        Err(error) { println("pattern rejected"); drop(error); }
        Ok(regex) {
            Text subject = new Text("");
            match (regex.search(subject)) {
                Err(error) { println("search errored"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("empty match missing"); }
                        Some(capture) {
                            println(capture.groupCount());
                            println(capture.hasGroup(0));
                            println(capture.hasGroup(1));
                            println(capture.hasGroup(2));
                            borrow Text empty = capture.group(1);
                            println(empty.byteLength());
                            println(capture.start(1));
                            println(capture.end(1));
                            println(capture.start(0));
                            println(capture.end(0));
                        }
                    }
                }
            }
            drop(regex);
        }
    }
}
"#);
    assert_eq!(output.stdout, "3\ntrue\ntrue\nfalse\n0\n0\n0\n0\n0\n".as_bytes());
}

#[test]
fn regex_reports_resource_exhaustion_instead_of_a_miss() {
    let output = execute(r#"
void main() {
    Text pattern = new Text("(*LIMIT_MATCH=100)(*NO_AUTO_POSSESS)(*NO_START_OPT)(a+)+$");
    match (Regex.compile(pattern)) {
        Err(error) { println("resource pattern rejected"); drop(error); }
        Ok(regex) {
            Text subject = new Text("aaaaaaaaaaaaaaaaaaaa!");
            match (regex.matches(subject)) {
                Err(error) { println("matches reported resource exhaustion"); drop(error); }
                Ok(value) { println("matches hid resource exhaustion"); println(value); }
            }
            match (regex.search(subject)) {
                Err(error) { println("search reported resource exhaustion"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("search hid resource exhaustion"); }
                        Some(capture) { println("search hid resource exhaustion"); drop(capture); }
                    }
                }
            }
            drop(regex);
        }
    }
}
"#);
    assert_eq!(
        output.stdout,
        "matches reported resource exhaustion\nsearch reported resource exhaustion\n".as_bytes()
    );
}

#[test]
fn invalid_utf8_sequences_are_rejected_at_the_os_text_boundary() {
    let invalid_sequences: [&[u8]; 4] = [
        &[0xc0, 0xaf],
        &[0xed, 0xa0, 0x80],
        &[0xf0, 0x9f],
        &[0xf4, 0x90, 0x80, 0x80],
    ];
    let mut paths = Vec::new();
    for bytes in invalid_sequences {
        let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("nacre-text-regex-{}-{serial}", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        paths.push(path.into_os_string());
    }
    let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
    let valid_path = std::env::temp_dir().join(format!("nacre-text-regex-{}-{serial}", std::process::id()));
    std::fs::write(&valid_path, b"\0\xc3\xa9\xf0\x9f\x99\x82").unwrap();
    paths.push(valid_path.into_os_string());
    let program = compile_native(r#"
void main() {
    Text nulNeedle = new Text("\0");
    int index = 0;
    while (index < Args.count()) {
        match (Args.get(index)) {
            Err(error) { println("argument rejected"); drop(error); }
            Ok(path) {
                match (Files.readText(path)) {
                    Err(error) { println("invalid UTF-8 rejected"); drop(error); }
                    Ok(text) {
                        println("valid UTF-8 accepted");
                        println(text.byteLength());
                        println(text.charCount());
                        println(text.indexOf(nulNeedle));
                        drop(text);
                    }
                }
            }
        }
        index = index + 1;
    }
}
"#);
    let output = run(&program, &paths);
    for path in paths {
        std::fs::remove_file(path).unwrap();
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut expected = b"invalid UTF-8 rejected\n".repeat(invalid_sequences.len());
    expected.extend_from_slice(b"valid UTF-8 accepted\n7\n3\n0\n");
    assert_eq!(output.stdout, expected);
    assert_clean_memory(&output);
}

#[test]
fn text_scalar_index_is_checked_and_opaque_regex_has_no_constructor() {
    let source = "void main() { Text value = new Text(\"é\"); println(value.charAt(1)); }";
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("text_regex.ncr", source)));
    let program = native::compile_c(&c).unwrap();
    let output = run(&program, &[]);
    assert_eq!(output.status.code(), Some(70), "{}", String::from_utf8_lossy(&output.stderr));

    assert!(compile_to_c("void main() { Regex regex = new Regex(); }").is_err());
    assert!(compile_to_c("void main() { RegexMatch result = new RegexMatch(); }").is_err());
}

#[test]
fn static_native_utility_type_cannot_be_used_as_value() {
    assert!(compile_to_c("void accept(Arrays values) {} void main() {}").is_err());
}

#[test]
fn utf8_regex_example_runs_as_a_native_program() {
    let output = execute(include_str!("../examples/text/utf8_regex.ncr"));
    assert_eq!(output.stdout, "12\n6\nalpha\n🚀\nbeta\n2\n6\n10\n🚀\nbytes=15 scalars=12\n".as_bytes());
}

#[test]
fn regex_rejects_byte_wise_patterns_that_can_split_utf8_captures() {
    let output = execute(r#"
void main() {
    Text pattern = new Text("\\C");
    match (Regex.compile(pattern)) {
        Err(error) { println(true); }
        Ok(regex) {
            Text subject = new Text("é");
            match (regex.search(subject)) {
                Err(error) { println(false); }
                Ok(found) { println(false); }
            }
        }
    }
}
"#);
    assert_eq!(output.stdout, b"true\n");
    assert_clean_memory(&output);
}
