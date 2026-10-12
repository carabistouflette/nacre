//! Lexical borrow checking over the typed, structured IR.
//!
//! Bindings of borrowed types are immutable and cannot escape. A local loan
//! lasts until the end of its block, and an argument loan until its call ends.
//! Owners and their owned fields are conservatively one alias domain. Reborrows
//! carry a parent capability: children may use it, but conflicting sibling or
//! parent accesses are rejected. Ownership/initialization across loops remains
//! the separate CFG analysis in ownership.rs.
//!
//! Views (classes with shared borrowed fields) are non-movable, non-exportable
//! local objects. Their constructor records which shared arguments are retained.
//! This deliberately restricted rule makes their lifetimes lexical and inferred.
//! Borrowed returns are tied to the receiver; there are no lifetime parameters or NLL.
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::diagnostic::{line_column, Diagnostic, Span};
use crate::ir::{self as t, CallTarget, Place, PlaceKind, Type};

const MAX_LOANS: usize = 8192;
const MAX_WORK: usize = 4_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Root {
    Local(usize),
    This,
    // Abstract referents in a method of a view. Call sites enforce the retained
    // shared loans, so these cannot alias an incoming mutable/owning parameter.
    ThisField(usize),
}
#[derive(Clone, Copy, Debug)]
struct Origin { root: Root, via: Option<usize> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Access { Read, Write, Destroy }
#[derive(Debug)]
struct Loan {
    root: Root, parent: Option<usize>, mutable: bool,
    span: Span, label: String, active: bool,
}
#[derive(Clone, Copy)]
struct Guard { root: Root, span: Span, projected: bool }

struct Analyzer<'a> {
    program: &'a t::Program,
    function: &'a t::Function,
    source: Option<&'a str>,
    loans: Vec<Loan>,
    bindings: HashMap<usize, Origin>,
    views: HashMap<usize, Vec<Option<Origin>>>,
    guards: Vec<Guard>,
    trace: String,
    work: usize,
}

impl<'a> Analyzer<'a> {
    fn new(program: &'a t::Program, function: &'a t::Function, source: Option<&'a str>) -> Self {
        let bindings = function.params.iter().filter_map(|(id, ty)| {
            if ty.is_borrow() { Some((*id, Origin { root: Root::Local(*id), via: None })) }
            else { None }
        }).collect();
        Self { program, function, source, loans: Vec::new(), bindings,
            views: HashMap::new(), guards: Vec::new(), trace: String::new(), work: 0 }
    }

    fn label(&self, root: Root) -> String {
        match root {
            Root::Local(id) => self.function.locals[id].name.clone(),
            Root::This => "this".to_owned(),
            Root::ThisField(field) => format!("this.borrowed_field#{field}"),
        }
    }
    fn log(&mut self, span: Span, text: String) {
        if let Some(source) = self.source {
            let (line, column) = line_column(source, span.start);
            writeln!(self.trace, "  @{line}:{column} {text}").unwrap();
        }
    }
    fn charge(&mut self, amount: usize, span: Span) -> Result<(), Diagnostic> {
        self.work = self.work.saturating_add(amount);
        if self.work > MAX_WORK {
            Err(Diagnostic::new(span, "[E3099] fonction trop complexe pour le borrow checker; separer les blocs ou les fonctions"))
        } else { Ok(()) }
    }
    fn origin(&self, place: &Place) -> Origin {
        match &place.kind {
            PlaceKind::Var(id) => self.bindings.get(id).copied()
                .unwrap_or(Origin { root: Root::Local(*id), via: None }),
            PlaceKind::This => Origin { root: Root::This, via: None },
            PlaceKind::Field { base, field } if place.ty.is_borrow() => {
                // A borrowed field can only occur immediately inside a view:
                // view-containing fields and borrows of views are type errors.
                match &base.kind {
                    PlaceKind::Var(id) => self.views.get(id)
                        .and_then(|fields| fields.get(*field)).copied().flatten()
                        .expect("typed, initialized local view field"),
                    PlaceKind::This => Origin { root: Root::ThisField(*field), via: None },
                    _ => unreachable!("nested views are rejected by the type checker"),
                }
            }
            PlaceKind::Field { base, .. } => self.origin(base),
        }
    }
    /// Reading a pointer stored in a field also reads its container. Merely
    /// reserving a named receiver does not yet grant exclusive object access.
    fn projections(&mut self, place: &Place, span: Span) -> Result<(), Diagnostic> {
        if let PlaceKind::Field { base, .. } = &place.kind {
            self.projections(base, span)?;
            self.access(self.origin(base), Access::Read, span)?;
        }
        Ok(())
    }
    fn access(&mut self, origin: Origin, mode: Access, span: Span) -> Result<(), Diagnostic> {
        self.charge(self.loans.len() + self.guards.len() + 1, span)?;
        let mut ancestors = HashSet::new();
        let mut cursor = origin.via;
        while let Some(id) = cursor {
            ancestors.insert(id);
            cursor = self.loans[id].parent;
            self.charge(1, span)?;
        }
        for (id, loan) in self.loans.iter().enumerate() {
            if !loan.active || loan.root != origin.root || ancestors.contains(&id) { continue; }
            if mode == Access::Read && !loan.mutable { continue; }
            let code = if mode == Access::Destroy { "E3002" } else { "E3001" };
            let action = match mode {
                Access::Read => "lecture", Access::Write => "acces exclusif",
                Access::Destroy => "deplacement, destruction ou remplacement",
            };
            return Err(Diagnostic::new(span, format!(
                "[{code}] {action} de `{}` interdit pendant un emprunt {} actif", self.label(origin.root),
                if loan.mutable { "exclusif" } else { "partage" }))
                .with_note(loan.span, format!("emprunt #{} `{}` commence ici", id, loan.label))
                .with_help("placer l'emprunt dans un bloc plus court; il reste actif jusqu'a la fin de son bloc (ou de son appel pour un argument)"));
        }
        for guard in &self.guards {
            if guard.root != origin.root { continue; }
            if mode == Access::Destroy {
                return Err(Diagnostic::new(span, format!(
                    "[E3002] deplacement ou destruction de `{}` pendant l'evaluation d'un receveur", self.label(origin.root)))
                    .with_note(guard.span, "le receveur doit rester vivant jusqu'a la fin de l'appel ou de l'affectation"));
            }
            if mode == Access::Write && guard.projected {
                return Err(Diagnostic::new(span, format!(
                    "[E3001] exclusive access to `{}` could invalidate a stabilized field receiver", self.label(origin.root)))
                    .with_note(guard.span, "evaluate the modifying expression before selecting the field receiver"));
            }
        }
        self.log(span, format!("{mode:?} {} via {:?}", self.label(origin.root), origin.via));
        Ok(())
    }
    fn borrow(&mut self, place: &Place, mutable: bool, span: Span, label: String) -> Result<Origin, Diagnostic> {
        self.projections(place, span)?;
        self.loan(self.origin(place), mutable, span, label)
    }
    fn loan(&mut self, origin: Origin, mutable: bool, span: Span, label: String) -> Result<Origin, Diagnostic> {
        self.access(origin, if mutable { Access::Write } else { Access::Read }, span)?;
        if self.loans.len() >= MAX_LOANS {
            return Err(Diagnostic::new(span, "[E3099] limite de 8192 emprunts par fonction"));
        }
        let id = self.loans.len();
        self.log(span, format!("loan #{id} {} <- {} parent={:?} holder={label}",
            if mutable { "exclusive" } else { "shared" }, self.label(origin.root), origin.via));
        self.loans.push(Loan { root: origin.root, parent: origin.via, mutable, span, label, active: true });
        Ok(Origin { root: origin.root, via: Some(id) })
    }
    fn end_since(&mut self, first: usize, keep: &HashSet<usize>, reason: &str) {
        for id in (first..self.loans.len()).rev() {
            if self.loans[id].active && !keep.contains(&id) {
                let span = self.loans[id].span;
                self.loans[id].active = false;
                self.log(span, format!("end loan #{id} ({reason})"));
            }
        }
    }
    fn is_view(&self, ty: Type) -> bool {
        ty.object_class().is_some_and(|id| self.program.classes[id].is_view())
    }
    fn run(&mut self) -> Result<(), Diagnostic> {
        let function = self.function;
        self.block(&function.body)?;
        // Borrowed parameters never deallocate their referents.
        for (id, ty) in &function.params {
            if ty.is_object() {
                self.access(Origin { root: Root::Local(*id), via: None }, Access::Destroy,
                    function.locals[*id].span)?;
            }
        }
        Ok(())
    }
    fn block(&mut self, block: &t::Block) -> Result<(), Diagnostic> {
        let mark = self.loans.len();
        let mut declared = Vec::new();
        for stmt in block {
            self.statement(stmt)?;
            if let t::Stmt::Let { id, ty, .. } = stmt { declared.push((*id, *ty)); }
        }
        // References cannot escape this scope; release their static loans
        // before validating reverse-declaration-order owner cleanup.
        self.end_since(mark, &HashSet::new(), "scope exit");
        for (id, ty) in declared.into_iter().rev() {
            if ty.is_object() {
                self.access(Origin { root: Root::Local(id), via: None }, Access::Destroy,
                    self.function.locals[id].span)?;
            }
            self.bindings.remove(&id);
            self.views.remove(&id);
        }
        Ok(())
    }
    fn statement(&mut self, stmt: &t::Stmt) -> Result<(), Diagnostic> {
        use t::Stmt as S;
        match stmt {
            S::Let { id, ty, value } => {
                if let Some(value) = value {
                    if ty.is_borrow() {
                        let origin = self.reference(value, self.function.locals[*id].name.clone())?;
                        self.bindings.insert(*id, origin);
                    } else if self.is_view(*ty) {
                        let t::ExprKind::New { class, constructor, args } = &value.kind
                            else { unreachable!("a view local is initialized directly with new") };
                        let fields = self.construct(*class, *constructor, args, value.span, true)?;
                        self.views.insert(*id, fields);
                    } else {
                        self.expression(value)?;
                    }
                }
            }
            S::Assign { target, value } => {
                let guards_before = self.guards.len();
                // Replacing a variable is destructive; assigning a field writes
                // its container, not the object denoted by a borrowed field.
                let destination = match &target.kind {
                    PlaceKind::Field { base, .. } => {
                        self.projections(base, value.span)?;
                        let origin = self.origin(base);
                        self.guards.push(Guard { root: origin.root, span: value.span,
                            projected: matches!(base.kind, PlaceKind::Field { .. }) });
                        (origin, Access::Write)
                    }
                    PlaceKind::Var(id) => (Origin { root: Root::Local(*id), via: None },
                        if target.ty.is_object() { Access::Destroy } else { Access::Write }),
                    PlaceKind::This => unreachable!("this cannot be assigned"),
                };
                let mark = self.loans.len();
                self.expression(value)?;
                self.guards.truncate(guards_before);
                self.access(destination.0, destination.1, value.span)?;
                self.end_since(mark, &HashSet::new(), "assignment completed");
            }
            S::Expr(expr) => { self.expression(expr)?; }
            S::If { condition, then_block, else_block } => {
                self.expression(condition)?;
                self.block(then_block)?;
                if let Some(block) = else_block { self.block(block)?; }
            }
            S::While { condition, body } => {
                // Bindings are not reassignable and loans cannot escape a loop
                // body. Re-entry therefore has the same outer loan environment.
                self.expression(condition)?;
                self.block(body)?;
            }
            S::Match { value, arms } => {
                let mark = self.loans.len();
                let origin = if value.ty.is_borrow() {
                    Some(self.reference(value, "borrowed match".to_owned())?)
                } else {
                    self.expression(value)?;
                    None
                };
                for arm in arms {
                    let arm_mark = self.loans.len();
                    if let Some(origin) = origin {
                        for id in &arm.bindings {
                            if self.function.locals[*id].ty.is_borrow() {
                                self.bindings.insert(*id, origin);
                            }
                        }
                    }
                    self.block(&arm.body)?;
                    for id in arm.bindings.iter().rev() {
                        let ty = self.function.locals[*id].ty;
                        if ty.is_object() {
                            self.access(Origin { root: Root::Local(*id), via: None }, Access::Destroy,
                                self.function.locals[*id].span)?;
                        }
                        self.bindings.remove(id);
                        self.views.remove(id);
                    }
                    self.end_since(arm_mark, &HashSet::new(), "match arm completed");
                }
                self.end_since(mark, &HashSet::new(), "match completed");
            }
            S::Return(value) => {
                if let Some(value) = value {
                    if self.function.return_type.is_borrow() {
                        let origin = self.reference(value, "receiver-bound return".to_owned())?;
                        if origin.root != Root::This {
                            return Err(Diagnostic::new(value.span, "[E3004] borrowed return must derive from this or a transitively owned receiver field")
                                .with_help("do not return locals, parameters, or externally borrowed view fields"));
                        }
                    } else {
                        self.expression(value)?;
                    }
                }
            }
            S::Break | S::Continue => {}
            S::Block(block) => self.block(block)?,
        }
        Ok(())
    }
    fn arguments(&mut self, args: &[t::Expr]) -> Result<Vec<Option<Origin>>, Diagnostic> {
        let mut origins = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            let origin = if arg.ty.is_borrow() {
                Some(self.reference(arg, format!("argument {}", index + 1))?)
            } else { self.expression(arg)?; None };
            origins.push(origin);
        }
        Ok(origins)
    }
    fn construct(&mut self, class: usize, constructor: usize, args: &[t::Expr], span: Span,
                 retain: bool) -> Result<Vec<Option<Origin>>, Diagnostic> {
        let mark = self.loans.len();
        let origins = self.arguments(args)?;
        self.log(span, format!("constructor fn#{constructor}"));
        let mut fields = vec![None; self.program.classes[class].fields.len()];
        let mut keep = HashSet::new();
        if retain {
            for (field, source) in self.program.classes[class].borrowed_from.iter().enumerate() {
                if let Some(index) = source {
                    let origin = origins[*index].expect("borrowed field sourced from a borrow parameter");
                    fields[field] = Some(origin);
                    keep.insert(origin.via.expect("argument loan"));
                }
            }
        }
        self.end_since(mark, &keep, "constructor completed");
        Ok(fields)
    }
    fn reference(&mut self, expr: &t::Expr, label: String) -> Result<Origin, Diagnostic> {
        use t::ExprKind as E;
        let Type::Borrow { mutable, .. } = expr.ty else {
            unreachable!("typed borrowed expression");
        };
        match &expr.kind {
            E::Borrow { place, mutable: requested } => {
                let origin = self.borrow(place, *requested, expr.span, label)?;
                // A newly created exclusive capability may be downgraded to shared.
                self.loans[origin.via.expect("new loan")].mutable = mutable;
                Ok(origin)
            }
            E::Read(place) => self.borrow(place, mutable, expr.span, label),
            E::Upcast { value, .. } => self.reference(value, label),
            E::Call { .. } => Ok(self.call(expr, Some(label))?.expect("borrowed method has receiver")),
            E::MapSlot { map, index, .. } => {
                self.expression(index)?;
                self.projections(map, expr.span)?;
                self.loan(self.origin(map), mutable, expr.span, label)
            }
            _ => unreachable!("borrowed values are places, upcasts, receiver calls, or Map traversal slots"),
        }
    }
    fn call(&mut self, expr: &t::Expr, label: Option<String>) -> Result<Option<Origin>, Diagnostic> {
        let t::ExprKind::Call { target, receiver, args } = &expr.kind else {
            unreachable!("call expression");
        };
        let mark = self.loans.len();
        let guards_before = self.guards.len();
        let receiver_origin = if let Some(place) = receiver {
            self.projections(place, expr.span)?;
            let origin = self.origin(place);
            self.guards.push(Guard { root: origin.root, span: expr.span,
                projected: matches!(place.kind, PlaceKind::Field { .. }) });
            self.log(expr.span, format!("reserve receiver {}", self.label(origin.root)));
            Some(origin)
        } else { None };
        self.arguments(args)?;
        self.guards.truncate(guards_before);
        if let Some(origin) = receiver_origin {
            let CallTarget::Function(id) = target else { unreachable!("println has no receiver") };
            // Arguments finish before the outer receiver capability activates.
            let mode = if self.program.functions[*id].readonly { Access::Read } else { Access::Write };
            self.access(origin, mode, expr.span)?;
            self.log(expr.span, format!("activate receiver for fn#{id}"));
        }
        self.end_since(mark, &HashSet::new(), "call completed");
        if let Type::Borrow { mutable, .. } = expr.ty {
            let origin = receiver_origin.expect("borrowed returns require an instance receiver");
            return self.loan(origin, mutable, expr.span, label.unwrap_or_else(|| "returned borrow".to_owned())).map(Some);
        }
        Ok(None)
    }
    fn expression(&mut self, expr: &t::Expr) -> Result<(), Diagnostic> {
        if expr.ty.is_borrow() {
            let mark = self.loans.len();
            self.reference(expr, "temporary borrowed expression".to_owned())?;
            self.end_since(mark, &HashSet::new(), "borrowed expression discarded");
            return Ok(());
        }
        use t::ExprKind as E;
        match &expr.kind {
            E::Int(_) | E::Boolean(_) | E::String(_) => {}
            E::Read(place) => {
                self.projections(place, expr.span)?;
                self.access(self.origin(place), Access::Read, expr.span)?;
            }
            E::Borrow { place, mutable } => {
                self.borrow(place, *mutable, expr.span, "field initialization".to_owned())?;
            }
            E::MapCapacity(map) => {
                self.projections(map, expr.span)?;
                self.access(self.origin(map), Access::Read, expr.span)?;
            }
            E::MapSlot { map, index, .. } => {
                self.expression(index)?;
                self.projections(map, expr.span)?;
                self.access(self.origin(map), Access::Read, expr.span)?;
            }
            E::Move(id) => {
                self.access(Origin { root: Root::Local(*id), via: None }, Access::Destroy, expr.span)?;
            }
            E::Drop(inner) | E::Upcast { value: inner, .. } => self.expression(inner)?,
            E::New { class, constructor, args } => {
                self.construct(*class, *constructor, args, expr.span, false)?;
            }
            E::Variant { args, .. } => {
                let mark = self.loans.len();
                self.arguments(args)?;
                self.end_since(mark, &HashSet::new(), "enum construction completed");
            }
            E::Call { .. } => { self.call(expr, None)?; }
            E::Unary { expr, .. } => self.expression(expr)?,
            E::Binary { left, right, .. } => {
                self.expression(left)?;
                // Check the RHS even for a constant short-circuit condition,
                // matching the conservative CFG policy. No loans escape it.
                self.expression(right)?;
            }
        }
        Ok(())
    }
}

pub fn check(program: &t::Program) -> Result<(), Diagnostic> {
    for function in &program.functions {
        if function.native.is_none() { Analyzer::new(program, function, None).run()?; }
    }
    Ok(())
}
pub fn dump(program: &t::Program, source: &str) -> Result<String, Diagnostic> {
    let mut out = String::from("Nacre — lexical borrow analysis\nLoan lifetimes are lexical; root = owner and its owned fields.\nReceiver reservations precede arguments; activation follows them.\nReturned borrows retain receiver-derived loans.\n\n");
    for function in &program.functions {
        if function.native.is_some() { continue; }
        let mut analyzer = Analyzer::new(program, function, Some(source));
        analyzer.run()?;
        writeln!(out, "fn #{} {} {{", function.id, function.name).unwrap();
        out.push_str(&analyzer.trace);
        out.push_str("}\n\n");
    }
    Ok(out)
}
