//! Independent parsing for source units followed by whole-program module resolution.
//! This is whole-program compilation, not separate package linking.
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use crate::ast;
use crate::diagnostic::{line_column, Diagnostic, Span};

pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_FILES: usize = 64;
/// Maximum number of directory entries inspected during source discovery.
pub const MAX_DIRECTORY_ENTRIES: usize = 4096;
#[derive(Debug)]
struct Unit { path: PathBuf, start: usize, end: usize, first_line: usize }
#[derive(Debug)]
pub struct SourceBundle { text: String, units: Vec<Unit> }
pub struct SourceLocation<'a> {
    pub path: &'a Path,
    pub source: &'a str,
    pub span: Span,
}

fn display_path(path: &Path) -> Cow<'_, str> {
    match path.to_str() {
        Some(path) => Cow::Borrowed(path),
        None => Cow::Owned(format!("{path:?}")),
    }
}

impl SourceBundle {
    /// Source paths are diagnostic identities; `module` headers define namespaces.
    pub fn from_sources(sources: Vec<(String, String)>) -> Result<Self, String> {
        Self::from_unique_sources(sources.into_iter().map(|(name, source)| {
            (PathBuf::from(name), source)
        }))
    }
    pub(crate) fn from_path_sources(sources: Vec<(PathBuf, String)>) -> Result<Self, String> {
        Self::from_unique_sources(sources.into_iter())
    }
    fn from_unique_sources(
        sources: impl ExactSizeIterator<Item = (PathBuf, String)>,
    ) -> Result<Self, String> {
        let mut names = HashSet::new();
        Self::from_units(sources.map(|(path, source)| {
            if !names.insert(path.clone()) {
                return Err(format!("duplicate source: {}", display_path(&path)));
            }
            Ok((path, source))
        }))
    }
    /// Resolve manifests and recursively collect .ncr files in deterministic order.
    /// Symlinks are rejected; hidden directories, target and out are excluded.
    pub fn from_paths(paths: &[PathBuf]) -> Result<Self, String> {
        let files = resolve_inputs(paths)?;
        let mut remaining = MAX_SOURCE_BYTES;
        Self::from_units(files.into_iter().map(|path| {
            let mut data = Vec::new();
            fs::File::open(&path).map_err(|e| format!("{}: {e}", display_path(&path)))?
                .take(remaining as u64 + 1).read_to_end(&mut data)
                .map_err(|e| format!("{}: {e}", display_path(&path)))?;
            if data.len() >= remaining { return Err("combined source exceeds the 1 MiB limit".to_owned()); }
            remaining -= data.len() + 1;
            let text = String::from_utf8(data).map_err(|e| format!("{}: expected UTF-8 ({e})", display_path(&path)))?;
            Ok((path, text))
        }))
    }

    /// Loads source units for test harnesses, verifying if `main` exists at AST level.
    /// If no entry point exists, injects a synthetic `void main() {}` while strictly enforcing limits.
    pub(crate) fn from_test_paths(paths: &[PathBuf]) -> Result<(Self, ast::Program), String> {
        let mut bundle = Self::from_paths(paths)?;
        let mut parsed = bundle.parse().map_err(|d| bundle.render(&d))?;
        let has_main = parsed.functions.iter().any(|f| f.name == "main")
            || parsed.classes.iter().any(|c| c.methods.iter().any(|m| m.is_static && m.name == "main"));
        if !has_main {
            if bundle.units.len() >= MAX_FILES {
                return Err("source file limit is 64".to_owned());
            }
            let dummy = "void main() {}\n";
            if bundle.text.len().saturating_add(dummy.len()) > MAX_SOURCE_BYTES {
                return Err("combined source exceeds the 1 MiB limit".to_owned());
            }
            let start = bundle.text.len();
            bundle.text.push_str(dummy);
            let end = bundle.text.len();
            bundle.units.push(Unit {
                path: PathBuf::from("__ncr_synthetic_main.ncr"),
                start,
                end,
                first_line: bundle.text.bytes().filter(|&b| b == b'\n').count() + 1,
            });
            Self::append_parsed_unit(&mut parsed, &bundle.text[start..end], start)
                .map_err(|d| bundle.render(&d))?;
        }
        Ok((bundle, parsed))
    }

    fn from_units(
        sources: impl ExactSizeIterator<Item = Result<(PathBuf, String), String>>,
    ) -> Result<Self, String> {
        if sources.len() == 0 { return Err("no source files".to_owned()); }
        if sources.len() > MAX_FILES { return Err("source file limit is 64".to_owned()); }
        let mut bundle = Self { text: String::new(), units: Vec::new() };
        let mut first_line = 1;
        for source in sources {
            let (path, source) = source?;
            if source.len().saturating_add(bundle.text.len()).saturating_add(1) > MAX_SOURCE_BYTES {
                return Err("combined source exceeds the 1 MiB limit".to_owned());
            }
            let start = bundle.text.len();
            bundle.text.push_str(&source);
            let end = bundle.text.len();
            bundle.text.push('\n');
            bundle.units.push(Unit { path, start, end, first_line });
            first_line += source.bytes().filter(|&b| b == b'\n').count() + 1;
        }
        Ok(bundle)
    }
    pub fn paths(&self) -> impl Iterator<Item = &Path> { self.units.iter().map(|u| u.path.as_path()) }
    pub fn source_count(&self) -> usize { self.units.len() }
    pub fn combined_source(&self) -> &str { &self.text }
    /// Borrow the text for an exact source path in this bundle.
    pub(crate) fn source_for_path(&self, path: &Path) -> Option<&str> {
        let unit = self.units.iter().find(|unit| unit.path.as_path() == path)?;
        Some(&self.text[unit.start..unit.end])
    }

    fn append_parsed_unit(
        program: &mut ast::Program,
        source: &str,
        source_start: usize,
    ) -> Result<(), Diagnostic> {
        let mut tokens = crate::lexer::lex(source).map_err(|mut error| {
            error.span.start += source_start;
            error.span.end += source_start;
            error
        })?;
        for token in &mut tokens {
            token.span.start += source_start;
            token.span.end += source_start;
        }
        let mut parsed = crate::parser::parse(tokens)?;
        let unit_id = program.units.len();
        for function in &mut parsed.functions { function.unit = unit_id; }
        for class in &mut parsed.classes {
            class.unit = unit_id;
            for method in &mut class.methods { method.unit = unit_id; }
            if let Some(constructor) = &mut class.constructor { constructor.unit = unit_id; }
        }
        for tr in &mut parsed.traits { tr.unit = unit_id; }
        for imp in &mut parsed.impls { imp.unit = unit_id; }
        program.units.append(&mut parsed.units);
        program.functions.extend(parsed.functions);
        program.classes.extend(parsed.classes);
        program.traits.extend(parsed.traits);
        program.impls.extend(parsed.impls);
        Ok(())
    }
    pub(crate) fn parse(&self) -> Result<ast::Program, Diagnostic> {
        let mut program = ast::Program { units: Vec::new(), functions: Vec::new(), classes: Vec::new(), traits: Vec::new(), impls: Vec::new() };
        for source_unit in &self.units {
            Self::append_parsed_unit(
                &mut program,
                &self.text[source_unit.start..source_unit.end],
                source_unit.start,
            )?;
        }
        Ok(program)
    }
    pub fn typed(&self) -> Result<crate::ir::Program, Diagnostic> { crate::check_parsed(self.parse()?) }
    pub fn validate(&self) -> Result<(), Diagnostic> { self.typed().map(|_| ()) }
    pub fn compile_to_c(&self) -> Result<String, Diagnostic> {
        let typed = self.typed()?;
        Ok(self.compile_to_c_with_typed(&typed))
    }
    /// Emit C from a typed program already checked against this bundle.
    pub(crate) fn compile_to_c_with_typed(&self, typed: &crate::ir::Program) -> String {
        crate::codegen::generate_mapped(
            typed,
            &self.text,
            self.runtime_source_map(),
            |span| {
                let loc = self.locate(span);
                let (line, _) = crate::diagnostic::line_column(loc.source, loc.span.start);
                (line, Some(display_path(loc.path).into_owned()))
            },
        )
    }
    pub fn emit_llvm_direct(&self) -> Result<String, Diagnostic> {
        let typed = self.typed()?;
        crate::llvm_codegen::emit_llvm_direct_artifact_with_sources(
            &typed,
            &self.text,
            self.direct_debug_sources(),
            false,
        )
        .map(|artifact| artifact.ir)
        .map_err(|e| Diagnostic::new(Span::default(), e))
    }
    /// Compile this complete source bundle through the direct LLVM backend.
    pub fn compile_llvm_direct(&self) -> Result<crate::native::NativeProgram, String> {
        self.compile_llvm_direct_with_flags(&[])
    }
    /// Compile with additional trusted native compiler flags.
    pub fn compile_llvm_direct_with_flags(&self, flags: &[&str]) -> Result<crate::native::NativeProgram, String> {
        self.compile_llvm_direct_optimized_with_flags("-O2", flags)
    }
    /// Compile direct LLVM IR and its stdlib-only runtime at an explicit optimization level.
    pub fn compile_llvm_direct_optimized_with_flags(
        &self,
        optimization: &str,
        flags: &[&str],
    ) -> Result<crate::native::NativeProgram, String> {
        let typed = self.typed().map_err(|diagnostic| self.render(&diagnostic))?;
        let mut artifact = crate::llvm_codegen::emit_llvm_direct_artifact_with_sources(
            &typed,
            &self.text,
            self.direct_debug_sources(),
            optimization != "-O0",
        )
        .map_err(|error| self.render(&Diagnostic::new(Span::default(), error)))?;
        if self.units.len() > 1 {
            artifact.runtime_c.insert_str(0, &self.runtime_source_map());
        }
        crate::native::compile_llvm_direct_with_runtime(
            &artifact.ir,
            &artifact.runtime_c,
            optimization,
            flags,
        )
    }
    pub fn emit_flow(&self) -> Result<String, Diagnostic> {
        crate::ownership::dump(&self.typed()?, &self.text)
    }
    pub fn emit_borrows(&self) -> Result<String, Diagnostic> {
        crate::borrowing::dump(&self.typed()?, &self.text)
    }
    pub fn emit_types(&self) -> Result<String, Diagnostic> { Ok(crate::type_report::dump(&self.typed()?)) }

    fn unit_at(&self, offset: usize) -> &Unit {
        let index = self.units.partition_point(|unit| unit.end < offset);
        self.units.get(index)
            .filter(|unit| offset >= unit.start && offset <= unit.end)
            .unwrap_or_else(|| self.units.last().expect("nonempty source bundle"))
    }
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let location = self.locate(diagnostic.span);
        let path = display_path(location.path);
        let mut result = diagnostic.render_primary(
            &path, location.source, location.span,
        );
        for (span, note) in &diagnostic.notes {
            let location = self.locate(*span);
            if location.span.end > location.span.start && location.span.end <= location.source.len() {
                let path = display_path(location.path);
                result.push_str(&diagnostic.render_secondary(
                    &path, location.source, location.span, note,
                ));
            } else {
                let (line, column) = line_column(location.source, location.span.start);
                result.push_str(&format!("  = note: {note} ({}:{line}:{column})\n", display_path(location.path)));
            }
        }
        if let Some(help) = &diagnostic.help { result.push_str(&format!("  = help: {help}\n")); }
        result
    }
    /// Map a bundle-wide span back to its original file and file-local offsets.
    pub fn locate(&self, span: Span) -> SourceLocation<'_> {
        let unit = self.unit_at(span.start);
        SourceLocation {
            path: &unit.path,
            source: &self.text[unit.start..unit.end],
            span: Span::new(
                span.start.saturating_sub(unit.start),
                span.end.saturating_sub(unit.start),
            ),
        }
    }
    fn direct_debug_sources<'a>(
        &'a self,
    ) -> impl Iterator<Item = crate::llvm_codegen::DirectDebugSource<'a>> + 'a {
        self.units.iter().map(|unit| crate::llvm_codegen::DirectDebugSource {
            path: &unit.path,
            source: &self.text[unit.start..unit.end],
            start: unit.start,
        })
    }
    /// A small source table in generated C keeps runtime errors file-local too.
    fn runtime_source_map(&self) -> String {
        if self.units.len() == 1 { return String::new(); }
        let mut out = String::from("#include <stddef.h>\n#define NACRE_MULTI_SOURCE 1\nstatic const char *ncr_source_file(size_t *line) {\n");
        for unit in self.units.iter().rev() {
            let path = display_path(&unit.path);
            out.push_str(&format!("    if (*line >= {}) {{ *line -= {}; return {}; }}\n",
                unit.first_line, unit.first_line - 1, c_string(&path)));
        }
        out.push_str("    return \"<unknown>\";\n}\n");
        out
    }
}
pub(crate) fn is_source_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "ncr")
}

/// Shared discovery policy for directory entries and unsaved path components.
pub(crate) fn is_excluded_name(name: &std::ffi::OsStr) -> bool {
    name.as_encoded_bytes().starts_with(b".") || name == "out" || name == "target"
}
pub(crate) fn read_directory_entries(
    path: &Path,
    visited: &mut usize,
) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(path).map_err(|error| format!("{}: {error}", display_path(path)))?;
    let mut output = Vec::new();
    for entry in entries {
        *visited += 1;
        if *visited > MAX_DIRECTORY_ENTRIES {
            return Err(format!(
                "directory scan limit is {MAX_DIRECTORY_ENTRIES} entries"
            ));
        }
        let entry = entry.map_err(|error| format!("{}: {error}", display_path(path)))?;
        output.push(entry.path());
    }
    output.sort();
    Ok(output)
}

fn collect(
    path: &Path,
    directory_depth: usize,
    visited: &mut usize,
    output: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if directory_depth > 32 {
        return Err("source directory nesting limit is 32".to_owned());
    }
    let meta =
        fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", display_path(path)))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "symbolic-link source is not allowed: {}",
            display_path(path)
        ));
    }
    if meta.is_file() {
        output.push(path.to_owned());
        if output.len() > MAX_FILES {
            return Err("source file limit is 64".to_owned());
        }
    } else if meta.is_dir() {
        for entry in read_directory_entries(path, visited)? {
            if is_excluded_name(entry.file_name().unwrap_or_default()) {
                continue;
            }
            let kind = fs::symlink_metadata(&entry)
                .map_err(|error| format!("{}: {error}", display_path(&entry)))?;
            if kind.is_dir() {
                collect(&entry, directory_depth + 1, visited, output)?;
            } else if is_source_file(&entry) {
                collect(&entry, directory_depth, visited, output)?;
            }
        }
    } else {
        return Err(format!("not a regular source file: {}", display_path(path)));
    }
    Ok(())
}
fn c_string(text: &str) -> String {
    let mut out = String::from("\"");
    for b in text.as_bytes() {
        match b {
            b'"' => out.push_str("\\\""), b'\\' => out.push_str("\\\\"),
            32..=126 => out.push(*b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out.push('"'); out
}

/// Resolve local package dependencies and workspace members into source files.
/// Packages share the language's module namespace and one entry point.
pub fn resolve_inputs(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut graph = PackageGraph::default();
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut visited_entries = 0usize;
    for path in paths {
        reject_symlink_components(path)?;
        if let Some(manifest) = manifest_path(path) {
            let start = graph.roots.len();
            graph.visit(&manifest, None, 0)?;
            for root in &graph.roots[start..] {
                let mut package_files = Vec::new();
                collect(root, 0, &mut visited_entries, &mut package_files)?;
                for file in package_files {
                    let canonical = file.canonicalize().map_err(|e| format!("{}: {e}", display_path(&file)))?;
                    if seen.insert(canonical) { files.push(file); }
                }
            }
        } else {
            let mut raw_files = Vec::new();
            collect(path, 0, &mut visited_entries, &mut raw_files)?;
            for file in raw_files {
                let canonical = file.canonicalize().map_err(|e| format!("{}: {e}", display_path(&file)))?;
                if !seen.insert(canonical) { return Err(format!("duplicate source: {}", display_path(&file))); }
                files.push(file);
            }
        }
        if files.len() > MAX_FILES { return Err(format!("source file limit is {MAX_FILES}")); }
    }
    Ok(files)
}

fn reject_symlink_components(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                current.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::Normal(_) => {
                current.push(component.as_os_str());
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|error| format!("{}: {error}", display_path(&current)))?;
                if metadata.file_type().is_symlink() {
                    return Err(format!(
                        "symbolic-link input path is not allowed: {}",
                        display_path(&current)
                    ));
                }
            }
        }
    }
    Ok(())
}

fn manifest_path(path: &Path) -> Option<PathBuf> {
    if path.file_name().is_some_and(|name| name == "Nacre.toml") {
        Some(path.to_owned())
    } else if path.is_dir() && path.join("Nacre.toml").exists() {
        Some(path.join("Nacre.toml"))
    } else {
        None
    }
}

/// Includes directory roots so that the editor can admit unsaved source files.
pub(crate) fn manifest_source_roots(manifest: &Path, overlay: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let mut graph = PackageGraph::default();
    graph.visit_with_overlay(manifest, None, 0, overlay)?;
    graph.roots.sort();
    graph.roots.dedup();
    let mut roots = Vec::new();
    let mut directories: Vec<PathBuf> = Vec::new();
    for path in graph.roots {
        if directories.iter().any(|directory| path.starts_with(directory)) { continue; }
        if path.is_dir() { directories.push(path.clone()); }
        roots.push(path);
    }
    Ok(roots)
}

#[derive(Default)]
struct PackageGraph {
    active: HashSet<PathBuf>,
    completed: HashSet<PathBuf>,
    names: HashMap<String, PathBuf>,
    roots: Vec<PathBuf>,
}

impl PackageGraph {
    fn visit(&mut self, path: &Path, expected_name: Option<&str>, depth: usize) -> Result<(), String> {
        self.visit_with_overlay(path, expected_name, depth, None)
    }

    fn visit_with_overlay(
        &mut self,
        path: &Path,
        expected_name: Option<&str>,
        depth: usize,
        overlay: Option<&str>,
    ) -> Result<(), String> {
        if depth > 32 { return Err("package dependency nesting limit is 32".to_owned()); }
        let exists = match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(format!("symbolic-link manifest is not allowed: {}", display_path(path)));
                }
                if overlay.is_some() && !metadata.is_file() {
                    return Err(format!("{}: manifest must be a regular file", display_path(path)));
                }
                true
            }
            Err(error) if overlay.is_some() && error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(format!("{}: {error}", display_path(path))),
        };
        let path = if exists {
            path.canonicalize().map_err(|e| format!("{}: {e}", display_path(path)))?
        } else {
            let parent = path.parent().ok_or("manifest has no parent directory")?;
            let name = path.file_name().ok_or("manifest has no file name")?;
            parent.canonicalize().map_err(|e| format!("{}: {e}", display_path(path)))?.join(name)
        };
        let manifest = match overlay {
            Some(contents) => crate::manifest::Manifest::parse(&path, contents)?,
            None => crate::manifest::Manifest::load(&path)?,
        };
        let root = path.parent().ok_or("manifest has no parent directory")?;
        if let Some(expected) = expected_name {
            let package = manifest.package.as_ref().ok_or_else(|| format!("{}: dependency must contain [package]", display_path(&path)))?;
            if package.name != expected {
                return Err(format!("{}: dependency name `{expected}` does not match package `{}`", display_path(&path), package.name));
            }
        }
        if self.active.contains(&path) {
            return Err(format!("package dependency cycle at {}", display_path(&path)));
        }
        if self.completed.contains(&path) { return Ok(()); }
        if self.completed.len() + self.active.len() >= MAX_FILES {
            return Err(format!("package manifest limit is {MAX_FILES}"));
        }
        if let Some(package) = &manifest.package {
            if let Some(previous) = self.names.get(&package.name) {
                if previous != root {
                    return Err(format!("conflicting package `{}` at {} and {}", package.name, display_path(previous), display_path(root)));
                }
            } else {
                self.names.insert(package.name.clone(), root.to_owned());
            }
        }
        self.active.insert(path.clone());
        for (name, dependency) in &manifest.dependencies {
            let dependency = root.join(&dependency.path);
            let dependency = if dependency.is_dir() { dependency.join("Nacre.toml") } else { dependency };
            self.visit(&dependency, Some(name), depth + 1)?;
        }
        if let Some(workspace) = &manifest.workspace {
            for member in &workspace.members {
                let member = root.join(member);
                let member = if member.is_dir() { member.join("Nacre.toml") } else { member };
                self.visit(&member, None, depth + 1)?;
            }
        }
        if let Some(package) = manifest.package {
            for source in package.sources {
                reject_source_symlinks(root, &source)?;
                self.roots.push(root.join(source));
            }
            if let Some(entry) = package.entry {
                reject_source_symlinks(root, &entry)?;
                let entry = root.join(entry);
                if !is_source_file(&entry) || !entry.is_file() {
                    return Err(format!("{}: package entry must name an existing .ncr file", display_path(&entry)));
                }
                self.roots.push(entry);
            }
        }
        self.active.remove(&path);
        self.completed.insert(path);
        Ok(())
    }
}

fn reject_source_symlinks(root: &Path, relative: &Path) -> Result<(), String> {
    let mut path = root.to_owned();
    for component in relative.components() {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!("symbolic-link source is not allowed: {}", display_path(&path)));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(format!("{}: {error}", display_path(&path))),
        }
    }
    Ok(())
}

/// Create a fresh project, never overwrite an existing path.
pub fn init(path: &Path) -> Result<(), String> {
    // Escape underscores and unsupported path bytes so distinct names cannot collide.
    let basename = path
        .file_name()
        .map(std::ffi::OsStr::as_encoded_bytes)
        .unwrap_or(b"app");
    let mut name = String::with_capacity(basename.len().saturating_mul(4));
    for &byte in basename {
        match byte {
            b'_' => name.push_str("_u"),
            b'-' => name.push('-'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => name.push(char::from(byte)),
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                name.push_str("_x");
                name.push(char::from(HEX[usize::from(byte >> 4)]));
                name.push(char::from(HEX[usize::from(byte & 0x0f)]));
            }
        }
    }
    if !name.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric) {
        name.insert_str(0, "app_");
    }
    fs::create_dir(path).map_err(|e| format!("unable to create {} (the path must be new): {e}", display_path(path)))?;
    // Encode punctuation injectively, and prefix digits/keywords into identifiers.
    let mut module_prefix = String::with_capacity(5 + name.len() * 2);
    module_prefix.push_str("pkg_");
    for byte in name.bytes() {
        match byte {
            b'_' => module_prefix.push_str("__"),
            b'-' => module_prefix.push_str("_h"),
            _ => module_prefix.push(char::from(byte)),
        }
    }
    module_prefix.push('.');
    let write = || -> std::io::Result<()> {
        fs::create_dir(path.join("src"))?;
        for (file, source) in [
            ("Main.ncr", include_str!("../examples/language/modules/Main.ncr")),
            ("Math.ncr", include_str!("../examples/language/modules/Math.ncr")),
            ("Box.ncr", include_str!("../examples/language/modules/Box.ncr")),
        ] {
            fs::write(path.join("src").join(file), source.replace("examples.", &module_prefix))?;
        }
        fs::write(path.join("Nacre.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nsources = [\"src\"]\nentry = \"src/Main.ncr\"\n"))?;
        fs::write(path.join(".gitignore"), "out/\n*.ll\n")?;
        fs::write(path.join("README.md"), "# Nacre project\n\n```sh\nnacre check\nnacre run\nnacre build --release\n```\n\nSource units declare modules and import public types or functions. Nacre.toml selects source roots and local path dependencies; all selected source files are compiled together.\n")?;
        Ok(())
    };
    write().map_err(|e| format!("partial project creation at {}: {e}", display_path(path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn c_filename_is_escaped() { assert_eq!(c_string("a\"\\\n"), "\"a\\\"\\\\\\012\""); }
}
