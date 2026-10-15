//! Recursive descent for declarations/statements; precedence climbing for expressions.
use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::{Token, TokenKind as K};

pub fn parse(tokens: Vec<Token>) -> Result<Program, Diagnostic> {
    Parser { tokens, position: TokenPosition::default(), depth: 0 }.program()
}

struct Parser { tokens: Vec<Token>, position: TokenPosition, depth: usize }

/// Speculative parsing must checkpoint the physical and virtual token positions together.
#[derive(Clone, Default)]
struct TokenPosition { index: usize, pending_greater: Option<Token> }

impl Parser {
    fn current(&self) -> &Token { self.position.pending_greater.as_ref().unwrap_or(&self.tokens[self.position.index]) }
    fn at(&self, kind: &K) -> bool { self.current().kind == *kind }
    fn bump(&mut self) -> Token {
        if let Some(token) = self.position.pending_greater.take() { return token; }
        let token = self.current().clone();
        if token.kind != K::Eof { self.position.index += 1; }
        token
    }
    fn eat(&mut self, kind: &K) -> bool {
        if self.at(kind) { self.bump(); true } else { false }
    }
    fn expect(&mut self, kind: &K, what: &str) -> Result<Token, Diagnostic> {
        if self.at(kind) { Ok(self.bump()) }
        else { Err(Diagnostic::new(self.current().span, format!("{what} expected"))) }
    }
    fn ident(&mut self) -> Result<(String, Span), Diagnostic> {
        let token = self.current().clone();
        match token.kind {
            K::Ident(name) => {
                self.bump();
                Ok((name, token.span))
            }
            K::Await => {
                self.bump();
                Ok(("await".to_owned(), token.span))
            }
            _ => Err(Diagnostic::new(self.current().span, "identifier expected")),
        }
    }
    fn qualified_ident(&mut self) -> Result<(String, Span), Diagnostic> {
        let (mut path, mut span) = self.ident()?;
        while self.eat(&K::Dot) {
            let (part, part_span) = self.ident()?;
            path.push('.');
            path.push_str(&part);
            span = span.join(part_span);
        }
        Ok((path, span))
    }
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, Diagnostic>) -> Result<T, Diagnostic> {
        if self.depth >= 128 {
            return Err(Diagnostic::new(self.current().span, "nesting is too deep"));
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }
    fn modifiers(&mut self) -> Result<(Visibility, bool, bool, bool), Diagnostic> {
        let mut visibility = None;
        let mut is_static = false;
        let mut readonly = false;
        let mut is_async = false;
        loop {
            let token = self.current().clone();
            match token.kind {
                K::Public | K::Private => {
                    if visibility.is_some() { return Err(Diagnostic::new(token.span, "visibility modifier repeated")); }
                    visibility = Some(if self.eat(&K::Public) { Visibility::Public } else { self.bump(); Visibility::Private });
                }
                K::Static => {
                    if is_static { return Err(Diagnostic::new(token.span, "static modifier repeated")); }
                    self.bump(); is_static = true;
                }
                K::Readonly => {
                    if readonly { return Err(Diagnostic::new(token.span, "readonly modifier repeated")); }
                    self.bump(); readonly = true;
                }
                K::Async => {
                    if is_async { return Err(Diagnostic::new(token.span, "async modifier repeated")); }
                    self.bump(); is_async = true;
                }
                _ => break,
            }
        }
        Ok((visibility.unwrap_or(Visibility::Public), is_static, readonly, is_async))
    }
    fn program(mut self) -> Result<Program, Diagnostic> {
        let module = if self.eat(&K::Module) {
            let (name, _) = self.qualified_ident()?;
            self.expect(&K::Semicolon, "`;` after module name")?;
            name
        } else { String::new() };
        let mut imports = Vec::new();
        while self.eat(&K::Import) {
            let start = self.tokens[self.position.index - 1].span;
            let (mut path, _) = self.ident()?;
            let mut wildcard = false;
            while self.eat(&K::Dot) {
                if self.eat(&K::Star) { wildcard = true; break; }
                let (part, _) = self.ident()?;
                path.push('.');
                path.push_str(&part);
            }
            let end = self.expect(&K::Semicolon, "`;` after import")?.span;
            imports.push(Import { path, wildcard, span: start.join(end) });
        }
        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut traits = Vec::new();
        let mut impls = Vec::new();
        while !self.at(&K::Eof) {
            let start = self.current().span;
            let (visibility, is_static, readonly, is_async) = self.modifiers()?;
            if self.eat(&K::Class) {
                if is_static || readonly || is_async {
                    return Err(Diagnostic::new(start, "a top-level class cannot be static, readonly, or async"));
                }
                classes.push(self.class(start, false, visibility)?);
            } else if self.eat(&K::Enum) {
                if is_static || readonly || is_async {
                    return Err(Diagnostic::new(start, "a top-level enum cannot be static, readonly, or async"));
                }
                classes.push(self.enum_decl(start, visibility)?);
            } else if self.eat(&K::Interface) {
                if is_static || readonly || is_async {
                    return Err(Diagnostic::new(start, "invalid modifier on a top-level interface"));
                }
                classes.push(self.class(start, true, visibility)?);
            } else if self.eat(&K::Trait) {
                if is_static || readonly || is_async {
                    return Err(Diagnostic::new(start, "invalid modifier on a top-level trait"));
                }
                traits.push(self.trait_decl(start, visibility)?);
            } else if self.eat(&K::Impl) {
                if is_static || readonly || is_async {
                    return Err(Diagnostic::new(start, "invalid modifier on a top-level impl block"));
                }
                impls.push(self.impl_block(start)?);
            } else {
                let return_type = self.ty(true)?;
                let (name, _) = self.ident()?;
                if readonly { return Err(Diagnostic::new(start, "readonly is reserved for instance methods")); }
                functions.push(self.function_tail(name, return_type, visibility, true, false, is_async, start)?);
            }
        }
        Ok(Program { units: vec![SourceUnit { module, imports }], functions, classes, traits, impls })
    }
    fn enum_decl(&mut self, start: Span, visibility: Visibility) -> Result<Class, Diagnostic> {
        let (name, _) = self.ident()?;
        let mut type_params = Vec::new();
        let mut type_bounds = Vec::new();
        if self.eat(&K::Less) {
            loop {
                let (param, span) = self.ident()?;
                if type_params.contains(&param) {
                    return Err(Diagnostic::new(span, "[E4101] duplicate type parameter"));
                }
                let bound = if self.eat(&K::Colon) {
                    let (b, _) = self.ident()?;
                    Some(b)
                } else {
                    None
                };
                type_params.push(param);
                type_bounds.push(bound);
                if type_params.len() > 8 {
                    return Err(Diagnostic::new(start, "[E4199] limit of 8 type parameters"));
                }
                if !self.eat(&K::Comma) { break; }
            }
            self.expect(&K::Greater, "`>`")?;
        }
        if self.at(&K::Implements) {
            return Err(Diagnostic::new(self.current().span, "an enum cannot implement an interface"));
        }
        self.expect(&K::LBrace, "`{`")?;
        let mut variants: Vec<Variant> = Vec::new();
        while !self.at(&K::RBrace) && !self.at(&K::Eof) {
            let (variant, variant_start) = self.ident()?;
            if variants.iter().any(|known| known.name == variant) {
                return Err(Diagnostic::new(variant_start, format!("variant `{variant}` is already declared in `{name}`")));
            }
            let mut fields = Vec::new();
            if self.eat(&K::LParen) {
                if !self.at(&K::RParen) {
                    loop {
                        fields.push(self.ty(false)?);
                        if !self.eat(&K::Comma) { break; }
                    }
                }
                self.expect(&K::RParen, "`)`")?;
            }
            let end = self.expect(&K::Semicolon, "`;` after a variant")?.span;
            variants.push(Variant { name: variant, fields, span: variant_start.join(end) });
        }
        let end = self.expect(&K::RBrace, "`}`")?.span;
        if variants.is_empty() {
            return Err(Diagnostic::new(start.join(end), "an enum must declare at least one variant"));
        }
        Ok(Class { native: None, is_builtin: false, is_builtin_result: false, name, span: start.join(end), fields: Vec::new(), methods: Vec::new(),
            constructor: None, type_params, type_bounds, implements: Vec::new(), is_interface: false, variants: Some(variants),
            qualified_name: String::new(), unit: 0, visibility })
    }
    fn class(&mut self, start: Span, is_interface: bool, visibility: Visibility) -> Result<Class, Diagnostic> {
        let (name, _) = self.ident()?;
        let mut type_params = Vec::new();
        let mut type_bounds = Vec::new();
        if self.eat(&K::Less) {
            loop {
                let (param, span) = self.ident()?;
                if type_params.contains(&param) {
                    return Err(Diagnostic::new(span, "[E4101] duplicate type parameter"));
                }
                let bound = if self.eat(&K::Colon) {
                    let (b, _) = self.ident()?;
                    Some(b)
                } else {
                    None
                };
                type_params.push(param);
                type_bounds.push(bound);
                if type_params.len() > 8 {
                    return Err(Diagnostic::new(start, "[E4199] limit of 8 type parameters"));
                }
                if !self.eat(&K::Comma) { break; }
            }
            self.expect(&K::Greater, "`>`")?;
        }
        let mut implements = Vec::new();
        if self.eat(&K::Implements) {
            if is_interface { return Err(Diagnostic::new(start, "[E4001] implements is reserved for classes")); }
            loop {
                let ty = self.ty(false)?;
                if !matches!(ty, Type::Named(_) | Type::Applied { .. }) {
                    return Err(Diagnostic::new(start, "[E4001] interface name expected after implements"));
                }
                implements.push(ty);
                if !self.eat(&K::Comma) { break; }
            }
        }
        self.expect(&K::LBrace, "`{`")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut constructor = None;
        while !self.at(&K::RBrace) && !self.at(&K::Eof) {
            let member_start = self.current().span;
            let (visibility, is_static, readonly, is_async) = self.modifiers()?;
            if is_interface {
                if visibility != Visibility::Public || is_static || is_async {
                    return Err(Diagnostic::new(member_start, "[E4001] an interface declares public instance methods"));
                }
                let return_type = self.ty(true)?;
                let (method_name, _) = self.ident()?;
                let params = self.parameters()?;
                let end = self.expect(&K::Semicolon, "`;` (interfaces cannot declare bodies, fields, or default methods)")?.span;
                methods.push(Function { name: method_name, span: member_start.join(end), return_type,
                    params, body: Vec::new(), visibility, is_static: false, readonly, is_async: false,
                    qualified_name: String::new(), unit: 0 });
                continue;
            }
            if matches!(&self.current().kind, K::Ident(n) if n == &name)
                && self.tokens.get(self.position.index + 1).is_some_and(|t| t.kind == K::LParen) {
                self.bump();
                if is_static || readonly || is_async { return Err(Diagnostic::new(member_start, "a constructor cannot be static, readonly, or async")); }
                if constructor.is_some() { return Err(Diagnostic::new(member_start, "one constructor is allowed per class; overloading is not supported")); }
                constructor = Some(self.function_tail(name.clone(), Type::Void, visibility, false, false, false, member_start)?);
                continue;
            }
            let ty = self.ty(true)?;
            let (member_name, _) = self.ident()?;
            if self.at(&K::LParen) {
                if readonly && is_static { return Err(Diagnostic::new(member_start, "readonly is reserved for instance methods")); }
                methods.push(self.function_tail(member_name, ty, visibility, is_static, readonly, is_async, member_start)?);
            } else {
                if is_async { return Err(Diagnostic::new(member_start, "async modifies a method, not a field")); }
                if readonly { return Err(Diagnostic::new(member_start, "readonly modifies a method, not a field")); }
                if is_static { return Err(Diagnostic::new(member_start, "static fields are not supported")); }
                if ty == Type::Void { return Err(Diagnostic::new(member_start, "a field cannot have type void")); }
                let initializer = if self.eat(&K::Equal) { Some(self.expression()?) } else { None };
                let end = self.expect(&K::Semicolon, "`;`")?.span;
                fields.push(Field { name: member_name, ty, visibility, initializer, span: member_start.join(end) });
            }
        }
        let end = self.expect(&K::RBrace, "`}`")?.span;
        Ok(Class { native: None, is_builtin: false, is_builtin_result: false, name, span: start.join(end), fields, methods, constructor, type_params, type_bounds, implements, is_interface,
            variants: None, qualified_name: String::new(), unit: 0, visibility })
    }
    fn trait_decl(&mut self, start: Span, visibility: Visibility) -> Result<Trait, Diagnostic> {
        let (name, _) = self.ident()?;
        self.expect(&K::LBrace, "`{`")?;
        let mut methods = Vec::new();
        while !self.at(&K::RBrace) && !self.at(&K::Eof) {
            let member_start = self.current().span;
            let (visibility, is_static, readonly, is_async) = self.modifiers()?;
            if is_static || is_async {
                return Err(Diagnostic::new(member_start, "trait methods cannot be static or async"));
            }
            let return_type = self.ty(true)?;
            let (method_name, _) = self.ident()?;
            let params = self.parameters()?;
            let end = self.expect(&K::Semicolon, "`;`")?.span;
            methods.push(Function {
                name: method_name, span: member_start.join(end), return_type,
                params, body: Vec::new(), visibility, is_static: false, readonly, is_async: false,
                qualified_name: String::new(), unit: 0,
            });
        }
        let end = self.expect(&K::RBrace, "`}`")?.span;
        Ok(Trait { name, span: start.join(end), methods, visibility })
    }

    fn impl_block(&mut self, start: Span) -> Result<Impl, Diagnostic> {
        let (trait_name, _) = self.ident()?;
        self.expect(&K::For, "`for` in impl block")?;
        let (target_class, _) = self.ident()?;
        self.expect(&K::LBrace, "`{`")?;
        let mut methods = Vec::new();
        while !self.at(&K::RBrace) && !self.at(&K::Eof) {
            let member_start = self.current().span;
            let (visibility, is_static, readonly, is_async) = self.modifiers()?;
            if is_static {
                return Err(Diagnostic::new(member_start, "impl methods cannot be static"));
            }
            let return_type = self.ty(true)?;
            let (method_name, _) = self.ident()?;
            methods.push(self.function_tail(method_name, return_type, visibility, false, readonly, is_async, member_start)?);
        }
        let end = self.expect(&K::RBrace, "`}`")?.span;
        Ok(Impl { trait_name, target_class, span: start.join(end), methods })
    }
    fn ty(&mut self, allow_void: bool) -> Result<Type, Diagnostic> {
        let token = self.bump();
        match token.kind {
            K::Borrow => {
                let mutable = self.eat(&K::Mut);
                let (name, _) = self.qualified_ident().map_err(|_| Diagnostic::new(token.span, "borrow expects a nominal type, not a primitive"))?;
                let args = self.type_arguments()?;
                Ok(Type::Borrow { name, args, mutable })
            }
            K::Byte => Ok(Type::Byte), K::CharType => Ok(Type::Char),
            K::Int => Ok(Type::Int), K::Long => Ok(Type::Long),
            K::Double => Ok(Type::Double), K::UnitType => Ok(Type::Unit),
            K::Boolean => Ok(Type::Boolean), K::StringType => Ok(Type::String),
            K::Ident(first) => {
                let mut name = first;
                while self.eat(&K::Dot) {
                    let (part, _) = self.ident()?;
                    name.push('.');
                    name.push_str(&part);
                }
                let args = self.type_arguments()?;
                Ok(if args.is_empty() { Type::Named(name) } else { Type::Applied { name, args } })
            },
            K::Void if allow_void => Ok(Type::Void),
            K::Void => Err(Diagnostic::new(token.span, "`void` is reserved for function return types")),
            _ => Err(Diagnostic::new(token.span, "expected type: byte, char, int, long, double, Unit, boolean, String, or a class name")),
        }
    }
    fn type_arguments(&mut self) -> Result<Vec<Type>, Diagnostic> {
        if !self.eat(&K::Less) { return Ok(Vec::new()); }
        self.nested(|p| {
            if p.at(&K::Greater) {
                return Err(Diagnostic::new(p.current().span, "[E4101] generic arguments must be explicit; diamond syntax `<>` is unsupported"));
            }
            let mut args = Vec::new();
            loop {
                args.push(p.ty(false)?);
                if args.len() > 8 { return Err(Diagnostic::new(p.current().span, "[E4199] limit of 8 type arguments")); }
                if !p.eat(&K::Comma) { break; }
            }
            p.type_greater()?;
            Ok(args)
        })
    }
    fn type_greater(&mut self) -> Result<Token, Diagnostic> {
        if self.at(&K::ShiftRight) {
            let token = self.bump();
            let middle = token.span.start + 1;
            self.position.pending_greater = Some(Token { kind: K::Greater, span: Span::new(middle, token.span.end) });
            return Ok(Token { kind: K::Greater, span: Span::new(token.span.start, middle) });
        }
        self.expect(&K::Greater, "`>`")
    }
    fn parameters(&mut self) -> Result<Vec<Param>, Diagnostic> {
        self.expect(&K::LParen, "`(`")?;
        let mut params = Vec::new();
        if !self.at(&K::RParen) {
            loop {
                let ty = self.ty(false)?;
                let (name, span) = self.ident()?;
                params.push(Param { name, ty, span });
                if !self.eat(&K::Comma) { break; }
            }
        }
        self.expect(&K::RParen, "`)`")?;
        Ok(params)
    }
    fn function_tail(&mut self, name: String, return_type: Type, visibility: Visibility,
                     is_static: bool, readonly: bool, is_async: bool, start: Span) -> Result<Function, Diagnostic> {
        let params = self.parameters()?;
        let body = self.block()?;
        let end = self.tokens[self.position.index - 1].span;
        Ok(Function { name, span: start.join(end), return_type, params, body, visibility, is_static, readonly, is_async,
            qualified_name: String::new(), unit: 0 })
    }
    fn block(&mut self) -> Result<Block, Diagnostic> {
        self.nested(|p| {
            p.expect(&K::LBrace, "`{` (accolades obligatoires)")?;
            let mut statements = Vec::new();
            while !p.at(&K::RBrace) && !p.at(&K::Eof) { statements.push(p.statement()?); }
            p.expect(&K::RBrace, "`}`")?;
            Ok(statements)
        })
    }
    fn statement(&mut self) -> Result<Stmt, Diagnostic> {
        self.nested(|p| p.statement_inner())
    }
    fn declaration_ahead(&mut self) -> bool {
        if matches!(self.current().kind,
            K::Byte | K::CharType | K::Int | K::Long | K::Double | K::UnitType |
            K::Boolean | K::StringType | K::Borrow) { return true; }
        if !matches!(self.current().kind, K::Ident(_)) { return false; }
        let saved = self.position.clone();
        let result = self.ty(false).is_ok() && matches!(self.current().kind, K::Ident(_));
        self.position = saved;
        result
    }
    fn statement_inner(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.current().span;
        let is_declaration = self.declaration_ahead();
        let kind = match &self.current().kind {
            _ if is_declaration => {
                let ty = self.ty(false)?;
                let (name, _) = self.ident()?;
                let value = if self.eat(&K::Equal) { Some(self.expression()?) } else { None };
                self.expect(&K::Semicolon, "`;`")?;
                StmtKind::Let { ty, name, value }
            }
            K::If => {
                self.bump();
                self.expect(&K::LParen, "`(`")?;
                let condition = self.expression()?;
                self.expect(&K::RParen, "`)`")?;
                let then_block = self.block()?;
                let else_block = if self.eat(&K::Else) {
                    Some(if self.at(&K::If) { vec![self.statement()?] } else { self.block()? })
                } else { None };
                StmtKind::If { condition, then_block, else_block }
            }
            K::While => {
                self.bump();
                self.expect(&K::LParen, "`(`")?;
                let condition = self.expression()?;
                self.expect(&K::RParen, "`)`")?;
                let body = self.block()?;
                StmtKind::While { condition, body }
            }
            K::For => {
                self.bump();
                self.expect(&K::LParen, "`(`")?;
                let header_start = self.current().span;
                let first_ty = self.ty(false)?;
                let (first_name, first_name_span) = self.ident()?;
                let (key, ty, name, name_span) = if self.eat(&K::Comma) {
                    let value_ty = self.ty(false)?;
                    let (value_name, value_name_span) = self.ident()?;
                    (Some((first_ty, first_name)), value_ty, value_name, value_name_span)
                } else {
                    (None, first_ty, first_name, first_name_span)
                };
                let header_span = header_start.join(name_span);
                self.expect(&K::Colon, "`:` after the element type and name")?;
                let collection = self.expression()?;
                self.expect(&K::RParen, "`)`")?;
                let body = self.block()?;
                StmtKind::ForEach { ty, name, key, collection, body, header_span }
            }
            K::Match => {
                self.bump();
                self.expect(&K::LParen, "`(`")?;
                let value = self.expression()?;
                self.expect(&K::RParen, "`)`")?;
                self.expect(&K::LBrace, "`{`")?;
                let mut arms = Vec::new();
                while !self.at(&K::RBrace) && !self.at(&K::Eof) {
                    let (variant, arm_start) = self.ident()?;
                    let mut bindings = Vec::new();
                    if self.eat(&K::LParen) {
                        if !self.at(&K::RParen) {
                            loop {
                                bindings.push(self.ident()?.0);
                                if !self.eat(&K::Comma) { break; }
                            }
                        }
                        self.expect(&K::RParen, "`)`")?;
                    }
                    let body = self.block()?;
                    let arm_end = self.tokens[self.position.index - 1].span;
                    arms.push(MatchArm { variant, bindings, body, span: arm_start.join(arm_end) });
                    self.eat(&K::Comma);
                }
                self.expect(&K::RBrace, "`}` after the match arms")?;
                StmtKind::Match { value, arms }
            }
            K::Return => {
                self.bump();
                let value = if self.at(&K::Semicolon) { None } else { Some(self.expression()?) };
                self.expect(&K::Semicolon, "`;`")?;
                StmtKind::Return(value)
            }
            K::Break | K::Continue => {
                let is_break = self.eat(&K::Break);
                if !is_break { self.bump(); }
                self.expect(&K::Semicolon, "`;`")?;
                if is_break { StmtKind::Break } else { StmtKind::Continue }
            }
            K::LBrace => StmtKind::Block(self.block()?),
            _ => {
                let expr = self.expression()?;
                let result = if self.eat(&K::Equal) {
                    if !matches!(&expr.kind, ExprKind::Var(_) | ExprKind::Field { .. }) {
                        return Err(Diagnostic::new(expr.span, "invalid assignment target: expected a variable or field"));
                    }
                    StmtKind::Assign { target: expr, value: self.expression()? }
                } else { StmtKind::Expr(expr) };
                self.expect(&K::Semicolon, "`;`")?;
                result
            }
        };
        let end = self.tokens[self.position.index - 1].span;
        Ok(Stmt { kind, span: start.join(end) })
    }
    fn expression(&mut self) -> Result<Expr, Diagnostic> { self.binary(1) }
    fn cast_ahead(&self) -> bool {
        let target = self.tokens.get(self.position.index + 1).map(|token| &token.kind);
        self.at(&K::LParen)
            && matches!(target, Some(K::Byte | K::CharType | K::Int | K::Long | K::Double))
            && self.tokens.get(self.position.index + 2).is_some_and(|token| token.kind == K::RParen)
    }
    fn binary(&mut self, min_precedence: u8) -> Result<Expr, Diagnostic> {
        self.nested(|p| {
            let mut left = p.unary()?;
            while let Some((op, precedence)) = binary_op(&p.current().kind) {
                if precedence < min_precedence { break; }
                p.bump();
                let right = p.binary(precedence + 1)?;
                let span = left.span.join(right.span);
                left = make_expr(ExprKind::Binary { op, left: Box::new(left), right: Box::new(right) }, span)?;
            }
            Ok(left)
        })
    }
    fn unary(&mut self) -> Result<Expr, Diagnostic> {
        self.nested(|p| {
            if p.cast_ahead() {
                let start = p.bump().span;
                let ty = p.ty(false)?;
                p.expect(&K::RParen, "`)` after primitive cast")?;
                let value = p.unary()?;
                let span = start.join(value.span);
                return make_expr(ExprKind::Cast { ty, expr: Box::new(value) }, span);
            }
            if p.at(&K::Borrow) {
                let start = p.bump().span;
                let mutable = p.eat(&K::Mut);
                let expr = p.unary()?;
                let span = start.join(expr.span);
                return make_expr(ExprKind::Borrow { expr: Box::new(expr), mutable }, span);
            }
            let op = match p.current().kind {
                K::Minus => Some(UnaryOp::Neg), K::Bang => Some(UnaryOp::Not),
                K::Plus => Some(UnaryOp::Plus), K::Tilde => Some(UnaryOp::BitNot), _ => None,
            };
            if let Some(op) = op {
                let start = p.bump().span;
                if op == UnaryOp::Neg
                    && let K::Number(text) = &p.current().kind {
                        if let Some(magnitude) = text.strip_suffix('L') {
                            if magnitude.parse::<u64>() == Ok(9_223_372_036_854_775_808) {
                                let end = p.bump().span;
                                return make_expr(ExprKind::Long(i64::MIN), start.join(end));
                            }
                        } else if text.parse::<u64>() == Ok(2_147_483_648) {
                            let end = p.bump().span;
                            return make_expr(ExprKind::Int(i32::MIN), start.join(end));
                        }
                    }
                let expr = p.unary()?;
                let span = start.join(expr.span);
                make_expr(ExprKind::Unary { op, expr: Box::new(expr) }, span)
            } else { p.postfix() }
        })
    }
    fn arguments(&mut self) -> Result<(Vec<Expr>, Span), Diagnostic> {
        self.expect(&K::LParen, "`(`")?;
        let mut args = Vec::new();
        if !self.at(&K::RParen) {
            loop {
                args.push(self.expression()?);
                if !self.eat(&K::Comma) { break; }
            }
        }
        let end = self.expect(&K::RParen, "`)`")?.span;
        Ok((args, end))
    }
    fn postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.primary()?;
        loop {
            if self.eat(&K::Dot) {
                if self.at(&K::Await) && !self.tokens.get(self.position.index + 1).is_some_and(|t| t.kind == K::LParen) {
                    self.bump();
                    let end = self.tokens[self.position.index - 1].span;
                    let span = expr.span.join(end);
                    expr = make_expr(ExprKind::Await(Box::new(expr)), span)?;
                    continue;
                }
                let (name, end) = self.ident()?;
                let span = expr.span.join(end);
                expr = if self.at(&K::LParen) {
                    let (args, end) = self.arguments()?;
                    make_expr(ExprKind::MethodCall { receiver: Box::new(expr), name, args }, span.join(end))?
                } else {
                    make_expr(ExprKind::Field { base: Box::new(expr), name }, span)?
                };
            } else if self.at(&K::Question) {
                let end = self.bump().span;
                let span = expr.span.join(end);
                expr = make_expr(ExprKind::Try(Box::new(expr)), span)?;
            } else {
                break;
            }
        }
        Ok(expr)
    }
    fn generic_static_call(&mut self, first: &str, start: Span) -> Result<Option<Expr>, Diagnostic> {
        let mut probe = self.position.index;
        while self.tokens.get(probe).is_some_and(|token| token.kind == K::Dot)
            && self.tokens.get(probe + 1).is_some_and(|token| matches!(token.kind, K::Ident(_))) {
            probe += 2;
        }
        if !self.tokens.get(probe).is_some_and(|token| token.kind == K::Less) {
            return Ok(None);
        }
        let saved = self.position.clone();
        let mut class = first.to_owned();
        while self.eat(&K::Dot) {
            let (part, _) = self.ident()?;
            class.push('.');
            class.push_str(&part);
        }
        let candidate = self.type_arguments().ok().and_then(|type_args| {
            if !self.eat(&K::Dot) { return None; }
            let (name, _) = self.ident().ok()?;
            self.at(&K::LParen).then_some((type_args, name))
        });
        let Some((type_args, name)) = candidate else {
            self.position = saved;
            return Ok(None);
        };
        let (args, end) = self.arguments()?;
        make_expr(ExprKind::StaticCall { class, type_args, name, args }, start.join(end)).map(Some)
    }
    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.bump();
        let mut span = token.span;
        let kind = match token.kind {
            K::Await => {
                let inner = self.expression()?;
                let end = inner.span;
                return make_expr(ExprKind::Await(Box::new(inner)), span.join(end));
            }
            K::Number(text) => number_literal(&text, span)?,
            K::String(text) => ExprKind::String(text),
            K::Character(value) => ExprKind::Char(value),
            K::True => ExprKind::Boolean(true), K::False => ExprKind::Boolean(false),
            K::This => ExprKind::This,
            K::New => {
                let (class, _) = self.qualified_ident()?;
                let type_args = self.type_arguments()?;
                let variant = if self.eat(&K::Dot) { Some(self.ident()?.0) } else { None };
                let (args, end) = self.arguments()?;
                span = span.join(end);
                if let Some(variant) = variant {
                    ExprKind::Variant { class, type_args, variant, args }
                } else {
                    ExprKind::New { class, type_args, args }
                }
            }
            K::Ident(name) => {
                if let Some(call) = self.generic_static_call(&name, span)? { return Ok(call); }
                if self.at(&K::LParen) {
                    let (args, end) = self.arguments()?;
                    span = span.join(end);
                    ExprKind::Call { name, args }
                } else { ExprKind::Var(name) }
            }
            K::LParen => {
                if self.eat(&K::RParen) { return make_expr(ExprKind::Unit, span.join(self.tokens[self.position.index - 1].span)); }
                let mut expr = self.expression()?;
                expr.span = span.join(self.expect(&K::RParen, "`)`")?.span);
                return Ok(expr);
            }
            _ => return Err(Diagnostic::new(span, "expected an expression")),
        };
        make_expr(kind, span)
    }

}

fn binary_op(kind: &K) -> Option<(BinaryOp, u8)> {
    use BinaryOp as B;
    Some(match kind {
        K::OrOr => (B::Or, 1), K::AndAnd => (B::And, 2),
        K::Pipe => (B::BitOr, 3), K::Caret => (B::BitXor, 4),
        K::Ampersand => (B::BitAnd, 5),
        K::EqualEqual => (B::Eq, 6), K::BangEqual => (B::Ne, 6),
        K::Less => (B::Lt, 7), K::LessEqual => (B::Le, 7),
        K::Greater => (B::Gt, 7), K::GreaterEqual => (B::Ge, 7),
        K::ShiftLeft => (B::ShiftLeft, 8), K::ShiftRight => (B::ShiftRight, 8),
        K::Plus => (B::Add, 9), K::Minus => (B::Sub, 9),
        K::Star => (B::Mul, 10), K::Slash => (B::Div, 10), K::Percent => (B::Rem, 10),
        _ => return None,
    })
}
fn number_literal(text: &str, span: Span) -> Result<ExprKind, Diagnostic> {
    if let Some(magnitude) = text.strip_suffix('L') {
        let value = magnitude.parse::<i64>()
            .map_err(|_| Diagnostic::new(span, "[E6101] long literal is outside the range -9223372036854775808..=9223372036854775807"))?;
        return Ok(ExprKind::Long(value));
    }
    if text.contains('.') || text.contains('e') || text.contains('E') {
        let value = text.parse::<f64>()
            .ok().filter(|value| value.is_finite())
            .ok_or_else(|| Diagnostic::new(span, "[E6101] double literal is invalid or outside the finite range"))?;
        return Ok(ExprKind::Double(value));
    }
    let value = text.parse::<i32>()
        .map_err(|_| Diagnostic::new(span, "[E6001] int literal is outside the range -2147483648..=2147483647"))?;
    Ok(ExprKind::Int(value))
}

// Cap the AST height at construction, not only during type checking. This
// prevents a very long left-associative expression from creating a deep Box
// chain whose destructor could overflow the compiler's stack on an error.
fn make_expr(kind: ExprKind, span: Span) -> Result<Expr, Diagnostic> {
    let depth = 1 + match &kind {
        ExprKind::Binary { left, right, .. } => left.depth.max(right.depth),
        ExprKind::Unary { expr, .. } | ExprKind::Borrow { expr, .. } |
        ExprKind::Cast { expr, .. } | ExprKind::Try(expr) => expr.depth,
        ExprKind::Call { args, .. } | ExprKind::StaticCall { args, .. } |
        ExprKind::New { args, .. } | ExprKind::Variant { args, .. } =>
            args.iter().map(|arg| arg.depth).max().unwrap_or(0),
        ExprKind::MethodCall { receiver, args, .. } =>
            receiver.depth.max(args.iter().map(|arg| arg.depth).max().unwrap_or(0)),
        ExprKind::Field { base, .. } => base.depth,
        _ => 0,
    };
    if depth > 128 {
        return Err(Diagnostic::new(span, "expression is too deep"));
    }
    Ok(Expr { kind, span, depth })
}
