//! Small, dependency-free UTF-8-aware lexer. Identifiers are deliberately ASCII.
use crate::diagnostic::{Diagnostic, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String), Number(String), String(String), Character(char),
    Class, Enum, Match, Interface, Implements, Trait, Impl, Module, Import, Public, Private, Static, New, This, Borrow, Mut, Readonly, Async, Await,
    Byte, CharType, Int, Long, Double, UnitType, Boolean, StringType, Void,
    If, Else, While, For, Return, Break, Continue, True, False,
    LParen, RParen, LBrace, RBrace, Comma, Colon, Semicolon, Dot, Question,
    Plus, Minus, Star, Slash, Percent, Tilde, Caret,
    Equal, EqualEqual, Bang, BangEqual, Less, LessEqual, Greater, GreaterEqual,
    ShiftLeft, ShiftRight, Ampersand, Pipe, AndAnd, OrOr, Eof,
}

#[derive(Debug, Clone)]
pub struct Token { pub kind: TokenKind, pub span: Span }

pub fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut lexer = Lexer { source, pos: 0 };
    let mut tokens = Vec::new();
    while lexer.pos < source.len() {
        lexer.skip_trivia()?;
        if lexer.pos == source.len() { break; }
        let start = lexer.pos;
        let c = lexer.bump().unwrap();
        let kind = if c.is_ascii_alphabetic() || c == '_' {
            while lexer.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') { lexer.bump(); }
            let word = &source[start..lexer.pos];
            match word {
                "class" => TokenKind::Class, "enum" => TokenKind::Enum, "match" => TokenKind::Match,
                "interface" => TokenKind::Interface, "implements" => TokenKind::Implements,
                "trait" => TokenKind::Trait, "impl" => TokenKind::Impl,
                "async" => TokenKind::Async, "await" => TokenKind::Await,
                "module" => TokenKind::Module, "import" => TokenKind::Import,
                "public" => TokenKind::Public,
                "private" => TokenKind::Private, "static" => TokenKind::Static,
                "new" => TokenKind::New, "this" => TokenKind::This,
                "borrow" => TokenKind::Borrow, "mut" => TokenKind::Mut,
                "readonly" => TokenKind::Readonly,
                "byte" => TokenKind::Byte, "char" => TokenKind::CharType, "int" => TokenKind::Int,
                "long" => TokenKind::Long, "double" => TokenKind::Double,
                "Unit" => TokenKind::UnitType, "boolean" => TokenKind::Boolean,
                "String" => TokenKind::StringType, "void" => TokenKind::Void,
                "if" => TokenKind::If, "else" => TokenKind::Else,
                "while" => TokenKind::While, "for" => TokenKind::For,
                "return" => TokenKind::Return,
                "break" => TokenKind::Break, "continue" => TokenKind::Continue,
                "true" => TokenKind::True, "false" => TokenKind::False,
                "extends" | "null" |
                "super" | "package" | "final" | "throw" |
                "try" | "catch" | "switch" | "float" => {
                    return Err(Diagnostic::new(Span::new(start, lexer.pos), format!("[E6001] `{word}` is not supported")));
                }
                _ => TokenKind::Ident(word.to_owned()),
            }
        } else if c.is_ascii_digit() {
            TokenKind::Number(lexer.number(start, false)?)
        } else if c == '.' && lexer.peek().is_some_and(|next| next.is_ascii_digit()) {
            TokenKind::Number(lexer.number(start, true)?)
        } else {
            match c {
                '"' => TokenKind::String(lexer.string(start)?),
                '\'' => TokenKind::Character(lexer.character(start)?),
                '(' => TokenKind::LParen, ')' => TokenKind::RParen,
                '{' => TokenKind::LBrace, '}' => TokenKind::RBrace,
                ',' => TokenKind::Comma, ':' => TokenKind::Colon, ';' => TokenKind::Semicolon, '.' => TokenKind::Dot, '?' => TokenKind::Question,
                '+' | '-' if lexer.peek() == Some(c) => {
                    lexer.bump();
                    return Err(Diagnostic::new(Span::new(start, lexer.pos), format!("`{c}{c}` is unsupported; use an assignment such as `i = i + 1`")));
                }
                '+' => TokenKind::Plus, '-' => TokenKind::Minus, '*' => TokenKind::Star,
                '/' => TokenKind::Slash, '%' => TokenKind::Percent,
                '~' => TokenKind::Tilde, '^' => TokenKind::Caret,
                '=' => if lexer.take('=') { TokenKind::EqualEqual } else { TokenKind::Equal },
                '!' => if lexer.take('=') { TokenKind::BangEqual } else { TokenKind::Bang },
                '<' => if lexer.take('=') { TokenKind::LessEqual }
                    else if lexer.take('<') { TokenKind::ShiftLeft } else { TokenKind::Less },
                '>' => if lexer.take('=') { TokenKind::GreaterEqual }
                    else if lexer.take('>') { TokenKind::ShiftRight } else { TokenKind::Greater },
                '&' => if lexer.take('&') { TokenKind::AndAnd } else { TokenKind::Ampersand },
                '|' => if lexer.take('|') { TokenKind::OrOr } else { TokenKind::Pipe },
                _ => return Err(Diagnostic::new(Span::new(start, lexer.pos), format!("[E6001] unexpected character: {c:?}"))),
            }
        };
        tokens.push(Token { kind, span: Span::new(start, lexer.pos) });
    }
    tokens.push(Token { kind: TokenKind::Eof, span: Span::new(source.len(), source.len()) });
    Ok(tokens)
}

struct Lexer<'a> { source: &'a str, pos: usize }

impl Lexer<'_> {
    fn peek(&self) -> Option<char> { self.source[self.pos..].chars().next() }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn take(&mut self, c: char) -> bool {
        if self.peek() == Some(c) { self.bump(); true } else { false }
    }
    fn skip_trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            while self.peek().is_some_and(char::is_whitespace) { self.bump(); }
            if self.source[self.pos..].starts_with("//") {
                while self.peek().is_some_and(|c| c != '\n') { self.bump(); }
            } else if self.source[self.pos..].starts_with("/*") {
                let start = self.pos;
                self.pos += 2;
                while self.pos < self.source.len() && !self.source[self.pos..].starts_with("*/") { self.bump(); }
                if self.pos == self.source.len() {
                    return Err(Diagnostic::new(Span::new(start, self.pos), "unterminated `/*` comment"));
                }
                self.pos += 2;
            } else { return Ok(()); }
        }
    }
    fn number(&mut self, start: usize, leading_dot: bool) -> Result<String, Diagnostic> {
        let mut has_dot = leading_dot;
        let mut has_exponent = false;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) { self.bump(); }
        if !leading_dot && self.peek() == Some('.') && !self.source[self.pos..].starts_with("..") {
            has_dot = true;
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) { self.bump(); }
        }
        if self.peek().is_some_and(|c| c == 'e' || c == 'E') {
            has_exponent = true;
            self.bump();
            if self.peek().is_some_and(|c| c == '+' || c == '-') { self.bump(); }
            let exponent_start = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) { self.bump(); }
            if self.pos == exponent_start {
                return Err(Diagnostic::new(Span::new(start, self.pos), "[E6101] exponent requires at least one digit"));
            }
        }
        if self.peek() == Some('L') {
            if has_dot || has_exponent {
                self.bump();
                return Err(Diagnostic::new(Span::new(start, self.pos), "[E6101] long suffix is only valid on integer literals"));
            }
            self.bump();
        }
        if self.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
            while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') { self.bump(); }
            return Err(Diagnostic::new(Span::new(start, self.pos), "[E6101] invalid numeric literal"));
        }
        Ok(self.source[start..self.pos].to_owned())
    }
    fn character(&mut self, start: usize) -> Result<char, Diagnostic> {
        let value = match self.bump() {
            Some('\\') => self.escape_char(start, "[E6101]", true)?,
            Some('\'') => return Err(Diagnostic::new(Span::new(start, self.pos), "[E6101] character literal cannot be empty")),
            Some('\n' | '\r') => return Err(Diagnostic::new(Span::new(start, self.pos), "[E6101] character literal cannot contain a line break")),
            Some(value) => value,
            None => return Err(Diagnostic::new(Span::new(start, self.pos), "[E6101] unterminated character literal")),
        };
        if !self.take('\'') {
            while self.peek().is_some_and(|c| c != '\'' && c != '\n' && c != '\r') { self.bump(); }
            self.take('\'');
            return Err(Diagnostic::new(Span::new(start, self.pos), "[E6101] character literal must contain exactly one Unicode scalar"));
        }
        Ok(value)
    }
    fn escape_char(&mut self, start: usize, code: &str, allow_unicode: bool) -> Result<char, Diagnostic> {
        let Some(escaped) = self.bump() else {
            return Err(Diagnostic::new(Span::new(start, self.pos), format!("{code} incomplete escape sequence")));
        };
        match escaped {
            'n' => Ok('\n'), 'r' => Ok('\r'), 't' => Ok('\t'), '0' => Ok('\0'),
            '"' => Ok('"'), '\'' => Ok('\''), '\\' => Ok('\\'),
            'u' if allow_unicode && self.take('{') => {
                let digits_start = self.pos;
                while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) { self.bump(); }
                let digits_end = self.pos;
                if digits_start == digits_end || !self.take('}') {
                    return Err(Diagnostic::new(Span::new(start, self.pos), format!("{code} Unicode escape must use `\\u{{...}}` with hexadecimal digits")));
                }
                let value = u32::from_str_radix(&self.source[digits_start..digits_end], 16)
                    .ok().and_then(char::from_u32)
                    .ok_or_else(|| Diagnostic::new(Span::new(start, self.pos), format!("{code} escape is not a Unicode scalar")))?;
                Ok(value)
            }
            _ => Err(Diagnostic::new(Span::new(start, self.pos), format!("{code} unsupported escape: \\{escaped}"))),
        }
    }
    fn string(&mut self, start: usize) -> Result<String, Diagnostic> {
        let mut result = String::new();
        while let Some(c) = self.bump() {
            match c {
                '"' => return Ok(result),
                '\n' | '\r' => return Err(Diagnostic::new(Span::new(start, self.pos), "string literal is not terminated before end of line")),
                '\\' => result.push(self.escape_char(start, "[E6001]", false)?),
                _ => result.push(c),
            }
        }
        Err(Diagnostic::new(Span::new(start, self.pos), "unterminated string literal"))
    }
}
