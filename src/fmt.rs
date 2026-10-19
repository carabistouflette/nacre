//! Deterministic code formatter for Nacre (`nacre fmt`).
//! Formats .ncr source files according to the standard Nacre code style.

use std::fs;
use std::path::{Path, PathBuf};

pub fn format_files(paths: &[PathBuf], check_only: bool) -> Result<i32, String> {
    let mut files = Vec::new();
    for p in paths {
        if p.is_file() {
            if p.extension().is_some_and(|ext| ext == "ncr") {
                files.push(p.clone());
            }
        } else if p.is_dir() {
            collect_ncr_files(p, &mut files)?;
        }
    }

    if files.is_empty() {
        return Err("no .ncr files found to format".to_owned());
    }

    let mut needs_formatting = 0;

    for file in &files {
        let original = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let formatted = format_source(&original)?;

        if formatted != original {
            needs_formatting += 1;
            if check_only {
                println!("Diff: {} needs formatting", file.display());
            } else {
                // Safety check: ensure formatted code still parses
                if let Err(e) = crate::validate(&formatted) {
                    return Err(format!(
                        "formatter safety check failed for {}: formatted code does not validate: {e:?}",
                        file.display()
                    ));
                }
                fs::write(file, &formatted).map_err(|e| format!("{}: {e}", file.display()))?;
                println!("Formatted {}", file.display());
            }
        }
    }

    if check_only {
        if needs_formatting > 0 {
            println!("\n{needs_formatting} file(s) need formatting.");
            Ok(1)
        } else {
            println!("All files are properly formatted.");
            Ok(0)
        }
    } else {
        println!("Checked {} file(s).", files.len());
        Ok(0)
    }
}

fn collect_ncr_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "ncr") {
            files.push(path);
        } else if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "target" && name != "out" {
                collect_ncr_files(&path, files)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    LineComment(String),
    BlockComment(String),
    StringLit(String),
    CharLit(String),
    Number(String),
    Ident(String),
    Keyword(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semi,
    Colon,
    Comma,
    Dot,
    Arrow,
    FatArrow,
    BinaryOp(String),
    UnaryOp(String),
    AssignOp(String),
    Newline,
    BlankLine,
}

pub fn format_source(source: &str) -> Result<String, String> {
    let tokens = tokenize(source)?;
    let mut out = String::with_capacity(source.len());

    let mut indent: usize = 0;
    let mut at_line_start = true;
    let mut paren_depth: usize = 0;
    let mut for_header_paren_depth: Option<usize> = None;
    let mut prev_token: Option<&Token> = None;

    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];

        match tok {
            Token::BlankLine => {
                if !at_line_start {
                    out.push('\n');
                }
                // Avoid multiple blank lines or leading blank lines
                if !out.is_empty() && !out.ends_with("\n\n") {
                    out.push('\n');
                }
                at_line_start = true;
                prev_token = None;
                i += 1;
                continue;
            }
            Token::Newline => {
                if !at_line_start {
                    out.push('\n');
                    at_line_start = true;
                }
                prev_token = None;
                i += 1;
                continue;
            }
            Token::RBrace => {
                indent = indent.saturating_sub(1);
                if at_line_start {
                    push_indent(&mut out, indent);
                } else {
                    out.push(' ');
                }
                out.push('}');
                at_line_start = false;

                // Check if next token is `else` or `;`
                let next = tokens.get(i + 1);
                if let Some(Token::Keyword(kw)) = next {
                    if kw == "else" {
                        out.push(' ');
                        out.push_str("else");
                        prev_token = Some(&tokens[i + 1]);
                        i += 2;
                        if let Some(Token::Keyword(next_kw)) = tokens.get(i)
                            && next_kw == "if" {
                                out.push(' ');
                                out.push_str("if");
                                prev_token = Some(&tokens[i]);
                                i += 1;
                            }
                        continue;
                    }
                } else if let Some(Token::Semi) = next {
                    out.push(';');
                    out.push('\n');
                    at_line_start = true;
                    i += 2;
                    prev_token = None;
                    continue;
                }

                out.push('\n');
                at_line_start = true;
                prev_token = None;
                i += 1;
                continue;
            }
            _ => {}
        }

        if at_line_start {
            push_indent(&mut out, indent);
            at_line_start = false;
        }

        match tok {
            Token::LBrace => {
                if let Some(prev) = prev_token
                    && !matches!(prev, Token::LBrace | Token::Newline) && !out.ends_with(' ') {
                        out.push(' ');
                    }
                out.push('{');
                indent += 1;
                out.push('\n');
                at_line_start = true;
                prev_token = None;
                i += 1;
                continue;
            }
            Token::Semi => {
                out.push(';');
                if for_header_paren_depth.is_some() {
                    out.push(' ');
                } else {
                    out.push('\n');
                    at_line_start = true;
                    prev_token = None;
                    i += 1;
                    continue;
                }
            }
            Token::Comma => {
                out.push(',');
                out.push(' ');
            }
            Token::Colon => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push(':');
                out.push(' ');
            }
            Token::LParen => {
                if let Some(Token::Keyword(kw)) = prev_token
                    && matches!(kw.as_str(), "if" | "while" | "for" | "match") {
                        if !out.ends_with(' ') {
                            out.push(' ');
                        }
                        if kw == "for" {
                            for_header_paren_depth = Some(paren_depth + 1);
                        }
                    }
                paren_depth += 1;
                out.push('(');
            }
            Token::RParen => {
                if for_header_paren_depth == Some(paren_depth) {
                    for_header_paren_depth = None;
                }
                paren_depth = paren_depth.saturating_sub(1);
                out.push(')');
            }
            Token::LBracket => {
                out.push('[');
            }
            Token::RBracket => {
                out.push(']');
            }
            Token::Dot => {
                out.push('.');
            }
            Token::Arrow => {
                if !out.ends_with(' ') { out.push(' '); }
                out.push_str("-> ");
            }
            Token::FatArrow => {
                if !out.ends_with(' ') { out.push(' '); }
                out.push_str("=> ");
            }
            Token::BinaryOp(op) | Token::AssignOp(op) => {
                if !out.ends_with(' ') { out.push(' '); }
                out.push_str(op);
                out.push(' ');
            }
            Token::UnaryOp(op) => {
                if let Some(prev) = prev_token {
                    if matches!(prev, Token::Ident(_) | Token::Number(_) | Token::RParen | Token::RBracket) {
                        // Binary context
                        if !out.ends_with(' ') { out.push(' '); }
                        out.push_str(op);
                        out.push(' ');
                    } else {
                        // Unary context
                        out.push_str(op);
                    }
                } else {
                    out.push_str(op);
                }
            }
            Token::LineComment(comment) => {
                if !out.ends_with(' ') && !out.ends_with('\n') && !out.is_empty() {
                    out.push_str("  ");
                }
                out.push_str(comment);
                out.push('\n');
                at_line_start = true;
                prev_token = None;
                i += 1;
                continue;
            }
            Token::BlockComment(comment) => {
                if !out.ends_with(' ') && !out.ends_with('\n') && !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(comment);
            }
            Token::Keyword(kw) => {
                if prev_token.is_some() && !out.ends_with(' ') && !out.ends_with('(') && !out.ends_with('[') {
                    out.push(' ');
                }
                out.push_str(kw);
            }
            Token::Ident(id) => {
                if prev_token.is_some() && !out.ends_with(' ') && !out.ends_with('(') && !out.ends_with('[') && !out.ends_with('.') {
                    out.push(' ');
                }
                out.push_str(id);
            }
            Token::Number(num) => {
                if prev_token.is_some() && !out.ends_with(' ') && !out.ends_with('(') && !out.ends_with('[') && !out.ends_with(':') {
                    out.push(' ');
                }
                out.push_str(num);
            }
            Token::StringLit(s) => {
                if prev_token.is_some() && !out.ends_with(' ') && !out.ends_with('(') && !out.ends_with('[') {
                    out.push(' ');
                }
                out.push_str(s);
            }
            Token::CharLit(c) => {
                if prev_token.is_some() && !out.ends_with(' ') && !out.ends_with('(') && !out.ends_with('[') {
                    out.push(' ');
                }
                out.push_str(c);
            }
            _ => {}
        }

        prev_token = Some(tok);
        i += 1;
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }

    Ok(out)
}

fn push_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    let keywords = [
        "class", "enum", "match", "interface", "implements", "trait", "impl", "module", "import",
        "public", "private", "static", "new", "this", "borrow", "mut", "readonly", "async", "await",
        "byte", "char", "int", "long", "double", "Unit", "boolean", "String", "void",
        "if", "else", "while", "for", "return", "break", "continue", "true", "false", "assert",
    ];

    while pos < len {
        let b = bytes[pos];

        if b == b'\n' {
            pos += 1;
            // Check for blank lines (consecutive newlines separated only by spaces/tabs)
            let mut newline_count = 1;
            while pos < len {
                if bytes[pos] == b' ' || bytes[pos] == b'\t' || bytes[pos] == b'\r' {
                    pos += 1;
                } else if bytes[pos] == b'\n' {
                    newline_count += 1;
                    pos += 1;
                } else {
                    break;
                }
            }
            if newline_count > 1 {
                tokens.push(Token::BlankLine);
            } else {
                tokens.push(Token::Newline);
            }
            continue;
        }

        if b.is_ascii_whitespace() {
            pos += 1;
            continue;
        }

        // Line comment //
        if b == b'/' && pos + 1 < len && bytes[pos + 1] == b'/' {
            let start = pos;
            while pos < len && bytes[pos] != b'\n' {
                pos += 1;
            }
            tokens.push(Token::LineComment(source[start..pos].to_string()));
            continue;
        }

        // Block comment /* */
        if b == b'/' && pos + 1 < len && bytes[pos + 1] == b'*' {
            let start = pos;
            pos += 2;
            while pos + 1 < len && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                pos += 1;
            }
            if pos + 1 < len {
                pos += 2;
            }
            tokens.push(Token::BlockComment(source[start..pos].to_string()));
            continue;
        }

        // String literal
        if b == b'"' {
            let start = pos;
            pos += 1;
            while pos < len && bytes[pos] != b'"' {
                if bytes[pos] == b'\\' && pos + 1 < len {
                    pos += 2;
                } else {
                    pos += 1;
                }
            }
            if pos < len {
                pos += 1;
            }
            tokens.push(Token::StringLit(source[start..pos].to_string()));
            continue;
        }

        // Character literal
        if b == b'\'' {
            let start = pos;
            pos += 1;
            while pos < len && bytes[pos] != b'\'' {
                if bytes[pos] == b'\\' && pos + 1 < len {
                    pos += 2;
                } else {
                    pos += 1;
                }
            }
            if pos < len {
                pos += 1;
            }
            tokens.push(Token::CharLit(source[start..pos].to_string()));
            continue;
        }

        // Identifier or keyword
        if (b.is_ascii_alphabetic()) || b == b'_' {
            let start = pos;
            while pos < len && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
                pos += 1;
            }
            let text = &source[start..pos];
            if keywords.contains(&text) {
                tokens.push(Token::Keyword(text.to_string()));
            } else {
                tokens.push(Token::Ident(text.to_string()));
            }
            continue;
        }

        // Number
        if b.is_ascii_digit() {
            let start = pos;
            while pos < len && (bytes[pos].is_ascii_digit() || bytes[pos] == b'.' || bytes[pos] == b'L' || bytes[pos] == b'x' || bytes[pos].is_ascii_hexdigit()) {
                pos += 1;
            }
            tokens.push(Token::Number(source[start..pos].to_string()));
            continue;
        }

        // Operators and punctuation
        if pos + 1 < len {
            match (bytes[pos], bytes[pos + 1]) {
                (b'-', b'>') => { tokens.push(Token::Arrow); pos += 2; continue; }
                (b'=', b'>') => { tokens.push(Token::FatArrow); pos += 2; continue; }
                (b'=', b'=') | (b'!', b'=') | (b'<', b'<') | (b'>', b'>')
                | (b'<', b'=') | (b'>', b'=') | (b'&', b'&') | (b'|', b'|') => {
                    tokens.push(Token::BinaryOp(source[pos..pos + 2].to_string()));
                    pos += 2;
                    continue;
                }
                (b'+', b'=') | (b'-', b'=') | (b'*', b'=') | (b'/', b'=') | (b'%', b'=') => {
                    tokens.push(Token::AssignOp(source[pos..pos + 2].to_string()));
                    pos += 2;
                    continue;
                }
                _ => {}
            }
        }

        match b {
            b'(' => tokens.push(Token::LParen),
            b')' => tokens.push(Token::RParen),
            b'{' => tokens.push(Token::LBrace),
            b'}' => tokens.push(Token::RBrace),
            b'[' => tokens.push(Token::LBracket),
            b']' => tokens.push(Token::RBracket),
            b';' => tokens.push(Token::Semi),
            b':' => tokens.push(Token::Colon),
            b',' => tokens.push(Token::Comma),
            b'.' => tokens.push(Token::Dot),
            b'+' | b'*' | b'/' | b'%' | b'<' | b'>' => {
                tokens.push(Token::BinaryOp((b as char).to_string()));
            }
            b'=' => tokens.push(Token::AssignOp("=".to_string())),
            b'-' => tokens.push(Token::UnaryOp("-".to_string())),
            b'!' => tokens.push(Token::UnaryOp("!".to_string())),
            _ => {
                let ch = source[pos..]
                    .chars()
                    .next()
                    .ok_or_else(|| "unexpected end of source while tokenizing".to_owned())?;
                tokens.push(Token::Ident(ch.to_string()));
                pos += ch.len_utf8();
                continue;
            }
        }
        pos += 1;
    }

    Ok(tokens)
}
