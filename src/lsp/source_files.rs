//! Project configuration and bounded filesystem admission for LSP sources.
//! Document overlays and diagnostic publication remain with the server.
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use serde_json::Value;
use crate::project::{is_excluded_name, is_source_file, MAX_FILES};
use super::file_uri::normalize_path;

const MAX_DIRECTORY_ENTRIES: usize = 4096;

pub(super) fn configured_source_roots(root: &Path) -> Result<Vec<PathBuf>, String> {
    let manifest = root.join("Nacre.toml");
    if manifest.exists() {
        return crate::project::manifest_source_roots(&manifest);
    }
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
pub(super) fn source_root_contains(root: &Path, path: &Path) -> bool {
    if root == path { return true; }
    if !path.starts_with(root) { return false; }
    let root_is_directory = root.is_dir() || (
        !is_source_file(root)
            && matches!(fs::symlink_metadata(root), Err(error) if error.kind() == io::ErrorKind::NotFound)
    );
    root_is_directory && path.strip_prefix(root).is_ok_and(|relative| {
        relative.components().all(|component| !is_excluded_name(component.as_os_str()))
    })
}
pub(super) fn reject_symlink_path(path: &Path, root: &Path) -> Result<(), String> {
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
pub(super) fn collect_recursive(path: &Path, depth: usize, visited: &mut usize, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 32 { return Err("source directory nesting limit is 32".to_owned()); }
    let metadata = fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() { return Err(format!("symbolic-link source is not allowed: {}", path.display())); }
    if metadata.is_file() {
        if is_source_file(path) { push_source(output, path.to_owned())?; }
        return Ok(());
    }
    if !metadata.is_dir() { return Ok(()); }
    for entry in directory_entries(path, visited)? {
        if is_excluded_name(entry.file_name().unwrap_or_default()) { continue; }
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

pub(super) fn read_source(path: &Path, remaining: usize) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(remaining as u64 + 1).read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() > remaining { return Err(format!("{} exceeds the project source size limit", path.display())); }
    String::from_utf8(bytes).map_err(|error| format!("{}: UTF-8 required ({error})", path.display()))
}
