use nacre::{compile_to_c, native};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn compile(source: &str) -> native::NativeProgram {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("uri_dns.ncr", source)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
}

fn execute(program: &native::NativeProgram) -> Output {
    let mut child = Command::new(program.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("native program starts");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().expect("native child status") {
            let output = child.wait_with_output().expect("native program output");
            assert_eq!(output.status, status);
            assert!(status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            assert_memory_balanced(&output);
            return output;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native URI/DNS test exceeded its finite deadline");
        }
        thread::sleep(Duration::from_millis(10));
    }
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

#[test]
fn uri_resolve_matches_rfc3986_normal_and_abnormal_reference_vectors() {
    const VECTORS: &[(&str, &str)] = &[
        ("g:h", "g:h"),
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g"),
        ("?y", "http://a/b/c/d;p?y"),
        ("g?y", "http://a/b/c/g?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("g#s", "http://a/b/c/g#s"),
        ("g?y#s", "http://a/b/c/g?y#s"),
        (";x", "http://a/b/c/;x"),
        ("g;x", "http://a/b/c/g;x"),
        ("g;x?y#s", "http://a/b/c/g;x?y#s"),
        ("", "http://a/b/c/d;p?q"),
        (".", "http://a/b/c/"),
        ("./", "http://a/b/c/"),
        ("..", "http://a/b/"),
        ("../", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../..", "http://a/"),
        ("../../", "http://a/"),
        ("../../g", "http://a/g"),
        ("/./g", "http://a/g"),
        ("/../g", "http://a/g"),
        ("g.", "http://a/b/c/g."),
        (".g", "http://a/b/c/.g"),
        ("g..", "http://a/b/c/g.."),
        ("..g", "http://a/b/c/..g"),
        ("./../g", "http://a/b/g"),
        ("./g/.", "http://a/b/c/g/"),
        ("g/./h", "http://a/b/c/g/h"),
        ("g/../h", "http://a/b/c/h"),
        ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
        ("g;x=1/../y", "http://a/b/c/y"),
        ("g?y/./x", "http://a/b/c/g?y/./x"),
        ("g#s/../x", "http://a/b/c/g#s/../x"),
    ];
    let checks = VECTORS.iter()
        .map(|(reference, expected)| format!("check(base, {:?}, {:?});", reference, expected))
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!(r#"
void check(borrow Uri base, borrow Text reference, String expected) {{
    match (Uri.parse(reference)) {{
        Err(error) {{ println(false); }}
        Ok(parsed) {{
            match (base.resolve(parsed)) {{
                Err(error) {{ println(false); }}
                Ok(resolved) {{
                    Text actual = resolved.toText();
                    println(actual.equalsString(expected));
                }}
            }}
        }}
    }}
}}
void main() {{
    match (Uri.parse("http://a/b/c/d;p?q")) {{
        Err(error) {{ println("base parse failed"); }}
        Ok(base) {{
            {checks}
        }}
    }}
}}
"#);
    let output = execute(&compile(&source));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), VECTORS.len(), "{stdout}");
    assert!(lines.iter().all(|line| *line == "true"), "{stdout}");
}

#[test]
fn uri_components_iri_normalization_and_owned_storage_are_exact() {
    let source = r#"
void main() {
    Text input = new Text("http://source.test/a%2Fb");
    match (Uri.parse(input)) {
        Err(error) { println(false); }
        Ok(original) {
            Uri cloned = original.clone();
            println(cloned.equals(original));
            input.clear();
            drop(original);
            println(cloned.toText().equalsString("http://source.test/a%2Fb"));
        }
    }

    match (Uri.fromIri("https://example.test/café/雪")) {
        Err(error) { println(false); }
        Ok(uri) { println(uri.toText().equalsString("https://example.test/caf%C3%A9/%E9%9B%AA")); }
    }
    Text encoded = Uri.encodeComponent("a/b +é\0");
    println(encoded.equalsString("a%2Fb%20%2B%C3%A9%00"));
    match (Uri.decodeComponent("%F0%9F%8C%8D")) {
        Err(error) { println(false); }
        Ok(decoded) { println(decoded.equalsString("🌍")); }
    }
    match (Uri.decodeComponent("%00A")) {
        Err(error) { println(false); }
        Ok(decoded) {
            println(decoded.byteLength() == 2);
            println(decoded.byteAt(0) == 0);
            println(decoded.byteAt(1) == 65);
        }
    }
    match (Uri.decodeComponent("%FF")) {
        Err(error) { println(true); }
        Ok(decoded) { println(false); }
    }
    match (Uri.decodeComponent("%Q0")) {
        Err(error) { println(true); }
        Ok(decoded) { println(false); }
    }
    match (Uri.percentDecode("%FF%00+")) {
        Err(error) { println(false); }
        Ok(bytes) {
            println(bytes.length() == 3L);
            println(bytes.get(0L) == (byte)255);
            println(bytes.get(1L) == (byte)0);
            println(bytes.get(2L) == (byte)'+');
        }
    }

    match (Uri.parse("http://user:pass@[2001:db8::1]:65535/a%2Fb?q=#frag")) {
        Err(error) { println(false); }
        Ok(uri) {
            match (uri.userInfo()) { None { println(false); } Some(value) { println(value.equalsString("user:pass")); } }
            match (uri.host()) { None { println(false); } Some(value) { println(value.equalsString("2001:db8::1")); } }
            println(uri.path().equalsString("/a%2Fb"));
            match (uri.port()) {
                Err(error) { println(false); }
                Ok(port) { match (port) { None { println(false); } Some(value) { println(value == 65535); } } }
            }
            match (uri.query()) { None { println(false); } Some(value) { println(value.equalsString("q=")); } }
            match (uri.fragment()) { None { println(false); } Some(value) { println(value.equalsString("frag")); } }
        }
    }
    match (Uri.parse("http://example.test/?#")) {
        Err(error) { println(false); }
        Ok(uri) {
            match (uri.query()) { None { println(false); } Some(value) { println(value.isEmpty()); } }
            match (uri.fragment()) { None { println(false); } Some(value) { println(value.isEmpty()); } }
        }
    }
    match (Uri.parse("http://example.test/")) {
        Err(error) { println(false); }
        Ok(uri) {
            match (uri.query()) { None { println(true); } Some(value) { println(false); } }
            match (uri.fragment()) { None { println(true); } Some(value) { println(false); } }
            match (uri.port()) {
                Err(error) { println(false); }
                Ok(port) { match (port) { None { println(true); } Some(value) { println(false); } } }
            }
        }

    }
    match (Uri.parse("mailto:fred@example.test")) {
        Err(error) { println(false); }
        Ok(uri) {
            println(uri.isAbsolute());
            match (uri.scheme()) { None { println(false); } Some(value) { println(value.equalsString("mailto")); } }
            println(uri.path().equalsString("fred@example.test"));
            match (uri.host()) { None { println(true); } Some(value) { println(false); } }
        }
    }

    match (Uri.parse("HTTP://Example.COM/a/../b%7e")) {
        Err(error) { println(false); }
        Ok(raw) {
            println(raw.toText().equalsString("HTTP://Example.COM/a/../b%7e"));
            match (raw.normalize()) {
                Err(error) { println(false); }
                Ok(normalized) {
                    println(normalized.toText().equalsString("http://example.com/b~"));
                    println(!raw.equals(normalized));
                }
            }
        }
    }

    match (Uri.parse("http://a/b/c/d;p?q")) {
        Err(error) { println(false); }
        Ok(base) {
            match (Uri.parse("http://a/b/c/g")) {
                Err(error) { println(false); }
                Ok(target) {
                    match (base.relativize(target)) {
                        Err(error) { println(false); }
                        Ok(relative) {
                            println(relative.toText().equalsString("g"));
                            match (base.resolve(relative)) {
                                Err(error) { println(false); }
                                Ok(roundTrip) { println(roundTrip.equals(target)); }
                            }
                        }
                    }
                }
            }
        }
    }
    match (Uri.parse("http://example.test:/")) {
        Err(error) { println(false); }
        Ok(uri) {
            match (uri.port()) {
                Err(error) { println(false); }
                Ok(port) { match (port) { None { println(true); } Some(value) { println(false); } } }
            }
        }
    }
    match (Uri.parse("http://example.test:65536/")) {
        Err(error) { println(true); }
        Ok(uri) {
            match (uri.port()) { Err(error) { println(true); } Ok(port) { println(false); } }
        }
    }
    match (Uri.parse("http://example.test/a b")) {
        Err(error) { println(true); }
        Ok(uri) { println(false); }
    }
    match (Uri.parse("http://example.test/a\0b")) {
        Err(error) { println(true); }
        Ok(uri) { println(false); }
    }
    match (Uri.parse("http://example.test/café")) {
        Err(error) { println(true); }
        Ok(uri) { println(false); }
    }
}
"#;
    let output = execute(&compile(source));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.lines().all(|line| line == "true"), "{stdout}");
    assert_eq!(stdout.lines().count(), 39, "{stdout}");
}

#[test]
fn dns_idna_and_local_resolution_use_owned_results_without_external_hosts() {
    let source = r#"
void main() {
    match (Dns.idna("Bücher.Example.")) {
        Err(error) { println(false); }
        Ok(ascii) { println(ascii.equalsString("xn--bcher-kva.example.")); }
    }
    match (Dns.idna("example%2Etest")) {
        Err(error) { println(false); }
        Ok(ascii) { println(ascii.equalsString("example.test")); }
    }
    match (Dns.idna("127.0.0.1")) {
        Err(error) { println(false); }
        Ok(ascii) { println(ascii.equalsString("127.0.0.1")); }
    }
    match (Dns.idna("2001:0DB8:0:0::1")) {
        Err(error) { println(false); }
        Ok(ascii) { println(ascii.equalsString("2001:db8::1")); }
    }
    match (Dns.idna("fe80::1%25lo")) {
        Err(error) { println(false); }
        Ok(ascii) { println(ascii.equalsString("fe80::1%lo")); }
    }
    match (Dns.idna("bad_host")) { Err(error) { println(true); } Ok(ascii) { println(false); } }
    match (Dns.idna("a..example")) { Err(error) { println(true); } Ok(ascii) { println(false); } }
    match (Dns.idna("example%00.test")) { Err(error) { println(true); } Ok(ascii) { println(false); } }
    match (Dns.resolve("bad_host", 0)) {
        Err(error) { println(true); }
        Ok(addresses) { println(false); }
    }

    match (Dns.resolve("localhost", 0)) {
        Err(error) { println(false); }
        Ok(addresses) {
            println(addresses.size() > 0);
            boolean numeric_seen = false;
            boolean reverse_seen = false;
            int index = 0;
            while (index < addresses.size()) {
                {
                    borrow SocketAddress address = addresses.getRef(index);
                    Text numeric = address.toText();
                    if (numeric.byteLength() > 0) { numeric_seen = true; }
                    match (Dns.reverse(address)) {
                        Err(error) { }
                        Ok(name) { if (name.byteLength() > 0) { reverse_seen = true; } }
                    }
                }
                index = index + 1;
            }
            println(numeric_seen);
            println(reverse_seen);
        }
    }
    match (Dns.localHostName()) {
        Err(error) { println(false); }
        Ok(name) { println(name.byteLength() > 0); }
    }
}
"#;
    let output = execute(&compile(source));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.lines().all(|line| line == "true"), "{stdout}");
    assert_eq!(stdout.lines().count(), 13, "{stdout}");
}
