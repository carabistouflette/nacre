use nacre::{compile_to_c, native};
mod support;
use std::ffi::OsString;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);

fn compile_native(source: &str) -> native::NativeProgram {
    support::compile(
        source,
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
fn text_join_presizes_and_preserves_parts_and_delimiters() {
    let output = execute(r#"
void main() {
    Text delimiter = new Text("::");
    List<Text> empty = new List<Text>();
    Text emptyJoined = Text.join(empty, delimiter);
    println(emptyJoined.byteLength());
    drop(emptyJoined);
    drop(empty);

    List<Text> single = new List<Text>();
    single.add(new Text("only"));
    Text singleJoined = Text.join(single, delimiter);
    singleJoined.print();
    drop(singleJoined);
    drop(single);

    List<Text> parts = new List<Text>();
    parts.add(new Text("A\0"));
    parts.add(new Text(""));
    parts.add(new Text("é"));
    Text joined = Text.join(parts, delimiter);
    println(joined.equalsString("A\0::::é"));
    joined.print();
    drop(joined);
    drop(parts);

    List<Text> largeParts = new List<Text>();
    largeParts.add(new Text("12345678901234567890"));
    largeParts.add(new Text("abcdefghijklmnopqrstuvwxyz"));
    Text large = Text.join(largeParts, delimiter);
    println(large.byteLength());
    println(large.equalsString("12345678901234567890::abcdefghijklmnopqrstuvwxyz"));
    drop(large);
    drop(largeParts);
}
"#);
    assert_eq!(output.stdout, "0\nonly\ntrue\nA\0::::é\n48\ntrue\n".as_bytes());
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
fn text_repeated_growth_and_clear_preserve_utf8_nul_and_independent_owners() {
    let output = execute(r#"
boolean repeated(borrow Text value, borrow Text token, int count) {
    if (value.byteLength() != token.byteLength() * count || value.charCount() != token.charCount() * count) { return false; }
    int index = 0;
    while (index < value.byteLength()) {
        if (value.byteAt(index) != token.byteAt(index % token.byteLength())) { return false; }
        index = index + 1;
    }
    return true;
}
void main() {
    Text token = new Text("é\0😀");
    Text value = new Text("");
    value.append("");
    int index = 0;
    while (index < 5000) { value.appendText(token); index = index + 1; }
    println(repeated(value, token, 5000));
    Text copy = value.clone();
    value.clear();
    println(value.isEmpty());
    index = 0;
    while (index < 3000) { value.append("é\0😀"); index = index + 1; }
    println(repeated(value, token, 3000));
    while (index < 8000) { value.appendText(token); index = index + 1; }
    println(repeated(value, token, 8000));
    println(repeated(copy, token, 5000));
    println(token.equalsString("é\0😀"));
    Text input = new Text("aaaaaaaaaaaaaaaaaaaa");
    Text needle = new Text("a");
    Text replacement = new Text("é\0😀");
    Text taken = input.replace(needle, replacement);
    println(repeated(taken, token, 20));
    taken.appendText(copy);
    println(repeated(taken, token, 5020));
    println(repeated(copy, token, 5000));
    value.clear();
    value.appendChar('終');
    value.appendChar('\0');
    value.appendChar('🙂');
    println(value.equalsString("終\0🙂"));
}
"#);
    assert_eq!(output.stdout, b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn text_integer_appends_preserve_signed_decimal_boundaries_and_storage() {
    use std::fmt::Write;

    let mut values = vec![0, 1, -1, i32::MIN, i32::MIN + 1, i32::MAX - 1, i32::MAX];
    for power in [10, 100, 1_000, 10_000, 100_000, 1_000_000, 10_000_000, 100_000_000, 1_000_000_000] {
        for value in [power - 1, power, power + 1] { values.extend([value, -value]); }
    }
    values.sort_unstable();
    values.dedup();
    let mut source = String::from("void main() { List<int> values = new List<int>();\n");
    for value in &values {
        if *value == i32::MIN { source.push_str("values.add((-2147483647 - 1));\n"); }
        else { writeln!(source, "values.add({value});").unwrap(); }
    }
    source.push_str(r#"
    Text accumulated = new Text("é\0😀|");
    for (int number : values) {
        Text empty = new Text("");
        empty.appendInt(number);
        empty.print();
        Text small = new Text("é\0😀|");
        small.appendInt(number);
        small.print();
        Text spill = new Text("é\0😀|xxxxxxxxxxxxxxx");
        spill.appendInt(number);
        spill.print();
        Text heap = new Text("é\0😀|xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
        heap.appendInt(number);
        heap.print();
        accumulated.appendInt(number);
        accumulated.append("|");
    }
    accumulated.print();
    println(accumulated.byteLength());
    println(accumulated.charCount());
    Text copy = accumulated.clone();
    accumulated.clear();
    int pass = 0;
    while (pass < 2) {
        int index = values.size();
        while (index > 0) {
            index = index - 1;
            accumulated.appendInt(values.get(index));
            accumulated.append("|");
        }
        pass = pass + 1;
    }
    accumulated.print();
    copy.appendInt((-2147483647 - 1));
    copy.print();
    println(accumulated.byteLength());
    println(accumulated.charCount());
}
"#);
    let mut expected = String::new();
    for value in &values {
        for prefix in ["", "é\0😀|", "é\0😀|xxxxxxxxxxxxxxx", "é\0😀|xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"] {
            writeln!(expected, "{prefix}{value}").unwrap();
        }
    }
    let mut joined = String::from("é\0😀|");
    for value in &values { write!(joined, "{value}|").unwrap(); }
    let mut refilled = String::new();
    for _ in 0..2 {
        for value in values.iter().rev() { write!(refilled, "{value}|").unwrap(); }
    }
    writeln!(expected, "{joined}\n{}\n{}", joined.len(), joined.chars().count()).unwrap();
    writeln!(expected, "{refilled}\n{joined}{}\n{}\n{}", i32::MIN, refilled.len(), refilled.chars().count()).unwrap();
    assert_eq!(execute(&source).stdout, expected.as_bytes());
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
    let program = support::compile(source, &[]).unwrap();
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

#[test]
fn text_byte_slices_own_storage_across_inline_and_heap_boundaries() {
    let output = execute(r#"
void checkOwnership(int length) {
    Text source = new Text("");
    int index = 0;
    while (index < 130) { source.append("x"); index = index + 1; }
    match (source.sliceBytes(0, length)) {
        Err(error) { println("unexpected slice error"); drop(error); drop(source); }
        Ok(part) {
            source.clear();
            index = 0;
            while (index < 1024) { source.append("y"); index = index + 1; }
            drop(source);
            println(part.byteLength() == length);
            boolean exact = true;
            index = 0;
            while (index < length) {
                if (part.byteAt(index) != 120) { exact = false; }
                index = index + 1;
            }
            println(exact);
            part.append("\0é🧭");
            Text expected = new Text("");
            index = 0;
            while (index < length) { expected.append("x"); index = index + 1; }
            expected.append("\0é🧭");
            println(part.equals(expected));
            drop(expected);
            drop(part);
        }
    }
}
void main() {
    checkOwnership(0);
    checkOwnership(23);
    checkOwnership(24);
    checkOwnership(129);
    Text source = new Text("A\0é€🧭Z");
    match (source.sliceBytes(3, 4)) {
        Ok(part) { println("split scalar accepted"); drop(part); }
        Err(error) {
            Text saved = error.clone();
            drop(error);
            println(saved.equalsString("Text.sliceBytes: invalid byte range"));
            drop(saved);
        }
    }
    match (source.sliceBytes(2, 11)) {
        Err(error) { println("valid Unicode slice rejected"); drop(error); drop(source); }
        Ok(part) {
            drop(source);
            println(part.equalsString("é€🧭"));
            drop(part);
        }
    }
}
"#);
    assert_eq!(output.stdout, b"true\n".repeat(14));
}

#[test]
fn text_scalar_slices_trim_and_split_preserve_independent_storage_and_unicode_boundaries() {
    let output = execute(r#"
void main() {
    Text source = new Text("  \t\n\0é€🧭Z\0  \n");
    Text trimmed = source.trim();
    println(trimmed.equalsString("\0é€🧭Z\0"));

    match (trimmed.slice(1, 4)) {
        Err(error) { println(false); drop(error); }
        Ok(slice) {
            println(slice.equalsString("é€🧭"));
            source.clear();
            source.append("new-source-content-long-enough-for-heap-growth-0123456789");
            println(slice.equalsString("é€🧭"));
            slice.append("!");
            println(slice.equalsString("é€🧭!"));
            drop(slice);
        }
    }
    drop(source);

    Text sentence = new Text("alpha::\0::beta::gamma");
    Text delim = new Text("::");
    List<Text> parts = sentence.split(delim);
    sentence.clear();
    drop(sentence);
    println(parts.size() == 4);
    {
        borrow Text p0 = parts.getRef(0);
        println(p0.equalsString("alpha"));
        borrow Text p1 = parts.getRef(1);
        println(p1.equalsString("\0"));
        borrow Text p2 = parts.getRef(2);
        println(p2.equalsString("beta"));
        borrow Text p3 = parts.getRef(3);
        println(p3.equalsString("gamma"));
    }
    drop(parts);
    drop(delim);
    drop(trimmed);
}
"#);
    assert_eq!(output.stdout, b"true\n".repeat(9));
}

#[test]
fn text_reserve_preserves_content_and_increases_capacity() {
    let output = execute(r#"
void main() {
    Text text = new Text("hello-é");
    println(text.capacity() >= 23);
    println(text.byteLength() == 8);
    text.reserve(100);
    println(text.capacity() >= 100);
    println(text.equalsString("hello-é"));
    text.append("-world");
    println(text.equalsString("hello-é-world"));
    println(text.capacity() >= 100);
    drop(text);
}
"#);
    assert_eq!(output.stdout, b"true\n".repeat(6));
}
