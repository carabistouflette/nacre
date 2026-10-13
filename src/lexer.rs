//! Small, dependency-free UTF-8-aware lexer. Identifiers are deliberately ASCII.
use crate::diagnostic::{Diagnostic, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String), Number(String), String(String),
    Class, Enum, Match, Interface, Implements, Module, Import, Public, Private, Static, New, This, Borrow, Mut, Readonly,
    Int, Boolean, StringType, Void,
    If, Else, While, For, Return, Break, Continue, True, False,
    LParen, RParen, LBrace, RBrace, Comma, Colon, Semicolon, Dot,
    Plus, Minus, Star, Slash, Percent,
    Equal, EqualEqual, Bang, BangEqual, Less, LessEqual, Greater, GreaterEqual,
    AndAnd, OrOr, Eof,
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
                "module" => TokenKind::Module, "import" => TokenKind::Import,
                "public" => TokenKind::Public,
                "private" => TokenKind::Private, "static" => TokenKind::Static,
                "new" => TokenKind::New, "this" => TokenKind::This,
                "borrow" => TokenKind::Borrow, "mut" => TokenKind::Mut,
                "readonly" => TokenKind::Readonly,
                "int" => TokenKind::Int, "boolean" => TokenKind::Boolean,
                "String" => TokenKind::StringType, "void" => TokenKind::Void,
                "if" => TokenKind::If, "else" => TokenKind::Else,
                "while" => TokenKind::While, "for" => TokenKind::For,
                "return" => TokenKind::Return,
                "break" => TokenKind::Break, "continue" => TokenKind::Continue,
                "true" => TokenKind::True, "false" => TokenKind::False,
                "extends" | "null" |
                "super" | "package" | "final" | "throw" |
                "try" | "catch" | "switch" | "long" | "double" | "float" | "char" => {
                    return Err(Diagnostic::new(Span::new(start, lexer.pos), format!("`{word}` is not yet supported")));
                }
                _ => TokenKind::Ident(word.to_owned()),
            }
        } else if c.is_ascii_digit() {
            while lexer.peek().is_some_and(|c| c.is_ascii_digit()) { lexer.bump(); }
            if lexer.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
                return Err(Diagnostic::new(Span::new(start, lexer.pos), "invalid decimal integer (no suffixes or hexadecimal)"));
            }
            TokenKind::Number(source[start..lexer.pos].to_owned())
        } else {
            match c {
                '"' => TokenKind::String(lexer.string(start)?),
                '(' => TokenKind::LParen, ')' => TokenKind::RParen,
                '{' => TokenKind::LBrace, '}' => TokenKind::RBrace,
                ',' => TokenKind::Comma, ':' => TokenKind::Colon, ';' => TokenKind::Semicolon, '.' => TokenKind::Dot,
                '+' | '-' if lexer.peek() == Some(c) => {
                    lexer.bump();
                    return Err(Diagnostic::new(Span::new(start, lexer.pos), format!("`{c}{c}` is unsupported; use an assignment such as `i = i + 1`")));
                }
                '+' => TokenKind::Plus, '-' => TokenKind::Minus, '*' => TokenKind::Star,
                '/' => TokenKind::Slash, '%' => TokenKind::Percent,
                '=' => if lexer.take('=') { TokenKind::EqualEqual } else { TokenKind::Equal },
                '!' => if lexer.take('=') { TokenKind::BangEqual } else { TokenKind::Bang },
                '<' => if lexer.take('=') { TokenKind::LessEqual } else { TokenKind::Less },
                '>' => if lexer.take('=') { TokenKind::GreaterEqual } else { TokenKind::Greater },
                '&' if lexer.take('&') => TokenKind::AndAnd,
                '|' if lexer.take('|') => TokenKind::OrOr,
                _ => return Err(Diagnostic::new(Span::new(start, lexer.pos), format!("unexpected character: {c:?}"))),
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
    fn string(&mut self, start: usize) -> Result<String, Diagnostic> {
        let mut result = String::new();
        while let Some(c) = self.bump() {
            match c {
                '"' => return Ok(result),
                '\n' | '\r' => return Err(Diagnostic::new(Span::new(start, self.pos), "string literal is not terminated before end of line")),
                '\\' => {
                    let escaped = match self.bump() {
                        Some('n') => '\n', Some('r') => '\r', Some('t') => '\t',
                        Some('0') => '\0', Some('"') => '"', Some('\\') => '\\',
                        Some(other) => return Err(Diagnostic::new(Span::new(self.pos - other.len_utf8() - 1, self.pos), format!("unsupported escape: \\{other}"))),
                        None => break,
                    };
                    result.push(escaped);
                }
                _ => result.push(c),
            }
        }
        Err(Diagnostic::new(Span::new(start, self.pos), "unterminated string literal"))
    }
}
