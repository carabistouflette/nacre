//! Independent parsing for source units followed by whole-program module resolution.
//! This is whole-program compilation, not separate package linking.
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use crate::ast;
use crate::diagnostic::{line_column, Diagnostic, Span};

pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_FILES: usize = 64;
#[derive(Debug)]
struct Unit { path: PathBuf, start: usize, end: usize, first_line: usize }
#[derive(Debug)]
pub struct SourceBundle { text: String, units: Vec<Unit> }
pub struct SourceLocation<'a> {
    pub path: &'a Path,
    pub source: &'a str,
    pub span: Span,
}

impl SourceBundle {
    /// Source paths are diagnostic identities; `module` headers define namespaces.
    pub fn from_sources(sources: Vec<(String, String)>) -> Result<Self, String> {
        let mut names = HashSet::new();
        Self::from_units(sources.into_iter().map(|(name, source)| {
            if !names.insert(name.clone()) { return Err(format!("duplicate source: {name}")); }
            Ok((PathBuf::from(name), source))
        }))
    }
    /// Recursively collect .ncr files, sorted within each requested directory.
    /// Symlinks are rejected; hidden directories, target and out are excluded.
    pub fn from_paths(paths: &[PathBuf]) -> Result<Self, String> {
        let mut files = Vec::new();
        for path in paths { collect(path, 0, &mut files)?; }
        let mut seen = HashSet::new();
        let mut remaining = MAX_SOURCE_BYTES;
        Self::from_units(files.into_iter().map(|path| {
            let canonical = path.canonicalize().map_err(|e| format!("{}: {e}", path.display()))?;
            if !seen.insert(canonical) { return Err(format!("duplicate source: {}", path.display())); }
            let mut data = Vec::new();
            fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?
                .take(remaining as u64 + 1).read_to_end(&mut data)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if data.len() >= remaining { return Err("combined source exceeds the 1 MiB limit".to_owned()); }
            remaining -= data.len() + 1;
            let text = String::from_utf8(data).map_err(|e| format!("{}: expected UTF-8 ({e})", path.display()))?;
            Ok((path, text))
        }))
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

    pub(crate) fn parse(&self) -> Result<ast::Program, Diagnostic> {
        let mut program = ast::Program { units: Vec::new(), functions: Vec::new(), classes: Vec::new() };
        for source_unit in &self.units {
            let source = &self.text[source_unit.start..source_unit.end];
            let mut tokens = crate::lexer::lex(source).map_err(|mut error| {
                error.span.start += source_unit.start; error.span.end += source_unit.start; error
            })?;
            for token in &mut tokens {
                token.span.start += source_unit.start; token.span.end += source_unit.start;
            }
            let mut parsed = crate::parser::parse(tokens)?;
            let unit_id = program.units.len();
            for function in &mut parsed.functions { function.unit = unit_id; }
            for class in &mut parsed.classes {
                class.unit = unit_id;
                for method in &mut class.methods { method.unit = unit_id; }
                if let Some(constructor) = &mut class.constructor { constructor.unit = unit_id; }
            }
            program.units.append(&mut parsed.units);
            program.functions.extend(parsed.functions);
            program.classes.extend(parsed.classes);
        }
        Ok(program)
    }
    pub fn typed(&self) -> Result<crate::ir::Program, Diagnostic> { crate::check_parsed(self.parse()?) }
    pub fn validate(&self) -> Result<(), Diagnostic> { self.typed().map(|_| ()) }
    pub fn compile_to_c(&self) -> Result<String, Diagnostic> {
        let typed = self.typed()?;
        let mut result = self.runtime_source_map();
        result.push_str(&crate::codegen::generate(&typed, &self.text));
        Ok(result)
    }
    pub fn emit_flow(&self) -> Result<String, Diagnostic> {
        crate::ownership::dump(&self.typed()?, &self.text)
    }
    pub fn emit_borrows(&self) -> Result<String, Diagnostic> {
        crate::borrowing::dump(&self.typed()?, &self.text)
    }
    pub fn emit_types(&self) -> Result<String, Diagnostic> { Ok(crate::type_report::dump(&self.typed()?)) }

    fn unit_at(&self, offset: usize) -> &Unit {
        self.units.iter().find(|u| offset >= u.start && offset <= u.end)
            .unwrap_or_else(|| self.units.last().expect("nonempty source bundle"))
    }
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let location = self.locate(diagnostic.span);
        let mut result = diagnostic.render_primary(
            &location.path.to_string_lossy(), location.source, location.span,
        );
        for (span, note) in &diagnostic.notes {
            let location = self.locate(*span);
            if location.span.end > location.span.start && location.span.end <= location.source.len() {
                result.push_str(&diagnostic.render_secondary(
                    &location.path.to_string_lossy(), location.source, location.span, note,
                ));
            } else {
                let (line, column) = line_column(location.source, location.span.start);
                result.push_str(&format!("  = note: {note} ({}:{line}:{column})\n", location.path.display()));
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
    /// A small source table in generated C keeps runtime errors file-local too.
    fn runtime_source_map(&self) -> String {
        if self.units.len() == 1 { return String::new(); }
        let mut out = String::from("#include <stddef.h>\n#define NACRE_MULTI_SOURCE 1\nstatic const char *ncr_source_file(size_t *line) {\n");
        for unit in self.units.iter().rev() {
            out.push_str(&format!("    if (*line >= {}) {{ *line -= {}; return {}; }}\n",
                unit.first_line, unit.first_line - 1, c_string(&unit.path.to_string_lossy())));
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

fn collect(path: &Path, depth: usize, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 32 { return Err("source directory nesting limit is 32".to_owned()); }
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.file_type().is_symlink() { return Err(format!("symbolic-link source is not allowed: {}", path.display())); }
    if meta.is_file() {
        output.push(path.to_owned());
        if output.len() > MAX_FILES { return Err("source file limit is 64".to_owned()); }
    } else if meta.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?
            .map(|r| r.map(|e| e.path())).collect::<Result<_, _>>()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        entries.sort();
        for entry in entries {
            if is_excluded_name(entry.file_name().unwrap_or_default()) { continue; }
            let kind = fs::symlink_metadata(&entry).map_err(|e| format!("{}: {e}", entry.display()))?;
            if kind.is_dir() || is_source_file(&entry) {
                collect(&entry, depth + 1, output)?;
            }
        }
    } else { return Err(format!("not a regular source file: {}", path.display())); }
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

/// Create a fresh project, never overwrite an existing path.
pub fn init(path: &Path) -> Result<(), String> {
    fs::create_dir(path).map_err(|e| format!("unable to create {} (the path must be new): {e}", path.display()))?;
    let write = || -> std::io::Result<()> {
        fs::create_dir(path.join("src"))?;
        fs::write(path.join("src/Main.ncr"), include_str!("../examples/language/modules/Main.ncr"))?;
        fs::write(path.join("src/Math.ncr"), include_str!("../examples/language/modules/Math.ncr"))?;
        fs::write(path.join("src/Box.ncr"), include_str!("../examples/language/modules/Box.ncr"))?;
        fs::write(path.join("nacre.json"), "{\"sources\":[\"src\"]}\n")?;
        fs::write(path.join(".gitignore"), "out/\n*.ll\n")?;
        fs::write(path.join("README.md"), "# Nacre project\n\n```sh\nnacre check src\nnacre run src\nnacre build src -o out/app\n```\n\nSource units declare modules and import public types or functions. All configured source files are compiled together.\n")?;
        Ok(())
    };
    write().map_err(|e| format!("partial project creation at {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn c_filename_is_escaped() { assert_eq!(c_string("a\"\\\n"), "\"a\\\"\\\\\\012\""); }
}
