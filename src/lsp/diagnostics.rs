//! Compiler diagnostics rendered as LSP JSON without server or document state.
use std::path::Path;
use serde_json::{json, Value};
use crate::diagnostic::{char_boundary, Diagnostic, Span};
use crate::project::SourceBundle;

/// Resolve borrowed URIs by source identity; omit unmapped primary errors or notes.
pub(super) fn diagnostic_json<'a>(
    diagnostic: &Diagnostic,
    bundle: &'a SourceBundle,
    uri_for_path: impl Fn(&Path) -> Option<&'a str>,
) -> Option<(String, Value)> {
    let primary = map_span(diagnostic.span, bundle, &uri_for_path)?;
    let mut related = Vec::new();
    for (span, message) in &diagnostic.notes {
        if let Some(location) = map_span(*span, bundle, &uri_for_path) {
            related.push(json!({
                "location": {
                    "uri": location.uri,
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
    Some((primary.uri.to_owned(), value))
}

struct LocalSpan<'a> {
    uri: &'a str,
    source: &'a str,
    start: usize,
    end: usize,
}

fn map_span<'a>(
    span: Span,
    bundle: &'a SourceBundle,
    uri_for_path: &impl Fn(&Path) -> Option<&'a str>,
) -> Option<LocalSpan<'a>> {
    let location = bundle.locate(span);
    let uri = uri_for_path(location.path)?;
    let start = char_boundary(location.source, location.span.start);
    let end = char_boundary(location.source, location.span.end).max(start);
    Some(LocalSpan { uri, source: location.source, start, end })
}

pub(super) fn range_json(source: &str, start: usize, end: usize) -> Value {
    let start = offset_position(source, start);
    let end = offset_position(source, end.max(start.2));
    json!({
        "start": { "line": start.0, "character": start.1 },
        "end": { "line": end.0, "character": end.1 }
    })
}

fn line_ending_end(bytes: &[u8], offset: usize) -> Option<usize> {
    match bytes.get(offset).copied() {
        Some(b'\r') if bytes.get(offset + 1) == Some(&b'\n') => Some(offset + 2),
        Some(b'\r' | b'\n') => Some(offset + 1),
        _ => None,
    }
}

pub(super) fn offset_position(source: &str, offset: usize) -> (usize, usize, usize) {
    let offset = char_boundary(source, offset);
    let bytes = source.as_bytes();
    let mut line = 0;
    let mut line_start = 0;
    let mut index = 0;
    while index < offset {
        if let Some(next_line_start) = line_ending_end(bytes, index) {
            if next_line_start > offset { break; }
            line += 1;
            line_start = next_line_start;
            index = next_line_start;
        } else {
            index += 1;
        }
    }
    let mut line_end = line_start;
    while line_end < bytes.len() && line_ending_end(bytes, line_end).is_none() {
        line_end += 1;
    }
    let prefix_end = offset.min(line_end);
    (line, source[line_start..prefix_end].encode_utf16().count(), offset)
}

pub(super) fn position_to_offset(source: &str, line: usize, character: usize) -> usize {
    let bytes = source.as_bytes();
    let mut current_line = 0;
    let mut line_start = 0;
    let mut index = 0;
    while current_line < line {
        if index >= bytes.len() { return source.len(); }
        if let Some(next_line_start) = line_ending_end(bytes, index) {
            current_line += 1;
            line_start = next_line_start;
            index = next_line_start;
        } else {
            index += 1;
        }
    }
    let mut line_end = line_start;
    while line_end < bytes.len() && line_ending_end(bytes, line_end).is_none() {
        line_end += 1;
    }
    let line_slice = &source[line_start..line_end];
    let mut utf16_count = 0;
    let mut byte_offset = 0;
    for ch in line_slice.chars() {
        if utf16_count >= character { break; }
        utf16_count += ch.len_utf16();
        byte_offset += ch.len_utf8();
    }
    char_boundary(source, line_start + byte_offset)
}

pub(super) fn word_at_offset(source: &str, offset: usize) -> Option<(&str, usize, usize)> {
    if source.is_empty() { return None; }
    let offset = char_boundary(source, offset.min(source.len()));
    let bytes = source.as_bytes();
    if offset >= bytes.len() {
        return None;
    }
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut start = offset;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    let word = std::str::from_utf8(&bytes[start..end]).ok()?;
    Some((word, start, end))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_recognize_lf_crlf_and_cr() {
        for (source, line_start) in [("a\nb", 2), ("a\r\nb", 3), ("a\rb", 2)] {
            assert_eq!(position_to_offset(source, 1, 0), line_start);
            assert_eq!(position_to_offset(source, 1, 1), line_start + 1);
            assert_eq!(offset_position(source, line_start), (1, 0, line_start));
        }
        assert_eq!(offset_position("a\rb", 1), (0, 1, 1));
    }

    #[test]
    fn ranges_use_utf16_and_clamp_utf8_crlf_and_reversed_offsets() {
        let source = "a😀\r\néz";
        for (offset, line, character) in [
            (0,0,0), (1,0,1), (2,0,1), (4,0,1), (5,0,3), (6,0,3),
            (7,1,0), (8,1,0), (9,1,1), (10,1,2), (usize::MAX,1,2),
        ] {
            assert_eq!(range_json(source, offset, offset), json!({
                "start": { "line": line, "character": character },
                "end": { "line": line, "character": character },
            }));
        }
        assert_eq!(range_json(source, 9, 1), json!({
            "start": { "line": 1, "character": 1 },
            "end": { "line": 1, "character": 1 },
        }));
    }

    #[test]
    fn cross_file_notes_keep_local_ranges_and_require_resolvable_uris() {
        let first = "a😀\r\néz";
        let second_start = first.len() + 1;
        let bundle = SourceBundle::from_sources(vec![
            ("First.ncr".to_owned(), first.to_owned()),
            ("Second.ncr".to_owned(), "βx".to_owned()),
        ]).unwrap();
        let diagnostic = Diagnostic::new(Span::new(second_start + 2, second_start + 3), "primary")
            .with_note(Span::new(1, 5), "related");
        let uris = |path: &Path| match path.to_str() {
            Some("First.ncr") => Some("file:///First.ncr"),
            Some("Second.ncr") => Some("file:///Second.ncr"),
            _ => None,
        };
        let (_, value) = diagnostic_json(&diagnostic, &bundle, uris).unwrap();
        assert_eq!(value["range"], json!({
            "start": { "line": 0, "character": 1 },
            "end": { "line": 0, "character": 2 },
        }));
        assert_eq!(value["relatedInformation"][0]["location"]["range"], json!({
            "start": { "line": 0, "character": 1 },
            "end": { "line": 0, "character": 3 },
        }));
        let (_, value) = diagnostic_json(&diagnostic, &bundle, |path| {
            if path == Path::new("Second.ncr") { Some("file:///Second.ncr") } else { None }
        }).unwrap();
        assert!(value.get("relatedInformation").is_none());
        assert!(diagnostic_json(&diagnostic, &bundle, |_| None).is_none());
    }
}
