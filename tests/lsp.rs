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
    let path = path.to_str().unwrap();
    if !path.starts_with('/') { result.push('/'); }
    for byte in path.bytes() {
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
