//! Bounded, deterministic monomorphization before nominal type checking.
//!
//! Nacre uses explicitly instantiated class/interface templates, NOT Java
//! erasure and NOT universally checked bounded generics. A template body is
//! checked for each requested specialization. Every specialization then passes
//! through the same type, ownership and borrow checkers as ordinary code.
use std::collections::{HashMap, HashSet, VecDeque};
use crate::ast::{self as a, Type};
use crate::diagnostic::{Diagnostic, Span};

const MAX_USER_SPECIALIZATIONS: usize = 64;
const MAX_TYPE_NAME: usize = 4096;
type Environment = HashMap<String, Type>;
fn supports_default_ordering(ty: &Type) -> bool {
    matches!(ty, Type::Byte | Type::Char | Type::Int | Type::Long | Type::Double | Type::Boolean | Type::String)
}

fn supports_default_hashing(ty: &Type) -> bool {
    ty.is_copy() || matches!(ty, Type::Named(name) if name == "Text")
}

fn rewrite_borrowed_type_parameter_to_value(
    methods: &mut [a::Function],
    method_name: &str,
    index: usize,
    parameter_name: &str,
) {
    if let Some(param) = methods.iter_mut().find(|method| method.name == method_name)
        .and_then(|method| method.params.get_mut(index))
        && matches!(&param.ty, Type::Borrow { name, args, .. } if name == parameter_name && args.is_empty()) {
            param.ty = Type::Named(parameter_name.to_owned());
        }
}

fn rewrite_borrowed_t_parameter_to_value(methods: &mut [a::Function], method_name: &str, index: usize) {
    rewrite_borrowed_type_parameter_to_value(methods, method_name, index, "T");
}

fn rewrite_borrowed_return_to_value(methods: &mut [a::Function], method_name: &str, parameter_name: &str) {
    if let Some(method) = methods.iter_mut().find(|method| method.name == method_name)
        && matches!(&method.return_type, Type::Borrow { name, args, .. } if name == parameter_name && args.is_empty()) {
            method.return_type = Type::Named(parameter_name.to_owned());
        }
}

fn prune_indexed_collection_methods(class: &mut a::Class, element: &Type) {
    if element.is_copy() {
        class.methods.retain(|method| !matches!(method.name.as_str(), "getRef" | "getMut"));
    } else {
        class.methods.retain(|method| !matches!(method.name.as_str(), "get" | "tryGet"));
    }
}

fn prune_list_methods(class: &mut a::Class, element: &Type) {
    prune_indexed_collection_methods(class, element);
    if !supports_default_ordering(element) {
        class.methods.retain(|method| !matches!(method.name.as_str(), "sort" | "binarySearch"));
    }
    if element.is_copy() {
        rewrite_borrowed_type_parameter_to_value(&mut class.methods, "binarySearchBy", 1, "T");
    }
}

fn prune_map_value_methods(class: &mut a::Class, value: &Type) {
    if value.is_copy() {
        class.methods.retain(|method| !matches!(method.name.as_str(), "getRef" | "getMut"));
    } else {
        class.methods.retain(|method| method.name != "tryGet");
    }
}


#[derive(Clone)]
struct Request { template: String, name: String, args: Vec<Type>, span: Span }
struct Expander {
    definitions: HashMap<String, a::Class>,
    requested: HashSet<String>,
    user_specializations: usize,
    pending: VecDeque<Request>,
    names: Vec<String>,
}

/// The strings are only for inspection. No C symbol is based on a source name.
pub fn expand(mut program: a::Program) -> Result<(a::Program, Vec<String>), Diagnostic> {
    if program.classes.iter().filter(|c| !c.is_builtin).count() > 128 {
        return Err(Diagnostic::new(Span::default(), "limit: 128 user classes or interfaces per program, including templates"));
    }
    let mut expander = Expander { definitions: HashMap::new(), requested: HashSet::new(),
        user_specializations: 0, pending: VecDeque::new(), names: Vec::new() };
    for class in &program.classes {
        if matches!(class.name.as_str(), "System" | "println") {
            return Err(Diagnostic::new(class.span, "class name reserved for output support"));
        }
        if expander.definitions.insert(class.qualified_name.clone(), class.clone()).is_some() {
            return Err(Diagnostic::new(class.span, format!("class or interface `{}` is declared more than once", class.qualified_name)));
        }
    }
    for class in &program.classes {
        if class.is_builtin { continue; }
        for name in &class.type_params {
            if expander.definitions.contains_key(name) || matches!(name.as_str(), "System" | "println" | "move" | "drop" | "assert") {
                return Err(Diagnostic::new(class.span, format!("[E4101] type parameter `{name}` shadows a type name or reserved name")));
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
    expander.request_unit_mutex_if_needed(&empty, Span::default())?;
    while let Some(request) = expander.pending.pop_front() {
        let mut class = expander.definitions[&request.template].clone();
        let env: Environment = class.type_params.iter().cloned().zip(request.args).collect();
        class.name = request.name.clone();
        class.qualified_name = request.name.clone();
        if class.is_builtin { class.span = request.span; }
        class.type_params.clear();
        if let Some(constructor) = &mut class.constructor {
            constructor.name = class.name.clone();
            constructor.qualified_name = format!("{}::{}", class.qualified_name, constructor.name);
        }
        expander.class(&mut class, &env).map_err(|d| d.with_note(request.span,
            format!("specialization `{}` requested here", request.name)))?;
        expander.request_unit_mutex_if_needed(&empty, request.span)?;
        concrete.push(class);
        if concrete.iter().filter(|c| !c.is_builtin).count() > 128 {
            return Err(Diagnostic::new(request.span, "[E4199] limit of 128 concrete types, including specializations"));
        }
    }
    program.classes = concrete;
    Ok((program, expander.names))
}

fn append_type_name(output: &mut String, ty: &Type) {
    match ty {
        Type::Byte => output.push_str("byte"), Type::Char => output.push_str("char"),
        Type::Int => output.push_str("int"), Type::Long => output.push_str("long"),
        Type::Double => output.push_str("double"), Type::Unit => output.push_str("Unit"),
        Type::Boolean => output.push_str("boolean"), Type::String => output.push_str("String"),
        Type::Void => output.push_str("void"),
        Type::Named(name) => output.push_str(name),
        Type::Applied { name, args } => {
            output.push_str(name);
            append_type_arguments(output, args);
        }
        Type::Borrow { name, args, mutable } => {
            output.push_str("borrow ");
            if *mutable { output.push_str("mut "); }
            output.push_str(name);
            if !args.is_empty() { append_type_arguments(output, args); }
        }
    }
}

fn append_type_arguments(output: &mut String, args: &[Type]) {
    output.push('<');
    for (index, arg) in args.iter().enumerate() {
        if index != 0 { output.push(','); }
        append_type_name(output, arg);
    }
    output.push('>');
}

impl Expander {
    fn request_unit_mutex_if_needed(&mut self, env: &Environment, span: Span) -> Result<(), Diagnostic> {
        let required = self.requested.iter()
            .any(|name| crate::stdlib::uses_unit_mutex(name));
        if required && !self.requested.contains("Mutex<Unit>") {
            self.ty(&Type::Applied {
                name: "Mutex".to_owned(),
                args: vec![Type::Unit],
            }, env, span, 0)?;
        }
        Ok(())
    }

    fn ty(&mut self, ty: &Type, env: &Environment, span: Span, depth: usize) -> Result<Type, Diagnostic> {
        if depth > 32 { return Err(Diagnostic::new(span, "[E4199] generic type is too deep (limit 32)")); }
        match ty {
            Type::Byte | Type::Char | Type::Int | Type::Long | Type::Double |
                Type::Unit | Type::Boolean | Type::String | Type::Void => Ok(ty.clone()),
            Type::Named(name) => {
                if let Some(replacement) = env.get(name) { return Ok(replacement.clone()); }
                if self.definitions.get(name).is_some_and(|c|
                    c.native.is_some() && c.type_params.is_empty()) && !self.requested.contains(name) {
                    self.requested.insert(name.clone());
                    self.names.push(name.clone());
                    self.pending.push_back(Request { template: name.clone(), name: name.clone(), args: Vec::new(), span });
                }
                if self.definitions.get(name).is_some_and(|c| !c.type_params.is_empty()) {
                    return Err(Diagnostic::new(span, format!("[E4102] `{name}` requires explicit type arguments")));
                }
                Ok(ty.clone())
            }
            Type::Applied { name, args } => {
                if env.contains_key(name) {
                    return Err(Diagnostic::new(span, "[E4102] a type parameter is not itself a generic constructor"));
                }
                let arity = self.definitions.get(name).ok_or_else(|| Diagnostic::new(span,
                    format!("[E4102] generic type `{name}` is unknown")))?.type_params.len();
                if arity == 0 || arity != args.len() {
                    return Err(Diagnostic::new(span, format!("[E4102] `{name}` expects {arity} type argument(s), {} provided", args.len())));
                }
                let args: Vec<_> = args.iter().map(|t| self.ty(t, env, span, depth + 1)).collect::<Result<_, _>>()?;
                for arg in &args {
                    if matches!(arg, Type::Void | Type::Borrow { .. }) {
                        return Err(Diagnostic::new(span, "[E4103] a generic argument cannot be `void` or a borrow; write `borrow Box<T>` at the use site"));
                    }
                    if let Type::Named(n) = arg
                        && !self.definitions.contains_key(n) && !self.requested.contains(n) {
                            return Err(Diagnostic::new(span, format!("[E4103] type argument `{n}` is unknown")));
                        }
                }
                let mut key = String::from(name);
                append_type_arguments(&mut key, &args);
                if key.len() > MAX_TYPE_NAME {
                    return Err(Diagnostic::new(span, "[E4199] specialization name is too long; recursive expansion is likely"));
                }
                if !self.requested.contains(&key) {
                    if !self.definitions[name].is_builtin {
                        if self.user_specializations >= MAX_USER_SPECIALIZATIONS {
                            return Err(Diagnostic::new(span, "[E4199] limit of 64 user-template specializations; recursive generic expansion is possible"));
                        }
                        self.user_specializations += 1;
                    }
                    self.requested.insert(key.clone());
                    self.names.push(key.clone());
                    self.pending.push_back(Request { template: name.clone(), name: key.clone(), args, span });
                }
                Ok(Type::Named(key))
            }
            Type::Borrow { name, args, mutable } => {
                if args.is_empty() && !mutable
                    && let Some(bound) = env.get(name)
                        && bound.is_copy() { return Ok(bound.clone()); }
                let base = if args.is_empty() { Type::Named(name.clone()) }
                    else { Type::Applied { name: name.clone(), args: args.clone() } };
                let base = self.ty(&base, env, span, depth + 1)?;
                let Type::Named(name) = base else {
                    return Err(Diagnostic::new(span, "[E4103] borrow requires an object type, not a specialized primitive"));
                };
                Ok(Type::Borrow { name, args: Vec::new(), mutable: *mutable })
            }
        }
    }
    fn class(&mut self, class: &mut a::Class, env: &Environment) -> Result<(), Diagnostic> {
        let mut erase_native_interface_marker = false;
        let mut native = class.native.take();
        match &mut native {
            Some(a::NativeClass::List(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                prune_list_methods(class, element);
                self.ty(&Type::Applied {
                    name: "ListIterator".to_owned(),
                    args: vec![element.clone()],
                }, env, class.span, 0)?;
            }
            Some(a::NativeClass::Deque(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                prune_indexed_collection_methods(class, element);
            }
            Some(a::NativeClass::Array(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                if element.is_copy() {
                    class.methods.retain(|method| !matches!(method.name.as_str(), "getRef" | "getMut"));
                } else {
                    class.methods.retain(|method| !matches!(method.name.as_str(), "get" | "tryGet"));
                }
            }
            Some(a::NativeClass::OrderedMap { key, value }) => {
                *key = self.ty(key, env, class.span, 0)?;
                *value = self.ty(value, env, class.span, 0)?;
                self.ty(&Type::Applied {
                    name: "Comparator".to_owned(),
                    args: vec![key.clone()],
                }, env, class.span, 0)?;
                if key.is_copy() {
                    for method_name in [
                        "contains", "tryRemove", "tryGet", "getRef", "getMut",
                        "floorIndex", "ceilingIndex", "lowerIndex", "higherIndex",
                    ] {
                        rewrite_borrowed_type_parameter_to_value(&mut class.methods, method_name, 0, "K");
                    }
                    rewrite_borrowed_return_to_value(&mut class.methods, "keyAt", "K");
                }
                if value.is_copy() {
                    rewrite_borrowed_return_to_value(&mut class.methods, "valueAt", "V");
                    class.methods.retain(|method| method.name != "valueAtMut");
                }
                prune_map_value_methods(class, value);
            }
            Some(a::NativeClass::OrderedSet(key)) => {
                *key = self.ty(key, env, class.span, 0)?;
                self.ty(&Type::Applied {
                    name: "Comparator".to_owned(),
                    args: vec![key.clone()],
                }, env, class.span, 0)?;
                if key.is_copy() {
                    for method_name in ["contains", "remove"] {
                        rewrite_borrowed_type_parameter_to_value(&mut class.methods, method_name, 0, "T");
                    }
                    rewrite_borrowed_return_to_value(&mut class.methods, "keyAt", "T");
                }
                self.ty(&Type::Applied {
                    name: "OrderedMap".to_owned(),
                    args: vec![key.clone(), Type::Unit],
                }, env, class.span, 0)?;
            }
            Some(a::NativeClass::PriorityQueue(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                self.ty(&Type::Applied {
                    name: "Comparator".to_owned(),
                    args: vec![element.clone()],
                }, env, class.span, 0)?;
                if element.is_copy() {
                    rewrite_borrowed_return_to_value(&mut class.methods, "valueAt", "T");
                    class.methods.retain(|method| method.name != "getRef");
                } else {
                    class.methods.retain(|method| !matches!(method.name.as_str(), "peek" | "tryPeek"));
                }
            }
            Some(a::NativeClass::ListIterator(element))
            | Some(a::NativeClass::FilterIterator(element))
            | Some(a::NativeClass::TakeIterator(element))
            | Some(a::NativeClass::SkipIterator(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
            }
            Some(a::NativeClass::Iterators(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                for name in ["TakeIterator", "SkipIterator"] {
                    self.ty(&Type::Applied {
                        name: name.to_owned(),
                        args: vec![element.clone()],
                    }, env, class.span, 0)?;
                }
            }
            Some(a::NativeClass::MapIterator(input, output)) => {
                *input = self.ty(input, env, class.span, 0)?;
                *output = self.ty(output, env, class.span, 0)?;
            }
            Some(a::NativeClass::Iterator(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                self.ty(&Type::Applied {
                    name: "ListIterator".to_owned(),
                    args: vec![element.clone()],
                }, env, class.span, 0)?;
                erase_native_interface_marker = true;
            }
            Some(a::NativeClass::Predicate(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                if element.is_copy() {
                    rewrite_borrowed_t_parameter_to_value(&mut class.methods, "test", 0);
                }
                erase_native_interface_marker = true;
            }
            Some(a::NativeClass::BitSet) => {}
            Some(a::NativeClass::Map { key, value, hasher }) => {
                *key = self.ty(key, env, class.span, 0)?;
                *value = self.ty(value, env, class.span, 0)?;
                if supports_default_hashing(key) {
                    *hasher = None;
                    class.methods.retain(|method| method.name != "withHasher");
                } else {
                    let policy = Type::Applied {
                        name: "Hasher".to_owned(),
                        args: vec![key.clone()],
                    };
                    *hasher = Some(self.ty(&policy, env, class.span, 0)?);
                    class.constructor = None;
                }
                if key.is_copy() {
                    for method in ["containsKey", "tryRemove", "tryGet", "getRef", "getMut"] {
                        rewrite_borrowed_type_parameter_to_value(&mut class.methods, method, 0, "K");
                    }
                }
                prune_map_value_methods(class, value);
            }
            Some(a::NativeClass::Set { key, hasher }) => {
                *key = self.ty(key, env, class.span, 0)?;
                if supports_default_hashing(key) {
                    *hasher = None;
                    class.methods.retain(|method| method.name != "withHasher");
                } else {
                    let policy = Type::Applied {
                        name: "Hasher".to_owned(),
                        args: vec![key.clone()],
                    };
                    *hasher = Some(self.ty(&policy, env, class.span, 0)?);
                    class.constructor = None;
                }
                if key.is_copy() {
                    for method in ["contains", "remove"] {
                        rewrite_borrowed_type_parameter_to_value(&mut class.methods, method, 0, "T");
                    }
                }
                let map = Type::Applied {
                    name: "Map".to_owned(),
                    args: vec![key.clone(), Type::Unit],
                };
                self.ty(&map, env, class.span, 0)?;
            }
            Some(a::NativeClass::Comparator(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                if element.is_copy() {
                    rewrite_borrowed_type_parameter_to_value(&mut class.methods, "compare", 0, "T");
                    rewrite_borrowed_type_parameter_to_value(&mut class.methods, "compare", 1, "T");
                }
                erase_native_interface_marker = true;
            }
            Some(a::NativeClass::Hasher(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                if element.is_copy() {
                    rewrite_borrowed_type_parameter_to_value(&mut class.methods, "hash", 0, "T");
                    rewrite_borrowed_type_parameter_to_value(&mut class.methods, "equals", 0, "T");
                    rewrite_borrowed_type_parameter_to_value(&mut class.methods, "equals", 1, "T");
                }
                erase_native_interface_marker = true;
            }
            Some(a::NativeClass::Thread(element))
            | Some(a::NativeClass::Mutex(element))
            | Some(a::NativeClass::Channel(element))
            | Some(a::NativeClass::Condition(element))
            | Some(a::NativeClass::Future(element))
            | Some(a::NativeClass::ThreadPool(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
            }
            Some(a::NativeClass::Sender(element))
            | Some(a::NativeClass::Receiver(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                let channel = Type::Applied {
                    name: "Channel".to_owned(),
                    args: vec![element.clone()],
                };
                self.ty(&channel, env, class.span, 0)?;
            }
            Some(a::NativeClass::Guard(element)) => {
                *element = self.ty(element, env, class.span, 0)?;
                let mutex = Type::Applied {
                    name: "Mutex".to_owned(),
                    args: vec![element.clone()],
                };
                self.ty(&mutex, env, class.span, 0)?;
                if element.is_copy() {
                    class.methods.retain(|method| !matches!(method.name.as_str(), "getRef" | "getMut"));
                } else {
                    class.methods.retain(|method| !matches!(method.name.as_str(), "read" | "write"));
                }
            }
            Some(a::NativeClass::WebSocket) => {
                // Native connection setup resolves addresses outside source-language bodies.
                self.ty(&Type::Named("SocketAddress".to_owned()), env, class.span, 0)?;
            }
            _ => {}
        }
        if !erase_native_interface_marker {
            class.native = native;
        }
        // Files' native text I/O uses the shared ByteBuffer reserve implementation.
        if matches!(class.native.as_ref(), Some(&a::NativeClass::Files)) {
            self.ty(&Type::Named("ByteBuffer".to_owned()), env, class.span, 0)?;
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
                S::ForEach { ty, key, collection, body, .. } => {
                    *ty = self.ty(ty, env, stmt.span, 0)?;
                    if let Some((key_ty, _)) = key {
                        *key_ty = self.ty(key_ty, env, stmt.span, 0)?;
                    }
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
            E::StaticCall { class, type_args, name, args } => {
                let ty = Type::Applied { name: class.clone(), args: std::mem::take(type_args) };
                let Type::Named(concrete) = self.ty(&ty, env, expr.span, 0)? else {
                    unreachable!("generic static receiver expands to a nominal type");
                };
                for argument in args.iter_mut() { self.expression(argument, env)?; }
                expr.kind = E::MethodCall {
                    receiver: Box::new(a::Expr { kind: E::Var(concrete), span: expr.span, depth: 1 }),
                    name: std::mem::take(name), args: std::mem::take(args),
                };
            }
            E::New { class, type_args, args } | E::Variant { class, type_args, args, .. } => {
                let ty = if type_args.is_empty() { Type::Named(class.clone()) }
                    else { Type::Applied { name: class.clone(), args: type_args.clone() } };
                let ty = self.ty(&ty, env, expr.span, 0)?;
                let Type::Named(name) = ty else {
                    return Err(Diagnostic::new(expr.span, "[E4103] construction requires a concrete object type"));
                };
                *class = name; type_args.clear();
                for arg in args { self.expression(arg, env)?; }
            }
            E::Field { base, .. } => self.expression(base, env)?,
            E::Call { args, .. } => { for arg in args { self.expression(arg, env)?; } }
            E::MethodCall { receiver, args, .. } => {
                if let E::Var(name) = &receiver.kind
                    && self.definitions.get(name).is_some_and(|class|
                        class.native.is_some() && class.type_params.is_empty()) {
                        self.ty(&Type::Named(name.clone()), env, receiver.span, 0)?;
                    }
                self.expression(receiver, env)?;
                for arg in args { self.expression(arg, env)?; }
            }
            E::Unary { expr, .. } | E::Borrow { expr, .. } | E::Cast { expr, .. } |
            E::Try(expr) => self.expression(expr, env)?,
            E::Binary { left, right, .. } => { self.expression(left, env)?; self.expression(right, env)?; }
            E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit
            | E::Boolean(_) | E::String(_) | E::Var(_) | E::This => {}
        }
        Ok(())
    }
}
