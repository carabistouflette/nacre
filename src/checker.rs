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
            a::Type::Int => Type::Int, a::Type::Boolean => Type::Boolean,
            a::Type::String => Type::String, a::Type::Void => Type::Void,
            a::Type::Named(name) => {
                let id = *self.names.get(name).ok_or_else(|| Diagnostic::new(span, format!("classe `{name}` inconnue")))?;
                if self.is_static_native(id) {
                    return Err(Diagnostic::new(span, format!("`{name}` est une classe native statique et ne peut pas etre utilisee comme valeur")));
                }
                Type::Object(id)
            }
            a::Type::Applied { .. } => return Err(Diagnostic::new(span, "[E4102] type generique non specialise")),
            a::Type::Borrow { name, mutable, .. } => {
                let class = *self.names.get(name).ok_or_else(|| Diagnostic::new(span, format!("classe `{name}` inconnue")))?;
                if self.is_static_native(class) {
                    return Err(Diagnostic::new(span, format!("`{name}` est une classe native statique; ses valeurs ne peuvent pas etre empruntees")));
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
            Type::Int => "int".to_owned(), Type::Boolean => "boolean".to_owned(),
            Type::String => "String".to_owned(), Type::Void => "void".to_owned(),
            Type::Object(id) => self.classes[id].name.clone(),
            Type::Borrow { class, mutable } => format!("borrow {}{}", if mutable { "mut " } else { "" }, self.classes[class].name),
        }
    }
    fn is_view(&self, ty: Type) -> bool {
        ty.object_class().is_some_and(|id| self.classes[id].has_borrow)
    }
    fn add_signature(&mut self, f: &a::Function, owner: Option<usize>, kind: Kind) -> Result<usize, Diagnostic> {
        if matches!(f.name.as_str(), "println" | "move" | "drop") { return Err(Diagnostic::new(f.span, format!("`{}` est une fonction integree reservee", f.name))); }
        let params: Vec<Type> = f.params.iter().map(|p| self.resolve_type(&p.ty, p.span)).collect::<Result<_, _>>()?;
        let return_type = self.resolve_type(&f.return_type, f.span)?;
        if return_type.is_borrow() && !matches!(kind, Kind::Instance | Kind::Interface) {
            return Err(Diagnostic::new(f.span, "[E3004] borrowed returns require an instance receiver"));
        }
        if matches!(return_type, Type::Borrow { mutable: true, .. }) && f.readonly {
            return Err(Diagnostic::new(f.span, "[E3003] a readonly receiver cannot return an exclusive borrow"));
        }
        if self.is_view(return_type) || params.iter().any(|ty| self.is_view(*ty)) {
            return Err(Diagnostic::new(f.span, "[E3007] une classe a champs empruntes est une vue locale; ni parametre ni retour de vue en V5"));
        }
        let id = self.signatures.len();
        let virtual_slot = if kind == Kind::Interface { Some(self.classes[owner.unwrap()].interface_methods.len()) } else { None };
        self.signatures.push(Signature { id, owner, kind, name: f.name.clone(), params, return_type, visibility: f.visibility, readonly: f.readonly, virtual_slot });
        Ok(id)
    }
}

pub fn check(program: &a::Program) -> Result<t::Program, Diagnostic> {
    if program.classes.iter().filter(|c| !c.is_builtin).count() > 128 {
        return Err(Diagnostic::new(Span::default(), "limite V5: 128 classes utilisateur par programme"));
    }
    let mut tables = Tables { names: HashMap::new(), classes: Vec::new(), globals: HashMap::new(), signatures: Vec::new() };
    for (id, class) in program.classes.iter().enumerate() {
        if class.name == "System" || class.name == "println" {
            return Err(Diagnostic::new(class.span, "nom de classe reserve au support d'affichage"));
        }
        if tables.names.insert(class.name.clone(), id).is_some() {
            return Err(Diagnostic::new(class.span, format!("classe `{}` deja declaree", class.name)));
        }
        tables.classes.push(ClassInfo { native: None, name: class.name.clone(), fields: Vec::new(),
            variants: None, methods: HashMap::new(), constructor: None, has_borrow: false,
            is_interface: class.is_interface, interface_methods: Vec::new(), implementations: Vec::new() });
    }
    for (id, class) in program.classes.iter().enumerate() {
        tables.classes[id].native = match &class.native {
            Some(a::NativeClass::Text) => Some(t::NativeClass::Text),
            Some(a::NativeClass::List(_)) => Some(t::NativeClass::List(Type::Void)),
            Some(a::NativeClass::Map(_, _)) => Some(t::NativeClass::Map(Type::Void, Type::Void)),
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
            Some(a::NativeClass::Map(key, value)) => {
                tables.classes[id].native = Some(t::NativeClass::Map(
                    tables.resolve_type(key, class.span)?,
                    tables.resolve_type(value, class.span)?,
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
                return Err(Diagnostic::new(field.span, "`System` est un nom reserve"));
            }
            if !names.insert(&field.name) {
                return Err(Diagnostic::new(field.span, format!("champ `{}` deja declare", field.name)));
            }
            let ty = tables.resolve_type(&field.ty, field.span)?;
            if matches!(ty, Type::Borrow { mutable: true, .. }) {
                return Err(Diagnostic::new(field.span, "[E3007] les champs empruntes sont en lecture seule en V5; borrow mut est reserve aux variables et parametres"));
            }
            if ty.is_borrow() && field.initializer.is_some() {
                return Err(Diagnostic::new(field.span, "[E3007] initialiser un champ emprunte dans le constructeur depuis un parametre borrow"));
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
                    return Err(Diagnostic::new(class.span, "[E5003] List<T> exige une valeur copiable ou un objet possede; les vues locales et les emprunts sont interdits"));
                }
            }
            Some(t::NativeClass::Map(key, value)) => {
                if key != Type::Int {
                    return Err(Diagnostic::new(class.span, "[E5004] Map<K, V> exige int comme type de cle"));
                }
                if tables.is_view(value) || matches!(value, Type::Void | Type::Borrow { .. }) {
                    return Err(Diagnostic::new(class.span, "[E5005] Map<int, V> exige une valeur possedee sans emprunt ni vue locale"));
                }
            }
            _ => {}
        }
    }
    for class in &program.classes {
        for field in &class.fields {
            if tables.is_view(tables.resolve_type(&field.ty, field.span)?) {
                return Err(Diagnostic::new(field.span, "[E3007] une vue locale ne peut pas etre stockee ou empruntee dans un autre objet"));
            }
        }
        if let Some(variants) = &class.variants {
            for variant in variants {
                for field in &variant.fields {
                    let ty = tables.resolve_type(field, variant.span)?;
                    if ty.is_borrow() || tables.is_view(ty) {
                        return Err(Diagnostic::new(variant.span,
                            "[E6003] un payload d'enum ne peut pas etre un emprunt ni contenir une vue"));
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
            return Err(Diagnostic::new(class.span, "cycle de representation enum inline: taille C infinie"));
        }
    }

    // Owned AST declarations keep inferred default constructors independent
    // from parser lifetimes. Their order also fixes stable C symbol IDs.
    let mut declarations: Vec<a::Function> = Vec::new();
    for function in &program.functions {
        let id = tables.add_signature(function, None, Kind::Free)?;
        if tables.globals.insert(function.name.clone(), id).is_some() {
            return Err(Diagnostic::new(function.span, format!("fonction `{}` deja declaree; pas de surcharge en V5", function.name)));
        }
        declarations.push(function.clone());
    }
    for (class_id, class) in program.classes.iter().enumerate() {
        for method in &class.methods {
            let kind = if class.is_interface { Kind::Interface } else if method.is_static { Kind::Static } else { Kind::Instance };
            let id = tables.add_signature(method, Some(class_id), kind)?;
            if tables.classes[class_id].methods.insert(method.name.clone(), id).is_some() {
                return Err(Diagnostic::new(method.span, format!("methode `{}` deja declaree; pas de surcharge en V5", method.name)));
            }
            if class.is_interface { tables.classes[class_id].interface_methods.push(id); }
            declarations.push(method.clone());
        }
        if class.is_interface || class.variants.is_some() || tables.is_static_native(class_id) { continue; }
        let constructor = class.constructor.clone().unwrap_or_else(|| a::Function {
            name: class.name.clone(), span: class.span, return_type: a::Type::Void,
            params: Vec::new(), body: Vec::new(), visibility: Visibility::Public, is_static: false, readonly: false,
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
                return Err(Diagnostic::new(class.span, "[E4001] implements exige une interface"));
            };
            if !tables.classes[interface_id].is_interface {
                return Err(Diagnostic::new(class.span, format!("[E4001] `{}` n'est pas une interface", tables.classes[interface_id].name)));
            }
            if !implemented.insert(interface_id) {
                return Err(Diagnostic::new(class.span, "[E4001] interface implementee plusieurs fois"));
            }
            if tables.is_view(Type::Object(class_id)) {
                return Err(Diagnostic::new(class.span, "[E4005] une vue locale ne peut pas etre effacee derriere une interface en V5"));
            }
            let mut methods = Vec::new();
            for id in &tables.classes[interface_id].interface_methods {
                let expected = &tables.signatures[*id];
                let implementation = *tables.classes[class_id].methods.get(&expected.name)
                    .ok_or_else(|| Diagnostic::new(class.span, format!("[E4002] implementation manquante de `{}.{}`", tables.classes[interface_id].name, expected.name)))?;
                let actual = &tables.signatures[implementation];
                if actual.kind != Kind::Instance || actual.visibility != Visibility::Public
                    || actual.params != expected.params || actual.return_type != expected.return_type
                    || actual.readonly != expected.readonly {
                    return Err(Diagnostic::new(declarations[implementation].span, format!("[E4003] signature incompatible pour `{}.{}`: verifier public, readonly, retour et modes des parametres", tables.classes[interface_id].name, expected.name))
                        .with_note(declarations[*id].span, "contrat de l'interface declare ici"));
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
                Type::Int | Type::Boolean | Type::String | Type::Void | Type::Borrow { .. } => true,
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
                "cycle de champs objets sans variant vide: aucun chemin de construction fini"));
        }
    }
    let entries: Vec<_> = tables.signatures.iter().filter(|s| s.name == "main" && matches!(s.kind, Kind::Free | Kind::Static)).collect();
    if entries.len() > 1 {
        return Err(Diagnostic::new(Span::default(), "entree ambigue: plusieurs fonctions ou methodes static nommees main"));
    }
    let main = entries.first().ok_or_else(|| Diagnostic::new(Span::default(), "fonction d'entree `void main()` manquante; seules les methodes `static` peuvent servir d'entree"))?;
    if main.return_type != Type::Void || !main.params.is_empty() {
        return Err(Diagnostic::new(declarations[main.id].span, "l'entree doit avoir la signature `void main()` sans parametres"));
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
            return Err(Diagnostic::new(function.span, format!("`{}` doit retourner un {} sur tous les chemins; une boucle n'est pas une preuve de retour en V5", function.name, tables.type_name(sig.return_type))));
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
        a::ExprKind::Unary { expr, .. } | a::ExprKind::Borrow { expr, .. } => expr.depth,
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
        if name == "System" { return Err(Diagnostic::new(span, "`System` est un nom reserve")); }
        if self.lookup(name).is_some() {
            return Err(Diagnostic::new(span, format!("variable `{name}` deja declaree dans une portee active")));
        }
        let id = self.variables.len();
        self.variables.push(t::Local { ty, name: name.to_owned(), span });
        self.scopes.last_mut().unwrap().insert(name.to_owned(), id);
        Ok(id)
    }
    fn require(&self, expected: Type, actual: Type, span: Span) -> Result<(), Diagnostic> {
        if expected == actual { Ok(()) } else {
            Err(Diagnostic::new(span, format!("type incompatible: attendu {}, trouve {}", self.tables.type_name(expected), self.tables.type_name(actual))))
        }
    }
    fn require_initialized(&self, span: Span) -> Result<(), Diagnostic> {
        if !self.constructor { return Ok(()); }
        let class = &self.tables.classes[self.owner.unwrap()];
        let missing: Vec<_> = class.fields.iter().zip(&self.initialized).filter(|(_, ready)| !**ready).map(|(f, _)| f.name.as_str()).collect();
        if missing.is_empty() { Ok(()) } else {
            Err(Diagnostic::new(span, format!("champs non initialises: {}; initialiser tous les champs avant un appel sur this ou une sortie du constructeur", missing.join(", "))))
        }
    }
    fn accessible(&self, visibility: Visibility, owner: usize, span: Span, what: &str) -> Result<(), Diagnostic> {
        if visibility == Visibility::Private && self.owner != Some(owner) {
            Err(Diagnostic::new(span, format!("{what} private: acces interdit depuis cette classe ou fonction")))
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
                if stops { return Err(Diagnostic::new(stmt.span, "instruction inaccessible apres return, break ou continue")); }
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
                    return Err(Diagnostic::new(stmt.span, "[E3005] un emprunt local exige un initialiseur et ne peut pas etre reaffecte"));
                }
                if ty.is_borrow() && self.tables.is_view(ty) {
                    return Err(Diagnostic::new(stmt.span, "[E3007] emprunter une vue locale n'est pas pris en charge en V5"));
                }
                if ty.is_object() && self.tables.is_view(ty)
                    && !value.as_ref().is_some_and(|v| matches!(v.kind, a::ExprKind::New { .. })) {
                    return Err(Diagnostic::new(stmt.span, "[E3007] une vue locale exige une initialisation directe par new"));
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
                    return Err(Diagnostic::new(stmt.span, "initialisation de champ exigee directement dans le constructeur, hors branche, boucle ou bloc imbrique en V5"));
                }
                if !is_initialization {
                    if target.ty.is_borrow() {
                        return Err(Diagnostic::new(stmt.span, "[E3005] reaffectation d'un emprunt interdite; creer un nouvel emprunt dans un bloc"));
                    }
                    if target.ty.is_object() && self.tables.is_view(target.ty) {
                        return Err(Diagnostic::new(stmt.span, "[E3007] remplacement d'une vue locale interdit"));
                    }
                    if let PlaceKind::Field { base, .. } = &target.kind { self.require_mutable(base, stmt.span)?; }
                }
                if target.ty.is_borrow() && is_initialization {
                    let a::ExprKind::Var(name) = &value.kind else {
                        return Err(Diagnostic::new(stmt.span, "[E3007] un champ emprunte doit provenir directement d'un parametre borrow du constructeur"));
                    };
                    let source = self.lookup(name).and_then(|id| self.parameter_ids.iter().position(|p| *p == id))
                        .ok_or_else(|| Diagnostic::new(stmt.span, "[E3007] un champ emprunte exige un parametre borrow, pas un proprietaire local"))?;
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
                        if self.locks.contains(id) { return Err(Diagnostic::new(stmt.span, "objet emprunte par un appel en cours: remplacement interdit")); }
                    }
                    PlaceKind::This => return Err(Diagnostic::new(stmt.span, "affectation de this interdite")),
                    PlaceKind::Field { .. } => {}
                }
                if let Some(field) = init_field { self.initialized[field] = true; }
                (t::Stmt::Assign { target, value }, false)
            }
            S::Expr(expr) => {
                if !matches!(&expr.kind, a::ExprKind::Call { .. } | a::ExprKind::MethodCall { .. }
                    | a::ExprKind::New { .. } | a::ExprKind::Variant { .. }) {
                    return Err(Diagnostic::new(expr.span, "seul un appel ou une construction peut etre utilise comme instruction d'expression"));
                }
                let value = self.expression(expr)?;
                if value.ty.is_object() && self.tables.is_view(value.ty) {
                    return Err(Diagnostic::new(expr.span, "[E3007] conserver une vue locale dans une variable initialisee par new"));
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
            S::ForEach { ty, name, collection, body, header_span } => {
                (self.for_each(ty, name, collection, body, *header_span, stmt.span)?, false)
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
                    .ok_or_else(|| Diagnostic::new(stmt.span, format!("match exige une enum, pas `{}`", self.tables.classes[class].name)))?;
                let mut seen = vec![false; variants.len()];
                let mut checked_arms = Vec::with_capacity(arms.len());
                let mut all_stop = true;
                for arm in arms {
                    let variant = variants.iter().position(|v| v.name == arm.variant)
                        .ok_or_else(|| Diagnostic::new(arm.span, format!("variant `{}` inconnue dans `{}`", arm.variant, self.tables.classes[class].name)))?;
                    if seen[variant] {
                        return Err(Diagnostic::new(arm.span, format!("variant `{}` repetee dans match", arm.variant)));
                    }
                    seen[variant] = true;
                    let payload = &variants[variant].fields;
                    if arm.bindings.len() != payload.len() {
                        return Err(Diagnostic::new(arm.span, format!("arm `{}` attend {} binding(s) de payload, {} fourni(s)",
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
                    return Err(Diagnostic::new(stmt.span, format!("match non exhaustif; variants manquantes: {}", missing.join(", "))));
                }
                (t::Stmt::Match { value, arms: checked_arms }, all_stop)
            }
            S::Return(value) => {
                if self.return_type == Type::Void && value.is_some() {
                    return Err(Diagnostic::new(stmt.span, "une fonction void utilise `return;`, sans valeur"));
                }
                let value = match value { Some(v) => Some(self.expected_expression(self.return_type, v)?), None => None };
                self.require(self.return_type, value.as_ref().map_or(Type::Void, |v| v.ty), stmt.span)?;
                self.require_initialized(stmt.span)?;
                (t::Stmt::Return(value), true)
            }
            S::Break | S::Continue => {
                if self.loop_depth == 0 {
                    return Err(Diagnostic::new(stmt.span, "break et continue sont autorises uniquement dans une boucle"));
                }
                (if matches!(stmt.kind, S::Break) { t::Stmt::Break } else { t::Stmt::Continue }, true)
            }
            S::Block(block) => { let (block, stops) = self.block(block)?; (t::Stmt::Block(block), stops) }
        })
    }

    fn for_each(&mut self, header: &a::Type, name: &str, collection: &a::Expr, body: &a::Block,
                header_span: Span, span: Span) -> Result<t::Stmt, Diagnostic> {
        let place = self.place(collection, false).ok();
        let value = if place.is_none() { Some(self.expression(collection)?) } else { None };
        let source_type = if let Some(place) = &place { place.ty } else { value.as_ref().unwrap().ty };
        let list_class = source_type.object_class()
            .ok_or_else(|| Diagnostic::new(collection.span, "la collection d'une boucle for-each doit etre une List<T> nommee ou empruntee"))?;
        let Some(t::NativeClass::List(element_type)) = self.tables.classes[list_class].native else {
            return Err(Diagnostic::new(collection.span, "la collection d'une boucle for-each doit etre une List<T>"));
        };
        let header_type = self.tables.resolve_type(header, header_span)?;
        let (exclusive, getter) = match element_type {
            Type::Int | Type::Boolean | Type::String => {
                if header_type != element_type {
                    return Err(Diagnostic::new(header_span, format!(
                        "type d'element incompatible: List<{}> exige le meme type copiable dans l'entete",
                        self.tables.type_name(element_type))));
                }
                (false, "get")
            }
            Type::Object(element_class) => {
                let Type::Borrow { class: header_class, mutable } = header_type else {
                    return Err(Diagnostic::new(header_span,
                        "[E5004] un element affine de List<T> doit etre lie par `borrow T` ou `borrow mut T`"));
                };
                if header_class != element_class {
                    return Err(Diagnostic::new(header_span, format!(
                        "type d'emprunt incompatible: attendu {}, trouve {}",
                        self.tables.type_name(Type::Borrow { class: element_class, mutable }),
                        self.tables.type_name(header_type))));
                }
                (mutable, if mutable { "getMut" } else { "getRef" })
            }
            _ => return Err(Diagnostic::new(collection.span, "type d'element non pris en charge dans List<T>")),
        };
        if matches!(source_type, Type::Object(_)) && place.is_none() {
            return Err(Diagnostic::new(collection.span,
                "une collection possedee temporaire ne peut pas etre parcourue; utiliser un emplacement stable ou un emprunt existant"));
        }
        if exclusive && matches!(source_type, Type::Borrow { mutable: false, .. }) {
            return Err(Diagnostic::new(collection.span,
                "[E3003] un parcours mutable exige un emprunt exclusif de la liste"));
        }
        let list_type = Type::Borrow { class: list_class, mutable: exclusive };
        let frozen_value = if let Some(place) = place {
            if matches!(place.kind, PlaceKind::This) { self.require_initialized(collection.span)?; }
            if exclusive { self.require_mutable(&place, collection.span)?; }
            t::Expr { kind: t::ExprKind::Borrow { place, mutable: exclusive },
                ty: list_type, span: collection.span }
        } else {
            let mut value = value.unwrap();
            let Type::Borrow { class, mutable } = value.ty else {
                return Err(Diagnostic::new(collection.span,
                    "une collection possedee temporaire ne peut pas etre parcourue; utiliser un emplacement stable ou un emprunt existant"));
            };
            if exclusive && !mutable {
                return Err(Diagnostic::new(collection.span,
                    "[E3003] un parcours mutable exige un emprunt exclusif de la liste"));
            }
            if class != list_class {
                return Err(Diagnostic::new(collection.span, "type d'emprunt de liste incompatible"));
            }
            value.ty = list_type;
            value
        };

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

    fn this_place(&self, span: Span) -> Result<Place, Diagnostic> {
        let id = self.receiver.ok_or_else(|| Diagnostic::new(span, "this ou membre d'instance inaccessible depuis une fonction ou methode static"))?;
        Ok(Place { kind: PlaceKind::This, ty: Type::Object(id) })
    }
    fn field_place(&self, base: Place, name: &str, span: Span, write: bool) -> Result<Place, Diagnostic> {
        let Some(class_id) = base.ty.object_class() else { return Err(Diagnostic::new(span, "acces a un champ sur une valeur qui n'est pas un objet")); };
        let class = &self.tables.classes[class_id];
        let field = class.fields.iter().position(|f| f.name == name)
            .ok_or_else(|| Diagnostic::new(span, format!("champ `{name}` inconnu dans `{}`", class.name)))?;
        let info = &class.fields[field];
        self.accessible(info.visibility, class_id, span, "champ")?;
        if matches!(base.kind, PlaceKind::This) && self.constructor && !write && !self.initialized[field] {
            return Err(Diagnostic::new(span, format!("champ `{name}` non initialise")));
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
                } else { Err(Diagnostic::new(expr.span, format!("variable `{name}` non declaree"))) }
            }
            a::ExprKind::This => self.this_place(expr.span),
            a::ExprKind::Field { base, name } => {
                let base = self.place(base, false)?;
                self.field_place(base, name, expr.span, write)
            }
            _ => Err(Diagnostic::new(expr.span, "recepteur temporaire non pris en charge en V5; stocker l'objet dans une variable locale")),
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
                    return Err(Diagnostic::new(expr.span, format!("type incompatible: attendu {}, trouve {}", self.tables.type_name(expected), self.tables.type_name(value.ty))));
                }
                // Downgrade the returned capability before retaining its caller loan.
                value.ty = Type::Borrow { class: actual, mutable };
                if actual != class {
                    return Ok(t::Expr { kind: t::ExprKind::Upcast { value: Box::new(value), interface: class, concrete: actual },
                        ty: expected, span: expr.span });
                }
                return Ok(value);
            }
            let place = self.place(expr, false).map_err(|d| d.with_help("un emprunt exige un objet nomme ou un champ; stocker d'abord un objet temporaire dans une variable"))?;
            if !place.ty.object_class().is_some_and(|actual| self.tables.can_upcast(actual, class)) {
                return Err(Diagnostic::new(expr.span, format!("type incompatible: attendu {}, trouve {}", self.tables.type_name(expected), self.tables.type_name(place.ty))));
            }
            if self.tables.is_view(expected) {
                return Err(Diagnostic::new(expr.span, "[E3007] emprunter une vue locale n'est pas pris en charge en V5"));
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
        self.require(expected, value.ty, value.span)?;
        Ok(value)
    }
    /// Shared access is transitive through owned fields. A borrowed field is
    /// always shared in V5, even when its containing view is mutable.
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
            Err(Diagnostic::new(span, "[E3003] modification interdite a travers un emprunt en lecture seule ou un receveur readonly")
                .with_help("declarer un parametre borrow mut pour modifier; appeler uniquement les methodes readonly sur un emprunt partage"))
        }
    }
    fn take_place(&mut self, place: Place, span: Span) -> Result<t::ExprKind, Diagnostic> {
        if !place.ty.is_object() { return Ok(t::ExprKind::Read(place)); }
        if self.tables.is_view(place.ty) {
            return Err(Diagnostic::new(span, "[E3007] deplacement d'une vue locale interdit; sa duree de vie est son bloc de declaration"));
        }
        let PlaceKind::Var(id) = place.kind else {
            return Err(Diagnostic::new(span, "deplacement de this ou d'un champ objet interdit en V5; seuls les objets locaux et parametres sont transferables"));
        };
        if self.locks.contains(&id) {
            return Err(Diagnostic::new(span, "objet emprunte comme recepteur d'un appel en cours; deplacement interdit"));
        }
        Ok(t::ExprKind::Move(id))
    }
    fn check_args(&mut self, sig: &Signature, args: &[a::Expr], span: Span) -> Result<Vec<t::Expr>, Diagnostic> {
        if sig.params.len() != args.len() {
            return Err(Diagnostic::new(span, format!("`{}` attend {} argument(s), {} recu(s)", sig.name, sig.params.len(), args.len())));
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
        if let Some(owner) = sig.owner { self.accessible(sig.visibility, owner, span, "methode ou constructeur")?; }
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
        if args.len() != 1 { return Err(Diagnostic::new(span, "println attend exactement un argument")); }
        let value = self.expression(&args[0])?;
        if value.ty == Type::Void { return Err(Diagnostic::new(span, "println ne peut pas afficher une expression void")); }
        if value.ty.object_class().is_some() { return Err(Diagnostic::new(span, "println ne peut pas afficher un objet; afficher un champ ou le resultat d'une methode")); }
        Ok((t::ExprKind::Call { target: CallTarget::Println, receiver: None, args: vec![value] }, Type::Void))
    }
    fn expression(&mut self, expr: &a::Expr) -> Result<t::Expr, Diagnostic> {
        if self.expr_depth >= 128 { return Err(Diagnostic::new(expr.span, "expression trop profonde pour cette V5")); }
        self.expr_depth += 1;
        let result = self.expression_inner(expr);
        self.expr_depth -= 1; result
    }
    fn expression_inner(&mut self, expr: &a::Expr) -> Result<t::Expr, Diagnostic> {
        use a::ExprKind as E;
        let (kind, ty) = match &expr.kind {
            E::Int(n) => (t::ExprKind::Int(*n), Type::Int),
            E::Boolean(b) => (t::ExprKind::Boolean(*b), Type::Boolean),
            E::String(s) => (t::ExprKind::String(s.clone()), Type::String),
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
                let class = *self.tables.names.get(class).ok_or_else(|| Diagnostic::new(expr.span, format!("classe `{class}` inconnue")))?;
                if self.tables.is_static_native(class) {
                    return Err(Diagnostic::new(expr.span, format!("`{}` est statique et ne peut pas etre construite", self.tables.classes[class].name)));
                }
                if self.tables.classes[class].is_interface {
                    return Err(Diagnostic::new(expr.span, "[E4004] une interface ne peut pas etre instanciee avec new"));
                }
                if self.tables.classes[class].variants.is_some() {
                    return Err(Diagnostic::new(expr.span, "une enum se construit avec `new Enum.Variant(...)`, pas avec `new Enum(...)`"));
                }
                let constructor = self.tables.classes[class].constructor
                    .ok_or_else(|| Diagnostic::new(expr.span, "cette classe n'a pas de constructeur"))?;
                let sig = self.tables.signatures[constructor].clone();
                self.accessible(sig.visibility, class, expr.span, "constructeur")?;
                let args = self.check_args(&sig, args, expr.span)?;
                (t::ExprKind::New { class, constructor, args }, Type::Object(class))
            }
            E::Variant { class, variant, args, .. } => {
                let class = *self.tables.names.get(class).ok_or_else(|| Diagnostic::new(expr.span, format!("enum `{class}` inconnue")))?;
                let variants = self.tables.classes[class].variants.as_ref()
                    .ok_or_else(|| Diagnostic::new(expr.span, format!("`{}` n'est pas une enum; variant constructor interdit", self.tables.classes[class].name)))?;
                let variant_id = variants.iter().position(|v| v.name == *variant)
                    .ok_or_else(|| Diagnostic::new(expr.span, format!("variant `{variant}` inconnue dans `{}`", self.tables.classes[class].name)))?;
                let fields = variants[variant_id].fields.clone();
                if args.len() != fields.len() {
                    return Err(Diagnostic::new(expr.span, format!("variant `{variant}` attend {} payload(s), {} fourni(s)", fields.len(), args.len())));
                }
                let values = args.iter().zip(fields).map(|(arg, expected)| self.expected_expression(expected, arg))
                    .collect::<Result<Vec<_>, _>>()?;
                (t::ExprKind::Variant { class, variant: variant_id, args: values }, Type::Object(class))
            }
            E::Call { name, args } => {
                if name == "println" { self.builtin(args, expr.span)? }
                else if name == "move" || name == "drop" {
                    if args.len() != 1 { return Err(Diagnostic::new(expr.span, format!("{name} attend exactement un argument"))); }
                    let value = self.expression(&args[0])?;
                    if !value.ty.is_object() { return Err(Diagnostic::new(expr.span, format!("{name} exige un objet possede, pas une primitive ou void"))); }
                    if name == "move" { (value.kind, value.ty) }
                    else { (t::ExprKind::Drop(Box::new(value)), Type::Void) }
                }
                else if let Some(id) = self.owner.and_then(|c| self.tables.classes[c].methods.get(name)).copied() {
                    let receiver = if self.tables.signatures[id].kind == Kind::Instance { Some(self.this_place(expr.span)?) } else { None };
                    self.call(id, receiver, args, expr.span)?
                } else {
                    let id = *self.tables.globals.get(name).ok_or_else(|| Diagnostic::new(expr.span, format!("fonction `{name}` inconnue")))?;
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
                            return Err(Diagnostic::new(expr.span, "[E5002] get ne copie pas les objets possedes d'une List; utiliser pop ou remove pour en transferer la propriete"));
                        }
                        let id = *self.tables.classes[class].methods.get(name).ok_or_else(|| Diagnostic::new(expr.span, format!("methode `{name}` inconnue")))?;
                        if self.tables.signatures[id].kind != Kind::Static {
                            return Err(Diagnostic::new(expr.span, "une methode d'instance exige un objet, pas un nom de classe"));
                        }
                        self.call(id, None, args, expr.span)?
                    } else {
                        let receiver = self.place(receiver, false)?;
                        let Some(class) = receiver.ty.object_class() else { return Err(Diagnostic::new(expr.span, "appel de methode sur une valeur qui n'est pas un objet")); };
                        if name == "get" && matches!(self.tables.classes[class].native, Some(t::NativeClass::List(Type::Object(_)))) {
                            return Err(Diagnostic::new(expr.span, "[E5002] get ne copie pas les objets possedes d'une List; utiliser pop ou remove pour en transferer la propriete"));
                        }
                        let id = *self.tables.classes[class].methods.get(name).ok_or_else(|| Diagnostic::new(expr.span, format!("methode `{name}` inconnue")))?;
                        if self.tables.signatures[id].kind == Kind::Static {
                            return Err(Diagnostic::new(expr.span, "appeler une methode static avec le nom de classe, pas un objet"));
                        }
                        self.call(id, Some(receiver), args, expr.span)?
                    }
                }
            }
            E::Unary { op, expr: inner } => {
                let inner = self.expression(inner)?;
                let expected = if *op == UnaryOp::Not { Type::Boolean } else { Type::Int };
                self.require(expected, inner.ty, inner.span)?;
                (t::ExprKind::Unary { op: *op, expr: Box::new(inner) }, expected)
            }
            E::Binary { op, left, right } => {
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                let ty = match op {
                    B::Add | B::Sub | B::Mul | B::Div | B::Rem => {
                        self.require(Type::Int, left.ty, left.span)?;
                        self.require(Type::Int, right.ty, right.span)?; Type::Int
                    }
                    B::Lt | B::Le | B::Gt | B::Ge => {
                        self.require(Type::Int, left.ty, left.span)?;
                        self.require(Type::Int, right.ty, right.span)?; Type::Boolean
                    }
                    B::And | B::Or => {
                        self.require(Type::Boolean, left.ty, left.span)?;
                        self.require(Type::Boolean, right.ty, right.span)?; Type::Boolean
                    }
                    B::Eq | B::Ne => {
                        if !matches!(left.ty, Type::Int | Type::Boolean) {
                            return Err(Diagnostic::new(expr.span, "== et != sont limites a int et boolean en V5"));
                        }
                        self.require(left.ty, right.ty, right.span)?; Type::Boolean
                    }
                };
                (t::ExprKind::Binary { op: *op, left: Box::new(left), right: Box::new(right) }, ty)
            }
        };
        Ok(t::Expr { kind, ty, span: expr.span })
    }
}
