//! Source locations are byte offsets. Source columns count Unicode scalars and
//! expand tabs to four spaces; snippet carets use Unicode display width.
use std::fmt;
use unicode_width::UnicodeWidthStr;

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
        let mut result = self.render_primary(filename, source, self.span);
        for (span, note) in &self.notes {
            if span.end > span.start && span.end <= source.len() {
                result.push_str(&self.render_secondary(filename, source, *span, note));
            } else {
                let (line, column) = line_column(source, span.start);
                result.push_str(&format!("  = note: {note} ({filename}:{line}:{column})\n"));
            }
        }
        if let Some(help) = &self.help { result.push_str(&format!("  = help: {help}\n")); }
        result
    }
    /// Render a secondary note with visual span diagram and indicator.
    pub(crate) fn render_secondary(&self, filename: &str, source: &str, span: Span, note: &str) -> String {
        let start = char_boundary(source, span.start);
        let (line, column) = line_column(source, start);
        let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source[start..].find('\n').map_or(source.len(), |i| start + i);
        let text = &source[line_start..line_end];
        let indent = display_columns(&source[line_start..start]);
        let end = char_boundary(source, span.end.min(line_end)).max(start);
        let width = display_columns(&source[start..end]).clamp(1, 60);
        format!("  ::: ({filename}:{line}:{column})\n  |\n{line:>3} | {}\n  | {}{} note: {note}\n",
            render_source_line(text), " ".repeat(indent), "-".repeat(width))
    }
    /// Render the primary error using a source-local span; notes and help are separate.
    pub(crate) fn render_primary(&self, filename: &str, source: &str, span: Span) -> String {
        let start = char_boundary(source, span.start);
        let (line, column) = line_column(source, start);
        let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source[start..].find('\n').map_or(source.len(), |i| start + i);
        let text = &source[line_start..line_end];
        let indent = display_columns(&source[line_start..start]);
        let end = char_boundary(source, span.end.min(line_end)).max(start);
        let width = display_columns(&source[start..end]).clamp(1, 60);
        format!("{filename}:{line}:{column}: error: {}\n  |\n{line:>3} | {}\n  | {}{}\n",
            self.message, render_source_line(text), " ".repeat(indent), "^".repeat(width))
    }
}

fn render_source_line(text: &str) -> String {
    let text = text.strip_suffix('\r').unwrap_or(text);
    let mut rendered = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\t' => rendered.push_str("    "),
            character if character.is_control() => rendered.push('�'),
            character => rendered.push(character),
        }
    }
    rendered
}

fn source_columns(text: &str) -> usize {
    text.strip_suffix('\r')
        .unwrap_or(text)
        .chars()
        .map(|character| if character == '\t' { 4 } else { 1 })
        .sum()
}

fn display_columns(text: &str) -> usize {
    let text = text.strip_suffix('\r').unwrap_or(text);
    let mut columns = 0;
    let mut segment_start = 0;
    for (index, character) in text.char_indices() {
        if character == '\t' || character.is_control() {
            columns += text[segment_start..index].width();
            columns += if character == '\t' { 4 } else { 1 };
            segment_start = index + character.len_utf8();
        }
    }
    columns + text[segment_start..].width()
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
    let column = source_columns(prefix.rsplit('\n').next().unwrap_or("")) + 1;
    (line, column)
}

/// Public diagnostics accept byte offsets; round malformed UTF-8 boundaries
/// down instead of panicking while reporting another error.
pub(crate) fn char_boundary(source: &str, offset: usize) -> usize {
    let mut at = offset.min(source.len());
    while !source.is_char_boundary(at) { at -= 1; }
    at
}
