//! A small stdio Language Server Protocol implementation backed by project validation.
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufReader, BufWriter, Write};
#[cfg(feature = "fuzzing")]
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::project::{is_source_file, SourceBundle, MAX_FILES, MAX_SOURCE_BYTES};

mod framing;
use framing::{read_frame, send};
mod file_uri;
use file_uri::{normalize_path, path_to_uri, uri_to_path};
mod diagnostics;
use diagnostics::diagnostic_json;
mod source_files;
use source_files::{
    configured_source_roots, is_project_config, source_root_contains, reject_symlink_path,
    collect_recursive, read_source,
};
mod queries;

const MAX_MESSAGE_BYTES: usize = 8 * MAX_SOURCE_BYTES;
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

fn validation_unavailable_diagnostic(message: String) -> Value {
    json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 0 }
        },
        "severity": 1,
        "source": "Nacre",
        "message": message
    })
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
                            "hoverProvider": true,
                            "documentSymbolProvider": true,
                            "definitionProvider": true,
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
            "textDocument/documentSymbol" if self.initialized && has_id => {
                let result = self.document_symbols(params)?;
                send(output, json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result }))
                    .map_err(|error| format!("LSP output failed: {error}"))?;
            }
            "textDocument/hover" if self.initialized && has_id => {
                let result = self.hover(params)?;
                send(output, json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result }))
                    .map_err(|error| format!("LSP output failed: {error}"))?;
            }
            "textDocument/definition" if self.initialized && has_id => {
                let result = self.definition(params)?;
                send(output, json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result }))
                    .map_err(|error| format!("LSP output failed: {error}"))?;
            }
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
        if !is_source_file(&path) && !is_project_config(&path) { return Ok(()); }
        if self.documents.get(&path).is_some_and(|existing| version < existing.version) { return Ok(()); }
        if !self.documents.contains_key(&path) && self.documents.len() >= MAX_FILES {
            self.log(output, 2, &format!("Cannot track more than {MAX_FILES} open Nacre documents"))?;
            self.publish_one(output, uri, None, Vec::new())?;
            return Ok(());
        }
        let mut previous_scopes = HashSet::new();
        if is_project_config(&path) {
            let marker_root = path.parent().unwrap_or_else(|| Path::new("/"));
            if self.scope_for(&path).root.as_path() != marker_root {
                previous_scopes.extend(self.published_by_scope.keys()
                    .filter(|scope| scope.root.as_path().starts_with(marker_root)
                        || marker_root.starts_with(scope.root.as_path()))
                    .cloned());
                previous_scopes.extend(self.documents.keys()
                    .map(|open_path| self.scope_for(open_path))
                    .filter(|scope| scope.root.as_path().starts_with(marker_root)
                        || marker_root.starts_with(scope.root.as_path())));
            }
        }
        let text = bounded_text(text);
        self.documents.insert(path.clone(), Document { uri: uri.to_owned(), version, text });
        for previous_scope in previous_scopes {
            if self.documents.keys().any(|open_path| self.scope_for(open_path) == previous_scope) {
                self.revalidate_scope(&previous_scope, output, None)?;
            } else {
                self.clear_scope(&previous_scope, output)?;
            }
        }
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
        if self.documents.contains_key(&path) {
            let scope = self.scope_for(&path);
            let is_config = is_project_config(&path);
            let affected_paths: Vec<PathBuf> = self.documents.keys()
                .filter(|open_path| *open_path != &path && self.scope_for(open_path) == scope)
                .cloned()
                .collect();
            self.documents.remove(&path);
            let canonical_uri = path_to_uri(&path).unwrap_or_else(|| uri.to_owned());
            let scopes: HashSet<ProjectScope> = affected_paths.iter()
                .map(|open_path| self.scope_for(open_path))
                .collect();
            if scopes.contains(&scope) {
                let excluded_uri = if is_config { None } else { Some(canonical_uri.as_str()) };
                self.revalidate_scope(&scope, output, excluded_uri)?;
            } else {
                self.clear_scope(&scope, output)?;
            }
            for affected_scope in scopes {
                if affected_scope != scope {
                    self.revalidate_scope(&affected_scope, output, None)?;
                }
            }
            if !is_config {
                self.publish_one(output, uri, None, Vec::new())?;
                if canonical_uri != uri { self.publish_one(output, &canonical_uri, None, Vec::new())?; }
            }
        }
        Ok(())
    }
    fn document_symbols(&self, params: &Value) -> Result<Value, String> {
        let Some(document) = params.get("textDocument") else { return Ok(json!([])); };
        let Some(uri) = document.get("uri").and_then(Value::as_str) else { return Ok(json!([])); };
        let Some(path) = uri_to_path(uri) else { return Ok(json!([])); };
        let scope = self.scope_for(&path);
        let bundle = match self.load_scope_bundle(&scope) {
            Ok(bundle) => bundle,
            Err(_) => return Ok(json!([])),
        };
        let Some(file_source) = bundle.source_for_path(&path) else { return Ok(json!([])); };
        let symbols = queries::document_symbols(file_source);
        Ok(json!(symbols))
    }

    fn hover(&self, params: &Value) -> Result<Value, String> {
        let Some(document) = params.get("textDocument") else { return Ok(Value::Null); };
        let Some(uri) = document.get("uri").and_then(Value::as_str) else { return Ok(Value::Null); };
        let Some(path) = uri_to_path(uri) else { return Ok(Value::Null); };
        let Some(pos) = params.get("position") else { return Ok(Value::Null); };
        let (Some(line), Some(character)) = (
            pos.get("line").and_then(Value::as_u64),
            pos.get("character").and_then(Value::as_u64),
        ) else { return Ok(Value::Null); };
        let scope = self.scope_for(&path);
        let bundle = match self.load_scope_bundle(&scope) {
            Ok(bundle) => bundle,
            Err(_) => return Ok(Value::Null),
        };
        let Some(file_source) = bundle.source_for_path(&path) else { return Ok(Value::Null); };
        let hover_value = queries::hover(&bundle, &path, file_source, line as usize, character as usize);
        Ok(hover_value.unwrap_or(Value::Null))
    }

    fn definition(&self, params: &Value) -> Result<Value, String> {
        let Some(document) = params.get("textDocument") else { return Ok(Value::Null); };
        let Some(uri) = document.get("uri").and_then(Value::as_str) else { return Ok(Value::Null); };
        let Some(path) = uri_to_path(uri) else { return Ok(Value::Null); };
        let Some(pos) = params.get("position") else { return Ok(Value::Null); };
        let (Some(line), Some(character)) = (
            pos.get("line").and_then(Value::as_u64),
            pos.get("character").and_then(Value::as_u64),
        ) else { return Ok(Value::Null); };
        let scope = self.scope_for(&path);
        let bundle = match self.load_scope_bundle(&scope) {
            Ok(bundle) => bundle,
            Err(_) => return Ok(Value::Null),
        };
        let Some(file_source) = bundle.source_for_path(&path) else { return Ok(Value::Null); };
        let def = queries::definition(&bundle, &path, file_source, line as usize, character as usize, |p| {
            self.documents.get(p).map(|document| document.uri.clone()).or_else(|| path_to_uri(p))
        });
        Ok(def.unwrap_or(Value::Null))
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
                if let Some(path) = folder.get("uri").and_then(Value::as_str).and_then(uri_to_path)
                    && !self.workspace_roots.contains(&path) && self.workspace_roots.len() < MAX_WORKSPACE_ROOTS {
                        self.workspace_roots.push(path);
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
                let mut load_diagnostics = HashMap::new();
                for (path, document) in &self.documents {
                    if self.scope_for(path) == *scope && document.text.is_none() {
                        let message = format!("document exceeds the {MAX_SOURCE_BYTES}-byte source limit");
                        load_diagnostics.insert(
                            document.uri.clone(),
                            vec![validation_unavailable_diagnostic(message)],
                        );
                    }
                }
                let manifest_path = scope.root.join("Nacre.toml");
                let legacy_path = scope.root.join("nacre.json");
                let config_path = if manifest_path.exists() || self.documents.contains_key(&manifest_path) {
                    Some(manifest_path)
                } else if legacy_path.exists() || self.documents.contains_key(&legacy_path) {
                    Some(legacy_path)
                } else {
                    None
                };
                if let Some(config_path) = config_path
                    && !self.documents.get(&config_path).is_some_and(|document| document.text.is_none()) {
                        let uri = self.documents.get(&config_path).map(|document| document.uri.clone())
                            .or_else(|| path_to_uri(&config_path));
                        if let Some(uri) = uri {
                            load_diagnostics.insert(uri, vec![validation_unavailable_diagnostic(message)]);
                        }
                }
                self.publish_scope_diagnostics(scope, output, excluded_uri, load_diagnostics, None)?;
                return Ok(());
            }
        };
        // SourceBundle owns the text; keep only the file metadata for publishing.
        let source_list = files.iter_mut().map(|file| {
            (file.path.clone(), std::mem::take(&mut file.text))
        }).collect();
        let bundle = match SourceBundle::from_path_sources(source_list) {
            Ok(bundle) => bundle,
            Err(message) => {
                self.log(output, 2, &format!("Nacre validation unavailable for {}: {message}", scope.root.display()))?;
                self.publish_scope_diagnostics(scope, output, excluded_uri, HashMap::new(), Some(&files))?;
                return Ok(());
            }
        };
        let mut diagnostics = HashMap::new();
        if let Err(diagnostic) = bundle.validate()
            && let Some((uri, value)) = diagnostic_json(&diagnostic, &bundle, |path| {
                files.iter().find(|file| file.path.as_path() == path).map(|file| file.uri.as_str())
            }) {
                diagnostics.insert(uri, vec![value]);
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
        for (path, document) in &self.documents {
            if self.scope_for(path) == *scope {
                targets.entry(document.uri.clone()).or_insert(Some(document.version));
            }
        }
        for uri in diagnostics.keys() {
            if !targets.contains_key(uri) {
                targets.insert(uri.clone(), None);
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
            let manifest_path = scope.root.join("Nacre.toml");
            let legacy_path = scope.root.join("nacre.json");
            let manifest_document = self.documents.get(&manifest_path);
            let legacy_document = self.documents.get(&legacy_path);
            if manifest_path.exists() || manifest_document.is_some() {
                if manifest_document.is_some_and(|document| document.text.is_none()) {
                    return Err(format!("{} exceeds the {MAX_SOURCE_BYTES}-byte project configuration limit", manifest_path.display()));
                }
                configured_source_roots(
                    &scope.root,
                    manifest_document.and_then(|document| document.text.as_deref()),
                    None,
                )?
            } else {
                if legacy_document.is_some_and(|document| document.text.is_none()) {
                    return Err(format!("{} exceeds the {MAX_SOURCE_BYTES}-byte project configuration limit", legacy_path.display()));
                }
                configured_source_roots(
                    &scope.root,
                    None,
                    legacy_document.and_then(|document| document.text.as_deref()),
                )?
            }
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
            if is_source_file(path) && self.scope_for(path) == *scope
                && let Some(input) = inputs.iter().find(|input| source_root_contains(input, path)) {
                    reject_symlink_path(path, input)?;
                    if document.text.is_none() {
                        return Err(format!("{} exceeds the {MAX_SOURCE_BYTES}-byte per-project source limit", path.display()));
                    }
                    paths.push(path.clone());
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
                path_to_uri(&path).ok_or_else(|| format!("{} cannot be represented as a local file URI", path.display()))?
            };
            let source_bytes = text.len();
            files.push(SourceFile {
                path, uri,
                version: document.map(|open| open.version),
                text,
            });
            combined_bytes += source_bytes + 1;
        }
        if files.is_empty() { return Err("no Nacre source files in project scope".to_owned()); }
        Ok(files)
    }
    fn load_scope_bundle(&self, scope: &ProjectScope) -> Result<SourceBundle, String> {
        let files = self.load_scope_files(scope)?;
        let sources = files.into_iter().map(|file| (file.path, file.text)).collect();
        SourceBundle::from_path_sources(sources)
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
            let manifest = dir.join("Nacre.toml");
            let legacy = dir.join("nacre.json");
            if manifest.is_file() || legacy.is_file()
                || self.documents.contains_key(&manifest) || self.documents.contains_key(&legacy) {
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
            if let Some(path) = folder.get("uri").and_then(Value::as_str).and_then(uri_to_path)
                && !roots.contains(&path) && roots.len() < MAX_WORKSPACE_ROOTS { roots.push(path); }
        }
    }
    if roots.is_empty() {
        if let Some(path) = params.get("rootUri").and_then(Value::as_str).and_then(uri_to_path) {
            roots.push(path);
        } else if let Some(root) = params.get("rootPath").and_then(Value::as_str)
            && let Some(path) = normalize_path(Path::new(root))
        {
            roots.push(path);
        }
    }
    roots
}

fn bounded_text(text: &str) -> Option<String> {
    (text.len() <= MAX_SOURCE_BYTES).then(|| text.to_owned())
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
