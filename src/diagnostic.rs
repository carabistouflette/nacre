//! Source locations are byte offsets; diagnostics display character columns.
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn join(self, other: Self) -> Self {
        Self::new(self.start, other.end)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
    pub notes: Vec<(Span, String)>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self { span, message: message.into(), notes: Vec::new(), help: None }
    }

    pub fn with_note(mut self, span: Span, message: impl Into<String>) -> Self {
        self.notes.push((span, message.into())); self
    }
    pub fn with_help(mut self, message: impl Into<String>) -> Self {
        self.help = Some(message.into()); self
    }
    pub fn render(&self, filename: &str, source: &str) -> String {
        let mut result = self.render_primary(filename, source);
        for (span, note) in &self.notes {
            let (line, column) = line_column(source, span.start);
            result.push_str(&format!("  = note: {note} ({filename}:{line}:{column})\n"));
        }
        if let Some(help) = &self.help { result.push_str(&format!("  = help: {help}\n")); }
        result
    }
    fn render_primary(&self, filename: &str, source: &str) -> String {
        let start = char_boundary(source, self.span.start);
        let (line, column) = line_column(source, start);
        let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source[start..].find('\n').map_or(source.len(), |i| start + i);
        let text = &source[line_start..line_end];
        let indent = source[line_start..start].chars().map(|c| if c == '\t' { 4 } else { 1 }).sum::<usize>();
        let end = char_boundary(source, self.span.end.min(line_end)).max(start);
        let width = source[start..end].chars().count().clamp(1, 60);
        format!("{filename}:{line}:{column}: error: {}\n  |\n{line:>3} | {}\n  | {}{}\n",
            self.message, text.replace('\t', "    "), " ".repeat(indent), "^".repeat(width))
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Diagnostic {}

pub fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..char_boundary(source, offset)];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

/// Public diagnostics accept byte offsets; round malformed UTF-8 boundaries
/// down instead of panicking while reporting another error.
pub(crate) fn char_boundary(source: &str, offset: usize) -> usize {
    let mut at = offset.min(source.len());
    while !source.is_char_boundary(at) { at -= 1; }
    at
}
