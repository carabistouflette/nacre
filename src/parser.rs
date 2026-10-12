//! Recursive descent for declarations/statements; precedence climbing for expressions.
use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::{Token, TokenKind as K};

pub fn parse(tokens: Vec<Token>) -> Result<Program, Diagnostic> {
    Parser { tokens, pos: 0, depth: 0 }.program()
}

struct Parser { tokens: Vec<Token>, pos: usize, depth: usize }

impl Parser {
    fn current(&self) -> &Token { &self.tokens[self.pos] }
    fn at(&self, kind: &K) -> bool { self.current().kind == *kind }
    fn bump(&mut self) -> Token {
        let token = self.current().clone();
        if token.kind != K::Eof { self.pos += 1; }
        token
    }
    fn eat(&mut self, kind: &K) -> bool {
        if self.at(kind) { self.bump(); true } else { false }
    }
    fn expect(&mut self, kind: &K, what: &str) -> Result<Token, Diagnostic> {
        if self.at(kind) { Ok(self.bump()) }
        else { Err(Diagnostic::new(self.current().span, format!("{what} attendu"))) }
    }
    fn ident(&mut self) -> Result<(String, Span), Diagnostic> {
        let token = self.bump();
        match token.kind {
            K::Ident(name) => Ok((name, token.span)),
            _ => Err(Diagnostic::new(token.span, "nom attendu")),
        }
    }
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, Diagnostic>) -> Result<T, Diagnostic> {
        if self.depth >= 128 {
            return Err(Diagnostic::new(self.current().span, "imbrication trop profonde pour cette V5"));
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }
    fn modifiers(&mut self) -> Result<(Visibility, bool, bool), Diagnostic> {
        let mut visibility = None;
        let mut is_static = false;
        let mut readonly = false;
        loop {
            let token = self.current().clone();
            match token.kind {
                K::Public | K::Private => {
                    if visibility.is_some() { return Err(Diagnostic::new(token.span, "visibilite declaree plusieurs fois")); }
                    visibility = Some(if self.eat(&K::Public) { Visibility::Public } else { self.bump(); Visibility::Private });
                }
                K::Static => {
                    if is_static { return Err(Diagnostic::new(token.span, "modificateur static duplique")); }
                    self.bump(); is_static = true;
                }
                K::Readonly => {
                    if readonly { return Err(Diagnostic::new(token.span, "modificateur readonly duplique")); }
                    self.bump(); readonly = true;
                }
                _ => break,
            }
        }
        // V5 has one source unit and no packages: omitted visibility means public.
        Ok((visibility.unwrap_or(Visibility::Public), is_static, readonly))
    }
    fn program(mut self) -> Result<Program, Diagnostic> {
        let mut functions = Vec::new();
        let mut classes = Vec::new();
        while !self.at(&K::Eof) {
            let start = self.current().span;
            let (visibility, is_static, readonly) = self.modifiers()?;
            if self.eat(&K::Class) {
                if is_static || readonly || visibility == Visibility::Private {
                    return Err(Diagnostic::new(start, "une classe de premier niveau ne peut pas etre private, static ou readonly"));
                }
                classes.push(self.class(start, false)?);
            } else if self.eat(&K::Enum) {
                if is_static || readonly || visibility == Visibility::Private {
                    return Err(Diagnostic::new(start, "une enum de premier niveau ne peut pas etre private, static ou readonly"));
                }
                classes.push(self.enum_decl(start)?);
            } else if self.eat(&K::Interface) {
                if is_static || readonly || visibility == Visibility::Private {
                    return Err(Diagnostic::new(start, "[E4001] modificateur invalide sur une interface"));
                }
                classes.push(self.class(start, true)?);
            } else {
                let return_type = self.ty(true)?;
                let (name, _) = self.ident()?;
                if readonly { return Err(Diagnostic::new(start, "readonly est reserve aux methodes d'instance")); }
                functions.push(self.function_tail(name, return_type, visibility, true, false, start)?);
            }
        }
        Ok(Program { functions, classes })
    }
    fn enum_decl(&mut self, start: Span) -> Result<Class, Diagnostic> {
        let (name, _) = self.ident()?;
        let mut type_params = Vec::new();
        if self.eat(&K::Less) {
            loop {
                let (param, span) = self.ident()?;
                if type_params.contains(&param) {
                    return Err(Diagnostic::new(span, "[E4101] parametre de type duplique"));
                }
                type_params.push(param);
                if type_params.len() > 8 {
                    return Err(Diagnostic::new(start, "[E4199] limite de 8 parametres de type"));
                }
                if !self.eat(&K::Comma) { break; }
            }
            self.expect(&K::Greater, "`>`")?;
        }
        if self.at(&K::Implements) {
            return Err(Diagnostic::new(self.current().span, "une enum ne peut pas implementer une interface"));
        }
        self.expect(&K::LBrace, "`{`")?;
        let mut variants: Vec<Variant> = Vec::new();
        while !self.at(&K::RBrace) && !self.at(&K::Eof) {
            let (variant, variant_start) = self.ident()?;
            if variants.iter().any(|known| known.name == variant) {
                return Err(Diagnostic::new(variant_start, format!("variant `{variant}` deja declaree dans `{name}`")));
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
            let end = self.expect(&K::Semicolon, "`;` apres une variant")?.span;
            variants.push(Variant { name: variant, fields, span: variant_start.join(end) });
        }
        let end = self.expect(&K::RBrace, "`}`")?.span;
        if variants.is_empty() {
            return Err(Diagnostic::new(start.join(end), "une enum doit declarer au moins une variant"));
        }
        Ok(Class { native: None, is_builtin: false, name, span: start.join(end), fields: Vec::new(), methods: Vec::new(),
            constructor: None, type_params, implements: Vec::new(), is_interface: false, variants: Some(variants) })
    }
    fn class(&mut self, start: Span, is_interface: bool) -> Result<Class, Diagnostic> {
        let (name, _) = self.ident()?;
        let mut type_params = Vec::new();
        if self.eat(&K::Less) {
            loop {
                let (param, span) = self.ident()?;
                if type_params.contains(&param) {
                    return Err(Diagnostic::new(span, "[E4101] parametre de type duplique"));
                }
                type_params.push(param);
                if type_params.len() > 8 {
                    return Err(Diagnostic::new(start, "[E4199] limite de 8 parametres de type"));
                }
                if !self.eat(&K::Comma) { break; }
            }
            self.expect(&K::Greater, "`>`")?;
        }
        let mut implements = Vec::new();
        if self.eat(&K::Implements) {
            if is_interface { return Err(Diagnostic::new(start, "[E4001] implements est reserve aux classes")); }
            loop {
                let ty = self.ty(false)?;
                if !matches!(ty, Type::Named(_) | Type::Applied { .. }) {
                    return Err(Diagnostic::new(start, "[E4001] nom d'interface attendu apres implements"));
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
            let (visibility, is_static, readonly) = self.modifiers()?;
            if is_interface {
                if visibility != Visibility::Public || is_static {
                    return Err(Diagnostic::new(member_start, "[E4001] une interface declare des methodes publiques d'instance"));
                }
                let return_type = self.ty(true)?;
                let (method_name, _) = self.ident()?;
                let params = self.parameters()?;
                let end = self.expect(&K::Semicolon, "`;` (pas de corps, de champ ou de methode default dans une interface V5)")?.span;
                methods.push(Function { name: method_name, span: member_start.join(end), return_type,
                    params, body: Vec::new(), visibility, is_static: false, readonly });
                continue;
            }
            if matches!(&self.current().kind, K::Ident(n) if n == &name)
                && self.tokens.get(self.pos + 1).is_some_and(|t| t.kind == K::LParen) {
                self.bump();
                if is_static || readonly { return Err(Diagnostic::new(member_start, "un constructeur ne peut pas etre static ou readonly")); }
                if constructor.is_some() { return Err(Diagnostic::new(member_start, "un seul constructeur par classe en V5; pas de surcharge")); }
                constructor = Some(self.function_tail(name.clone(), Type::Void, visibility, false, false, member_start)?);
                continue;
            }
            let ty = self.ty(true)?;
            let (member_name, _) = self.ident()?;
            if self.at(&K::LParen) {
                if readonly && is_static { return Err(Diagnostic::new(member_start, "readonly est reserve aux methodes d'instance")); }
                methods.push(self.function_tail(member_name, ty, visibility, is_static, readonly, member_start)?);
            } else {
                if readonly { return Err(Diagnostic::new(member_start, "readonly est un modificateur de methode, pas de champ")); }
                if is_static { return Err(Diagnostic::new(member_start, "les champs static ne sont pas pris en charge en V5")); }
                if ty == Type::Void { return Err(Diagnostic::new(member_start, "un champ ne peut pas avoir le type void")); }
                let initializer = if self.eat(&K::Equal) { Some(self.expression()?) } else { None };
                let end = self.expect(&K::Semicolon, "`;`")?.span;
                fields.push(Field { name: member_name, ty, visibility, initializer, span: member_start.join(end) });
            }
        }
        let end = self.expect(&K::RBrace, "`}`")?.span;
        Ok(Class { native: None, is_builtin: false, name, span: start.join(end), fields, methods, constructor, type_params, implements, is_interface,
            variants: None })
    }
    fn ty(&mut self, allow_void: bool) -> Result<Type, Diagnostic> {
        let token = self.bump();
        match token.kind {
            K::Borrow => {
                let mutable = self.eat(&K::Mut);
                let (name, _) = self.ident().map_err(|_| Diagnostic::new(token.span, "borrow attend un nom de classe (pas une primitive)"))?;
                let args = self.type_arguments()?;
                Ok(Type::Borrow { name, args, mutable })
            }
            K::Int => Ok(Type::Int), K::Boolean => Ok(Type::Boolean),
            K::StringType => Ok(Type::String), K::Ident(name) => {
                let args = self.type_arguments()?;
                Ok(if args.is_empty() { Type::Named(name) } else { Type::Applied { name, args } })
            },
            K::Void if allow_void => Ok(Type::Void),
            K::Void => Err(Diagnostic::new(token.span, "`void` est reserve au type de retour d'une fonction")),
            _ => Err(Diagnostic::new(token.span, "type attendu: int, boolean, String ou nom de classe")),
        }
    }
    fn type_arguments(&mut self) -> Result<Vec<Type>, Diagnostic> {
        if !self.eat(&K::Less) { return Ok(Vec::new()); }
        self.nested(|p| {
            if p.at(&K::Greater) {
                return Err(Diagnostic::new(p.current().span, "[E4101] les arguments generiques sont explicites; pas de diamant <> en V5"));
            }
            let mut args = Vec::new();
            loop {
                args.push(p.ty(false)?);
                if args.len() > 8 { return Err(Diagnostic::new(p.current().span, "[E4199] limite de 8 arguments de type")); }
                if !p.eat(&K::Comma) { break; }
            }
            p.expect(&K::Greater, "`>`")?;
            Ok(args)
        })
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
                     is_static: bool, readonly: bool, start: Span) -> Result<Function, Diagnostic> {
        let params = self.parameters()?;
        let body = self.block()?;
        let end = self.tokens[self.pos - 1].span;
        Ok(Function { name, span: start.join(end), return_type, params, body, visibility, is_static, readonly })
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
        if matches!(self.current().kind, K::Int | K::Boolean | K::StringType | K::Borrow) { return true; }
        if !matches!(self.current().kind, K::Ident(_)) { return false; }
        let save = self.pos;
        let result = self.ty(false).is_ok() && matches!(self.current().kind, K::Ident(_));
        self.pos = save;
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
                    let arm_end = self.tokens[self.pos - 1].span;
                    arms.push(MatchArm { variant, bindings, body, span: arm_start.join(arm_end) });
                    self.eat(&K::Comma);
                }
                self.expect(&K::RBrace, "`}` apres les arms de match")?;
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
                        return Err(Diagnostic::new(expr.span, "cible d'affectation invalide: variable ou champ attendu"));
                    }
                    StmtKind::Assign { target: expr, value: self.expression()? }
                } else { StmtKind::Expr(expr) };
                self.expect(&K::Semicolon, "`;`")?;
                result
            }
        };
        let end = self.tokens[self.pos - 1].span;
        Ok(Stmt { kind, span: start.join(end) })
    }
    fn expression(&mut self) -> Result<Expr, Diagnostic> { self.binary(1) }
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
            let op = match p.current().kind {
                K::Minus => Some(UnaryOp::Neg), K::Bang => Some(UnaryOp::Not),
                K::Plus => Some(UnaryOp::Plus), _ => None,
            };
            if let Some(op) = op {
                let start = p.bump().span;
                // The magnitude 2147483648 is only legal directly after unary minus.
                if op == UnaryOp::Neg {
                    if let K::Number(text) = &p.current().kind {
                        if text.parse::<u64>() == Ok(2_147_483_648) {
                            let end = p.bump().span;
                            return make_expr(ExprKind::Int(i32::MIN), start.join(end));
                        }
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
        while self.eat(&K::Dot) {
            let (name, end) = self.ident()?;
            let span = expr.span.join(end);
            expr = if self.at(&K::LParen) {
                let (args, end) = self.arguments()?;
                make_expr(ExprKind::MethodCall { receiver: Box::new(expr), name, args }, span.join(end))?
            } else {
                make_expr(ExprKind::Field { base: Box::new(expr), name }, span)?
            };
        }
        Ok(expr)
    }
    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.bump();
        let mut span = token.span;
        let kind = match token.kind {
            K::Number(text) => ExprKind::Int(text.parse::<i32>().map_err(|_| Diagnostic::new(span, "entier hors limites: -2147483648 a 2147483647"))?),
            K::String(text) => ExprKind::String(text),
            K::True => ExprKind::Boolean(true), K::False => ExprKind::Boolean(false),
            K::This => ExprKind::This,
            K::New => {
                let (class, _) = self.ident()?;
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
                if self.at(&K::LParen) {
                    let (args, end) = self.arguments()?;
                    span = span.join(end);
                    ExprKind::Call { name, args }
                } else { ExprKind::Var(name) }
            }
            K::LParen => {
                let mut expr = self.expression()?;
                expr.span = span.join(self.expect(&K::RParen, "`)`")?.span);
                return Ok(expr);
            }
            _ => return Err(Diagnostic::new(span, "expression attendue")),
        };
        make_expr(kind, span)
    }

}

fn binary_op(kind: &K) -> Option<(BinaryOp, u8)> {
    use BinaryOp as B;
    Some(match kind {
        K::OrOr => (B::Or, 1), K::AndAnd => (B::And, 2),
        K::EqualEqual => (B::Eq, 3), K::BangEqual => (B::Ne, 3),
        K::Less => (B::Lt, 4), K::LessEqual => (B::Le, 4),
        K::Greater => (B::Gt, 4), K::GreaterEqual => (B::Ge, 4),
        K::Plus => (B::Add, 5), K::Minus => (B::Sub, 5),
        K::Star => (B::Mul, 6), K::Slash => (B::Div, 6), K::Percent => (B::Rem, 6),
        _ => return None,
    })
}

// Cap the AST height at construction, not only during type checking. This
// prevents a very long left-associative expression from creating a deep Box
// chain whose destructor could overflow the compiler's stack on an error.
fn make_expr(kind: ExprKind, span: Span) -> Result<Expr, Diagnostic> {
    let depth = 1 + match &kind {
        ExprKind::Binary { left, right, .. } => left.depth.max(right.depth),
        ExprKind::Unary { expr, .. } => expr.depth,
        ExprKind::Call { args, .. } | ExprKind::New { args, .. } | ExprKind::Variant { args, .. } =>
            args.iter().map(|arg| arg.depth).max().unwrap_or(0),
        ExprKind::MethodCall { receiver, args, .. } => receiver.depth.max(args.iter().map(|arg| arg.depth).max().unwrap_or(0)),
        ExprKind::Field { base, .. } => base.depth,
        _ => 0,
    };
    if depth > 128 {
        return Err(Diagnostic::new(span, "expression trop profonde pour cette V5"));
    }
    Ok(Expr { kind, span, depth })
}
