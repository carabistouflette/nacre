//! Name resolution, nominal types, visibility and call-site receiver locks.
//! Definite initialization and moves across control flow are checked separately
//! by ownership.rs. Object parameters consume; borrowed returns are receiver-bound.
use std::collections::{HashMap, HashSet};
use crate::ast::{self as a, BinaryOp as B, UnaryOp, Visibility};
use crate::diagnostic::{Diagnostic, Span};
use crate::ir::{self as t, CallTarget, Place, PlaceKind, Type};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind { Free, Static, Instance, Constructor, Interface }
#[derive(Clone)]
struct Signature {
    id: usize, owner: Option<usize>, kind: Kind, name: String,
    params: Vec<Type>, return_type: Type, visibility: Visibility, readonly: bool, virtual_slot: Option<usize>,
}
struct FieldInfo { name: String, ty: Type, visibility: Visibility }
struct ClassInfo {
    native: Option<t::NativeClass>,
    name: String, fields: Vec<FieldInfo>, variants: Option<Vec<t::Variant>>,
    methods: HashMap<String, usize>, constructor: Option<usize>, has_borrow: bool,
    is_interface: bool, interface_methods: Vec<usize>, implementations: Vec<(usize, Vec<usize>)>,
}
struct Tables {
    names: HashMap<String, usize>, classes: Vec<ClassInfo>,
    globals: HashMap<String, usize>, signatures: Vec<Signature>,
}
impl Tables {
    fn resolve_type(&self, ty: &a::Type, span: Span) -> Result<Type, Diagnostic> {
        Ok(match ty {
            a::Type::Byte => Type::Byte, a::Type::Char => Type::Char,
            a::Type::Int => Type::Int, a::Type::Long => Type::Long, a::Type::Double => Type::Double,
            a::Type::Unit => Type::Unit, a::Type::Boolean => Type::Boolean,
            a::Type::String => Type::String, a::Type::Void => Type::Void,
            a::Type::Named(name) => {
                let id = *self.names.get(name).ok_or_else(|| Diagnostic::new(span, format!("class `{name}` is unknown")))?;
                if self.is_static_native(id) {
                    return Err(Diagnostic::new(span, format!("`{name}` is a static native class and cannot be used as a value")));
                }
                Type::Object(id)
            }
            a::Type::Applied { .. } => return Err(Diagnostic::new(span, "[E4102] unspecialized generic type")),
            a::Type::Borrow { name, mutable, .. } => {
                let class = *self.names.get(name).ok_or_else(|| Diagnostic::new(span, format!("class `{name}` is unknown")))?;
                if self.is_static_native(class) {
                    return Err(Diagnostic::new(span, format!("`{name}` is a static native class; its values cannot be borrowed")));
                }
                Type::Borrow { class, mutable: *mutable }
            }
        })
    }
    fn can_upcast(&self, from: usize, to: usize) -> bool {
        from == to || (self.classes[to].is_interface &&
            self.classes[from].implementations.iter().any(|(interface, _)| *interface == to))
    }
    fn is_static_native(&self, id: usize) -> bool {
        matches!(self.classes[id].native, Some(t::NativeClass::Console | t::NativeClass::Files | t::NativeClass::Args))
    }
    fn type_name(&self, ty: Type) -> String {
        match ty {
            Type::Byte => "byte".to_owned(), Type::Char => "char".to_owned(),
            Type::Int => "int".to_owned(), Type::Long => "long".to_owned(),
            Type::Double => "double".to_owned(), Type::Unit => "Unit".to_owned(),
            Type::Boolean => "boolean".to_owned(), Type::String => "String".to_owned(),
            Type::Void => "void".to_owned(),
            Type::Object(id) => self.classes[id].name.clone(),
            Type::Borrow { class, mutable } => format!("borrow {}{}", if mutable { "mut " } else { "" }, self.classes[class].name),
        }
    }
    fn is_view(&self, ty: Type) -> bool {
        ty.object_class().is_some_and(|id| self.classes[id].has_borrow)
    }
    fn map_key_binding_type(&self, key: a::MapKeyKind) -> Type {
        match key {
            a::MapKeyKind::Int => Type::Int,
            a::MapKeyKind::Text => {
                let class = self.classes.iter().position(|class| matches!(class.native, Some(t::NativeClass::Text)))
                    .expect("native Text class accompanies Map<Text, V>");
                Type::Borrow { class, mutable: false }
            }
        }
    }
    fn add_signature(&mut self, f: &a::Function, owner: Option<usize>, kind: Kind) -> Result<usize, Diagnostic> {
        if matches!(f.name.as_str(), "println" | "move" | "drop") { return Err(Diagnostic::new(f.span, format!("`{}` is a reserved built-in function", f.name))); }
        let params: Vec<Type> = f.params.iter().map(|p| self.resolve_type(&p.ty, p.span)).collect::<Result<_, _>>()?;
        let return_type = self.resolve_type(&f.return_type, f.span)?;
        if return_type.is_borrow() && !matches!(kind, Kind::Instance | Kind::Interface) {
            return Err(Diagnostic::new(f.span, "[E3004] borrowed returns require an instance receiver"));
        }
        if matches!(return_type, Type::Borrow { mutable: true, .. }) && f.readonly {
            return Err(Diagnostic::new(f.span, "[E3003] a readonly receiver cannot return an exclusive borrow"));
        }
        if self.is_view(return_type) || params.iter().any(|ty| self.is_view(*ty)) {
            return Err(Diagnostic::new(f.span, "[E3007] a class with borrowed fields is a local view; local views cannot be parameters or return values"));
        }
        let id = self.signatures.len();
        let virtual_slot = if kind == Kind::Interface { Some(self.classes[owner.unwrap()].interface_methods.len()) } else { None };
        self.signatures.push(Signature { id, owner, kind, name: f.name.clone(), params, return_type, visibility: f.visibility, readonly: f.readonly, virtual_slot });
        Ok(id)
    }
}

fn numeric_common(left: Type, right: Type) -> Option<Type> {
    let left = left.promoted_numeric()?;
    let right = right.promoted_numeric()?;
    Some(if left.numeric_rank()? >= right.numeric_rank()? { left } else { right })
}

fn coerce_numeric(value: t::Expr, target: Type) -> t::Expr {
    if value.ty == target { return value; }
    let span = value.span;
    t::Expr { kind: t::ExprKind::Cast { value: Box::new(value) }, ty: target, span }
}

pub fn check(program: &a::Program) -> Result<t::Program, Diagnostic> {
    if program.classes.iter().filter(|c| !c.is_builtin).count() > 128 {
        return Err(Diagnostic::new(Span::default(), "limit: 128 user classes per program"));
    }
    let mut tables = Tables { names: HashMap::new(), classes: Vec::new(), globals: HashMap::new(), signatures: Vec::new() };
    for (id, class) in program.classes.iter().enumerate() {
        if class.name == "System" || class.name == "println" {
            return Err(Diagnostic::new(class.span, "class name reserved for output support"));
        }
        if tables.names.insert(class.qualified_name.clone(), id).is_some() {
            return Err(Diagnostic::new(class.span, format!("type `{}` is declared more than once", class.qualified_name)));
        }
        tables.classes.push(ClassInfo { native: None, name: class.qualified_name.clone(), fields: Vec::new(),
            variants: None, methods: HashMap::new(), constructor: None, has_borrow: false,
            is_interface: class.is_interface, interface_methods: Vec::new(), implementations: Vec::new() });
    }
    for (id, class) in program.classes.iter().enumerate() {
        tables.classes[id].native = match &class.native {
            Some(a::NativeClass::Text) => Some(t::NativeClass::Text),
            Some(a::NativeClass::List(_)) => Some(t::NativeClass::List(Type::Void)),
            Some(a::NativeClass::Map { key_kind: Some(key_kind), .. }) =>
                Some(t::NativeClass::Map(*key_kind, Type::Void)),
            Some(a::NativeClass::Map { .. }) =>
                return Err(Diagnostic::new(class.span, "[E5004] Map<K, V> key classification is missing")),
            Some(a::NativeClass::Console) => Some(t::NativeClass::Console),
            Some(a::NativeClass::Files) => Some(t::NativeClass::Files),
            Some(a::NativeClass::Args) => Some(t::NativeClass::Args),
            None => None,
        };
    }
    for (id, class) in program.classes.iter().enumerate() {
        match &class.native {
            Some(a::NativeClass::List(element)) => {
                tables.classes[id].native = Some(t::NativeClass::List(tables.resolve_type(element, class.span)?));
            }
            Some(a::NativeClass::Map { value, .. }) => {
                let Some(t::NativeClass::Map(key_kind, _)) = tables.classes[id].native
                    else { unreachable!("specialized native Map key metadata"); };
                tables.classes[id].native = Some(t::NativeClass::Map(
                    key_kind, tables.resolve_type(value, class.span)?,
                ));
            }
            _ => {}
        }
        let variants = class.variants.as_ref().map(|variants| {
            variants.iter().map(|variant| Ok(t::Variant {
                name: variant.name.clone(),
                fields: variant.fields.iter().map(|field| tables.resolve_type(field, variant.span))
                    .collect::<Result<_, _>>()?,
            })).collect::<Result<Vec<_>, Diagnostic>>()
        }).transpose()?;
        tables.classes[id].variants = variants;
        let mut names = HashSet::new();
        for field in &class.fields {
            if field.name == "System" {
                return Err(Diagnostic::new(field.span, "`System` is a reserved name"));
            }
            if !names.insert(&field.name) {
                return Err(Diagnostic::new(field.span, format!("field `{}` is already declared", field.name)));
            }
            let ty = tables.resolve_type(&field.ty, field.span)?;
            if matches!(ty, Type::Borrow { mutable: true, .. }) {
                return Err(Diagnostic::new(field.span, "[E3007] borrowed fields are read-only; `borrow mut` is reserved for variables and parameters"));
            }
            if ty.is_borrow() && field.initializer.is_some() {
                return Err(Diagnostic::new(field.span, "[E3007] initialize a borrowed field in the constructor directly from a borrow parameter"));
            }
            tables.classes[id].fields.push(FieldInfo { name: field.name.clone(), ty, visibility: field.visibility });
        }
    }
    // View-ness propagates through class fields and every enum payload.
    let mut has_borrow = vec![false; program.classes.len()];
    loop {
        let mut changed = false;
        for (id, info) in tables.classes.iter().enumerate() {
            let contains_view = |ty: Type| ty.is_borrow()
                || ty.object_class().is_some_and(|child| has_borrow[child]);
            if !has_borrow[id] && (info.fields.iter().any(|f| contains_view(f.ty))
                || info.variants.as_ref().is_some_and(|variants|
                    variants.iter().any(|v| v.fields.iter().copied().any(contains_view)))) {
                has_borrow[id] = true;
                changed = true;
            }
        }
        if !changed { break; }
    }
    for (info, value) in tables.classes.iter_mut().zip(has_borrow) { info.has_borrow = value; }
    for (id, class) in program.classes.iter().enumerate() {
        match tables.classes[id].native {
            Some(t::NativeClass::List(element)) => {
                if tables.is_view(element) || matches!(element, Type::Void | Type::Borrow { .. }) {
                    return Err(Diagnostic::new(class.span, "[E5003] List<T> requires a copied value or owned object; local views and borrows are not allowed"));
                }
            }
            Some(t::NativeClass::Map(_, value)) if tables.is_view(value) || matches!(value, Type::Void | Type::Borrow { .. }) => {
                return Err(Diagnostic::new(class.span, "[E5005] Map<K, V> requires an owned value without a borrow or local view"));
            }
            _ => {}
        }
    }
    for class in &program.classes {
        for field in &class.fields {
            if tables.is_view(tables.resolve_type(&field.ty, field.span)?) {
                return Err(Diagnostic::new(field.span, "[E3007] a local view cannot be stored in or borrowed by another object"));
            }
        }
        if let Some(variants) = &class.variants {
            for variant in variants {
                for field in &variant.fields {
                    let ty = tables.resolve_type(field, variant.span)?;
                    if ty.is_borrow() || tables.is_view(ty) {
                        return Err(Diagnostic::new(variant.span,
                            "[E6003] an enum payload cannot be a borrow or contain a local view"));
                    }
                }
            }
        }
    }
    // Inline enum fields must have a finite C layout even when another
    // alternative could terminate at runtime.
    fn inline_visit(id: usize, tables: &Tables, states: &mut [u8]) -> bool {
        if states[id] == 1 { return false; }
        if states[id] == 2 { return true; }
        states[id] = 1;
        if let Some(variants) = &tables.classes[id].variants {
            for field in variants.iter().flat_map(|variant| &variant.fields) {
                if let Type::Object(child) = field {
                    if tables.classes[*child].variants.is_some() && !inline_visit(*child, tables, states) {
                        return false;
                    }
                }
            }
        }
        states[id] = 2;
        true
    }
    let mut inline_states = vec![0; program.classes.len()];
    for (id, class) in program.classes.iter().enumerate() {
        if class.variants.is_some() && !inline_visit(id, &tables, &mut inline_states) {
            return Err(Diagnostic::new(class.span, "inline enum representation cycle: infinite C size"));
        }
    }

    // Owned AST declarations keep inferred default constructors independent
    // from parser lifetimes. Their order also fixes stable C symbol IDs.
    let mut declarations: Vec<a::Function> = Vec::new();
    for function in &program.functions {
        let id = tables.add_signature(function, None, Kind::Free)?;
        if tables.globals.insert(function.qualified_name.clone(), id).is_some() {
            return Err(Diagnostic::new(function.span, format!("function `{}` is declared more than once; overloading is not supported", function.qualified_name)));
        }
        declarations.push(function.clone());
    }
    for (class_id, class) in program.classes.iter().enumerate() {
        for method in &class.methods {
            let kind = if class.is_interface { Kind::Interface } else if method.is_static { Kind::Static } else { Kind::Instance };
            let id = tables.add_signature(method, Some(class_id), kind)?;
            if tables.classes[class_id].methods.insert(method.name.clone(), id).is_some() {
                return Err(Diagnostic::new(method.span, format!("method `{}` is already declared; overloading is not supported", method.name)));
            }
            if class.is_interface { tables.classes[class_id].interface_methods.push(id); }
            declarations.push(method.clone());
        }
        if class.is_interface || class.variants.is_some() || tables.is_static_native(class_id) { continue; }
        let constructor = class.constructor.clone().unwrap_or_else(|| a::Function {
            name: class.name.clone(), span: class.span, return_type: a::Type::Void,
            params: Vec::new(), body: Vec::new(), visibility: Visibility::Public, is_static: false, readonly: false,
            qualified_name: format!("{}::{}", class.qualified_name, class.name), unit: class.unit,
        });
        let id = tables.add_signature(&constructor, Some(class_id), Kind::Constructor)?;
        tables.classes[class_id].constructor = Some(id);
        declarations.push(constructor);
    }
    // Nominal implements contracts are exact, including borrow modes and readonly.
    // All signatures are registered before validating forward implementations.
    for (class_id, class) in program.classes.iter().enumerate() {
        let mut implemented = HashSet::new();
        for interface in &class.implements {
            let Type::Object(interface_id) = tables.resolve_type(interface, class.span)? else {
                return Err(Diagnostic::new(class.span, "[E4001] `implements` requires an interface"));
            };
            if !tables.classes[interface_id].is_interface {
                return Err(Diagnostic::new(class.span, format!("[E4001] `{}` is not an interface", tables.classes[interface_id].name)));
            }
            if !implemented.insert(interface_id) {
                return Err(Diagnostic::new(class.span, "[E4001] interface is implemented more than once"));
            }
            if tables.is_view(Type::Object(class_id)) {
                return Err(Diagnostic::new(class.span, "[E4005] a local view cannot be erased behind an interface"));
            }
            let mut methods = Vec::new();
            for id in &tables.classes[interface_id].interface_methods {
                let expected = &tables.signatures[*id];
                let implementation = *tables.classes[class_id].methods.get(&expected.name)
                    .ok_or_else(|| Diagnostic::new(class.span, format!("[E4002] missing implementation of `{}.{}`", tables.classes[interface_id].name, expected.name)))?;
                let actual = &tables.signatures[implementation];
                if actual.kind != Kind::Instance || actual.visibility != Visibility::Public
                    || actual.params != expected.params || actual.return_type != expected.return_type
                    || actual.readonly != expected.readonly {
                    return Err(Diagnostic::new(declarations[implementation].span, format!("[E4003] incompatible signature for `{}.{}`: check public visibility, readonly, return type, and parameter modes", tables.classes[interface_id].name, expected.name))
                        .with_note(declarations[*id].span, "interface contract declared here"));
                }
                methods.push(implementation);
            }
            tables.classes[class_id].implementations.push((interface_id, methods));
        }
    }
    // Ordinary objects still require mandatory fields. Enums are alternatives:
    // one finite variant is a real base case; native List<T> and Map<K,V> are empty.
    let mut productive = vec![false; program.classes.len()];
    loop {
        let mut changed = false;
        for (id, info) in tables.classes.iter().enumerate() {
            if productive[id] { continue; }
            let productive_type = |ty: Type| match ty {
                Type::Object(child) => productive[child],
                Type::Byte | Type::Char | Type::Int | Type::Long | Type::Double | Type::Unit |
                Type::Boolean | Type::String | Type::Void | Type::Borrow { .. } => true,
            };
            let can_construct = if info.native.is_some() {
                true
            } else if info.is_interface {
                tables.classes.iter().enumerate().any(|(candidate, class)|
                    productive[candidate] && class.implementations.iter().any(|(interface, _)| *interface == id))
            } else {
                info.variants.as_ref().map_or_else(
                    || info.fields.iter().all(|field| productive_type(field.ty)),
                    |variants| variants.iter().any(|variant| variant.fields.iter().copied().all(productive_type)))
            };
            if can_construct { productive[id] = true; changed = true; }
        }
        if !changed { break; }
    }
    for (id, class) in program.classes.iter().enumerate() {
        if !productive[id] && !class.is_interface {
            return Err(Diagnostic::new(class.span,
                "cycle of object fields without an empty variant: no finite construction path"));
        }
    }
    let entries: Vec<_> = tables.signatures.iter().filter(|s| s.name == "main" && matches!(s.kind, Kind::Free | Kind::Static)).collect();
    if entries.len() > 1 {
        return Err(Diagnostic::new(Span::default(), "ambiguous entry point: multiple free functions or static methods named `main`"));
    }
    let main = entries.first().ok_or_else(|| Diagnostic::new(Span::default(), "missing `void main()` entry point; only static methods can be entry points"))?;
    if main.return_type != Type::Void || !main.params.is_empty() {
        return Err(Diagnostic::new(declarations[main.id].span, "`main` must have the signature `void main()` with no parameters"));
    }
    let main_id = main.id;
    let mut functions = Vec::new();
    let mut borrowed_from: Vec<Vec<Option<usize>>> = tables.classes.iter().map(|c| vec![None; c.fields.len()]).collect();
    for (id, function) in declarations.iter().enumerate() {
        let sig = &tables.signatures[id];
        let receiver = if matches!(sig.kind, Kind::Instance | Kind::Constructor | Kind::Interface) { sig.owner } else { None };
        let mut checker = Checker {
            tables: &tables, owner: sig.owner, receiver, constructor: sig.kind == Kind::Constructor, readonly: sig.readonly,
            scopes: vec![HashMap::new()], variables: Vec::new(),
            locks: Vec::new(), loop_depth: 0, block_depth: 0, expr_depth: 0,
            return_type: sig.return_type, parameter_ids: Vec::new(),
            field_sources: receiver.map_or_else(Vec::new, |c| vec![None; tables.classes[c].fields.len()]),
            initialized: receiver.map_or_else(Vec::new, |c| vec![sig.kind != Kind::Constructor; tables.classes[c].fields.len()]),
        };
        let mut params = Vec::new();
        for (param, ty) in function.params.iter().zip(&sig.params) {
            let var = checker.declare(&param.name, *ty, param.span)?;
            params.push((var, *ty));
            checker.parameter_ids.push(var);
        }
        if let Some(native_class) = sig.owner.and_then(|c| tables.classes[c].native) {
            let native = crate::stdlib::method(native_class, &function.name, sig.kind == Kind::Constructor);
            functions.push(t::Function { native: Some(native), id,
                name: format!("{}.{}", tables.classes[sig.owner.unwrap()].name, function.name),
                receiver, readonly: sig.readonly, return_type: sig.return_type, params,
                body: Vec::new(), locals: checker.variables, virtual_slot: None });
            continue;
        }
        if sig.kind == Kind::Interface {
            functions.push(t::Function { native: None, id, name: format!("{}.{}", tables.classes[sig.owner.unwrap()].name, function.name),
                receiver, readonly: sig.readonly, return_type: sig.return_type, params,
                body: Vec::new(), locals: checker.variables, virtual_slot: sig.virtual_slot });
            continue;
        }
        let mut body = Vec::new();
        if checker.constructor {
            let class = &program.classes[sig.owner.unwrap()];
            // Initializers see fields, not constructor parameters.
            let saved = std::mem::take(&mut checker.scopes);
            checker.scopes.push(HashMap::new());
            checker.block_depth = 1;
            for field in &class.fields {
                if let Some(value) = &field.initializer {
                    let target = a::Expr { kind: a::ExprKind::Field {
                        base: Box::new(a::Expr { kind: a::ExprKind::This, span: field.span, depth: 1 }),
                        name: field.name.clone(),
                    }, span: field.span, depth: 2 };
                    let stmt = a::Stmt { kind: a::StmtKind::Assign { target, value: value.clone() }, span: field.span };
                    body.push(checker.statement(&stmt)?.0);
                }
            }
            checker.block_depth = 0;
            checker.scopes = saved;
        }
        let (checked_body, stops) = checker.block(&function.body)?;
        body.extend(checked_body);
        if checker.constructor { checker.require_initialized(function.span)?; }
        if sig.return_type != Type::Void && !stops {
            return Err(Diagnostic::new(function.span, format!("`{}` must return {} on every path; a loop does not prove a return", function.name, tables.type_name(sig.return_type))));
        }
        let name = sig.owner.map_or_else(|| function.name.clone(),
            |c| format!("{}.{}", tables.classes[c].name, function.name));
        if checker.constructor { borrowed_from[sig.owner.unwrap()] = checker.field_sources.clone(); }
        functions.push(t::Function { native: None, id, name, receiver, readonly: sig.readonly, return_type: sig.return_type,
            params, body, locals: checker.variables, virtual_slot: None });
    }
    let classes = tables.classes.iter().enumerate().map(|(id, c)| t::Class {
        native: c.native, id, name: c.name.clone(), fields: c.fields.iter().map(|f| f.ty).collect(),
        variants: c.variants.clone(), has_borrow: c.has_borrow,
        borrowed_from: borrowed_from[id].clone(), is_interface: c.is_interface,
        interface_methods: c.interface_methods.clone(), implementations: c.implementations.clone()
    }).collect();
    Ok(t::Program { classes, functions, main: main_id })
}

struct Checker<'a> {
    tables: &'a Tables, owner: Option<usize>, receiver: Option<usize>, constructor: bool, readonly: bool,
    scopes: Vec<HashMap<String, usize>>, variables: Vec<t::Local>,
    locks: Vec<usize>, loop_depth: usize, block_depth: usize, expr_depth: usize,
    return_type: Type, initialized: Vec<bool>,
    parameter_ids: Vec<usize>, field_sources: Vec<Option<usize>>,
}

fn synthetic_expr(kind: a::ExprKind, span: Span) -> a::Expr {
    let depth = 1 + match &kind {
        a::ExprKind::Binary { left, right, .. } => left.depth.max(right.depth),
        a::ExprKind::Unary { expr, .. } | a::ExprKind::Borrow { expr, .. } | a::ExprKind::Cast { expr, .. } => expr.depth,
        a::ExprKind::Call { args, .. } | a::ExprKind::New { args, .. } | a::ExprKind::Variant { args, .. } =>
            args.iter().map(|arg| arg.depth).max().unwrap_or(0),
        a::ExprKind::MethodCall { receiver, args, .. } =>
            receiver.depth.max(args.iter().map(|arg| arg.depth).max().unwrap_or(0)),
        a::ExprKind::Field { base, .. } => base.depth,
        _ => 0,
    };
    a::Expr { kind, span, depth }
}
impl Checker<'_> {
    fn lookup(&self, name: &str) -> Option<usize> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }
    fn declare(&mut self, name: &str, ty: Type, span: Span) -> Result<usize, Diagnostic> {
        if name == "System" { return Err(Diagnostic::new(span, "`System` is a reserved name")); }
        if self.lookup(name).is_some() {
            return Err(Diagnostic::new(span, format!("variable `{name}` is already declared in an active scope")));
        }
        let id = self.variables.len();
        self.variables.push(t::Local { ty, name: name.to_owned(), span });
        self.scopes.last_mut().unwrap().insert(name.to_owned(), id);
        Ok(id)
    }
    fn require(&self, expected: Type, actual: Type, span: Span) -> Result<(), Diagnostic> {
        if expected == actual { Ok(()) } else {
            Err(Diagnostic::new(span, format!("type mismatch: expected {}, found {}", self.tables.type_name(expected), self.tables.type_name(actual))))
        }
    }
    fn require_initialized(&self, span: Span) -> Result<(), Diagnostic> {
        if !self.constructor { return Ok(()); }
        let class = &self.tables.classes[self.owner.unwrap()];
        let missing: Vec<_> = class.fields.iter().zip(&self.initialized).filter(|(_, ready)| !**ready).map(|(f, _)| f.name.as_str()).collect();
        if missing.is_empty() { Ok(()) } else {
            Err(Diagnostic::new(span, format!("uninitialized fields: {}; initialize every field before a call on `this` or leaving the constructor", missing.join(", "))))
        }
    }
    fn accessible(&self, visibility: Visibility, owner: usize, span: Span, what: &str) -> Result<(), Diagnostic> {
        if visibility == Visibility::Private && self.owner != Some(owner) {
            Err(Diagnostic::new(span, format!("{what} is private: access is not allowed outside its class or function")))
        } else { Ok(()) }
    }
    fn block(&mut self, block: &a::Block) -> Result<(t::Block, bool), Diagnostic> {
        self.block_with_prefix(&[], block)
    }
    fn block_with_prefix(&mut self, prefix: &[a::Stmt], block: &a::Block) -> Result<(t::Block, bool), Diagnostic> {
        self.scopes.push(HashMap::new()); self.block_depth += 1;
        let result = (|| {
            let mut out = Vec::new(); let mut stops = false;
            for stmt in prefix.iter().chain(block.iter()) {
                if stops { return Err(Diagnostic::new(stmt.span, "unreachable statement after return, break, or continue")); }
                let (stmt, terminal) = self.statement(stmt)?;
                out.push(stmt); stops = terminal;
            }
            Ok((out, stops))
        })();
        self.block_depth -= 1; self.scopes.pop(); result
    }
    fn statement(&mut self, stmt: &a::Stmt) -> Result<(t::Stmt, bool), Diagnostic> {
        use a::StmtKind as S;
        Ok(match &stmt.kind {
            S::Let { ty, name, value } => {
                let ty = self.tables.resolve_type(ty, stmt.span)?;
                if ty.is_borrow() && value.is_none() {
                    return Err(Diagnostic::new(stmt.span, "[E3005] a local borrow requires an initializer and cannot be reassigned"));
                }
                if ty.is_borrow() && self.tables.is_view(ty) {
                    return Err(Diagnostic::new(stmt.span, "[E3007] borrowing a local view is unsupported"));
                }
                if ty.is_object() && self.tables.is_view(ty)
                    && !value.as_ref().is_some_and(|v| matches!(v.kind, a::ExprKind::New { .. })) {
                    return Err(Diagnostic::new(stmt.span, "[E3007] a local view must be initialized directly with `new`"));
                }
                let value = value.as_ref().map(|v| self.expected_expression(ty, v)).transpose()?;
                let id = self.declare(name, ty, stmt.span)?;
                (t::Stmt::Let { id, ty, value }, false)
            }
            S::Assign { target, value } => {
                let target = self.place(target, true)?;
                let init_field = match &target.kind {
                    PlaceKind::Field { base, field } if matches!(base.kind, PlaceKind::This) && self.constructor => Some(*field),
                    _ => None,
                };
                let is_initialization = init_field.is_some_and(|f| !self.initialized[f]);
                if is_initialization && (self.block_depth != 1 || self.loop_depth != 0) {
                    return Err(Diagnostic::new(stmt.span, "field initialization must occur directly in the constructor, outside branches, loops, and nested blocks"));
                }
                if !is_initialization {
                    if target.ty.is_borrow() {
                        return Err(Diagnostic::new(stmt.span, "[E3005] reassigning a borrow is not allowed; create a new borrow in a block"));
                    }
                    if target.ty.is_object() && self.tables.is_view(target.ty) {
                        return Err(Diagnostic::new(stmt.span, "[E3007] replacing a local view is not allowed"));
                    }
                    if let PlaceKind::Field { base, .. } = &target.kind { self.require_mutable(base, stmt.span)?; }
                }
                if target.ty.is_borrow() && is_initialization {
                    let a::ExprKind::Var(name) = &value.kind else {
                        return Err(Diagnostic::new(stmt.span, "[E3007] a borrowed field must come directly from a constructor borrow parameter"));
                    };
                    let source = self.lookup(name).and_then(|id| self.parameter_ids.iter().position(|p| *p == id))
                        .ok_or_else(|| Diagnostic::new(stmt.span, "[E3007] a borrowed field requires a borrow parameter, not a local owner"))?;
                    let source_type = self.variables[self.parameter_ids[source]].ty;
                    self.require(target.ty, source_type, stmt.span)?;
                    self.field_sources[init_field.unwrap()] = Some(source);
                }
                // Stabilize the receiver while evaluating an assignment RHS.
                let lock = if matches!(target.kind, PlaceKind::Field { .. }) { target.root_var() } else { None };
                if let Some(id) = lock { self.locks.push(id); }
                let value = self.expected_expression(target.ty, value);
                if lock.is_some() { self.locks.pop(); }
                let value = value?;
                self.require(target.ty, value.ty, value.span)?;
                match &target.kind {
                    PlaceKind::Var(id) => {
                        if self.locks.contains(id) { return Err(Diagnostic::new(stmt.span, "object is borrowed by an in-progress call: replacement is not allowed")); }
                    }
                    PlaceKind::This => return Err(Diagnostic::new(stmt.span, "cannot assign to `this`")),
                    PlaceKind::Field { .. } => {}
                }
                if let Some(field) = init_field { self.initialized[field] = true; }
                (t::Stmt::Assign { target, value }, false)
            }
            S::Expr(expr) => {
                if !matches!(&expr.kind, a::ExprKind::Call { .. } | a::ExprKind::MethodCall { .. }
                    | a::ExprKind::New { .. } | a::ExprKind::Variant { .. }) {
                    return Err(Diagnostic::new(expr.span, "only a call or construction can be used as an expression statement"));
                }
                let value = self.expression(expr)?;
                if value.ty.is_object() && self.tables.is_view(value.ty) {
                    return Err(Diagnostic::new(expr.span, "[E3007] a local view cannot be kept in a variable initialized with `new`"));
                }
                (t::Stmt::Expr(value), false)
            }
            S::If { condition, then_block, else_block } => {
                let condition = self.expression(condition)?;
                self.require(Type::Boolean, condition.ty, condition.span)?;
                let (then_block, then_stops) = self.block(then_block)?;
                let (else_block, else_stops) = match else_block {
                    Some(block) => { let (b, stops) = self.block(block)?; (Some(b), stops) }
                    None => (None, false),
                };
                (t::Stmt::If { condition, then_block, else_block }, then_stops && else_stops)
            }
            S::While { condition, body } => {
                self.loop_depth += 1;
                let condition = self.expression(condition)?;
                self.require(Type::Boolean, condition.ty, condition.span)?;
                let (body, _) = self.block(body)?;
                self.loop_depth -= 1;
                (t::Stmt::While { condition, body }, false)
            }
            S::ForEach { .. } => {
                (self.for_each(stmt)?, false)
            }
            S::Match { value, arms } => {
                let value = self.expression(value)?;
                let class = value.ty.object_class()
                    .ok_or_else(|| Diagnostic::new(stmt.span, "match requires an owned or borrowed enum"))?;
                let borrowed = match value.ty {
                    Type::Borrow { mutable, .. } => Some(mutable),
                    _ => None,
                };
                let variants = self.tables.classes[class].variants.clone()
                    .ok_or_else(|| Diagnostic::new(stmt.span, format!("match requires an enum, not `{}`", self.tables.classes[class].name)))?;
                let mut seen = vec![false; variants.len()];
                let mut checked_arms = Vec::with_capacity(arms.len());
                let mut all_stop = true;
                for arm in arms {
                    let variant = variants.iter().position(|v| v.name == arm.variant)
                        .ok_or_else(|| Diagnostic::new(arm.span, format!("unknown variant `{}` in `{}`", arm.variant, self.tables.classes[class].name)))?;
                    if seen[variant] {
                        return Err(Diagnostic::new(arm.span, format!("variant `{}` is repeated in the match", arm.variant)));
                    }
                    seen[variant] = true;
                    let payload = &variants[variant].fields;
                    if arm.bindings.len() != payload.len() {
                        return Err(Diagnostic::new(arm.span, format!("match arm `{}` expects {} payload binding(s), {} provided",
                            arm.variant, payload.len(), arm.bindings.len())));
                    }
                    self.scopes.push(HashMap::new());
                    let mut bindings = Vec::with_capacity(payload.len());
                    for (name, ty) in arm.bindings.iter().zip(payload) {
                        let binding_type = match (*ty, borrowed) {
                            (Type::Object(class), Some(mutable)) => Type::Borrow { class, mutable },
                            _ => *ty,
                        };
                        let id = self.declare(name, binding_type, arm.span)?;
                        bindings.push(id);
                    }
                    let (body, stops) = self.block(&arm.body)?;
                    self.scopes.pop();
                    all_stop &= stops;
                    checked_arms.push(t::MatchArm { variant, bindings, body });
                }
                let missing: Vec<_> = variants.iter().enumerate().filter(|(id, _)| !seen[*id])
                    .map(|(_, variant)| variant.name.as_str()).collect();
                if !missing.is_empty() {
                    return Err(Diagnostic::new(stmt.span, format!("match is not exhaustive; missing variants: {}", missing.join(", "))));
                }
                (t::Stmt::Match { value, arms: checked_arms }, all_stop)
            }
            S::Return(value) => {
                if self.return_type == Type::Void && value.is_some() {
                    return Err(Diagnostic::new(stmt.span, "a void function must use `return;` without a value"));
                }
                let value = match value { Some(v) => Some(self.expected_expression(self.return_type, v)?), None => None };
                self.require(self.return_type, value.as_ref().map_or(Type::Void, |v| v.ty), stmt.span)?;
                self.require_initialized(stmt.span)?;
                (t::Stmt::Return(value), true)
            }
            S::Break | S::Continue => {
                if self.loop_depth == 0 {
                    return Err(Diagnostic::new(stmt.span, "`break` and `continue` are only allowed inside a loop"));
                }
                (if matches!(stmt.kind, S::Break) { t::Stmt::Break } else { t::Stmt::Continue }, true)
            }
            S::Block(block) => { let (block, stops) = self.block(block)?; (t::Stmt::Block(block), stops) }
        })
    }

    fn for_each(&mut self, stmt: &a::Stmt) -> Result<t::Stmt, Diagnostic> {
        let a::StmtKind::ForEach { ty: header, name, key, collection, body, header_span } = &stmt.kind
            else { unreachable!("for-each statement"); };
        let header_span = *header_span;
        let span = stmt.span;
        let place = self.place(collection, false).ok();
        let value = if place.is_none() { Some(self.expression(collection)?) } else { None };
        let source_type = if let Some(place) = &place { place.ty } else { value.as_ref().unwrap().ty };
        let source_class = source_type.object_class()
            .ok_or_else(|| Diagnostic::new(collection.span, "collection must be a named or borrowed List<T> or Map<K, V>"))?;
        match self.tables.classes[source_class].native {
            Some(t::NativeClass::Map(_, _)) => {
                if key.is_none() {
                    return Err(Diagnostic::new(header_span, "Map<K, V> requires two typed bindings: key and value"));
                }
                return self.for_each_map(stmt, place, value, source_type, source_class);
            }
            Some(t::NativeClass::List(_)) => {
                if key.is_some() {
                    return Err(Diagnostic::new(header_span, "List<T> allows only one element binding"));
                }
            }
            _ => return Err(Diagnostic::new(collection.span, "for-each collection must be a List<T> or Map<K, V>")),
        }
        let list_class = source_class;
        let Some(t::NativeClass::List(element_type)) = self.tables.classes[list_class].native else {
            unreachable!("checked List traversal type");
        };
        let (binding_type, exclusive) = self.traversal_binding(header, element_type, header_span)?;
        let getter = if binding_type.is_borrow() {
            if exclusive { "getMut" } else { "getRef" }
        } else { "get" };
        let list_type = Type::Borrow { class: list_class, mutable: exclusive };
        let frozen_value = self.traversal_receiver(place, value, source_type, list_type, collection.span)?;

        self.scopes.push(HashMap::new());
        self.block_depth += 1;
        let result = (|| {
            let unique = self.variables.len();
            let list_name = format!("$nacre_for_list_{unique}");
            let cursor_name = format!("$nacre_for_index_{unique}");
            let list_id = self.declare(&list_name, list_type, span)?;
            let cursor_id = self.declare(&cursor_name, Type::Int, span)?;
            let mut lowered = vec![
                t::Stmt::Let { id: list_id, ty: list_type, value: Some(frozen_value) },
                t::Stmt::Let { id: cursor_id, ty: Type::Int,
                    value: Some(t::Expr { kind: t::ExprKind::Int(0), ty: Type::Int, span }) },
            ];

            let cursor = synthetic_expr(a::ExprKind::Var(cursor_name.clone()), span);
            let size_receiver = synthetic_expr(a::ExprKind::Var(list_name.clone()), span);
            let size = synthetic_expr(a::ExprKind::MethodCall {
                receiver: Box::new(size_receiver), name: "size".to_owned(), args: Vec::new(),
            }, collection.span);
            let condition = synthetic_expr(a::ExprKind::Binary {
                op: B::Lt, left: Box::new(cursor), right: Box::new(size),
            }, span);

            let element_receiver = synthetic_expr(a::ExprKind::Var(list_name), span);
            let element_index = synthetic_expr(a::ExprKind::Var(cursor_name.clone()), span);
            let element_value = synthetic_expr(a::ExprKind::MethodCall {
                receiver: Box::new(element_receiver), name: getter.to_owned(), args: vec![element_index],
            }, collection.span);
            let element = a::Stmt { kind: a::StmtKind::Let {
                ty: header.clone(), name: name.to_owned(), value: Some(element_value),
            }, span: header_span };
            let increment = synthetic_expr(a::ExprKind::Binary {
                op: B::Add,
                left: Box::new(synthetic_expr(a::ExprKind::Var(cursor_name.clone()), span)),
                right: Box::new(synthetic_expr(a::ExprKind::Int(1), span)),
            }, span);
            let advance = a::Stmt { kind: a::StmtKind::Assign {
                target: synthetic_expr(a::ExprKind::Var(cursor_name), span), value: increment,
            }, span };
            let condition = self.expression(&condition)?;
            self.require(Type::Boolean, condition.ty, condition.span)?;
            self.loop_depth += 1;
            let checked_body = self.block_with_prefix(&[element, advance], body);
            self.loop_depth -= 1;
            lowered.push(t::Stmt::While { condition, body: checked_body?.0 });
            Ok(t::Stmt::Block(lowered))
        })();
        self.block_depth -= 1;
        self.scopes.pop();
        result
    }
    /// Lists and maps share the same copied/borrowed value-binding contract.
    fn traversal_binding(&self, header: &a::Type, value_type: Type, span: Span) -> Result<(Type, bool), Diagnostic> {
        let header_type = self.tables.resolve_type(header, span)?;
        match value_type {
            ty if ty.is_copy() => {
                if header_type == ty { Ok((ty, false)) } else {
                    Err(Diagnostic::new(span, format!(
                        "element type mismatch: expected {}, found {}",
                        self.tables.type_name(value_type), self.tables.type_name(header_type))))
                }
            }
            Type::Object(class) => {
                let Type::Borrow { class: header_class, mutable } = header_type else {
                    return Err(Diagnostic::new(span,
                        "[E5004] an affine element must be bound as `borrow T` or `borrow mut T`"));
                };
                if header_class != class {
                    return Err(Diagnostic::new(span, format!(
                        "incompatible borrow type: expected {}, found {}",
                        self.tables.type_name(Type::Borrow { class, mutable }),
                        self.tables.type_name(header_type))));
                }
                Ok((header_type, mutable))
            }
            _ => Err(Diagnostic::new(span, "unsupported element type for traversal")),
        }
    }

    /// Freeze one stable place or an already borrowed result for the whole loop.
    fn traversal_receiver(&mut self, place: Option<Place>, value: Option<t::Expr>, source_type: Type,
                          borrow_type: Type, span: Span) -> Result<t::Expr, Diagnostic> {
        let Type::Borrow { mutable, .. } = borrow_type else { unreachable!("traversal receiver type"); };
        if mutable && matches!(source_type, Type::Borrow { mutable: false, .. }) {
            return Err(Diagnostic::new(span, "[E3003] mutable traversal requires an exclusive borrow of the collection"));
        }
        if let Some(place) = place {
            if matches!(place.kind, PlaceKind::This) { self.require_initialized(span)?; }
            if mutable { self.require_mutable(&place, span)?; }
            Ok(t::Expr { kind: t::ExprKind::Borrow { place, mutable }, ty: borrow_type, span })
        } else {
            let mut value = value.expect("non-place traversal expression was checked");
            if !value.ty.is_borrow() {
                return Err(Diagnostic::new(span,
                    "a temporary owned collection cannot be traversed; use a stable place or an existing borrow"));
            }
            value.ty = borrow_type;
            Ok(value)
        }
    }

    fn for_each_map(&mut self, stmt: &a::Stmt, place: Option<Place>, value: Option<t::Expr>,
                    source_type: Type, map_class: usize) -> Result<t::Stmt, Diagnostic> {
        let Some(t::NativeClass::Map(map_key, map_value)) = self.tables.classes[map_class].native
            else { unreachable!("checked map traversal class"); };
        let a::StmtKind::ForEach { ty: header, name, key: Some((key_header, key_name)),
                                  collection, body, header_span } = &stmt.kind
            else { unreachable!("checked two-binding map traversal"); };
        let header_span = *header_span;
        let span = stmt.span;
        let key_binding_type = self.tables.map_key_binding_type(map_key);
        let key_type = self.tables.resolve_type(key_header, header_span)?;
        if key_type != key_binding_type {
            return Err(Diagnostic::new(header_span, format!(
                "incompatible map key binding type: expected {}", self.tables.type_name(key_binding_type))));
        }
        let (value_binding_type, exclusive) = self.traversal_binding(header, map_value, header_span)?;
        let map_type = Type::Borrow { class: map_class, mutable: exclusive };
        let frozen_value = self.traversal_receiver(place, value, source_type, map_type, collection.span)?;

        self.scopes.push(HashMap::new());
        self.block_depth += 1;
        let result = (|| {
            let unique = self.variables.len();
            let map_name = format!("$nacre_for_map_{unique}");
            let cursor_name = format!("$nacre_for_bucket_{unique}");
            let map_id = self.declare(&map_name, map_type, span)?;
            let cursor_id = self.declare(&cursor_name, Type::Int, span)?;
            let capacity_id = self.declare(&format!("$nacre_for_capacity_{unique}"), Type::Int, span)?;
            let key_id = self.declare(key_name, key_binding_type, header_span)?;
            let value_id = self.declare(name, value_binding_type, header_span)?;

            let map_place = Place { kind: PlaceKind::Var(map_id), ty: map_type };
            let cursor_place = Place { kind: PlaceKind::Var(cursor_id), ty: Type::Int };
            let typed_expr = |kind, ty, expr_span| t::Expr { kind, ty, span: expr_span };
            let cursor_read = || typed_expr(t::ExprKind::Read(cursor_place.clone()), Type::Int, span);
            let slot = |kind, ty, pair| typed_expr(t::ExprKind::MapSlot {
                map: map_place.clone(),
                index: Box::new(cursor_read()),
                kind,
                pair,
            }, ty, collection.span);
            let advance = || t::Stmt::Assign {
                target: cursor_place.clone(),
                value: typed_expr(t::ExprKind::Binary {
                    op: B::Add,
                    left: Box::new(cursor_read()),
                    right: Box::new(typed_expr(t::ExprKind::Int(1), Type::Int, span)),
                }, Type::Int, span),
            };

            let condition = typed_expr(t::ExprKind::Binary {
                op: B::Lt,
                left: Box::new(cursor_read()),
                right: Box::new(typed_expr(t::ExprKind::Read(Place {
                    kind: PlaceKind::Var(capacity_id), ty: Type::Int,
                }), Type::Int, collection.span)),
            }, Type::Boolean, span);
            let map_slot_occupied = slot(t::MapSlotKind::Occupied, Type::Boolean, None);
            let key_value = slot(t::MapSlotKind::Key, key_binding_type, Some(key_id));
            let entry_value = slot(t::MapSlotKind::Value, value_binding_type, Some(key_id));
            let mut occupied_body = vec![
                t::Stmt::Let { id: key_id, ty: key_binding_type, value: Some(key_value) },
                t::Stmt::Let { id: value_id, ty: value_binding_type, value: Some(entry_value) },
                advance(),
            ];

            let (user_body, _) = {
                self.loop_depth += 1;
                let result = self.block(body);
                self.loop_depth -= 1;
                result?
            };
            occupied_body.extend(user_body);
            let loop_body = vec![t::Stmt::If {
                condition: map_slot_occupied,
                then_block: occupied_body,
                else_block: Some(vec![advance()]),
            }];
            Ok(t::Stmt::Block(vec![
                t::Stmt::Let { id: map_id, ty: map_type, value: Some(frozen_value) },
                // Structural mutation is forbidden by the root loan, so capacity is invariant.
                t::Stmt::Let { id: capacity_id, ty: Type::Int,
                    value: Some(typed_expr(t::ExprKind::MapCapacity(map_place), Type::Int, collection.span)) },
                t::Stmt::Let { id: cursor_id, ty: Type::Int,
                    value: Some(typed_expr(t::ExprKind::Int(0), Type::Int, span)) },
                t::Stmt::While { condition, body: loop_body },
            ]))
        })();
        self.block_depth -= 1;
        self.scopes.pop();
        result
    }


    fn this_place(&self, span: Span) -> Result<Place, Diagnostic> {
        let id = self.receiver.ok_or_else(|| Diagnostic::new(span, "`this` or an instance member is unavailable in a free or static function"))?;
        Ok(Place { kind: PlaceKind::This, ty: Type::Object(id) })
    }
    fn field_place(&self, base: Place, name: &str, span: Span, write: bool) -> Result<Place, Diagnostic> {
        let Some(class_id) = base.ty.object_class() else { return Err(Diagnostic::new(span, "cannot access a field on a value that is not an object")); };
        let class = &self.tables.classes[class_id];
        let field = class.fields.iter().position(|f| f.name == name)
            .ok_or_else(|| Diagnostic::new(span, format!("field `{name}` is unknown in `{}`", class.name)))?;
        let info = &class.fields[field];
        self.accessible(info.visibility, class_id, span, "field")?;
        if matches!(base.kind, PlaceKind::This) && self.constructor && !write && !self.initialized[field] {
            return Err(Diagnostic::new(span, format!("field `{name}` is not initialized")));
        }
        Ok(Place { kind: PlaceKind::Field { base: Box::new(base), field }, ty: info.ty })
    }
    fn place(&self, expr: &a::Expr, write: bool) -> Result<Place, Diagnostic> {
        match &expr.kind {
            a::ExprKind::Var(name) => {
                if let Some(id) = self.lookup(name) {
                    Ok(Place { kind: PlaceKind::Var(id), ty: self.variables[id].ty })
                } else if self.owner.is_some_and(|c| self.tables.classes[c].fields.iter().any(|f| f.name == *name)) {
                    let base = self.this_place(expr.span)?;
                    self.field_place(base, name, expr.span, write)
                } else { Err(Diagnostic::new(expr.span, format!("variable `{name}` is not declared"))) }
            }
            a::ExprKind::This => self.this_place(expr.span),
            a::ExprKind::Field { base, name } => {
                let base = self.place(base, false)?;
                self.field_place(base, name, expr.span, write)
            }
            _ => Err(Diagnostic::new(expr.span, "temporary receivers are unsupported; store the object in a local variable")),
        }
    }
    /// Borrowing changes the call contract, not ownership. No allocation or copy.
    fn expected_expression(&mut self, expected: Type, expr: &a::Expr) -> Result<t::Expr, Diagnostic> {
        if let Type::Borrow { class, mutable } = expected {
            if matches!(expr.kind, a::ExprKind::Call { .. } | a::ExprKind::MethodCall { .. } | a::ExprKind::Borrow { .. }) {
                let mut value = self.expression(expr)?;
                let Type::Borrow { class: actual, mutable: actual_mutable } = value.ty else {
                    return Err(Diagnostic::new(expr.span, "[E3004] a temporary owner cannot supply a borrowed result"));
                };
                if mutable && !actual_mutable {
                    return Err(Diagnostic::new(expr.span, "[E3003] shared access cannot become an exclusive borrow"));
                }
                if !self.tables.can_upcast(actual, class) {
                    return Err(Diagnostic::new(expr.span, format!("type mismatch: expected {}, found {}", self.tables.type_name(expected), self.tables.type_name(value.ty))));
                }
                // Downgrade the returned capability before retaining its caller loan.
                value.ty = Type::Borrow { class: actual, mutable };
                if actual != class {
                    return Ok(t::Expr { kind: t::ExprKind::Upcast { value: Box::new(value), interface: class, concrete: actual },
                        ty: expected, span: expr.span });
                }
                return Ok(value);
            }
            let place = self.place(expr, false).map_err(|d| d.with_help("a borrow requires a named object or field; first store a temporary object in a variable"))?;
            if !place.ty.object_class().is_some_and(|actual| self.tables.can_upcast(actual, class)) {
                return Err(Diagnostic::new(expr.span, format!("type mismatch: expected {}, found {}", self.tables.type_name(expected), self.tables.type_name(place.ty))));
            }
            if self.tables.is_view(expected) {
                return Err(Diagnostic::new(expr.span, "[E3007] borrowing a local view is unsupported"));
            }
            if matches!(place.kind, PlaceKind::This) { self.require_initialized(expr.span)?; }
            if mutable { self.require_mutable(&place, expr.span)?; }
            return Ok(t::Expr { kind: t::ExprKind::Borrow { place, mutable }, ty: expected, span: expr.span });
        }
        let value = self.expression(expr)?;
        if let (Type::Object(interface), Type::Object(concrete)) = (expected, value.ty) {
            if interface != concrete && self.tables.can_upcast(concrete, interface) {
                return Ok(t::Expr { kind: t::ExprKind::Upcast { value: Box::new(value), interface, concrete },
                    ty: expected, span: expr.span });
            }
        }
        if expected == value.ty { return Ok(value); }
        if expected.is_numeric() && value.ty.is_numeric() {
            let source = value.ty.promoted_numeric().expect("numeric source has a promotion");
            if expected.promoted_numeric() == Some(expected)
                && source.numeric_rank().zip(expected.numeric_rank()).is_some_and(|(from, to)| from <= to) {
                return Ok(coerce_numeric(value, expected));
            }
        }
        self.require(expected, value.ty, value.span)?;
        Ok(value)
    }
    /// Shared access is transitive through owned fields. A borrowed field is
    /// always shared, even when its containing view is mutable.
    fn writable(&self, place: &Place) -> bool {
        if matches!(place.ty, Type::Borrow { mutable: false, .. }) { return false; }
        match &place.kind {
            PlaceKind::This => !self.readonly,
            PlaceKind::Var(_) => true,
            PlaceKind::Field { base, .. } => self.writable(base),
        }
    }
    fn require_mutable(&self, place: &Place, span: Span) -> Result<(), Diagnostic> {
        if self.writable(place) { Ok(()) } else {
            Err(Diagnostic::new(span, "[E3003] modification is not allowed through a shared borrow or readonly receiver")
                .with_help("declare a `borrow mut` parameter to modify; call only readonly methods through a shared borrow"))
        }
    }
    fn take_place(&mut self, place: Place, span: Span) -> Result<t::ExprKind, Diagnostic> {
        if !place.ty.is_object() { return Ok(t::ExprKind::Read(place)); }
        if self.tables.is_view(place.ty) {
            return Err(Diagnostic::new(span, "[E3007] moving a local view is not allowed; its lifetime is the block where it is declared"));
        }
        let PlaceKind::Var(id) = place.kind else {
            return Err(Diagnostic::new(span, "moving `this` or an object field is not allowed; only local objects and parameters can be transferred"));
        };
        if self.locks.contains(&id) {
            return Err(Diagnostic::new(span, "object is borrowed as the receiver of an in-progress call; moving it is not allowed"));
        }
        Ok(t::ExprKind::Move(id))
    }
    fn check_args(&mut self, sig: &Signature, args: &[a::Expr], span: Span) -> Result<Vec<t::Expr>, Diagnostic> {
        if sig.params.len() != args.len() {
            return Err(Diagnostic::new(span, format!("`{}` expects {} argument(s), {} received", sig.name, sig.params.len(), args.len())));
        }
        let mut out = Vec::new();
        for (arg, expected) in args.iter().zip(&sig.params) {
            let value = self.expected_expression(*expected, arg)?;
            out.push(value);
        }
        Ok(out)
    }
    fn call(&mut self, id: usize, receiver: Option<Place>, args: &[a::Expr], span: Span) -> Result<(t::ExprKind, Type), Diagnostic> {
        let sig = self.tables.signatures[id].clone();
        if let Some(owner) = sig.owner { self.accessible(sig.visibility, owner, span, "method or constructor")?; }
        if let Some(place) = &receiver {
            if matches!(place.kind, PlaceKind::This) { self.require_initialized(span)?; }
            if !sig.readonly { self.require_mutable(place, span)?; }
        }
        let lock = receiver.as_ref().and_then(Place::root_var);
        if let Some(id) = lock { self.locks.push(id); }
        let args = self.check_args(&sig, args, span);
        if lock.is_some() { self.locks.pop(); }
        Ok((t::ExprKind::Call { target: CallTarget::Function(id), receiver, args: args? }, sig.return_type))
    }
    fn builtin(&mut self, args: &[a::Expr], span: Span) -> Result<(t::ExprKind, Type), Diagnostic> {
        if args.len() != 1 { return Err(Diagnostic::new(span, "println expects exactly one argument")); }
        let value = self.expression(&args[0])?;
        if value.ty == Type::Void { return Err(Diagnostic::new(span, "println cannot print a void expression")); }
        if value.ty.object_class().is_some() { return Err(Diagnostic::new(span, "println cannot print an object; print a field or the result of a method")); }
        Ok((t::ExprKind::Call { target: CallTarget::Println, receiver: None, args: vec![value] }, Type::Void))
    }
    fn expression(&mut self, expr: &a::Expr) -> Result<t::Expr, Diagnostic> {
        if self.expr_depth >= 128 { return Err(Diagnostic::new(expr.span, "expression too deep")); }
        self.expr_depth += 1;
        let result = self.expression_inner(expr);
        self.expr_depth -= 1; result
    }
    fn expression_inner(&mut self, expr: &a::Expr) -> Result<t::Expr, Diagnostic> {
        use a::ExprKind as E;
        let (kind, ty) = match &expr.kind {
            E::Char(value) => (t::ExprKind::Char(*value), Type::Char),
            E::Int(n) => (t::ExprKind::Int(*n), Type::Int),
            E::Long(value) => (t::ExprKind::Long(*value), Type::Long),
            E::Double(value) => (t::ExprKind::Double(*value), Type::Double),
            E::Unit => (t::ExprKind::Unit, Type::Unit),
            E::Boolean(b) => (t::ExprKind::Boolean(*b), Type::Boolean),
            E::String(s) => (t::ExprKind::String(s.clone()), Type::String),
            E::Cast { ty, expr: value } => {
                let target = self.tables.resolve_type(ty, expr.span)?;
                let value = self.expression(value)?;
                if !target.is_numeric() || !value.ty.is_numeric() {
                    return Err(Diagnostic::new(expr.span, "[E6102] numeric casts require numeric source and target types"));
                }
                (t::ExprKind::Cast { value: Box::new(value) }, target)
            }
            E::Borrow { expr: inner, mutable } => {
                let place = self.place(inner, false)?;
                let class = place.ty.object_class()
                    .ok_or_else(|| Diagnostic::new(expr.span, "[E3004] borrow requires a named nominal value"))?;
                let value = self.expected_expression(Type::Borrow { class, mutable: *mutable }, inner)?;
                (value.kind, value.ty)
            }
            E::Var(_) | E::This | E::Field { .. } => {
                let place = self.place(expr, false)?;
                let ty = place.ty;
                (self.take_place(place, expr.span)?, ty)
            }
            E::New { class, args, .. } => {
                let class = *self.tables.names.get(class).ok_or_else(|| Diagnostic::new(expr.span, format!("class `{class}` is unknown")))?;
                if self.tables.is_static_native(class) {
                    return Err(Diagnostic::new(expr.span, format!("`{}` is static and cannot be constructed", self.tables.classes[class].name)));
                }
                if self.tables.classes[class].is_interface {
                    return Err(Diagnostic::new(expr.span, "[E4004] an interface cannot be instantiated with `new`"));
                }
                if self.tables.classes[class].variants.is_some() {
                    return Err(Diagnostic::new(expr.span, "construct an enum with `new Enum.Variant(...)`, not `new Enum(...)`"));
                }
                let constructor = self.tables.classes[class].constructor
                    .ok_or_else(|| Diagnostic::new(expr.span, "this class has no constructor"))?;
                let sig = self.tables.signatures[constructor].clone();
                self.accessible(sig.visibility, class, expr.span, "constructor")?;
                let args = self.check_args(&sig, args, expr.span)?;
                (t::ExprKind::New { class, constructor, args }, Type::Object(class))
            }
            E::Variant { class, variant, args, .. } => {
                let class = *self.tables.names.get(class).ok_or_else(|| Diagnostic::new(expr.span, format!("enum `{class}` is unknown")))?;
                let variants = self.tables.classes[class].variants.as_ref()
                    .ok_or_else(|| Diagnostic::new(expr.span, format!("`{}` is not an enum; variant construction is not allowed", self.tables.classes[class].name)))?;
                let variant_id = variants.iter().position(|v| v.name == *variant)
                    .ok_or_else(|| Diagnostic::new(expr.span, format!("variant `{variant}` is unknown in `{}`", self.tables.classes[class].name)))?;
                let fields = variants[variant_id].fields.clone();
                if args.len() != fields.len() {
                    return Err(Diagnostic::new(expr.span, format!("variant `{variant}` expects {} payload(s), {} provided", fields.len(), args.len())));
                }
                let values = args.iter().zip(fields).map(|(arg, expected)| self.expected_expression(expected, arg))
                    .collect::<Result<Vec<_>, _>>()?;
                (t::ExprKind::Variant { class, variant: variant_id, args: values }, Type::Object(class))
            }
            E::Call { name, args } => {
                if name == "println" { self.builtin(args, expr.span)? }
                else if name == "move" || name == "drop" {
                    if args.len() != 1 { return Err(Diagnostic::new(expr.span, format!("{name} expects exactly one argument"))); }
                    let value = self.expression(&args[0])?;
                    if !value.ty.is_object() { return Err(Diagnostic::new(expr.span, format!("{name} requires an owned object, not a primitive or void"))); }
                    if name == "move" { (value.kind, value.ty) }
                    else { (t::ExprKind::Drop(Box::new(value)), Type::Void) }
                }
                else if let Some(id) = self.owner.and_then(|c| self.tables.classes[c].methods.get(name)).copied() {
                    let receiver = if self.tables.signatures[id].kind == Kind::Instance { Some(self.this_place(expr.span)?) } else { None };
                    self.call(id, receiver, args, expr.span)?
                } else {
                    let id = *self.tables.globals.get(name).ok_or_else(|| Diagnostic::new(expr.span, format!("function `{name}` is unknown")))?;
                    self.call(id, None, args, expr.span)?
                }
            }
            E::MethodCall { receiver, name, args } => {
                let system_println = name == "println" && matches!(&receiver.kind,
                    E::Field { base, name } if name == "out" && matches!(&base.kind, E::Var(s) if s == "System"));
                if system_println { self.builtin(args, expr.span)? }
                else {
                    let static_class = match &receiver.kind {
                        E::Var(n) if self.lookup(n).is_none() && !self.owner.is_some_and(|c| self.tables.classes[c].fields.iter().any(|f| &f.name == n)) => self.tables.names.get(n).copied(),
                        _ => None,
                    };
                    if let Some(class) = static_class {
                        if name == "get" && matches!(self.tables.classes[class].native, Some(t::NativeClass::List(Type::Object(_)))) {
                            return Err(Diagnostic::new(expr.span, "[E5002] get does not copy owned objects from a List; use pop or remove to transfer ownership"));
                        }
                        let id = *self.tables.classes[class].methods.get(name).ok_or_else(|| Diagnostic::new(expr.span, format!("method `{name}` is unknown")))?;
                        if self.tables.signatures[id].kind != Kind::Static {
                            return Err(Diagnostic::new(expr.span, "an instance method requires an object, not a class name"));
                        }
                        self.call(id, None, args, expr.span)?
                    } else {
                        let receiver = self.place(receiver, false)?;
                        let Some(class) = receiver.ty.object_class() else { return Err(Diagnostic::new(expr.span, "cannot call a method on a value that is not an object")); };
                        if name == "get" && matches!(self.tables.classes[class].native, Some(t::NativeClass::List(Type::Object(_)))) {
                            return Err(Diagnostic::new(expr.span, "[E5002] get does not copy owned objects from a List; use pop or remove to transfer ownership"));
                        }
                        let id = *self.tables.classes[class].methods.get(name).ok_or_else(|| Diagnostic::new(expr.span, format!("method `{name}` is unknown")))?;
                        if self.tables.signatures[id].kind == Kind::Static {
                            return Err(Diagnostic::new(expr.span, "call a static method with the class name, not an object"));
                        }
                        self.call(id, Some(receiver), args, expr.span)?
                    }
                }
            }
            E::Unary { op, expr: inner } => {
                let inner = self.expression(inner)?;
                let target = match op {
                    UnaryOp::Not if inner.ty == Type::Boolean => Type::Boolean,
                    UnaryOp::Neg | UnaryOp::Plus => inner.ty.promoted_numeric()
                        .ok_or_else(|| Diagnostic::new(expr.span, "[E6105] unary arithmetic requires a numeric operand"))?,
                    UnaryOp::BitNot => inner.ty.promoted_numeric().filter(|ty| ty.is_integer())
                        .ok_or_else(|| Diagnostic::new(expr.span, "[E6105] bitwise not requires an integer operand"))?,
                    UnaryOp::Not => return Err(Diagnostic::new(expr.span, "[E6105] logical not requires a boolean operand")),
                };
                let inner = if target.is_numeric() { coerce_numeric(inner, target) } else { inner };
                (t::ExprKind::Unary { op: *op, expr: Box::new(inner) }, target)
            }
            E::Binary { op, left, right } => {
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                let (left, right, ty) = match op {
                    B::Add | B::Sub | B::Mul | B::Div | B::Rem |
                    B::Lt | B::Le | B::Gt | B::Ge => {
                        let common = numeric_common(left.ty, right.ty)
                            .ok_or_else(|| Diagnostic::new(expr.span, "[E6105] numeric operator requires numeric operands"))?;
                        let result = if matches!(op, B::Lt | B::Le | B::Gt | B::Ge) { Type::Boolean } else { common };
                        (coerce_numeric(left, common), coerce_numeric(right, common), result)
                    }
                    B::BitAnd | B::BitOr | B::BitXor => {
                        let common = numeric_common(left.ty, right.ty).filter(|ty| ty.is_integer())
                            .ok_or_else(|| Diagnostic::new(expr.span, "[E6105] bitwise operators require integer operands"))?;
                        (coerce_numeric(left, common), coerce_numeric(right, common), common)
                    }
                    B::ShiftLeft | B::ShiftRight => {
                        let left_type = left.ty.promoted_numeric().filter(|ty| ty.is_integer())
                            .ok_or_else(|| Diagnostic::new(expr.span, "[E6105] shift operators require an integer left operand"))?;
                        let count_type = right.ty.promoted_numeric();
                        if count_type != Some(Type::Int) {
                            return Err(Diagnostic::new(expr.span, "[E6105] shift count must be a byte, char, or int"));
                        }
                        (coerce_numeric(left, left_type), coerce_numeric(right, Type::Int), left_type)
                    }
                    B::And | B::Or => {
                        self.require(Type::Boolean, left.ty, left.span)?;
                        self.require(Type::Boolean, right.ty, right.span)?;
                        (left, right, Type::Boolean)
                    }
                    B::Eq | B::Ne => {
                        if let Some(common) = numeric_common(left.ty, right.ty) {
                            (coerce_numeric(left, common), coerce_numeric(right, common), Type::Boolean)
                        } else if left.ty == Type::Boolean && right.ty == Type::Boolean {
                            (left, right, Type::Boolean)
                        } else {
                            return Err(Diagnostic::new(expr.span, "[E6105] equality requires matching numeric or boolean operands"));
                        }
                    }
                };
                (t::ExprKind::Binary { op: *op, left: Box::new(left), right: Box::new(right) }, ty)
            }
        };
        Ok(t::Expr { kind, ty, span: expr.span })
    }
}
