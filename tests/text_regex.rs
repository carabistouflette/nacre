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
fn regex_uses_utf8_ere_and_owned_capture_text_with_byte_offsets() {
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
fn regex_reports_invalid_patterns_and_nul_boundaries_as_errors() {
    let output = execute(r#"
void main() {
    Text invalid = new Text("[");
    match (Regex.compile(invalid)) {
        Err(error) { println("invalid pattern rejected"); drop(error); }
        Ok(regex) { println("invalid pattern accepted"); drop(regex); }
    }
    Text javaLookahead = new Text("(?=a)");
    match (Regex.compile(javaLookahead)) {
        Err(error) { println("non-POSIX syntax rejected"); drop(error); }
        Ok(regex) { println("non-POSIX syntax accepted"); drop(regex); }
    }
    Text nulPattern = new Text("a\0b");
    match (Regex.compile(nulPattern)) {
        Err(error) { println("NUL pattern rejected"); drop(error); }
        Ok(regex) { println("NUL pattern accepted"); drop(regex); }
    }
    Text pattern = new Text("a");
    match (Regex.compile(pattern)) {
        Err(error) { println("pattern unavailable"); drop(error); }
        Ok(regex) {
            Text subject = new Text("a\0b");
            match (regex.matches(subject)) {
                Err(error) { println("NUL subject rejected by matches"); drop(error); }
                Ok(value) { println("NUL subject accepted by matches"); println(value); }
            }
            match (regex.search(subject)) {
                Err(error) { println("NUL subject rejected by search"); drop(error); }
                Ok(found) {
                    match (found) {
                        None { println("NUL subject had no match"); }
                        Some(capture) { println("NUL subject matched"); drop(capture); }
                    }
                }
            }
            drop(regex);
        }
    }
}
"#);
    assert_eq!(output.stdout,
        "invalid pattern rejected\nnon-POSIX syntax rejected\nNUL pattern rejected\nNUL subject rejected by matches\nNUL subject rejected by search\n".as_bytes());
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
fn utf8_regex_example_runs_as_a_native_program() {
    let output = execute(include_str!("../examples/text/utf8_regex.ncr"));
    assert_eq!(output.stdout, "12\n6\nalpha\n🚀\nbeta\n2\n6\n10\n🚀\nbytes=15 scalars=12\n".as_bytes());
}
