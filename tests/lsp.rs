//! End-to-end checks for the Nacre stdio LSP protocol.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nacre-lsp-é {}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    published: HashMap<String, Value>,
}

impl Client {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_nacre"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            published: HashMap::new(),
        }
    }

    fn send(&mut self, message: Value) {
        let body = serde_json::to_vec(&message).unwrap();
        let input = self.input.as_mut().unwrap();
        write!(input, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        input.write_all(&body).unwrap();
        input.flush().unwrap();
    }

    fn send_malformed_json(&mut self) {
        let body = b"{\"jsonrpc\": ";
        let input = self.input.as_mut().unwrap();
        write!(input, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        input.write_all(body).unwrap();
        input.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            assert_ne!(self.output.read_line(&mut line).unwrap(), 0, "LSP process closed stdout");
            if line == "\r\n" || line == "\n" { break; }
            if let Some((name, value)) = line.trim_end().split_once(':')
                && name.eq_ignore_ascii_case("Content-Length") {
                content_length = Some(value.trim().parse::<usize>().unwrap());
            }
        }
        let mut body = vec![0; content_length.expect("Content-Length header")];
        self.output.read_exact(&mut body).unwrap();
        let message: Value = serde_json::from_slice(&body).unwrap();
        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics") {
            let params = &message["params"];
            if let Some(uri) = params["uri"].as_str() {
                self.published.insert(uri.to_owned(), params.clone());
            }
        }
        message
    }

    fn response(&mut self, id: i64) -> Value {
        loop {
            let message = self.read();
            if message.get("id").and_then(Value::as_i64) == Some(id) { return message; }
        }
    }

    fn diagnostics(&mut self, uri: &str) -> Value {
        // Fence the notification with a request: all preceding publications are
        // cached before its response, independent of HashMap publication order.
        self.send(json!({ "jsonrpc": "2.0", "id": 1000, "method": "test/barrier" }));
        self.response(1000);
        self.published.get(uri).expect("server published diagnostics for this document").clone()
    }

    fn finish(mut self) {
        drop(self.input.take());
        let status = self.child.wait().unwrap();
        assert!(status.success(), "LSP process exited with {status}");
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn uri(path: &Path) -> String {
    let mut result = String::from("file://");
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let path = path.to_str().unwrap().as_bytes();
    if !path.starts_with(b"/") { result.push('/'); }
    for &byte in path {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

#[test]
fn stdio_lsp_reports_utf16_diagnostics_edits_and_unsaved_project_overlays() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    fs::create_dir(&source_dir).unwrap();
    fs::write(temp.0.join("nacre.json"), r#"{"sources":["src"]}"#).unwrap();

    let main_path = source_dir.join("Main file-é.ncr");
    let use_path = source_dir.join("Use.ncr");
    let item_path = source_dir.join("Item.ncr");
    fs::write(&main_path, "void main() {}\n").unwrap();
    fs::write(&use_path, "void resolveItem() { Item item = new Item(); }\n").unwrap();
    fs::write(&item_path, "\n").unwrap();
    let main_uri = uri(&main_path);
    let use_uri = uri(&use_path);
    let item_uri = uri(&item_path);

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    let initialized = client.response(1);
    assert_eq!(initialized["result"]["capabilities"]["textDocumentSync"]["change"].as_i64(), Some(1));
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({ "jsonrpc": "2.0", "id": 3, "method": "exit" }));
    assert_eq!(client.response(3)["error"]["code"], -32600);

    let invalid = "void main() { /* 😀 */ int value = true; }\n";
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri.clone(), "languageId": "nacre", "version": 1, "text": invalid } }
    }));
    let first = client.diagnostics(&uri(&main_path));
    let diagnostic = &first["diagnostics"][0];
    assert_eq!(diagnostic["severity"].as_u64(), Some(1));
    let column = invalid.lines().next().unwrap().find("true").unwrap();
    let utf16_column = invalid.lines().next().unwrap()[..column].encode_utf16().count();
    assert_eq!(diagnostic["range"]["start"]["line"].as_u64(), Some(0));
    assert_eq!(diagnostic["range"]["start"]["character"].as_u64(), Some(utf16_column as u64));

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "not/a/server/method", "params": {} }));
    assert_eq!(client.response(2)["error"]["code"], -32601);
    client.send_malformed_json();
    assert_eq!(client.read()["error"]["code"], -32700);

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri(&main_path), "version": 2 },
            "contentChanges": [{ "text": "void main() {}\n" }]
        }
    }));
    let missing_item = client.diagnostics(&use_uri);
    assert!(!missing_item["diagnostics"].as_array().unwrap().is_empty());
    assert!(client.diagnostics(&uri(&main_path))["diagnostics"].as_array().unwrap().is_empty());

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": use_uri.clone(), "languageId": "nacre", "version": 1,
            "text": "void resolveItem() { Item item = new Item(); }\n" } }
    }));
    assert!(!client.diagnostics(&uri(&use_path))["diagnostics"].as_array().unwrap().is_empty());

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": item_uri.clone(), "languageId": "nacre", "version": 4,
            "text": "class Item {}\n" } }
    }));
    assert!(client.diagnostics(&uri(&use_path))["diagnostics"].as_array().unwrap().is_empty());
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didSave",
        "params": { "textDocument": { "uri": uri(&main_path) }, "text": "void main() {}\n" }
    }));
    assert!(client.diagnostics(&uri(&main_path))["diagnostics"].as_array().unwrap().is_empty());

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didClose",
        "params": { "textDocument": { "uri": uri(&use_path) } }
    }));
    assert!(client.diagnostics(&uri(&use_path))["diagnostics"].as_array().unwrap().is_empty());

    client.send(json!({ "jsonrpc": "2.0", "id": 3, "method": "shutdown" }));
    assert!(client.response(3)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn lsp_position_queries_resolve_bare_cr_line_endings() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    fs::create_dir(&source_dir).unwrap();
    fs::write(
        temp.0.join("Nacre.toml"),
        "[package]\nname = 'cr_line_endings'\nversion = '0.1.0'\nsources = ['src']\n",
    ).unwrap();
    let source = "class Item {}\rvoid main() { Item item = new Item(); }\r";
    let path = source_dir.join("Main.ncr");
    fs::write(&path, source).unwrap();
    let document_uri = uri(&path);
    let character = source.split('\r').nth(1).unwrap().find("Item").unwrap();

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": document_uri.clone(), "languageId": "nacre", "version": 1, "text": source } }
    }));
    client.send(json!({
        "jsonrpc": "2.0", "id": 2, "method": "textDocument/definition",
        "params": { "textDocument": { "uri": document_uri }, "position": { "line": 1, "character": character } }
    }));
    let definition = client.response(2);
    assert_eq!(definition["result"]["uri"], uri(&path));
    assert_eq!(definition["result"]["range"]["start"]["line"], 0);

    client.send(json!({ "jsonrpc": "2.0", "id": 3, "method": "shutdown" }));
    assert!(client.response(3)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn oversized_open_document_reports_a_source_diagnostic() {
    let temp = TempDir::new();
    let path = temp.0.join("Main.ncr");
    let document_uri = uri(&path);
    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));

    let prefix = "void main() {}\n";
    let text = format!(
        "{prefix}{}",
        " ".repeat(nacre::project::MAX_SOURCE_BYTES + 1 - prefix.len())
    );
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": document_uri, "languageId": "nacre", "version": 9, "text": text } }
    }));
    let diagnostics = client.diagnostics(&uri(&path));
    let items = diagnostics["diagnostics"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{diagnostics}");
    assert_eq!(items[0]["severity"].as_u64(), Some(1));
    assert!(items[0]["message"].as_str().unwrap().contains("source limit"), "{diagnostics}");
    assert_eq!(diagnostics["version"].as_i64(), Some(9));

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn unsaved_cross_module_privacy_errors_clear_when_imported_overlay_changes() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    fs::create_dir(&source_dir).unwrap();
    fs::write(temp.0.join("nacre.json"), r#"{"sources":["src"]}"#).unwrap();

    let main_path = source_dir.join("Main.ncr");
    let item_path = source_dir.join("Item.ncr");
    let main_source = "module lsp.app;\nimport lsp.model.Item;\nvoid main() { Item value = new Item(); }\n";
    fs::write(&main_path, main_source).unwrap();
    fs::write(&item_path, "module lsp.model;\npublic class Item {}\n").unwrap();
    let main_uri = uri(&main_path);
    let item_uri = uri(&item_path);

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri, "languageId": "nacre", "version": 1, "text": main_source } }
    }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": item_uri.clone(), "languageId": "nacre", "version": 1,
            "text": "module lsp.model;\nprivate class Item {}\n" } }
    }));
    let private_error = client.diagnostics(&uri(&main_path));
    assert!(private_error["diagnostics"][0]["message"].as_str().unwrap().contains("private type"));

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": item_uri, "version": 2 },
            "contentChanges": [{ "text": "module lsp.model;\npublic class Item {}\n" }]
        }
    }));
    assert!(client.diagnostics(&uri(&main_path))["diagnostics"].as_array().unwrap().is_empty());

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn unsaved_sources_under_a_missing_configured_root_resolve_and_revalidate() {
    missing_source_root_overlays("nacre.json", r#"{"sources":["src"]}"#);
    missing_source_root_overlays("Nacre.toml", "[package]\nname = 'editor'\nversion = '0.1.0'\nsources = ['src']\n");
}

fn missing_source_root_overlays(marker: &str, configuration: &str) {
    let temp = TempDir::new();
    fs::write(temp.0.join(marker), configuration).unwrap();
    let source_dir = temp.0.join("src");
    let main_path = source_dir.join("Main file-é.ncr");
    let item_path = source_dir.join("Item.ncr");
    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri(&main_path), "languageId": "nacre", "version": 7,
            "text": "void main() { Item value = new Item(); }\n" } }
    }));
    assert!(!client.diagnostics(&uri(&main_path))["diagnostics"].as_array().unwrap().is_empty());

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri(&item_path), "languageId": "nacre", "version": 12,
            "text": "class Item {}\n" } }
    }));
    let resolved = client.diagnostics(&uri(&main_path));
    assert!(resolved["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(resolved["version"], 7);
    assert_eq!(client.diagnostics(&uri(&item_path))["version"], 12);
    assert!(!source_dir.exists(), "unsaved overlays must not require or create disk sources");

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didClose",
        "params": { "textDocument": { "uri": uri(&item_path) } }
    }));
    assert!(!client.diagnostics(&uri(&main_path))["diagnostics"].as_array().unwrap().is_empty());

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn lsp_supports_document_symbols_hover_and_definition() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    fs::create_dir(&source_dir).unwrap();
    fs::write(temp.0.join("Nacre.toml"), "[package]\nname = 'demo'\nversion = '0.1.0'\nsources = ['src']\n").unwrap();

    let types_path = source_dir.join("Types.ncr");
    let main_path = source_dir.join("Main.ncr");
    let types_source = "public class Item {\n    private int count;\n    public Item(int count) { this.count = count; }\n    public readonly int getCount() { return this.count; }\n}\npublic class OtherItem {\n    public readonly long getCount() { return 7L; }\n}\npublic enum Status { Ready; Active(int); }\npublic void helper() {}\n";
    let main_source = "void main() { Item item = new Item(42); helper(); OtherItem other = new OtherItem(); println(other.getCount()); }\n";
    fs::write(&types_path, types_source).unwrap();
    fs::write(&main_path, main_source).unwrap();

    let types_uri = uri(&types_path);
    let main_uri = uri(&main_path);

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    let init = client.response(1);
    assert_eq!(init["result"]["capabilities"]["documentSymbolProvider"], true);
    assert_eq!(init["result"]["capabilities"]["hoverProvider"], true);
    assert_eq!(init["result"]["capabilities"]["definitionProvider"], true);

    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": types_uri.clone(), "languageId": "nacre", "version": 1, "text": types_source } }
    }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri.clone(), "languageId": "nacre", "version": 1, "text": main_source } }
    }));

    // Document symbols query on Types.ncr
    client.send(json!({
        "jsonrpc": "2.0", "id": 10, "method": "textDocument/documentSymbol",
        "params": { "textDocument": { "uri": types_uri.clone() } }
    }));
    let sym_resp = client.response(10);
    let symbols = sym_resp["result"].as_array().expect("symbols array");
    assert!(symbols.iter().any(|s| s["name"] == "Item" && s["kind"] == 5));
    assert!(symbols.iter().any(|s| s["name"] == "Status" && s["kind"] == 10));
    assert!(symbols.iter().any(|s| s["name"] == "helper" && s["kind"] == 12));

    // Hover query on Main.ncr over "Item" (line 0, col 15)
    client.send(json!({
        "jsonrpc": "2.0", "id": 20, "method": "textDocument/hover",
        "params": { "textDocument": { "uri": main_uri.clone() }, "position": { "line": 0, "character": 15 } }
    }));
    let hover_resp = client.response(20);
    let hover_content = hover_resp["result"]["contents"]["value"].as_str().expect("hover markdown");
    assert!(hover_content.contains("class Item"));

    // Hover query on Main.ncr over "helper" (line 0, col 42)
    client.send(json!({
        "jsonrpc": "2.0", "id": 21, "method": "textDocument/hover",
        "params": { "textDocument": { "uri": main_uri.clone() }, "position": { "line": 0, "character": 42 } }
    }));
    let hover_helper = client.response(21);
    let helper_content = hover_helper["result"]["contents"]["value"].as_str().expect("hover markdown");
    assert!(helper_content.contains("void helper()"));
    client.send(json!({
        "jsonrpc": "2.0", "id": 22, "method": "textDocument/hover",
        "params": { "textDocument": { "uri": main_uri.clone() }, "position": { "line": 0, "character": main_source.find("getCount").unwrap() } }
    }));
    let other_hover = client.response(22);
    let other_hover_content = other_hover["result"]["contents"]["value"].as_str().expect("method hover markdown");
    assert!(other_hover_content.contains("long getCount()"));

    // Definition query on Main.ncr over "Item" (line 0, col 15)
    client.send(json!({
        "jsonrpc": "2.0", "id": 30, "method": "textDocument/definition",
        "params": { "textDocument": { "uri": main_uri.clone() }, "position": { "line": 0, "character": 15 } }
    }));
    let def_resp = client.response(30);
    assert_eq!(def_resp["result"]["uri"], types_uri);

    let other_method_line = types_source.lines().position(|line| line.contains("return 7")).unwrap() as u64;
    let other_call_character = main_source.find("getCount").unwrap() as u64;
    client.send(json!({
        "jsonrpc": "2.0", "id": 31, "method": "textDocument/definition",
        "params": { "textDocument": { "uri": main_uri.clone() }, "position": { "line": 0, "character": other_call_character } }
    }));
    let other_method_definition = client.response(31);
    assert_eq!(other_method_definition["result"]["uri"], types_uri);
    assert_eq!(other_method_definition["result"]["range"]["start"]["line"].as_u64(), Some(other_method_line));

    client.send(json!({ "jsonrpc": "2.0", "id": 40, "method": "shutdown" }));
    assert!(client.response(40)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn document_symbols_survive_syntax_errors_in_other_units() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    fs::create_dir(&source_dir).unwrap();
    fs::write(
        temp.0.join("Nacre.toml"),
        "[package]\nname = 'demo'\nversion = '0.1.0'\nsources = ['src']\n",
    ).unwrap();
    let visible_path = source_dir.join("Visible.ncr");
    fs::write(&visible_path, "public class Visible {}\n").unwrap();
    fs::write(source_dir.join("Broken.ncr"), "public class Broken {\n").unwrap();
    let visible_uri = uri(&visible_path);

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "id": 2, "method": "textDocument/documentSymbol",
        "params": { "textDocument": { "uri": visible_uri } }
    }));
    let response = client.response(2);
    let symbols = response["result"].as_array().expect("symbols array");
    assert!(symbols.iter().any(|symbol| symbol["name"] == "Visible"), "{symbols:?}");

    client.send(json!({ "jsonrpc": "2.0", "id": 3, "method": "shutdown" }));
    assert!(client.response(3)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn lsp_reports_manifest_diagnostics_when_nacre_toml_is_invalid() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    fs::create_dir(&source_dir).unwrap();
    let manifest_path = temp.0.join("Nacre.toml");
    fs::write(&manifest_path, "[package]\nname = \n").unwrap(); // Syntax error in TOML

    let main_path = source_dir.join("Main.ncr");
    fs::write(&main_path, "void main() {}\n").unwrap();

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri(&main_path), "languageId": "nacre", "version": 1, "text": "void main() {}\n" } }
    }));

    let manifest_diags = client.diagnostics(&uri(&manifest_path));
    let diags = manifest_diags["diagnostics"].as_array().expect("diagnostics array");
    assert!(!diags.is_empty(), "expected diagnostic on invalid Nacre.toml");
    assert_eq!(diags[0]["severity"].as_u64(), Some(1));

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[cfg(unix)]
#[test]
fn lsp_rejects_manifest_paths_through_symlinked_project_roots() {
    let temp = TempDir::new();
    let real = temp.0.join("real-project");
    let source_dir = real.join("src");
    fs::create_dir_all(&source_dir).unwrap();
    let manifest_path = real.join("Nacre.toml");
    fs::write(
        &manifest_path,
        "[package]\nname = 'demo'\nversion = '0.1.0'\nsources = ['src']\n",
    )
    .unwrap();
    fs::write(source_dir.join("Main.ncr"), "void main() {}\n").unwrap();

    let alias = temp.0.join("linked-project");
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let alias_manifest = alias.join("Nacre.toml");
    let alias_source = alias.join("src/Main.ncr");
    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&alias), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri(&alias_source), "languageId": "nacre", "version": 1, "text": "void main() {}\n" } }
    }));

    let diagnostics = client.diagnostics(&uri(&alias_manifest));
    let items = diagnostics["diagnostics"].as_array().expect("diagnostics array");
    assert!(items.iter().any(|diagnostic| {
        diagnostic["message"]
            .as_str()
            .is_some_and(|message| message.contains("symbolic-link"))
    }), "{items:?}");

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[cfg(unix)]
#[test]
fn lsp_rejects_broken_manifest_symlinks() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    fs::create_dir_all(&source_dir).unwrap();
    let manifest_path = temp.0.join("Nacre.toml");
    std::os::unix::fs::symlink(temp.0.join("missing-manifest"), &manifest_path).unwrap();
    let source_path = source_dir.join("Main.ncr");
    fs::write(&source_path, "void main() {}\n").unwrap();

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri(&source_path), "languageId": "nacre", "version": 1, "text": "void main() {}\n" } }
    }));

    let diagnostics = client.diagnostics(&uri(&manifest_path));
    let items = diagnostics["diagnostics"].as_array().expect("diagnostics array");
    assert!(items.iter().any(|diagnostic| {
        diagnostic["message"]
            .as_str()
            .is_some_and(|message| message.contains("symbolic-link"))
    }), "{items:?}");

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn lsp_uses_unsaved_nacre_toml_overlays_and_reverts_on_close() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    let alternate_dir = temp.0.join("alternate");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&alternate_dir).unwrap();

    let manifest_path = temp.0.join("Nacre.toml");
    let invalid_disk_manifest = "[package]\nname = \n";
    fs::write(&manifest_path, invalid_disk_manifest).unwrap();
    let main_path = source_dir.join("Main.ncr");
    let alternate_path = alternate_dir.join("Invalid.ncr");
    fs::write(&main_path, "void main() {}\n").unwrap();
    fs::write(&alternate_path, "void main() { Missing value = new Missing(); }\n").unwrap();

    let manifest_uri = uri(&manifest_path);
    let main_uri = uri(&main_path);
    let alternate_uri = uri(&alternate_path);
    let alternate_manifest =
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nsources = [\"alternate\"]\n";
    let source_manifest =
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nsources = [\"src\"]\n";

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri, "languageId": "nacre", "version": 1, "text": "void main() {}\n" } }
    }));

    let initial = client.diagnostics(&manifest_uri);
    assert!(!initial["diagnostics"].as_array().unwrap().is_empty());
    fs::write(&manifest_path, source_manifest).unwrap();

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": manifest_uri, "languageId": "toml", "version": 1, "text": alternate_manifest } }
    }));
    let repaired = client.diagnostics(&manifest_uri);
    assert!(repaired["diagnostics"].as_array().unwrap().is_empty(),
        "a valid unsaved manifest must clear its stale diagnostic");
    let alternate = client.diagnostics(&alternate_uri);
    assert!(!alternate["diagnostics"].as_array().unwrap().is_empty(),
        "the unsaved source-root setting must select the alternate directory");
    assert_eq!(fs::read_to_string(&manifest_path).unwrap(), source_manifest,
        "editing a manifest overlay must not write it to disk");

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": manifest_uri, "version": 2 },
            "contentChanges": [{ "text": source_manifest }]
        }
    }));
    let alternate = client.diagnostics(&alternate_uri);
    assert!(alternate["diagnostics"].as_array().unwrap().is_empty(),
        "changing source roots must clear diagnostics for the removed root");
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": manifest_uri, "version": 3 },
            "contentChanges": [{ "text": alternate_manifest }]
        }
    }));
    let alternate = client.diagnostics(&alternate_uri);
    assert!(!alternate["diagnostics"].as_array().unwrap().is_empty(),
        "a later manifest edit must reselect the alternate source root");

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didClose",
        "params": { "textDocument": { "uri": manifest_uri } }
    }));
    let restored = client.diagnostics(&alternate_uri);
    assert!(restored["diagnostics"].as_array().unwrap().is_empty(),
        "closing the manifest must restore the on-disk source root");

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}


#[test]
fn lsp_closing_new_unsaved_project_manifest_restores_unmarked_scope() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    let alternate_dir = temp.0.join("alternate");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&alternate_dir).unwrap();

    let manifest_path = temp.0.join("Nacre.toml");
    let main_path = source_dir.join("Main.ncr");
    fs::write(&main_path, "void main() { Missing value = new Missing(); }\n").unwrap();
    fs::write(alternate_dir.join("Main.ncr"), "void main() {}\n").unwrap();
    let manifest_uri = uri(&manifest_path);
    let main_uri = uri(&main_path);

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri, "languageId": "nacre", "version": 1, "text": "void main() { Missing value = new Missing(); }\n" } }
    }));
    let unconfigured = client.diagnostics(&main_uri);
    assert!(!unconfigured["diagnostics"].as_array().unwrap().is_empty());

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": manifest_uri, "languageId": "toml", "version": 1,
                "text": "[package]\nname = \"app\"\nversion = \"0.1.0\"\nsources = [\"alternate\"]\n"
            }
        }
    }));
    let configured = client.diagnostics(&main_uri);
    assert!(configured["diagnostics"].as_array().unwrap().is_empty(),
        "the unsaved new manifest should replace the unconfigured source snapshot");

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didClose",
        "params": { "textDocument": { "uri": manifest_uri } }
    }));
    let restored = client.diagnostics(&main_uri);
    assert!(!restored["diagnostics"].as_array().unwrap().is_empty(),
        "closing a new unsaved manifest must revalidate remaining files without project configuration");
    assert!(!manifest_path.exists(), "the LSP overlay must not create the manifest on disk");

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}
#[test]
fn lsp_uses_unsaved_legacy_nacre_json_overlays() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    let alternate_dir = temp.0.join("alternate");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&alternate_dir).unwrap();

    let config_path = temp.0.join("nacre.json");
    let disk_config = r#"{"sources":["src"]}"#;
    fs::write(&config_path, disk_config).unwrap();
    let main_path = source_dir.join("Main.ncr");
    let alternate_path = alternate_dir.join("Invalid.ncr");
    fs::write(&main_path, "void main() {}\n").unwrap();
    fs::write(&alternate_path, "void main() { Missing value = new Missing(); }\n").unwrap();

    let config_uri = uri(&config_path);
    let main_uri = uri(&main_path);
    let alternate_uri = uri(&alternate_path);
    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri, "languageId": "nacre", "version": 1, "text": "void main() {}\n" } }
    }));
    client.diagnostics(&main_uri);

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": config_uri, "languageId": "json", "version": 1, "text": r#"{"sources":["alternate"]}"# } }
    }));
    let alternate = client.diagnostics(&alternate_uri);
    assert!(!alternate["diagnostics"].as_array().unwrap().is_empty(),
        "an unsaved legacy config must select its alternate source root");
    assert_eq!(fs::read_to_string(&config_path).unwrap(), disk_config,
        "editing a project config overlay must not write it to disk");

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didClose",
        "params": { "textDocument": { "uri": config_uri } }
    }));
    let restored = client.diagnostics(&alternate_uri);
    assert!(restored["diagnostics"].as_array().unwrap().is_empty(),
        "closing the config must restore its on-disk source root");

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn lsp_does_not_parse_configuration_overlays_as_source_files() {
    let temp = TempDir::new();
    let manifest_path = temp.0.join("Nacre.toml");
    let main_path = temp.0.join("Main.ncr");
    let manifest = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nsources = [\".\"]\n";
    fs::write(&manifest_path, manifest).unwrap();
    fs::write(&main_path, "void main() {}\n").unwrap();
    let manifest_uri = uri(&manifest_path);
    let main_uri = uri(&main_path);

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri, "languageId": "nacre", "version": 1, "text": "void main() {}\n" } }
    }));
    client.diagnostics(&main_uri);

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": manifest_uri, "languageId": "toml", "version": 1, "text": manifest } }
    }));
    let config_diagnostics = client.diagnostics(&manifest_uri);
    assert!(config_diagnostics["diagnostics"].as_array().unwrap().is_empty(),
        "a config overlay selected as a source root must not be parsed as Nacre");

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn lsp_closing_config_overlay_restores_saved_config_diagnostic() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    fs::create_dir_all(&source_dir).unwrap();
    let manifest_path = temp.0.join("Nacre.toml");
    let invalid_manifest = "[package]\nname = \n";
    let valid_manifest =
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nsources = [\"src\"]\n";
    fs::write(&manifest_path, invalid_manifest).unwrap();
    let main_path = source_dir.join("Main.ncr");
    fs::write(&main_path, "void main() {}\n").unwrap();
    let manifest_uri = uri(&manifest_path);
    let main_uri = uri(&main_path);

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri, "languageId": "nacre", "version": 1, "text": "void main() {}\n" } }
    }));
    assert!(!client.diagnostics(&manifest_uri)["diagnostics"].as_array().unwrap().is_empty());

    fs::write(&manifest_path, valid_manifest).unwrap();
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": manifest_uri, "languageId": "toml", "version": 1, "text": valid_manifest } }
    }));
    assert!(client.diagnostics(&manifest_uri)["diagnostics"].as_array().unwrap().is_empty());

    fs::write(&manifest_path, invalid_manifest).unwrap();
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didClose",
        "params": { "textDocument": { "uri": manifest_uri } }
    }));
    let restored = client.diagnostics(&manifest_uri);
    assert!(!restored["diagnostics"].as_array().unwrap().is_empty(),
        "closing a valid overlay must publish errors from the invalid saved manifest");

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn lsp_new_nested_manifest_reconciles_parent_scope_diagnostics() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    let alternate_dir = source_dir.join("alternate");
    fs::create_dir_all(&alternate_dir).unwrap();

    let parent_manifest = temp.0.join("Nacre.toml");
    fs::write(
        &parent_manifest,
        "[package]\nname = \"parent\"\nversion = \"0.1.0\"\nsources = [\"src/Main.ncr\", \"src/Invalid.ncr\"]\n",
    ).unwrap();
    let main_path = source_dir.join("Main.ncr");
    let invalid_path = source_dir.join("Invalid.ncr");
    let alternate_path = alternate_dir.join("Main.ncr");
    let child_manifest = source_dir.join("Nacre.toml");
    fs::write(&main_path, "void main() {}\n").unwrap();
    fs::write(&invalid_path, "void fail() { Missing value = new Missing(); }\n").unwrap();
    fs::write(&alternate_path, "void main() {}\n").unwrap();
    let main_uri = uri(&main_path);
    let invalid_uri = uri(&invalid_path);
    let child_manifest_uri = uri(&child_manifest);

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri, "languageId": "nacre", "version": 1, "text": "void main() {}\n" } }
    }));
    assert!(!client.diagnostics(&invalid_uri)["diagnostics"].as_array().unwrap().is_empty());

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": child_manifest_uri, "languageId": "toml", "version": 1,
                "text": "[package]\nname = \"child\"\nversion = \"0.1.0\"\nsources = [\"alternate/Main.ncr\"]\n"
            }
        }
    }));
    let after_marker = client.diagnostics(&invalid_uri);
    assert!(after_marker["diagnostics"].as_array().unwrap().is_empty(),
        "diagnostics from a source file that left the parent scope must be cleared");
    assert!(!child_manifest.exists(), "opening the nested config must remain an unsaved overlay");

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[cfg(unix)]
#[test]
fn lsp_does_not_resolve_through_intermediate_symlinked_source_roots() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new();
    let project = temp.0.join("project");
    let outside_source = temp.0.join("outside/src");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&outside_source).unwrap();
    fs::write(project.join("nacre.json"), r#"{"sources":["link/src"]}"#).unwrap();
    fs::write(outside_source.join("External.ncr"), "class External {}\n").unwrap();
    symlink(temp.0.join("outside"), project.join("link")).unwrap();

    let main_path = project.join("Main.ncr");
    let main_source = "void main() { External item = new External(); }\n";
    fs::write(&main_path, main_source).unwrap();

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&project), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "id": 2, "method": "textDocument/definition",
        "params": {
            "textDocument": { "uri": uri(&main_path) },
            "position": { "line": 0, "character": main_source.find("External").unwrap() }
        }
    }));
    assert!(client.response(2)["result"].is_null());

    client.send(json!({ "jsonrpc": "2.0", "id": 3, "method": "shutdown" }));
    assert!(client.response(3)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}
#[cfg(unix)]
#[test]
fn lsp_opens_non_utf8_source_paths_and_publishes_diagnostics() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = TempDir::new();
    let source_dir = temp.0.join("src");
    fs::create_dir(&source_dir).unwrap();
    fs::write(temp.0.join("nacre.json"), r#"{"sources":["src"]}"#).unwrap();

    let main_path = source_dir.join("Main.ncr");
    let main_source = "void main() {}\n";
    fs::write(&main_path, main_source).unwrap();
    let invalid_path = source_dir.join(OsString::from_vec(b"Invalid-\xff.ncr".to_vec()));
    let invalid_source = "void broken() { int value = true; }\n";
    fs::write(&invalid_path, invalid_source).unwrap();
    let invalid_uri = uri(&invalid_path);
    assert!(invalid_uri.ends_with("Invalid-%FF.ncr"), "{invalid_uri}");

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": uri(&temp.0), "capabilities": {} }
    }));
    client.response(1);
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri(&main_path), "languageId": "nacre", "version": 1, "text": main_source } }
    }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": invalid_uri.clone(), "languageId": "nacre", "version": 1, "text": invalid_source } }
    }));

    let published = client.diagnostics(&invalid_uri);
    let diagnostics = published["diagnostics"].as_array().expect("diagnostics array");
    assert!(!diagnostics.is_empty(), "{published}");
    assert_eq!(diagnostics[0]["severity"].as_u64(), Some(1));

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}

#[test]
fn lsp_workspace_folder_changes_revalidate_the_project_boundary() {
    let temp = TempDir::new();
    let workspace = temp.0.join("workspace");
    fs::create_dir(&workspace).unwrap();
    fs::write(
        temp.0.join("Nacre.toml"),
        "[package]\nname = \"root\"\nversion = \"0.1.0\"\nsources = [\"workspace\"]\n",
    )
    .unwrap();
    let main_path = workspace.join("Main.ncr");
    let model_path = workspace.join("Model.ncr");
    let main_source = "void main() { Item item = new Item(); }\n";
    fs::write(&main_path, main_source).unwrap();
    fs::write(&model_path, "class Item {}\n").unwrap();
    let main_uri = uri(&main_path);

    let mut client = Client::spawn();
    client.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "workspaceFolders": [],
            "capabilities": { "workspace": { "workspaceFolders": { "supported": true } } }
        }
    }));
    let initialized = client.response(1);
    assert!(
        initialized["result"]["capabilities"]["workspace"]["workspaceFolders"]["changeNotifications"]
            .as_bool()
            .unwrap(),
        "server must advertise workspace-folder notifications"
    );
    client.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": main_uri.clone(), "languageId": "nacre", "version": 1, "text": main_source } }
    }));
    assert!(
        client.diagnostics(&main_uri)["diagnostics"].as_array().unwrap().is_empty(),
        "the parent manifest initially includes both workspace sources"
    );

    client.send(json!({
        "jsonrpc": "2.0", "method": "workspace/didChangeWorkspaceFolders",
        "params": { "event": { "added": [{ "uri": uri(&workspace), "name": "workspace" }], "removed": [] } }
    }));
    assert!(
        !client.diagnostics(&main_uri)["diagnostics"].as_array().unwrap().is_empty(),
        "adding a workspace boundary excludes the ancestor project manifest"
    );

    client.send(json!({
        "jsonrpc": "2.0", "method": "workspace/didChangeWorkspaceFolders",
        "params": { "event": { "added": [], "removed": [{ "uri": uri(&workspace), "name": "workspace" }] } }
    }));
    assert!(
        client.diagnostics(&main_uri)["diagnostics"].as_array().unwrap().is_empty(),
        "removing the workspace boundary restores the ancestor project scope"
    );

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
    assert!(client.response(2)["result"].is_null());
    client.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    client.finish();
}
