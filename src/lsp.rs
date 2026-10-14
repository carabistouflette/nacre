//! A small stdio Language Server Protocol implementation backed by project validation.
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufReader, BufWriter, Read, Write};
#[cfg(feature = "fuzzing")]
use std::io::BufRead;
use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};

use crate::diagnostic::{char_boundary, Diagnostic, Span};
use crate::project::{SourceBundle, MAX_FILES, MAX_SOURCE_BYTES};

mod framing;
use framing::{read_frame, send};

const MAX_MESSAGE_BYTES: usize = 8 * MAX_SOURCE_BYTES;
const MAX_DIRECTORY_ENTRIES: usize = 4096;
const MAX_URI_BYTES: usize = 16 * 1024;
#[cfg(feature = "fuzzing")]
const MAX_FUZZ_INPUT_BYTES: usize = 64 * 1024;

/// Outcome from exercising the real LSP frame reader and JSON decoder.
#[cfg(feature = "fuzzing")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FuzzFrameOutcome {
    InputTooLarge,
    EndOfInput,
    JsonFrame,
    InvalidJson,
    FramingError,
}
const MAX_WORKSPACE_ROOTS: usize = 64;

/// Run the Nacre language server over stdin/stdout using LSP JSON-RPC framing.
pub fn run() -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = BufWriter::new(stdout.lock());
    let mut server = Server::default();

    loop {
        let body = read_frame(&mut input, MAX_MESSAGE_BYTES).map_err(|error| format!("LSP input framing failed: {error}"))?;
        let Some(body) = body else { return Ok(()); };
        let message: Value = match serde_json::from_slice(&body) {
            Ok(message) => message,
            Err(_) => {
                send(&mut output, json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": "Parse error" }
                })).map_err(|error| format!("LSP output failed: {error}"))?;
                continue;
            }
        };
        match server.handle(message, &mut output)? {
            Control::Continue => {}
            Control::Exit { graceful: true } => return Ok(()),
            Control::Exit { graceful: false } => return Err("LSP exit notification received before shutdown".to_owned()),
        }
    }
}

#[derive(Default)]
struct Server {
    initialized: bool,
    shutdown_requested: bool,
    workspace_roots: Vec<PathBuf>,
    documents: HashMap<PathBuf, Document>,
    published_by_scope: HashMap<ProjectScope, HashSet<String>>,
}

struct Document {
    uri: String,
    version: i64,
    // Oversized documents retain their version but not their unbounded source text.
    text: Option<String>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ProjectScope {
    root: PathBuf,
    has_marker: bool,
}

struct SourceFile {
    path: PathBuf,
    uri: String,
    text: String,
    version: Option<i64>,
}

enum Control {
    Continue,
    Exit { graceful: bool },
}

impl Server {
    fn handle(&mut self, message: Value, output: &mut impl Write) -> Result<Control, String> {
        let Some(object) = message.as_object() else {
            self.respond_error(output, Value::Null, -32600, "Invalid Request")?;
            return Ok(Control::Continue);
        };
        let has_id = object.contains_key("id");
        let id = object.get("id").cloned();
        let valid_version = object.get("jsonrpc").and_then(Value::as_str) == Some("2.0");
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            if has_id { self.respond_error(output, id.unwrap_or(Value::Null), -32600, "Invalid Request")?; }
            return Ok(Control::Continue);
        };
        if !valid_version {
            if has_id { self.respond_error(output, id.unwrap_or(Value::Null), -32600, "Invalid Request")?; }
            return Ok(Control::Continue);
        }
        let empty_params = Value::Null;
        let params = object.get("params").unwrap_or(&empty_params);

        if method == "exit" {
            if has_id {
                self.respond_error(output, id.unwrap_or(Value::Null), -32600, "Invalid Request")?;
                return Ok(Control::Continue);
            }
            return Ok(Control::Exit { graceful: self.shutdown_requested });
        }
        if self.shutdown_requested {
            if has_id { self.respond_error(output, id.unwrap_or(Value::Null), -32600, "Server is shutting down")?; }
            return Ok(Control::Continue);
        }

        match method {
            "initialize" => {
                if !has_id {
                    return Ok(Control::Continue);
                }
                if self.initialized {
                    self.respond_error(output, id.unwrap_or(Value::Null), -32600, "Server is already initialized")?;
                    return Ok(Control::Continue);
                }
                self.workspace_roots = initial_roots(params);
                self.initialized = true;
                send(output, json!({
                    "jsonrpc": "2.0",
                    "id": id.unwrap_or(Value::Null),
                    "result": {
                        "capabilities": {
                            "textDocumentSync": {
                                "openClose": true,
                                "change": 1,
                                "save": { "includeText": false }
                            },
                            "workspace": {
                                "workspaceFolders": {
                                    "supported": true,
                                    "changeNotifications": true
                                }
                            }
                        },
                        "serverInfo": {
                            "name": "nacre",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                })).map_err(|error| format!("LSP output failed: {error}"))?;
            }
            "shutdown" => {
                if has_id {
                    self.shutdown_requested = true;
                    send(output, json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": null }))
                        .map_err(|error| format!("LSP output failed: {error}"))?;
                }
            }
            "initialized" => {}
            "workspace/didChangeWorkspaceFolders" if self.initialized => {
                self.change_workspace_folders(params);
                self.clear_published(output)?;
                self.revalidate_all(output)?;
            }
            "textDocument/didOpen" if self.initialized => self.did_open(params, output)?,
            "textDocument/didChange" if self.initialized => self.did_change(params, output)?,
            "textDocument/didSave" if self.initialized => self.did_save(params, output)?,
            "textDocument/didClose" if self.initialized => self.did_close(params, output)?,
            _ if has_id => self.respond_error(output, id.unwrap_or(Value::Null), -32601, "Method not found")?,
            _ => {}
        }
        Ok(Control::Continue)
    }

    fn respond_error(&self, output: &mut impl Write, id: Value, code: i32, message: &str) -> Result<(), String> {
        send(output, json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }))
            .map_err(|error| format!("LSP output failed: {error}"))
    }

    fn did_open(&mut self, params: &Value, output: &mut impl Write) -> Result<(), String> {
        let Some(document) = params.get("textDocument") else { return Ok(()); };
        let (Some(uri), Some(version), Some(text)) = (
            document.get("uri").and_then(Value::as_str),
            document.get("version").and_then(Value::as_i64),
            document.get("text").and_then(Value::as_str),
        ) else { return Ok(()); };
        let Some(path) = uri_to_path(uri) else { return Ok(()); };
        if !is_source_file(&path) { return Ok(()); }
        if self.documents.get(&path).is_some_and(|existing| version < existing.version) { return Ok(()); }
        if !self.documents.contains_key(&path) && self.documents.len() >= MAX_FILES {
            self.log(output, 2, &format!("Cannot track more than {MAX_FILES} open Nacre source files"))?;
            self.publish_one(output, uri, None, Vec::new())?;
            return Ok(());
        }
        let text = bounded_text(text);
        self.documents.insert(path.clone(), Document { uri: uri.to_owned(), version, text });
        self.revalidate_path(&path, output, None)
    }

    fn did_change(&mut self, params: &Value, output: &mut impl Write) -> Result<(), String> {
        let Some(document) = params.get("textDocument") else { return Ok(()); };
        let (Some(uri), Some(version)) = (
            document.get("uri").and_then(Value::as_str),
            document.get("version").and_then(Value::as_i64),
        ) else { return Ok(()); };
        let Some(path) = uri_to_path(uri) else { return Ok(()); };
        let Some(change_list) = params.get("contentChanges").and_then(Value::as_array) else { return Ok(()); };
        let mut replacement = None;
        for change in change_list {
            if change.get("range").is_some() { return Ok(()); }
            let Some(text) = change.get("text").and_then(Value::as_str) else { return Ok(()); };
            replacement = Some(text);
        }
        let Some(text) = replacement else { return Ok(()); };
        let Some(open) = self.documents.get_mut(&path) else { return Ok(()); };
        if version <= open.version { return Ok(()); }
        open.version = version;
        open.text = bounded_text(text);
        self.revalidate_path(&path, output, None)
    }

    fn did_save(&mut self, params: &Value, output: &mut impl Write) -> Result<(), String> {
        let Some(document) = params.get("textDocument") else { return Ok(()); };
        let Some(uri) = document.get("uri").and_then(Value::as_str) else { return Ok(()); };
        let Some(path) = uri_to_path(uri) else { return Ok(()); };
        let Some(open) = self.documents.get_mut(&path) else { return Ok(()); };
        if let Some(text) = params.get("text").and_then(Value::as_str) {
            open.text = bounded_text(text);
        }
        self.revalidate_path(&path, output, None)
    }

    fn did_close(&mut self, params: &Value, output: &mut impl Write) -> Result<(), String> {
        let Some(uri) = params.get("textDocument").and_then(|doc| doc.get("uri")).and_then(Value::as_str) else {
            return Ok(());
        };
        let Some(path) = uri_to_path(uri) else { return Ok(()); };
        if self.documents.remove(&path).is_some() {
            let scope = self.scope_for(&path);
            let canonical_uri = path_to_uri(&path).unwrap_or_else(|| uri.to_owned());
            if self.documents.keys().any(|open_path| self.scope_for(open_path) == scope) {
                self.revalidate_scope(&scope, output, Some(&canonical_uri))?;
            } else {
                self.clear_scope(&scope, output)?;
            }
            self.publish_one(output, uri, None, Vec::new())?;
            if canonical_uri != uri { self.publish_one(output, &canonical_uri, None, Vec::new())?; }
        }
        Ok(())
    }

    fn change_workspace_folders(&mut self, params: &Value) {
        let Some(event) = params.get("event") else { return; };
        if let Some(removed) = event.get("removed").and_then(Value::as_array) {
            for folder in removed {
                if let Some(path) = folder.get("uri").and_then(Value::as_str).and_then(uri_to_path) {
                    self.workspace_roots.retain(|root| root != &path);
                }
            }
        }
        if let Some(added) = event.get("added").and_then(Value::as_array) {
            for folder in added {
                if let Some(path) = folder.get("uri").and_then(Value::as_str).and_then(uri_to_path) {
                    if !self.workspace_roots.contains(&path) && self.workspace_roots.len() < MAX_WORKSPACE_ROOTS {
                        self.workspace_roots.push(path);
                    }
                }
            }
        }
    }

    fn clear_published(&mut self, output: &mut impl Write) -> Result<(), String> {
        for uris in self.published_by_scope.values() {
            for uri in uris { self.publish_one(output, uri, None, Vec::new())?; }
        }
        self.published_by_scope.clear();
        Ok(())
    }
    fn clear_scope(&mut self, scope: &ProjectScope, output: &mut impl Write) -> Result<(), String> {
        if let Some(uris) = self.published_by_scope.remove(scope) {
            for uri in uris { self.publish_one(output, &uri, None, Vec::new())?; }
        }
        Ok(())
    }

    fn revalidate_all(&mut self, output: &mut impl Write) -> Result<(), String> {
        let scopes: HashSet<_> = self.documents.keys().map(|path| self.scope_for(path)).collect();
        for scope in scopes { self.revalidate_scope(&scope, output, None)?; }
        Ok(())
    }

    fn revalidate_path(&mut self, path: &Path, output: &mut impl Write, excluded_uri: Option<&str>) -> Result<(), String> {
        let scope = self.scope_for(path);
        self.revalidate_scope(&scope, output, excluded_uri)
    }

    fn revalidate_scope(&mut self, scope: &ProjectScope, output: &mut impl Write, excluded_uri: Option<&str>) -> Result<(), String> {
        let mut files = match self.load_scope_files(scope) {
            Ok(files) => files,
            Err(message) => {
                self.log(output, 2, &format!("Nacre validation unavailable for {}: {message}", scope.root.display()))?;
                self.publish_scope_diagnostics(scope, output, excluded_uri, HashMap::new(), None)?;
                return Ok(());
            }
        };
        // SourceBundle owns the text; keep only the file metadata for publishing.
        let source_list = files.iter_mut().map(|file| {
            (file.path.to_str().expect("URI paths are UTF-8").to_owned(), std::mem::take(&mut file.text))
        }).collect();
        let bundle = match SourceBundle::from_sources(source_list) {
            Ok(bundle) => bundle,
            Err(message) => {
                self.log(output, 2, &format!("Nacre validation unavailable for {}: {message}", scope.root.display()))?;
                self.publish_scope_diagnostics(scope, output, excluded_uri, HashMap::new(), Some(&files))?;
                return Ok(());
            }
        };
        let mut diagnostics = HashMap::new();
        if let Err(diagnostic) = bundle.validate() {
            if let Some((uri, value)) = diagnostic_json(&diagnostic, &bundle, &files) {
                diagnostics.insert(uri, vec![value]);
            }
        }
        self.publish_scope_diagnostics(scope, output, excluded_uri, diagnostics, Some(&files))
    }

    fn publish_scope_diagnostics(
        &mut self,
        scope: &ProjectScope,
        output: &mut impl Write,
        excluded_uri: Option<&str>,
        diagnostics: HashMap<String, Vec<Value>>,
        files: Option<&[SourceFile]>,
    ) -> Result<(), String> {
        let mut targets: HashMap<String, Option<i64>> = HashMap::new();
        if let Some(files) = files {
            for file in files {
                targets.insert(file.uri.clone(), file.version);
            }
        }
        if files.is_none() {
            for (path, document) in &self.documents {
                if self.scope_for(path) == *scope {
                    targets.insert(document.uri.clone(), Some(document.version));
                }
            }
        }
        if let Some(excluded_uri) = excluded_uri { targets.remove(excluded_uri); }
        let current: HashSet<String> = targets.keys().cloned().collect();
        if let Some(previous) = self.published_by_scope.get(scope) {
            for uri in previous.difference(&current) {
                self.publish_one(output, uri, None, Vec::new())?;
            }
        }
        for (uri, version) in &targets {
            self.publish_one(output, uri, *version, diagnostics.get(uri).cloned().unwrap_or_default())?;
        }
        self.published_by_scope.insert(scope.clone(), current);
        Ok(())
    }
    fn load_scope_files(&self, scope: &ProjectScope) -> Result<Vec<SourceFile>, String> {
        let inputs = if scope.has_marker {
            configured_source_roots(&scope.root)?
        } else {
            vec![scope.root.clone()]
        };
        let mut paths = Vec::new();
        let mut visited_entries = 0;
        for input in &inputs {
            let has_overlay = self.documents.keys().any(|path| source_root_contains(input, path));
            let missing = match fs::symlink_metadata(input) {
                Err(error) => error.kind() == io::ErrorKind::NotFound,
                Ok(_) => false,
            };
            if !(missing && has_overlay) {
                collect_recursive(input, 0, &mut visited_entries, &mut paths)?;
            }
        }
        for (path, document) in &self.documents {
            if self.scope_for(path) == *scope {
                if let Some(input) = inputs.iter().find(|input| source_root_contains(input, path)) {
                    reject_symlink_path(path, input)?;
                    if document.text.is_none() {
                        return Err(format!("{} exceeds the {MAX_SOURCE_BYTES}-byte per-project source limit", path.display()));
                    }
                    paths.push(path.clone());
                }
            }
        }
        paths.sort();
        paths.dedup();
        if paths.len() > MAX_FILES { return Err(format!("source file limit is {MAX_FILES}")); }

        let mut files = Vec::with_capacity(paths.len());
        let mut combined_bytes = 0usize;
        for path in paths {
            let document = self.documents.get(&path);
            let text = if let Some(document) = document {
                document.text.clone().ok_or_else(|| format!("{} exceeds the source size limit", path.display()))?
            } else {
                read_source(&path, MAX_SOURCE_BYTES.saturating_sub(combined_bytes))?
            };
            if text.len().saturating_add(combined_bytes).saturating_add(1) > MAX_SOURCE_BYTES {
                return Err(format!("sources exceed the {MAX_SOURCE_BYTES}-byte total source limit"));
            }
            let uri = if let Some(document) = document {
                document.uri.clone()
            } else {
                path_to_uri(&path).ok_or_else(|| format!("{} is not a UTF-8 file path", path.display()))?
            };
            let source_bytes = text.len();
            files.push(SourceFile {
                path: path.clone(), uri,
                version: document.map(|open| open.version),
                text,
            });
            combined_bytes += source_bytes + 1;
        }
        if files.is_empty() { return Err("no Nacre source files in project scope".to_owned()); }
        Ok(files)
    }

    fn scope_for(&self, path: &Path) -> ProjectScope {
        let directory = path.parent().unwrap_or_else(|| Path::new("/"));
        let workspace_root = self.workspace_roots.iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.components().count());
        let boundary = workspace_root.map(PathBuf::as_path);
        let mut current = Some(directory);
        for _ in 0..=32 {
            let Some(dir) = current else { break; };
            if dir.join("nacre.json").is_file() {
                return ProjectScope { root: dir.to_owned(), has_marker: true };
            }
            if boundary == Some(dir) { break; }
            current = dir.parent();
        }
        ProjectScope { root: path.to_owned(), has_marker: false }
    }
    fn publish_one(&self, output: &mut impl Write, uri: &str, version: Option<i64>, diagnostics: Vec<Value>) -> Result<(), String> {
        let mut params = json!({ "uri": uri, "diagnostics": diagnostics });
        if let Some(version) = version { params["version"] = json!(version); }
        send(output, json!({ "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": params }))
            .map_err(|error| format!("LSP output failed: {error}"))
    }

    fn log(&self, output: &mut impl Write, kind: i32, message: &str) -> Result<(), String> {
        send(output, json!({ "jsonrpc": "2.0", "method": "window/logMessage", "params": { "type": kind, "message": message } }))
            .map_err(|error| format!("LSP output failed: {error}"))
    }
}

fn initial_roots(params: &Value) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(folders) = params.get("workspaceFolders").and_then(Value::as_array) {
        for folder in folders {
            if let Some(path) = folder.get("uri").and_then(Value::as_str).and_then(uri_to_path) {
                if !roots.contains(&path) && roots.len() < MAX_WORKSPACE_ROOTS { roots.push(path); }
            }
        }
    }
    if roots.is_empty() {
        if let Some(path) = params.get("rootUri").and_then(Value::as_str).and_then(uri_to_path) {
            roots.push(path);
        } else if let Some(root) = params.get("rootPath").and_then(Value::as_str) {
            let path = normalize_path(Path::new(root));
            roots.push(path);
        }
    }
    roots
}

fn bounded_text(text: &str) -> Option<String> {
    (text.len() <= MAX_SOURCE_BYTES).then(|| text.to_owned())
}

fn is_source_file(path: &Path) -> bool { path.extension().is_some_and(|extension| extension == "ncr") }

fn configured_source_roots(root: &Path) -> Result<Vec<PathBuf>, String> {
    const MAX_CONFIG_BYTES: usize = 64 * 1024;
    let config = root.join("nacre.json");
    let metadata = fs::symlink_metadata(&config).map_err(|error| format!("{}: {error}", config.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{} must be a regular project configuration file", config.display()));
    }
    let mut bytes = Vec::new();
    fs::File::open(&config).map_err(|error| format!("{}: {error}", config.display()))?
        .take(MAX_CONFIG_BYTES as u64 + 1).read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", config.display()))?;
    if bytes.len() > MAX_CONFIG_BYTES { return Err(format!("{} exceeds the {MAX_CONFIG_BYTES}-byte configuration limit", config.display())); }
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| format!("{}: invalid JSON ({error})", config.display()))?;
    let sources = value.get("sources").and_then(Value::as_array)
        .ok_or_else(|| format!("{} must contain a string array named `sources`", config.display()))?;
    if sources.is_empty() { return Err(format!("{} has no source paths", config.display())); }
    if sources.len() > MAX_FILES { return Err(format!("source path limit is {MAX_FILES}")); }
    let mut roots = Vec::with_capacity(sources.len());
    for source in sources {
        let source = source.as_str().filter(|source| !source.is_empty())
            .ok_or_else(|| format!("{} contains a non-string or empty source path", config.display()))?;
        let source = Path::new(source);
        if source.is_absolute() { return Err(format!("{} source paths must be relative", config.display())); }
        let path = normalize_path(&root.join(source));
        if !path.starts_with(root) { return Err(format!("{} source path escapes the project root", config.display())); }
        roots.push(path);
    }
    Ok(roots)
}
fn source_root_contains(root: &Path, path: &Path) -> bool {
    if root == path { return true; }
    if !path.starts_with(root) { return false; }
    let root_is_directory = root.is_dir() || (
        !is_source_file(root)
            && matches!(fs::symlink_metadata(root), Err(error) if error.kind() == io::ErrorKind::NotFound)
    );
    root_is_directory && path.strip_prefix(root).is_ok_and(|relative| {
        relative.components().all(|component| {
            let name = component.as_os_str().to_string_lossy();
            !name.starts_with('.') && !matches!(name.as_ref(), "out" | "target")
        })
    })
}
fn reject_symlink_path(path: &Path, root: &Path) -> Result<(), String> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        match fs::symlink_metadata(candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!("symbolic-link source is not allowed: {}", candidate.display()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{}: {error}", candidate.display())),
        }
        if candidate == root { return Ok(()); }
        current = candidate.parent();
    }
    Err(format!("{} is outside configured source root {}", path.display(), root.display()))

}
fn collect_recursive(path: &Path, depth: usize, visited: &mut usize, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 32 { return Err("source directory nesting limit is 32".to_owned()); }
    let metadata = fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() { return Err(format!("symbolic-link source is not allowed: {}", path.display())); }
    if metadata.is_file() {
        if is_source_file(path) { push_source(output, path.to_owned())?; }
        return Ok(());
    }
    if !metadata.is_dir() { return Ok(()); }
    for entry in directory_entries(path, visited)? {
        let name = entry.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if name.starts_with('.') || matches!(name.as_str(), "out" | "target") { continue; }
        let kind = fs::symlink_metadata(&entry).map_err(|error| format!("{}: {error}", entry.display()))?;
        if kind.is_dir() { collect_recursive(&entry, depth + 1, visited, output)?; }
        else if is_source_file(&entry) {
            if kind.file_type().is_symlink() { return Err(format!("symbolic-link source is not allowed: {}", entry.display())); }
            if kind.is_file() && entry.to_str().is_some() { push_source(output, entry)?; }
        }
    }
    Ok(())
}

fn directory_entries(path: &Path, visited: &mut usize) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut output = Vec::new();
    for entry in entries {
        *visited += 1;
        if *visited > MAX_DIRECTORY_ENTRIES { return Err(format!("directory scan limit is {MAX_DIRECTORY_ENTRIES} entries")); }
        output.push(entry.map_err(|error| format!("{}: {error}", path.display()))?.path());
    }
    output.sort();
    Ok(output)
}

fn push_source(output: &mut Vec<PathBuf>, path: PathBuf) -> Result<(), String> {
    output.push(path);
    if output.len() > MAX_FILES { return Err(format!("source file limit is {MAX_FILES}")); }
    Ok(())
}

fn read_source(path: &Path, remaining: usize) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(remaining as u64 + 1).read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() > remaining { return Err(format!("{} exceeds the project source size limit", path.display())); }
    String::from_utf8(bytes).map_err(|error| format!("{}: UTF-8 required ({error})", path.display()))
}

fn diagnostic_json(diagnostic: &Diagnostic, bundle: &SourceBundle, files: &[SourceFile]) -> Option<(String, Value)> {
    let primary = map_span(diagnostic.span, bundle, files)?;
    let mut related = Vec::new();
    for (span, message) in &diagnostic.notes {
        if let Some(location) = map_span(*span, bundle, files) {
            related.push(json!({
                "location": {
                    "uri": location.file.uri,
                    "range": range_json(location.source, location.start, location.end)
                },
                "message": message
            }));
        }
    }
    let message = diagnostic.help.as_ref().map_or_else(
        || diagnostic.message.clone(),
        |help| format!("{}\nhelp: {help}", diagnostic.message),
    );
    let mut value = json!({
        "range": range_json(primary.source, primary.start, primary.end),
        "severity": 1,
        "source": "Nacre",
        "message": message
    });
    if !related.is_empty() { value["relatedInformation"] = json!(related); }
    Some((primary.file.uri.clone(), value))
}

struct LocalSpan<'a> {
    file: &'a SourceFile,
    source: &'a str,
    start: usize,
    end: usize,
}

fn map_span<'a>(span: Span, bundle: &'a SourceBundle, files: &'a [SourceFile]) -> Option<LocalSpan<'a>> {
    let location = bundle.locate(span);
    let file = files.iter().find(|file| file.path.as_path() == location.path)?;
    let start = char_boundary(location.source, location.span.start);
    let end = char_boundary(location.source, location.span.end).max(start);
    Some(LocalSpan { file, source: location.source, start, end })
}

fn range_json(source: &str, start: usize, end: usize) -> Value {
    let start = offset_position(source, start);
    let end = offset_position(source, end.max(start.2));
    json!({
        "start": { "line": start.0, "character": start.1 },
        "end": { "line": end.0, "character": end.1 }
    })
}

fn offset_position(source: &str, offset: usize) -> (usize, usize, usize) {
    let offset = char_boundary(source, offset);
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let line_prefix = prefix[line_start..].strip_suffix('\r').unwrap_or(&prefix[line_start..]);
    (line, line_prefix.encode_utf16().count(), offset)
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    if uri.len() > MAX_URI_BYTES { return None; }
    let (scheme, rest) = uri.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("file") { return None; }
    if rest.contains('?') || rest.contains('#') { return None; }
    let slash = rest.find('/')?;
    let authority = &rest[..slash];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") { return None; }
    let encoded = &rest[slash..];
    let mut bytes = Vec::with_capacity(encoded.len());
    let source = encoded.as_bytes();
    let mut index = 0;
    while index < source.len() {
        if source[index] == b'%' {
            let high = hex(source.get(index + 1).copied()?)?;
            let low = hex(source.get(index + 2).copied()?)?;
            bytes.push(high * 16 + low);
            index += 3;
        } else {
            bytes.push(source[index]);
            index += 1;
        }
    }
    if bytes.contains(&0) { return None; }
    let decoded = String::from_utf8(bytes).ok()?;
    let path = Path::new(&decoded);
    if !path.is_absolute() { return None; }
    Some(normalize_path(path))
}


fn path_to_uri(path: &Path) -> Option<String> {
    let path = normalize_path(path);
    let path = path.to_str()?;
    let mut uri = String::from("file://");
    if !path.starts_with('/') { uri.push('/'); }
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            uri.push(byte as char);
        } else {
            uri.push('%');
            uri.push(char::from_digit((byte >> 4) as u32, 16)?.to_ascii_uppercase());
            uri.push(char::from_digit((byte & 0x0f) as u32, 16)?.to_ascii_uppercase());
        }
    }
    Some(uri)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/" )).join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => { normalized.pop(); }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}


#[cfg(feature = "fuzzing")]
pub fn fuzz_read_frame(input: &[u8]) -> FuzzFrameOutcome {
    if input.len() > MAX_FUZZ_INPUT_BYTES {
        return FuzzFrameOutcome::InputTooLarge;
    }
    let chunk_size = input.first().map_or(1, |byte| usize::from(*byte & 0x1f) + 1);
    let mut reader = FuzzChunkedCursor { cursor: io::Cursor::new(input), chunk_size };
    match read_frame(&mut reader, MAX_MESSAGE_BYTES) {
        Ok(None) => FuzzFrameOutcome::EndOfInput,
        Ok(Some(body)) => {
            if serde_json::from_slice::<Value>(&body).is_ok() {
                FuzzFrameOutcome::JsonFrame
            } else {
                FuzzFrameOutcome::InvalidJson
            }
        }
        Err(_) => FuzzFrameOutcome::FramingError,
    }
}

#[cfg(feature = "fuzzing")]
struct FuzzChunkedCursor<'a> {
    cursor: io::Cursor<&'a [u8]>,
    chunk_size: usize,
}

#[cfg(feature = "fuzzing")]
impl Read for FuzzChunkedCursor<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let count = available.len().min(output.len());
        output[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}

#[cfg(feature = "fuzzing")]
impl BufRead for FuzzChunkedCursor<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let available = self.cursor.fill_buf()?;
        Ok(&available[..available.len().min(self.chunk_size)])
    }

    fn consume(&mut self, amount: usize) {
        self.cursor.consume(amount);
    }
}


#[cfg(all(test, feature = "fuzzing"))]
mod fuzz_framing_tests {
    use super::{fuzz_read_frame, FuzzFrameOutcome, MAX_FUZZ_INPUT_BYTES, MAX_MESSAGE_BYTES};
    use super::framing::MAX_HEADER_BYTES;

    fn frame(body: &[u8]) -> Vec<u8> {
        let mut result = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        result.extend_from_slice(body);
        result
    }

    #[test]
    fn decodes_fragmented_utf8_json_frame() {
        let input = frame(r#"{"jsonrpc":"2.0","params":{"text":"été"}}"#.as_bytes());
        assert_eq!(fuzz_read_frame(&input), FuzzFrameOutcome::JsonFrame);
    }

    #[test]
    fn rejects_malformed_duplicate_and_truncated_headers() {
        for input in [
            b"Content-Length: nope\r\n\r\n".as_slice(),
            b"Content-Length: 2\r\ncontent-length: 2\r\n\r\n{}".as_slice(),
            b"Content-Length: 2\r\n".as_slice(),
        ] {
            assert_eq!(fuzz_read_frame(input), FuzzFrameOutcome::FramingError);
        }
    }

    #[test]
    fn rejects_oversized_header_and_declared_body() {
        let oversized_header = format!("X: {}\r\n\r\n", "a".repeat(MAX_HEADER_BYTES));
        assert_eq!(fuzz_read_frame(oversized_header.as_bytes()), FuzzFrameOutcome::FramingError);

        let oversized_length = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        assert_eq!(fuzz_read_frame(oversized_length.as_bytes()), FuzzFrameOutcome::FramingError);
    }

    #[test]
    fn distinguishes_truncated_body_and_invalid_utf8_json() {
        assert_eq!(
            fuzz_read_frame(b"Content-Length: 4\r\n\r\n{}"),
            FuzzFrameOutcome::FramingError,
        );
        let mut invalid_utf8_frame = b"Content-Length: 1\r\n\r\n".to_vec();
        invalid_utf8_frame.push(0xff);
        assert_eq!(fuzz_read_frame(&invalid_utf8_frame), FuzzFrameOutcome::InvalidJson);
    }

    #[test]
    fn rejects_probe_inputs_above_its_fixed_bound() {
        assert_eq!(
            fuzz_read_frame(&vec![0; MAX_FUZZ_INPUT_BYTES + 1]),
            FuzzFrameOutcome::InputTooLarge,
        );
    }
}
