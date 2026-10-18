//! Resolve source-unit namespaces before generic expansion and type checking.
use std::collections::{HashMap, HashSet};
use crate::ast::{self as a, Expr, ExprKind as E, StmtKind as S, Type, Visibility};
use crate::diagnostic::{Diagnostic, Span};

#[derive(Clone)]
struct ClassSymbol { qualified: String, visibility: Visibility, span: Span }
#[derive(Clone)]
struct FunctionSymbol { qualified: String, visibility: Visibility, span: Span }
#[derive(Clone)]
struct ImportBinding { qualified: String, span: Span }

struct ImportSubject<'a> {
    module: &'a str,
    name: &'a str,
    kind: &'a str,
    visibility: Visibility,
    declaration: Span,
}
#[derive(Default)]
struct ImportScope {
    types: HashMap<String, ImportBinding>,
    functions: HashMap<String, ImportBinding>,
}

struct Resolver {
    modules: HashSet<String>,
    unit_modules: Vec<String>,
    classes: HashMap<(String, String), ClassSymbol>,
    functions: HashMap<(String, String), FunctionSymbol>,
    imports: Vec<ImportScope>,
}

struct BodyContext {
    unit: usize,
    type_params: Vec<String>,
    owner_methods: HashSet<String>,
    owner_fields: HashSet<String>,
    scopes: Vec<HashSet<String>>,
}

pub fn resolve(program: &mut a::Program) -> Result<(), Diagnostic> {
    if program.units.is_empty() {
        program.units.push(a::SourceUnit { module: String::new(), imports: Vec::new() });
    }
    for class in &mut program.classes {
        let module = &program.units[class.unit].module;
        class.qualified_name = qualify(module, &class.name);
        for method in &mut class.methods {
            method.unit = class.unit;
            method.qualified_name = format!("{}::{}", class.qualified_name, method.name);
        }
        if let Some(constructor) = &mut class.constructor {
            constructor.unit = class.unit;
            constructor.qualified_name = format!("{}::{}", class.qualified_name, constructor.name);
        }
    }

    for tr in &mut program.traits {
        let module = &program.units[tr.unit].module;
        tr.qualified_name = qualify(module, &tr.name);
    }

    let mut trait_map: HashMap<&str, HashMap<&str, (usize, &a::Trait)>> = HashMap::new();
    for (index, tr) in program.traits.iter().enumerate() {
        let module = &program.units[tr.unit].module;
        let traits = trait_map.entry(module.as_str()).or_default();
        if traits.insert(tr.name.as_str(), (index, tr)).is_some() {
            return Err(Diagnostic::new(tr.span, format!("duplicate trait `{}` in module `{}`", tr.name, module)));
        }
    }

    for class in &mut program.classes {
        let unit = &program.units[class.unit];
        for name in class.type_bounds.iter_mut().flatten() {
            let (_, tr) = resolve_trait(&trait_map, unit, name, class.span)?;
            tr.qualified_name.clone_into(name);
        }
    }

    let mut trait_impl_targets = Vec::with_capacity(program.impls.len());

    for imp in &mut program.impls {
        let unit = &program.units[imp.unit];
        let (trait_index, tr) = resolve_trait(&trait_map, unit, &imp.trait_name, imp.span)?;
        let imp_module = &unit.module;
        let qualified_class = qualify(imp_module, &imp.target_class);
        let mut class_candidates = Vec::new();
        for (i, c) in program.classes.iter().enumerate() {
            if c.unit == imp.unit && c.name == imp.target_class {
                class_candidates.push((1, i));
            } else if c.qualified_name == qualified_class {
                class_candidates.push((2, i));
            } else if program.units[c.unit].module.is_empty() && c.name == imp.target_class {
                class_candidates.push((3, i));
            }
        }
        class_candidates.sort_by_key(|(p, _)| *p);
        let target_idx = match class_candidates.as_slice() {
            [] => return Err(Diagnostic::new(imp.span, format!("unknown class `{}` in impl block", imp.target_class))),
            [(p1, _), (p2, _), ..] if p1 == p2 => {
                return Err(Diagnostic::new(imp.span, format!("ambiguous class `{}` in impl block", imp.target_class)));
            }
            [(_, idx), ..] => *idx,
        };
        trait_impl_targets.push((trait_index, target_idx));
        let class = &mut program.classes[target_idx];
        class.qualified_name.clone_into(&mut imp.target_class);
        tr.qualified_name.clone_into(&mut imp.trait_name);
        for req in &tr.methods {
            if !imp.methods.iter().any(|method| method.name == req.name) {
                return Err(Diagnostic::new(imp.span, format!(
                    "missing method `{}` required by trait `{}`", req.name, tr.name
                )));
            }
        }

        for mut method in imp.methods.clone() {
            if class.methods.iter().any(|m| m.name == method.name) {
                return Err(Diagnostic::new(method.span, format!(
                    "duplicate method `{}` on class `{}`", method.name, class.name
                )));
            }
            // Implementation methods use imports from the source unit declaring the impl.
            method.unit = imp.unit;
            method.qualified_name = format!("{}::{}", class.qualified_name, method.name);
            class.methods.push(method);
        }
    }

    drop(trait_map);
    let resolver = Resolver::new(program)?;
    for function in &mut program.functions {
        function.qualified_name = qualify(&program.units[function.unit].module, &function.name);
    }
    for class in &mut program.classes { resolver.class(class)?; }
    for function in &mut program.functions { resolver.function(function, None, &[])?; }
    check_trait_implementations(program, &resolver, &trait_impl_targets)?;
    Ok(())
}

fn resolve_trait<'a>(
    trait_map: &HashMap<&str, HashMap<&str, (usize, &'a a::Trait)>>,
    unit: &a::SourceUnit,
    name: &str,
    span: Span,
) -> Result<(usize, &'a a::Trait), Diagnostic> {
    let local = trait_map.get(unit.module.as_str()).and_then(|traits| traits.get(name)).copied();
    if let Some(found) = local {
        return Ok(found);
    }
    let root = trait_map.get("").and_then(|traits| traits.get(name)).copied();
    if let Some((index, tr)) = root {
        if tr.visibility == Visibility::Private && !unit.module.is_empty() {
            return Err(private_trait_error(tr, span));
        }
        return Ok((index, tr));
    }

    let mut matched: Option<(usize, &'a a::Trait)> = None;
    let mut private_match: Option<(usize, &'a a::Trait)> = None;
    for import in &unit.imports {
        let module = if import.wildcard {
            Some(import.path.as_str())
        } else if let Some((module, imported_name)) = import.path.rsplit_once('.')
            && imported_name == name {
                Some(module)
            } else {
                None
            };
        let Some(module) = module else { continue; };
        let Some((index, tr)) = trait_map.get(module).and_then(|traits| traits.get(name)).copied() else { continue; };
        if tr.visibility == Visibility::Private {
            private_match.get_or_insert((index, tr));
            continue;
        }
        if let Some((_, previous)) = matched
            && previous.qualified_name != tr.qualified_name {
                return Err(Diagnostic::new(
                    span,
                    format!("ambiguous trait `{name}`: found multiple matching traits in imported modules"),
                ));
            }
        matched = Some((index, tr));
    }

    if let Some(found) = matched {
        Ok(found)
    } else if let Some((_, tr)) = private_match {
        Err(private_trait_error(tr, span))
    } else {
        Err(Diagnostic::new(span, format!("unknown trait `{name}`")))
    }
}

fn check_trait_implementations(
    program: &mut a::Program,
    resolver: &Resolver,
    trait_impl_targets: &[(usize, usize)],
) -> Result<(), Diagnostic> {
    for tr in &mut program.traits {
        let unit = tr.unit;
        for method in &mut tr.methods {
            method.unit = unit;
            resolver.function(method, None, &[])?;
        }
    }

    for (imp, &(trait_index, class_index)) in program.impls.iter().zip(trait_impl_targets) {
        let tr = &program.traits[trait_index];
        let class = &program.classes[class_index];
        for req in &tr.methods {
            let implementation = class
                .methods
                .iter()
                .find(|method| method.name == req.name)
                .ok_or_else(|| {
                    Diagnostic::new(
                        imp.span,
                        format!(
                            "missing method `{}` required by trait `{}`",
                            req.name, tr.name
                        ),
                    )
                })?;
            if implementation.params.len() != req.params.len() {
                return Err(Diagnostic::new(
                    implementation.span,
                    format!(
                        "method `{}` has {} parameters but trait `{}` expects {}",
                        req.name,
                        implementation.params.len(),
                        tr.name,
                        req.params.len()
                    ),
                ));
            }
            for (index, (expected, actual)) in
                req.params.iter().zip(&implementation.params).enumerate()
            {
                if expected.ty != actual.ty {
                    return Err(Diagnostic::new(
                        actual.span,
                        format!(
                            "parameter {} of method `{}` has type {:?}, but trait `{}` expects {:?}",
                            index + 1,
                            req.name,
                            actual.ty,
                            tr.name,
                            expected.ty
                        ),
                    ));
                }
            }
            if implementation.return_type != req.return_type {
                return Err(Diagnostic::new(
                    implementation.span,
                    format!(
                        "method `{}` has return type {:?} but trait `{}` expects {:?}",
                        req.name, implementation.return_type, tr.name, req.return_type
                    ),
                ));
            }
            if implementation.readonly != req.readonly {
                let actual = if implementation.readonly {
                    "readonly"
                } else {
                    "mutable"
                };
                let expected = if req.readonly { "readonly" } else { "mutable" };
                return Err(Diagnostic::new(
                    implementation.span,
                    format!(
                        "method `{}` has a {actual} receiver but trait `{}` requires a {expected} receiver",
                        req.name, tr.name
                    ),
                ));
            }
            if implementation.visibility != req.visibility {
                let actual = if implementation.visibility == Visibility::Public {
                    "public"
                } else {
                    "private"
                };
                let expected = if req.visibility == Visibility::Public {
                    "public"
                } else {
                    "private"
                };
                return Err(Diagnostic::new(
                    implementation.span,
                    format!(
                        "method `{}` is {actual} but trait `{}` requires {expected} visibility",
                        req.name, tr.name
                    ),
                ));
            }
            if implementation.is_static != req.is_static {
                let expected = if req.is_static { "static" } else { "instance" };
                return Err(Diagnostic::new(
                    implementation.span,
                    format!(
                        "method `{}` must be {expected} to implement trait `{}`",
                        req.name, tr.name
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn private_trait_error(tr: &a::Trait, span: Span) -> Diagnostic {
    let module = tr.qualified_name.rsplit_once("::").map_or("", |(module, _)| module);
    let message = if module.is_empty() {
        format!("trait `{}` is private to the unnamed module", tr.name)
    } else {
        format!("trait `{}` is private to module `{module}`", tr.name)
    };
    Diagnostic::new(span, message).with_note(tr.span, "private declaration is here")
}

fn qualify(module: &str, name: &str) -> String {
    if module.is_empty() { name.to_owned() } else { format!("{module}::{name}") }
}

fn split_qualified(path: &str) -> (&str, &str) {
    path.rsplit_once('.').unwrap_or(("", path))
}

impl Resolver {
    fn new(program: &a::Program) -> Result<Self, Diagnostic> {
        let mut modules = HashSet::from([String::new()]);
        let unit_modules: Vec<String> = program.units.iter().map(|unit| unit.module.clone()).collect();
        modules.extend(unit_modules.iter().cloned());
        let mut classes = HashMap::new();
        for class in &program.classes {
            let Some(module) = unit_modules.get(class.unit) else {
                return Err(Diagnostic::new(class.span, "class belongs to an unknown source unit"));
            };
            if crate::stdlib::is_builtin_type_name(&class.name) {
                return Err(Diagnostic::new(class.span, format!("[E5001] `{}` is a reserved built-in type", class.name)));
            }
            if matches!(class.name.as_str(), "System" | "println") {
                return Err(Diagnostic::new(class.span, format!("`{}` is reserved by the runtime", class.name)));
            }
            let key = (module.clone(), class.name.clone());
            let symbol = ClassSymbol { qualified: qualify(module, &class.name), visibility: class.visibility, span: class.span };
            if let Some(previous) = classes.insert(key, symbol) {
                return Err(Diagnostic::new(class.span, format!("type `{}` is declared more than once in module `{module}`", class.name))
                    .with_note(previous.span, "previous declaration is here"));
            }
        }
        let mut functions = HashMap::new();
        for function in &program.functions {
            let Some(module) = unit_modules.get(function.unit) else {
                return Err(Diagnostic::new(function.span, "function belongs to an unknown source unit"));
            };
            if matches!(function.name.as_str(), "println" | "move" | "drop" | "assert") {
                return Err(Diagnostic::new(function.span, format!("`{}` is a reserved built-in function", function.name)));
            }
            let key = (module.clone(), function.name.clone());
            let symbol = FunctionSymbol { qualified: qualify(module, &function.name), visibility: function.visibility, span: function.span };
            if let Some(previous) = functions.insert(key, symbol) {
                return Err(Diagnostic::new(function.span, format!("function `{}` is declared more than once in module `{module}`; overloading is not supported", function.name))
                    .with_note(previous.span, "previous declaration is here"));
            }
        }

        let mut resolver = Self {
            modules, unit_modules, classes, functions,
            imports: (0..program.units.len()).map(|_| ImportScope::default()).collect(),
        };
        resolver.build_imports(program)?;
        Ok(resolver)
    }

    fn build_imports(&mut self, program: &a::Program) -> Result<(), Diagnostic> {
        for (unit_id, unit) in program.units.iter().enumerate() {
            let mut seen = HashSet::new();
            for import in &unit.imports {
                if !seen.insert((import.path.clone(), import.wildcard)) {
                    return Err(Diagnostic::new(import.span, format!("duplicate import `{}`", import.path)));
                }
                if import.wildcard {
                    if !self.modules.contains(&import.path) {
                        return Err(Diagnostic::new(import.span, format!("module `{}` does not exist", import.path)));
                    }
                    for class in &program.classes {
                        let module = &self.unit_modules[class.unit];
                        if module == &import.path && class.visibility == Visibility::Public {
                            let binding = ImportBinding { qualified: qualify(module, &class.name), span: import.span };
                            Self::add_import(&mut self.imports[unit_id].types, &class.name, binding, import.span, "type")?;
                        }
                    }
                    for function in &program.functions {
                        let module = &self.unit_modules[function.unit];
                        if module == &import.path && function.visibility == Visibility::Public {
                            let binding = ImportBinding { qualified: qualify(module, &function.name), span: import.span };
                            Self::add_import(&mut self.imports[unit_id].functions, &function.name, binding, import.span, "function")?;
                        }
                    }
                    continue;
                }
                let (module, name) = split_qualified(&import.path);
                if !self.modules.contains(module) {
                    return Err(Diagnostic::new(import.span, format!("import `{}` names a missing module", import.path)));
                }
                let class = self.classes.get(&(module.to_owned(), name.to_owned()));
                let function = self.functions.get(&(module.to_owned(), name.to_owned()));
                if class.is_some() && function.is_some() {
                    return Err(Diagnostic::new(import.span, format!("import `{}` is ambiguous between a type and a function", import.path)));
                }
                if let Some(class) = class {
                    self.check_import_visibility(&unit.module, import.span, ImportSubject {
                        module, name, kind: "type", visibility: class.visibility, declaration: class.span,
                    })?;
                    let binding = ImportBinding { qualified: class.qualified.clone(), span: import.span };
                    Self::add_import(&mut self.imports[unit_id].types, name, binding, import.span, "type")?;
                } else if let Some(function) = function {
                    self.check_import_visibility(&unit.module, import.span, ImportSubject {
                        module, name, kind: "function", visibility: function.visibility, declaration: function.span,
                    })?;
                    let binding = ImportBinding { qualified: function.qualified.clone(), span: import.span };
                    Self::add_import(&mut self.imports[unit_id].functions, name, binding, import.span, "function")?;
                } else if module.is_empty() && crate::stdlib::is_builtin_type_name(name) {
                    continue;
                } else {
                    return Err(Diagnostic::new(import.span, format!("`{}` is not declared in module `{module}`", name)));
                }
            }
            for (name, binding) in &self.imports[unit_id].types {
                if crate::stdlib::is_builtin_type_name(name) && binding.qualified != *name {
                    return Err(Diagnostic::new(binding.span, format!("an import cannot shadow built-in type `{name}`")));
                }
                if let Some(local) = self.classes.get(&(unit.module.clone(), name.clone()))
                    && local.qualified != binding.qualified {
                        return Err(Diagnostic::new(binding.span, format!("imported type `{name}` conflicts with a declaration in this module"))
                            .with_note(local.span, "local declaration is here"));
                    }
            }
            for (name, binding) in &self.imports[unit_id].functions {
                if let Some(local) = self.functions.get(&(unit.module.clone(), name.clone()))
                    && local.qualified != binding.qualified {
                        return Err(Diagnostic::new(binding.span, format!("imported function `{name}` conflicts with a declaration in this module"))
                            .with_note(local.span, "local declaration is here"));
                    }
            }
        }
        Ok(())
    }

    fn add_import(
        target: &mut HashMap<String, ImportBinding>, name: &str, binding: ImportBinding,
        span: Span, kind: &str,
    ) -> Result<(), Diagnostic> {
        if let Some(previous) = target.get(name) {
            if previous.qualified != binding.qualified {
                return Err(Diagnostic::new(span, format!("ambiguous imported {kind} `{name}`"))
                    .with_note(previous.span, "conflicting import is here"));
            }
        } else {
            target.insert(name.to_owned(), binding);
        }
        Ok(())
    }

    fn check_import_visibility(&self, current_module: &str, import: Span, subject: ImportSubject<'_>) -> Result<(), Diagnostic> {
        if subject.visibility == Visibility::Private && subject.module != current_module {
            Err(Diagnostic::new(import, format!("cannot import private {} `{}` from module `{}`", subject.kind, subject.name, subject.module))
                .with_note(subject.declaration, "private declaration is here"))
        } else { Ok(()) }
    }

    fn class(&self, class: &mut a::Class) -> Result<(), Diagnostic> {
        self.check_type_parameters(class.unit, &class.type_params, class.span)?;
        let owner_methods: HashSet<String> = class.methods.iter().map(|method| method.name.clone()).collect();
        let owner_fields: HashSet<String> = class.fields.iter().map(|field| field.name.clone()).collect();
        let params = class.type_params.clone();
        for interface in &mut class.implements { self.resolve_type(class.unit, &params, interface, class.span)?; }
        if let Some(variants) = &mut class.variants {
            for variant in variants {
                for field in &mut variant.fields { self.resolve_type(class.unit, &params, field, variant.span)?; }
            }
        }
        for field in &mut class.fields {
            self.resolve_type(class.unit, &params, &mut field.ty, field.span)?;
            if let Some(value) = &mut field.initializer {
                let mut context = BodyContext { unit: class.unit, type_params: params.clone(), owner_methods: owner_methods.clone(),
                    owner_fields: owner_fields.clone(), scopes: Vec::new() };
                self.expression(value, &mut context)?;
            }
        }
        for method in &mut class.methods { self.function(method, Some((&owner_methods, &owner_fields)), &params)?; }
        if let Some(constructor) = &mut class.constructor { self.function(constructor, Some((&owner_methods, &owner_fields)), &params)?; }
        Ok(())
    }

    fn check_type_parameters(&self, unit: usize, parameters: &[String], span: Span) -> Result<(), Diagnostic> {
        let module = &self.unit_modules[unit];
        for parameter in parameters {
            if crate::stdlib::is_builtin_type_name(parameter)
                || self.classes.contains_key(&(module.clone(), parameter.clone()))
                || self.imports[unit].types.contains_key(parameter)
                || matches!(parameter.as_str(), "System" | "println" | "move" | "drop" | "assert") {
                return Err(Diagnostic::new(span, format!("[E4101] type parameter `{parameter}` shadows a type or reserved name")));
            }
        }
        Ok(())
    }

    fn function(
        &self, function: &mut a::Function,
        owner: Option<(&HashSet<String>, &HashSet<String>)>,
        type_params: &[String],
    ) -> Result<(), Diagnostic> {
        for parameter in &mut function.params {
            self.resolve_type(function.unit, type_params, &mut parameter.ty, parameter.span)?;
        }
        self.resolve_type(function.unit, type_params, &mut function.return_type, function.span)?;
        let mut context = BodyContext {
            unit: function.unit,
            type_params: type_params.to_vec(),
            owner_methods: owner.map_or_else(HashSet::new, |(methods, _)| methods.clone()),
            owner_fields: owner.map_or_else(HashSet::new, |(_, fields)| fields.clone()),
            scopes: vec![function.params.iter().map(|parameter| parameter.name.clone()).collect()],
        };
        self.block(&mut function.body, &mut context)
    }

    fn resolve_type(&self, unit: usize, parameters: &[String], ty: &mut Type, span: Span) -> Result<(), Diagnostic> {
        match ty {
            Type::Named(name) => {
                if let Some(qualified) = self.try_class_name(unit, name, parameters, span)? {
                    *name = qualified;
                } else if !self.unit_modules[unit].is_empty() || name.contains('.') {
                    return Err(Diagnostic::new(span, format!("type `{name}` is not declared or imported")));
                }
            }
            Type::Applied { name, args } => {
                if let Some(qualified) = self.try_class_name(unit, name, parameters, span)? {
                    *name = qualified;
                } else if !self.unit_modules[unit].is_empty() || name.contains('.') {
                    return Err(Diagnostic::new(span, format!("type `{name}` is not declared or imported")));
                }
                for argument in args { self.resolve_type(unit, parameters, argument, span)?; }
            }
            Type::Borrow { name, args, .. } => {
                if let Some(qualified) = self.try_class_name(unit, name, parameters, span)? {
                    *name = qualified;
                } else if !self.unit_modules[unit].is_empty() || name.contains('.') {
                    return Err(Diagnostic::new(span, format!("type `{name}` is not declared or imported")));
                }
                for argument in args { self.resolve_type(unit, parameters, argument, span)?; }
            }
            Type::Byte | Type::Char | Type::Int | Type::Long | Type::Double | Type::Unit |
            Type::Boolean | Type::String | Type::Void => {}
        }
        Ok(())
    }

    fn try_class_name(
        &self, unit: usize, name: &str, parameters: &[String], span: Span,
    ) -> Result<Option<String>, Diagnostic> {
        if parameters.iter().any(|parameter| parameter == name) { return Ok(Some(name.to_owned())); }
        if crate::stdlib::is_builtin_type_name(name) { return Ok(Some(name.to_owned())); }
        if let Some((module, local)) = name.rsplit_once('.') {
            if let Some(symbol) = self.classes.get(&(module.to_owned(), local.to_owned())) {
                self.check_class_access(symbol, module, unit, span, name)?;
                return Ok(Some(symbol.qualified.clone()));
            }
            return Ok(None);
        }
        let module = &self.unit_modules[unit];
        if let Some(symbol) = self.classes.get(&(module.clone(), name.to_owned())) {
            return Ok(Some(symbol.qualified.clone()));
        }
        if let Some(binding) = self.imports[unit].types.get(name) { return Ok(Some(binding.qualified.clone())); }
        Ok(None)
    }

    fn check_class_access(&self, symbol: &ClassSymbol, module: &str, unit: usize, span: Span, name: &str) -> Result<(), Diagnostic> {
        if symbol.visibility == Visibility::Private && self.unit_modules[unit] != module {
            Err(Diagnostic::new(span, format!("type `{name}` is private to module `{module}`"))
                .with_note(symbol.span, "private declaration is here"))
        } else { Ok(()) }
    }

    fn check_function_access(&self, symbol: &FunctionSymbol, module: &str, unit: usize, span: Span, name: &str) -> Result<(), Diagnostic> {
        if symbol.visibility == Visibility::Private && self.unit_modules[unit] != module {
            Err(Diagnostic::new(span, format!("function `{name}` is private to module `{module}`"))
                .with_note(symbol.span, "private declaration is here"))
        } else { Ok(()) }
    }

    fn lookup_function(&self, unit: usize, name: &str, _span: Span) -> Result<Option<String>, Diagnostic> {
        let module = &self.unit_modules[unit];
        if let Some(symbol) = self.functions.get(&(module.clone(), name.to_owned())) {
            return Ok(Some(symbol.qualified.clone()));
        }
        if let Some(binding) = self.imports[unit].functions.get(name) { return Ok(Some(binding.qualified.clone())); }
        Ok(None)
    }

    fn qualified_function(&self, unit: usize, path: &str, span: Span) -> Result<Option<String>, Diagnostic> {
        let (module, name) = split_qualified(path);
        if module.is_empty() || !self.modules.contains(module) { return Ok(None); }
        if let Some(symbol) = self.functions.get(&(module.to_owned(), name.to_owned())) {
            self.check_function_access(symbol, module, unit, span, path)?;
            return Ok(Some(symbol.qualified.clone()));
        }
        Err(Diagnostic::new(span, format!("module `{module}` has no function `{name}`")))
    }

    fn resolve_pattern(&self, unit: usize, type_params: &[String], pattern: &mut a::Pattern) -> Result<(), Diagnostic> {
        match &mut pattern.kind {
            a::PatternKind::Name(name) | a::PatternKind::Variant { name, .. } => {
                if let Some((class, variant)) = name.rsplit_once('.') {
                    if let Some(qualified) = self.try_class_name(unit, class, type_params, pattern.span)? {
                        *name = format!("{qualified}.{variant}");
                    } else if !self.unit_modules[unit].is_empty() || class.contains('.') {
                        return Err(Diagnostic::new(pattern.span, format!("enum `{class}` is not declared or imported")));
                    }
                }
                if let a::PatternKind::Variant { fields, .. } = &mut pattern.kind {
                    for field in fields { self.resolve_pattern(unit, type_params, field)?; }
                }
            }
            a::PatternKind::Record { class, fields } => {
                *class = self.try_class_name(unit, class, type_params, pattern.span)?
                    .ok_or_else(|| Diagnostic::new(pattern.span, format!("type `{class}` is not declared or imported")))?;
                for (_, field) in fields { self.resolve_pattern(unit, type_params, field)?; }
            }
            a::PatternKind::Tuple(first, second) => {
                self.resolve_pattern(unit, type_params, first)?;
                self.resolve_pattern(unit, type_params, second)?;
            }
        }
        Ok(())
    }
    fn block(&self, block: &mut a::Block, context: &mut BodyContext) -> Result<(), Diagnostic> {
        context.scopes.push(HashSet::new());
        for statement in block {
            self.statement(statement, context)?;
        }
        context.scopes.pop();
        Ok(())
    }

    fn statement(&self, statement: &mut a::Stmt, context: &mut BodyContext) -> Result<(), Diagnostic> {
        match &mut statement.kind {
            S::Let { ty, name, value } => {
                self.resolve_type(context.unit, &context.type_params, ty, statement.span)?;
                if let Some(value) = value { self.expression(value, context)?; }
                context.scopes.last_mut().expect("function block scope").insert(name.clone());
            }
            S::Assign { target, value } => { self.expression(target, context)?; self.expression(value, context)?; }
            S::Expr(expr) => self.expression(expr, context)?,
            S::If { condition, then_block, else_block } => {
                self.expression(condition, context)?;
                self.block(then_block, context)?;
                if let Some(block) = else_block { self.block(block, context)?; }
            }
            S::While { condition, body } => { self.expression(condition, context)?; self.block(body, context)?; }
            S::ForEach { ty, name, key, collection, body, .. } => {
                self.resolve_type(context.unit, &context.type_params, ty, statement.span)?;
                if let Some((key_type, _)) = key { self.resolve_type(context.unit, &context.type_params, key_type, statement.span)?; }
                self.expression(collection, context)?;
                context.scopes.push(HashSet::from([name.clone()]));
                if let Some((_, key_name)) = key { context.scopes.last_mut().unwrap().insert(key_name.clone()); }
                self.block(body, context)?;
                context.scopes.pop();
            }
            S::Match { value, arms } => {
                self.expression(value, context)?;
                for arm in arms {
                    self.resolve_pattern(context.unit, &context.type_params, &mut arm.pattern)?;
                    let mut bindings = HashSet::new();
                    collect_pattern_bindings(&arm.pattern, &mut bindings);
                    context.scopes.push(bindings);
                    if let Some(guard) = &mut arm.guard { self.expression(guard, context)?; }
                    self.block(&mut arm.body, context)?;
                    context.scopes.pop();
                }
            }
            S::Return(Some(expr)) => self.expression(expr, context)?,
            S::Block(block) => self.block(block, context)?,
            S::Return(None) | S::Break | S::Continue => {}
        }
        Ok(())
    }

    fn expression(&self, expression: &mut Expr, context: &mut BodyContext) -> Result<(), Diagnostic> {
        if let E::MethodCall { receiver, name, args } = &mut expression.kind {
            let system_print = name == "println" && matches!(&receiver.kind,
                E::Field { base, name } if name == "out" && matches!(&base.kind, E::Var(system) if system == "System"));
            if !system_print
                && let Some(path) = expression_path(receiver) {
                    let root = path.split('.').next().unwrap_or(&path);
                    let shadowed = context.scopes.iter().rev().any(|scope| scope.contains(root))
                        || context.owner_fields.contains(root);
                    if !shadowed {
                        let full_path = format!("{path}.{name}");
                        if let Some(target) = self.qualified_function(context.unit, &full_path, expression.span)? {
                            for argument in args.iter_mut() { self.expression(argument, context)?; }
                            let moved_args = std::mem::take(args);
                            expression.kind = E::Call { name: target, args: moved_args };
                            return Ok(());
                        }
                        if !context.type_params.iter().any(|parameter| parameter == &path) {
                            if let Some(class) = self.try_class_name(context.unit, &path, &context.type_params, receiver.span)? {
                                **receiver = Expr { kind: E::Var(class), span: receiver.span, depth: 1 };
                            } else if path.contains('.') {
                                return Err(Diagnostic::new(expression.span, format!("qualified member `{full_path}` is not declared or imported")));
                            }
                        }
                    }
                }
        }
        match &mut expression.kind {
            E::StaticCall { class, type_args, args, .. } => {
                *class = self.try_class_name(context.unit, class, &context.type_params, expression.span)?
                    .ok_or_else(|| Diagnostic::new(expression.span, format!("type `{class}` is not declared or imported")))?;
                for argument in type_args { self.resolve_type(context.unit, &context.type_params, argument, expression.span)?; }
                for argument in args { self.expression(argument, context)?; }
            }
            E::Call { name, args } => {
                if !matches!(name.as_str(), "println" | "move" | "drop" | "assert")
                    && !context.owner_methods.contains(name.as_str()) {
                    match self.lookup_function(context.unit, name, expression.span)? {
                        Some(target) => *name = target,
                        None if self.unit_modules[context.unit].is_empty() => {}
                        None => return Err(Diagnostic::new(expression.span, format!("function `{name}` is not declared or imported"))),
                    }
                }
                for argument in args { self.expression(argument, context)?; }
            }
            E::New { class, type_args, args } => {
                for argument in type_args.iter_mut() { self.resolve_type(context.unit, &context.type_params, argument, expression.span)?; }
                if let Some(qualified) = self.try_class_name(context.unit, class, &context.type_params, expression.span)? {
                    *class = qualified;
                } else if let Some((enum_name, variant)) = class.rsplit_once('.') {
                    if let Some(qualified) = self.try_class_name(context.unit, enum_name, &context.type_params, expression.span)? {
                        let class_name = qualified;
                        let variant_name = variant.to_owned();
                        let moved_args = std::mem::take(args);
                        expression.kind = E::Variant { class: class_name, type_args: std::mem::take(type_args), variant: variant_name, args: moved_args };
                        if let E::Variant { args, .. } = &mut expression.kind {
                            for argument in args { self.expression(argument, context)?; }
                        }
                        return Ok(());
                    } else if !self.unit_modules[context.unit].is_empty() || class.contains('.') {
                        return Err(Diagnostic::new(expression.span, format!("type or enum `{class}` is not declared or imported")));
                    }
                } else if !self.unit_modules[context.unit].is_empty() || class.contains('.') {
                    return Err(Diagnostic::new(expression.span, format!("type `{class}` is not declared or imported")));
                }
                for argument in args { self.expression(argument, context)?; }
            }
            E::Variant { class, type_args, args, .. } => {
                if let Some(qualified) = self.try_class_name(context.unit, class, &context.type_params, expression.span)? {
                    *class = qualified;
                } else if !self.unit_modules[context.unit].is_empty() || class.contains('.') {
                    return Err(Diagnostic::new(expression.span, format!("enum `{class}` is not declared or imported")));
                }
                for argument in type_args { self.resolve_type(context.unit, &context.type_params, argument, expression.span)?; }
                for argument in args { self.expression(argument, context)?; }
            }
            E::MethodCall { receiver, args, .. } => {
                self.expression(receiver, context)?;
                for argument in args { self.expression(argument, context)?; }
            }
            E::Field { base, .. } => self.expression(base, context)?,
            E::Unary { expr, .. } | E::Borrow { expr, .. } | E::Try(expr) | E::Await(expr) => self.expression(expr, context)?,
            E::Cast { ty, expr } => {
                self.resolve_type(context.unit, &context.type_params, ty, expression.span)?;
                self.expression(expr, context)?;
            }
            E::Tuple(first, second) => { self.expression(first, context)?; self.expression(second, context)?; }
            E::Binary { left, right, .. } => { self.expression(left, context)?; self.expression(right, context)?; }
            E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit |
            E::Boolean(_) | E::String(_) | E::Var(_) | E::This => {}
        }
        Ok(())
    }
}

fn expression_path(expression: &Expr) -> Option<String> {
    match &expression.kind {
        E::Var(name) => Some(name.clone()),
        E::Field { base, name } => Some(format!("{}.{}", expression_path(base)?, name)),
        _ => None,
    }
}
fn collect_pattern_bindings(pattern: &a::Pattern, output: &mut HashSet<String>) {
    match &pattern.kind {
        a::PatternKind::Name(name) if name != "_" => { output.insert(name.clone()); }
        a::PatternKind::Variant { fields, .. } => {
            for field in fields { collect_pattern_bindings(field, output); }
        }
        a::PatternKind::Record { fields, .. } => {
            for (_, field) in fields { collect_pattern_bindings(field, output); }
        }
        a::PatternKind::Tuple(first, second) => {
            collect_pattern_bindings(first, output);
            collect_pattern_bindings(second, output);
        }
        a::PatternKind::Name(_) => {}
    }
}
