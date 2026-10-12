//! Bounded, deterministic monomorphization before nominal type checking.
//!
//! This V5 uses explicitly instantiated class/interface templates, NOT Java
//! erasure and NOT universally checked bounded generics. A template body is
//! checked for each requested specialization. Every specialization then passes
//! through the same type, ownership and borrow checkers as ordinary code.
use std::collections::{HashMap, HashSet, VecDeque};
use crate::ast::{self as a, Type};
use crate::diagnostic::{Diagnostic, Span};

const MAX_SPECIALIZATIONS: usize = 64;
const MAX_TYPE_NAME: usize = 4096;
type Environment = HashMap<String, Type>;

#[derive(Clone)]
struct Request { template: String, name: String, args: Vec<Type>, span: Span }
struct Expander {
    definitions: HashMap<String, a::Class>,
    requested: HashSet<String>,
    pending: VecDeque<Request>,
    names: Vec<String>,
}

/// The strings are only for inspection. No C symbol is based on a source name.
pub fn expand(mut program: a::Program) -> Result<(a::Program, Vec<String>), Diagnostic> {
    if program.classes.iter().filter(|c| !c.is_builtin).count() > 128 {
        return Err(Diagnostic::new(Span::default(), "limite V5: 128 classes ou interfaces utilisateur par programme, modeles compris"));
    }
    let mut expander = Expander { definitions: HashMap::new(), requested: HashSet::new(),
        pending: VecDeque::new(), names: Vec::new() };
    for class in &program.classes {
        if matches!(class.name.as_str(), "System" | "println") {
            return Err(Diagnostic::new(class.span, "nom de classe reserve au support d'affichage"));
        }
        if expander.definitions.insert(class.name.clone(), class.clone()).is_some() {
            return Err(Diagnostic::new(class.span, format!("classe ou interface `{}` deja declaree", class.name)));
        }
    }
    for class in &program.classes {
        if class.is_builtin { continue; }
        for name in &class.type_params {
            if expander.definitions.contains_key(name) || matches!(name.as_str(), "System" | "println" | "move" | "drop") {
                return Err(Diagnostic::new(class.span, format!("[E4101] le parametre de type `{name}` masque un nom de type ou un nom reserve")));
            }
        }
    }
    let empty = Environment::new();
    for function in &mut program.functions { expander.function(function, &empty)?; }
    let mut concrete = Vec::new();
    for mut class in program.classes {
        if class.type_params.is_empty() && class.native.is_none() {
            expander.class(&mut class, &empty)?;
            concrete.push(class);
        }
    }
    while let Some(request) = expander.pending.pop_front() {
        let mut class = expander.definitions[&request.template].clone();
        let env: Environment = class.type_params.iter().cloned().zip(request.args).collect();
        class.name = request.name.clone();
        if class.is_builtin { class.span = request.span; }
        class.type_params.clear();
        if let Some(constructor) = &mut class.constructor { constructor.name = class.name.clone(); }
        expander.class(&mut class, &env).map_err(|d| d.with_note(request.span,
            format!("specialisation `{}` demandee ici", request.name)))?;
        concrete.push(class);
        if concrete.iter().filter(|c| !c.is_builtin).count() > 128 {
            return Err(Diagnostic::new(request.span, "[E4199] limite de 128 types concrets, specialisations comprises"));
        }
    }
    program.classes = concrete;
    Ok((program, expander.names))
}

fn display_type(ty: &Type) -> String {
    match ty {
        Type::Int => "int".to_owned(), Type::Boolean => "boolean".to_owned(),
        Type::String => "String".to_owned(), Type::Void => "void".to_owned(),
        Type::Named(name) => name.clone(),
        Type::Applied { name, args } => format!("{}<{}>", name, args.iter().map(display_type).collect::<Vec<_>>().join(",")),
        Type::Borrow { name, args, mutable } => format!("borrow {}{}{}", if *mutable { "mut " } else { "" }, name,
            if args.is_empty() { String::new() } else { format!("<{}>", args.iter().map(display_type).collect::<Vec<_>>().join(",")) }),
    }
}

impl Expander {
    fn ty(&mut self, ty: &Type, env: &Environment, span: Span, depth: usize) -> Result<Type, Diagnostic> {
        if depth > 32 { return Err(Diagnostic::new(span, "[E4199] type generique trop profond (limite 32)")); }
        match ty {
            Type::Int | Type::Boolean | Type::String | Type::Void => Ok(ty.clone()),
            Type::Named(name) => {
                if let Some(replacement) = env.get(name) { return Ok(replacement.clone()); }
                if self.definitions.get(name).is_some_and(|c|
                    c.native.is_some() && c.type_params.is_empty()) && !self.requested.contains(name) {
                    if self.requested.len() >= MAX_SPECIALIZATIONS {
                        return Err(Diagnostic::new(span, "[E4199] limite de 64 specialisations et types natifs demandes"));
                    }
                    self.requested.insert(name.clone());
                    self.names.push(name.clone());
                    self.pending.push_back(Request { template: name.clone(), name: name.clone(), args: Vec::new(), span });
                }
                if self.definitions.get(name).is_some_and(|c| !c.type_params.is_empty()) {
                    return Err(Diagnostic::new(span, format!("[E4102] `{name}` exige des arguments de type explicites")));
                }
                Ok(ty.clone())
            }
            Type::Applied { name, args } => {
                if env.contains_key(name) {
                    return Err(Diagnostic::new(span, "[E4102] un parametre de type n'est pas lui-meme un constructeur generique"));
                }
                let arity = self.definitions.get(name).ok_or_else(|| Diagnostic::new(span,
                    format!("[E4102] type generique `{name}` inconnu")))?.type_params.len();
                if arity == 0 || arity != args.len() {
                    return Err(Diagnostic::new(span, format!("[E4102] `{name}` attend {arity} argument(s) de type, {} recu(s)", args.len())));
                }
                let args: Vec<_> = args.iter().map(|t| self.ty(t, env, span, depth + 1)).collect::<Result<_, _>>()?;
                for arg in &args {
                    if matches!(arg, Type::Void | Type::Borrow { .. }) {
                        return Err(Diagnostic::new(span, "[E4103] argument generique interdit: ni void ni emprunt; ecrire borrow Box<T> au point d'utilisation"));
                    }
                    if let Type::Named(n) = arg {
                        if !self.definitions.contains_key(n) && !self.requested.contains(n) {
                            return Err(Diagnostic::new(span, format!("[E4103] argument de type `{n}` inconnu")));
                        }
                    }
                }
                let key = format!("{}<{}>", name, args.iter().map(display_type).collect::<Vec<_>>().join(","));
                if key.len() > MAX_TYPE_NAME {
                    return Err(Diagnostic::new(span, "[E4199] nom de specialisation trop long; expansion recursive probable"));
                }
                if !self.requested.contains(&key) {
                    if self.requested.len() >= MAX_SPECIALIZATIONS {
                        return Err(Diagnostic::new(span, "[E4199] limite de 64 specialisations; expansion generique recursive possible"));
                    }
                    self.requested.insert(key.clone());
                    self.names.push(key.clone());
                    self.pending.push_back(Request { template: name.clone(), name: key.clone(), args, span });
                }
                Ok(Type::Named(key))
            }
            Type::Borrow { name, args, mutable } => {
                let base = if args.is_empty() { Type::Named(name.clone()) }
                    else { Type::Applied { name: name.clone(), args: args.clone() } };
                let base = self.ty(&base, env, span, depth + 1)?;
                let Type::Named(name) = base else {
                    return Err(Diagnostic::new(span, "[E4103] borrow exige un type objet, pas une primitive specialisee"));
                };
                Ok(Type::Borrow { name, args: Vec::new(), mutable: *mutable })
            }
        }
    }
    fn class(&mut self, class: &mut a::Class, env: &Environment) -> Result<(), Diagnostic> {
        if let Some(a::NativeClass::List(element)) = &mut class.native {
            *element = self.ty(element, env, class.span, 0)?;
            // Primitive elements copy; nominal elements require receiver-bound loans.
            if matches!(element, Type::Int | Type::Boolean | Type::String) {
                class.methods.retain(|method| !matches!(method.name.as_str(), "getRef" | "getMut"));
            } else {
                class.methods.retain(|method| !matches!(method.name.as_str(), "get" | "tryGet"));
            }
        }
        for interface in &mut class.implements { *interface = self.ty(interface, env, class.span, 0)?; }
        if let Some(variants) = &mut class.variants {
            for variant in variants {
                for field in &mut variant.fields {
                    *field = self.ty(field, env, variant.span, 0)?;
                }
            }
        }
        for field in &mut class.fields {
            field.ty = self.ty(&field.ty, env, field.span, 0)?;
            if let Some(value) = &mut field.initializer { self.expression(value, env)?; }
        }
        for method in &mut class.methods { self.function(method, env)?; }
        if let Some(constructor) = &mut class.constructor { self.function(constructor, env)?; }
        Ok(())
    }
    fn function(&mut self, function: &mut a::Function, env: &Environment) -> Result<(), Diagnostic> {
        function.return_type = self.ty(&function.return_type, env, function.span, 0)?;
        for param in &mut function.params { param.ty = self.ty(&param.ty, env, param.span, 0)?; }
        self.block(&mut function.body, env)
    }
    fn block(&mut self, block: &mut a::Block, env: &Environment) -> Result<(), Diagnostic> {
        use a::StmtKind as S;
        for stmt in block {
            match &mut stmt.kind {
                S::Let { ty, value, .. } => {
                    *ty = self.ty(ty, env, stmt.span, 0)?;
                    if let Some(value) = value { self.expression(value, env)?; }
                }
                S::Assign { target, value } => { self.expression(target, env)?; self.expression(value, env)?; }
                S::Expr(expr) => self.expression(expr, env)?,
                S::If { condition, then_block, else_block } => {
                    self.expression(condition, env)?;
                    self.block(then_block, env)?;
                    if let Some(block) = else_block { self.block(block, env)?; }
                }
                S::While { condition, body } => { self.expression(condition, env)?; self.block(body, env)?; }
                S::ForEach { ty, collection, body, .. } => {
                    *ty = self.ty(ty, env, stmt.span, 0)?;
                    self.expression(collection, env)?;
                    self.block(body, env)?;
                }
                S::Match { value, arms } => {
                    self.expression(value, env)?;
                    for arm in arms { self.block(&mut arm.body, env)?; }
                }
                S::Return(Some(expr)) => self.expression(expr, env)?,
                S::Block(block) => self.block(block, env)?,
                S::Return(None) | S::Break | S::Continue => {}
            }
        }
        Ok(())
    }
    fn expression(&mut self, expr: &mut a::Expr, env: &Environment) -> Result<(), Diagnostic> {
        use a::ExprKind as E;
        match &mut expr.kind {
            E::New { class, type_args, args } | E::Variant { class, type_args, args, .. } => {
                let ty = if type_args.is_empty() { Type::Named(class.clone()) }
                    else { Type::Applied { name: class.clone(), args: type_args.clone() } };
                let ty = self.ty(&ty, env, expr.span, 0)?;
                let Type::Named(name) = ty else {
                    return Err(Diagnostic::new(expr.span, "[E4103] construction exige un type objet concret"));
                };
                *class = name; type_args.clear();
                for arg in args { self.expression(arg, env)?; }
            }
            E::Field { base, .. } => self.expression(base, env)?,
            E::Call { args, .. } => { for arg in args { self.expression(arg, env)?; } }
            E::MethodCall { receiver, args, .. } => {
                if let E::Var(name) = &receiver.kind {
                    if self.definitions.get(name).is_some_and(|c|
                        matches!(c.native.as_ref(), Some(a::NativeClass::Console | a::NativeClass::Files | a::NativeClass::Args))) {
                        self.ty(&Type::Named(name.clone()), env, receiver.span, 0)?;
                    }
                }
                self.expression(receiver, env)?;
                for arg in args { self.expression(arg, env)?; }
            }
            E::Unary { expr, .. } | E::Borrow { expr, .. } => self.expression(expr, env)?,
            E::Binary { left, right, .. } => { self.expression(left, env)?; self.expression(right, env)?; }
            E::Int(_) | E::Boolean(_) | E::String(_) | E::Var(_) | E::This => {}
        }
        Ok(())
    }
}
