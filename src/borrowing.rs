//! Control-flow-sensitive borrow checking over the typed, structured IR.
//!
//! Borrowed bindings are immutable and cannot escape their scope. Local loans
//! remain live through their last use on every reachable control-flow path;
//! argument loans remain active through the call. Owners and their owned fields
//! are conservatively one alias domain. Reborrows carry a parent capability:
//! children may use it, but conflicting sibling or parent accesses are rejected.
//! Ownership/initialization across loops remains the separate CFG analysis in
//! ownership.rs.
//!
//! Views (classes with shared borrowed fields) are non-movable, non-exportable
//! local objects. Their constructor records which shared arguments are retained.
//! Borrowed returns are tied to the receiver; there are no lifetime parameters.
//! A separate backward liveness pass handles local loan last uses across branches
//! and loop back-edges.
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
    span: Span, label: String, active: bool, holders: Vec<usize>,
}
#[derive(Clone, Copy)]
struct Guard { root: Root, span: Span, projected: bool }
type LiveSet = HashSet<usize>;

/// Backward liveness for locals whose values can retain a borrow. The checker
/// stores one live set per statement; structured branches get independent sets,
/// and loops converge over their back-edge before the forward borrow walk.
struct Liveness {
    at: HashMap<*const t::Stmt, LiveSet>,
    tracked: HashSet<usize>,
    work: usize,
}

impl Liveness {
    fn new(program: &t::Program, function: &t::Function) -> Self {
        let tracked = function.locals.iter().enumerate().filter_map(|(id, local)| {
            let is_view = local.ty.object_class()
                .is_some_and(|class| program.classes[class].is_view());
            (local.ty.is_borrow() || is_view).then_some(id)
        }).collect();
        Self { at: HashMap::new(), tracked, work: 0 }
    }

    fn span(function: &t::Function, stmt: &t::Stmt) -> Span {
        match stmt {
            t::Stmt::Let { id, .. } => function.locals[*id].span,
            t::Stmt::Assign { value, .. } | t::Stmt::Expr(value) => value.span,
            t::Stmt::If { condition, .. } | t::Stmt::While { condition, .. } => condition.span,
            t::Stmt::Match { value, .. } => value.span,
            t::Stmt::Return(Some(value)) => value.span,
            t::Stmt::Return(None) | t::Stmt::Break | t::Stmt::Continue | t::Stmt::Block(_) =>
                function.locals.first().map_or_else(Span::default, |local| local.span),
        }
    }

    fn charge(&mut self, amount: usize, span: Span) -> Result<(), Diagnostic> {
        self.work = self.work.saturating_add(amount);
        if self.work > MAX_WORK {
            Err(Diagnostic::new(span, "[E3099] borrow analysis is too complex; split the function into smaller blocks or functions"))
        } else {
            Ok(())
        }
    }

    fn union(&mut self, into: &mut LiveSet, from: &LiveSet, span: Span)
        -> Result<(), Diagnostic>
    {
        self.charge(from.len(), span)?;
        into.extend(from.iter().copied());
        Ok(())
    }

    fn use_local(&mut self, id: usize, live: &mut LiveSet, span: Span)
        -> Result<(), Diagnostic>
    {
        self.charge(1, span)?;
        if self.tracked.contains(&id) { live.insert(id); }
        Ok(())
    }

    fn place(&mut self, place: &Place, live: &mut LiveSet, span: Span)
        -> Result<(), Diagnostic>
    {
        if let Some(id) = place.root_var() { self.use_local(id, live, span)?; }
        Ok(())
    }

    fn expression(&mut self, expr: &t::Expr, live: &mut LiveSet)
        -> Result<(), Diagnostic>
    {
        self.charge(1, expr.span)?;
        use t::ExprKind as E;
        match &expr.kind {
            E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit
            | E::Boolean(_) | E::String(_) => {}
            E::Read(place) | E::Borrow { place, .. } | E::MapCapacity(place) =>
                self.place(place, live, expr.span)?,
            E::MapSlot { map, index, .. } => {
                self.place(map, live, expr.span)?;
                self.expression(index, live)?;
            }
            E::IteratorFold { plan, initial, folder, .. } => {
                self.iterator_plan(plan, live)?;
                self.expression(initial, live)?;
                self.expression(folder, live)?;
            }
            E::IteratorSizeHint { plan, .. } => self.iterator_plan(plan, live)?,
            E::Move(id) => self.use_local(*id, live, expr.span)?,
            E::Drop(inner) | E::Cast { value: inner } | E::Upcast { value: inner, .. }
            | E::Unary { expr: inner, .. } | E::Try { value: inner, .. } =>
                self.expression(inner, live)?,
            E::New { args, .. } | E::Variant { args, .. } => {
                for arg in args { self.expression(arg, live)?; }
            }
            E::Call { receiver, args, .. } => {
                if let Some(receiver) = receiver { self.place(receiver, live, expr.span)?; }
                for arg in args { self.expression(arg, live)?; }
            }
            E::Binary { left, right, .. } => {
                self.expression(left, live)?;
                self.expression(right, live)?;
            }
        }
        Ok(())
    }

    fn iterator_plan(&mut self, plan: &t::IteratorPlan, live: &mut LiveSet)
        -> Result<(), Diagnostic>
    {
        self.charge(1, plan.span)?;
        let receiver = match &plan.source {
            t::IteratorSource::List { collection, .. }
            | t::IteratorSource::Array { collection, .. }
            | t::IteratorSource::Map { collection, .. } => collection,
        };
        match receiver {
            t::IteratorReceiver::Borrowed(place) => self.place(place, live, plan.span)?,
            t::IteratorReceiver::BorrowedExpr(value)
            | t::IteratorReceiver::Owned(value) => self.expression(value, live)?,
        }
        for stage in &plan.stages {
            match stage {
                t::IteratorStage::Map { callback, .. }
                | t::IteratorStage::Filter { callback, .. } => self.expression(callback, live)?,
                t::IteratorStage::Take { count } => self.expression(count, live)?,
                t::IteratorStage::Enumerate { .. } => {}
                t::IteratorStage::Zip { right, .. } => self.iterator_plan(right, live)?,
            }
        }
        Ok(())
    }

    fn statement(&mut self, stmt: &t::Stmt, after: &LiveSet, function: &t::Function)
        -> Result<(LiveSet, LiveSet), Diagnostic>
    {
        use t::Stmt as S;
        let span = Self::span(function, stmt);
        self.charge(1, span)?;
        let (before, during) = match stmt {
            S::Let { id, value, .. } => {
                let mut before = after.clone();
                before.remove(id);
                if let Some(value) = value { self.expression(value, &mut before)?; }
                let mut during = before.clone();
                self.union(&mut during, after, span)?;
                (before, during)
            }
            S::Assign { target, value } => {
                let mut before = after.clone();
                match &target.kind {
                    PlaceKind::Var(id) => { before.remove(id); }
                    _ => self.place(target, &mut before, value.span)?,
                }
                self.expression(value, &mut before)?;
                let mut during = before.clone();
                self.union(&mut during, after, span)?;
                (before, during)
            }
            S::Expr(expr) => {
                let mut before = after.clone();
                self.expression(expr, &mut before)?;
                let mut during = before.clone();
                self.union(&mut during, after, span)?;
                (before, during)
            }
            S::If { condition, then_block, else_block } => {
                let mut before = self.block(then_block, after, function)?;
                if let Some(block) = else_block {
                    let else_before = self.block(block, after, function)?;
                    self.union(&mut before, &else_before, span)?;
                }
                self.expression(condition, &mut before)?;
                let mut during = before.clone();
                self.union(&mut during, after, span)?;
                (before, during)
            }
            S::While { condition, body } => {
                let mut condition_live = LiveSet::new();
                self.expression(condition, &mut condition_live)?;
                let mut header = after.clone();
                loop {
                    self.charge(1, span)?;
                    let mut body_after = header.clone();
                    self.union(&mut body_after, after, span)?;
                    let body_before = self.block(body, &body_after, function)?;
                    let mut next = after.clone();
                    self.union(&mut next, &condition_live, span)?;
                    self.union(&mut next, &body_before, span)?;
                    if next == header { break; }
                    header = next;
                }
                let mut during = header.clone();
                self.union(&mut during, after, span)?;
                (header, during)
            }
            S::Match { value, arms } => {
                let mut before = after.clone();
                let mut during = after.clone();
                let mut value_live = LiveSet::new();
                self.expression(value, &mut value_live)?;
                self.union(&mut before, &value_live, value.span)?;
                self.union(&mut during, &value_live, value.span)?;
                for arm in arms {
                    let mut arm_live = self.block(&arm.body, after, function)?;
                    if let Some(guard) = &arm.guard { self.expression(guard, &mut arm_live)?; }
                    self.union(&mut during, &arm_live, span)?;
                    let mut arm_before = arm_live;
                    let mut bindings = Vec::new();
                    collect_match_bindings(&arm.pattern, &mut bindings);
                    for (id, guard_id) in bindings {
                        arm_before.remove(&id);
                        if let Some(guard_id) = guard_id { arm_before.remove(&guard_id); }
                    }
                    self.union(&mut before, &arm_before, span)?;
                }
                self.union(&mut during, &before, span)?;
                (before, during)
            }
            S::Return(value) => {
                let mut before = LiveSet::new();
                if let Some(value) = value { self.expression(value, &mut before)?; }
                (before.clone(), before)
            }
            S::Break | S::Continue => (after.clone(), after.clone()),
            S::Block(block) => {
                let before = self.block(block, after, function)?;
                let mut during = before.clone();
                self.union(&mut during, after, span)?;
                (before, during)
            }
        };
        Ok((before, during))
    }

    fn block(&mut self, block: &t::Block, after: &LiveSet, function: &t::Function)
        -> Result<LiveSet, Diagnostic>
    {
        let mut live = after.clone();
        for stmt in block.iter().rev() {
            let (before, during) = self.statement(stmt, &live, function)?;
            self.at.insert(stmt as *const t::Stmt, during);
            live = before;
        }
        for stmt in block {
            if let t::Stmt::Let { id, .. } = stmt { live.remove(id); }
        }
        Ok(live)
    }

    fn analyze(mut self, function: &t::Function)
        -> Result<HashMap<*const t::Stmt, LiveSet>, Diagnostic>
    {
        self.block(&function.body, &LiveSet::new(), function)?;
        Ok(self.at)
    }
}


struct Analyzer<'a> {
    program: &'a t::Program,
    function: &'a t::Function,
    source: Option<&'a str>,
    loans: Vec<Loan>,
    bindings: HashMap<usize, Origin>,
    views: HashMap<usize, Vec<Option<Origin>>>,
    // Map row projections share one scoped origin, not independent slot loans.
    map_pairs: Vec<(usize, Origin)>,
    guards: Vec<Guard>,
    live_at: HashMap<*const t::Stmt, LiveSet>,
    current_stmt: Option<*const t::Stmt>,
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
            views: HashMap::new(), map_pairs: Vec::new(), guards: Vec::new(),
            live_at: HashMap::new(), current_stmt: None, trace: String::new(), work: 0 }
    }
    fn holder_is_live(&self, holder: usize) -> bool {
        self.current_stmt
            .and_then(|stmt| self.live_at.get(&stmt))
            .map_or(true, |live| live.contains(&holder))
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
            Err(Diagnostic::new(span, "[E3099] borrow analysis is too complex; split the function into smaller blocks or functions"))
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
            if !loan.active || loan.root != origin.root || ancestors.contains(&id)
                || (!loan.holders.is_empty() &&
                    !loan.holders.iter().any(|holder| self.holder_is_live(*holder))) {
                continue;
            }
            if mode == Access::Read && !loan.mutable { continue; }
            let code = if mode == Access::Destroy { "E3002" } else { "E3001" };
            let action = match mode {
                Access::Read => "read access", Access::Write => "exclusive access",
                Access::Destroy => "move, destroy, or replace",
            };
            return Err(Diagnostic::new(span, format!(
                "[{code}] {action} of `{}` conflicts with an active {} borrow", self.label(origin.root),
                if loan.mutable { "exclusive" } else { "shared" }))
                .with_note(loan.span, format!("borrow #{} `{}` begins here", id, loan.label))
                .with_help("finish using the borrowed value before this access, or put its remaining uses in a shorter block"));
        }
        for guard in &self.guards {
            if guard.root != origin.root { continue; }
            if mode == Access::Destroy {
                return Err(Diagnostic::new(span, format!(
                    "[E3002] moving or destroying `{}` while evaluating a receiver", self.label(origin.root)))
                    .with_note(guard.span, "the receiver must remain alive until the call or assignment completes"));
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
            return Err(Diagnostic::new(span, "[E3099] limit of 8192 loans per function"));
        }
        let id = self.loans.len();
        self.log(span, format!("loan #{id} {} <- {} parent={:?} holder={label}",
            if mutable { "exclusive" } else { "shared" }, self.label(origin.root), origin.via));
        self.loans.push(Loan {
            root: origin.root, parent: origin.via, mutable, span, label, active: true,
            holders: Vec::new(),
        });
        Ok(Origin { root: origin.root, via: Some(id) })
    }
    fn hold(&mut self, origin: Origin, local: usize) {
        if let Some(id) = origin.via {
            let holders = &mut self.loans[id].holders;
            if !holders.contains(&local) { holders.push(local); }
        }
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
        let live_at = Liveness::new(self.program, self.function).analyze(self.function)?;
        self.live_at = live_at;
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
        let pair_mark = self.map_pairs.len();
        let previous_stmt = self.current_stmt;
        let mut declared = Vec::new();
        for stmt in block {
            self.current_stmt = Some(stmt as *const t::Stmt);
            self.statement(stmt)?;
            if let t::Stmt::Let { id, ty, .. } = stmt { declared.push((*id, *ty)); }
        }
        self.current_stmt = previous_stmt;
        self.end_since(mark, &HashSet::new(), "scope exit");
        self.map_pairs.truncate(pair_mark);
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
                        self.hold(origin, *id);
                        self.bindings.insert(*id, origin);
                    } else if self.is_view(*ty) {
                        let t::ExprKind::New { class, constructor, args } = &value.kind
                            else { unreachable!("a view local is initialized directly with new") };
                        let fields = self.construct(*class, *constructor, args, value.span, true)?;
                        for origin in fields.iter().flatten().copied() { self.hold(origin, *id); }
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
                let pair_mark = self.map_pairs.len();
                let origin = if value.ty.is_borrow() {
                    Some(self.reference(value, "borrowed match".to_owned())?)
                } else {
                    self.expression(value)?;
                    None
                };
                for arm in arms {
                    let arm_mark = self.loans.len();
                    let mut bindings = Vec::new();
                    collect_match_bindings(&arm.pattern, &mut bindings);
                    if let Some(origin) = origin {
                        for (id, guard_id) in &bindings {
                            if self.function.locals[*id].ty.is_borrow() {
                                self.hold(origin, *id);
                                self.bindings.insert(*id, origin);
                            }
                            if let Some(guard_id) = guard_id
                                && self.function.locals[*guard_id].ty.is_borrow() {
                                    self.hold(origin, *guard_id);
                                    self.bindings.insert(*guard_id, origin);
                                }
                        }
                    }
                    if let Some(guard) = &arm.guard {
                        self.expression(guard)?;
                        for (_, guard_id) in &bindings {
                            if let Some(guard_id) = guard_id { self.bindings.remove(guard_id); }
                        }
                    }
                    self.block(&arm.body)?;
                    for (id, guard_id) in bindings.iter().rev() {
                        let ty = self.function.locals[*id].ty;
                        if ty.is_object() {
                            self.access(Origin { root: Root::Local(*id), via: None }, Access::Destroy,
                                self.function.locals[*id].span)?;
                        }
                        self.bindings.remove(id);
                        self.views.remove(id);
                        if let Some(guard_id) = guard_id {
                            self.bindings.remove(guard_id);
                            self.views.remove(guard_id);
                        }
                    }
                    self.end_since(arm_mark, &HashSet::new(), "match arm completed");
                }
                self.end_since(mark, &HashSet::new(), "match completed");
                self.map_pairs.truncate(pair_mark);
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
            E::MapSlot { map, index, pair, .. } => {
                self.expression(index)?;
                self.projections(map, expr.span)?;
                let parent = self.origin(map);
                if let Some(pair) = pair {
                    if let Some((_, origin)) = self.map_pairs.iter().rev().find(|(id, _)| id == pair) {
                        return Ok(*origin);
                    }
                    // The value binding controls pair mutability; its key stays a shared typed view.
                    let mutable = matches!(map.ty, Type::Borrow { mutable: true, .. });
                    let origin = self.loan(parent, mutable, expr.span, label)?;
                    self.map_pairs.push((*pair, origin));
                    Ok(origin)
                } else {
                    self.loan(parent, mutable, expr.span, label)
                }
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
    fn iterator_plan(&mut self, plan: &t::IteratorPlan) -> Result<(), Diagnostic> {
        let receiver = match &plan.source {
            t::IteratorSource::List { collection, .. }
            | t::IteratorSource::Array { collection, .. }
            | t::IteratorSource::Map { collection, .. } => collection,
        };
        match receiver {
            t::IteratorReceiver::Borrowed(place) => {
                self.borrow(place, false, plan.span, "iterator source".to_owned())?;
            }
            t::IteratorReceiver::BorrowedExpr(value) => {
                self.reference(value, "iterator source".to_owned())?;
            }
            t::IteratorReceiver::Owned(value) => self.expression(value)?,
        }
        for stage in &plan.stages {
            match stage {
                t::IteratorStage::Map { callback, .. }
                | t::IteratorStage::Filter { callback, .. } => self.expression(callback)?,
                t::IteratorStage::Take { count } => self.expression(count)?,
                t::IteratorStage::Enumerate { .. } => {}
                t::IteratorStage::Zip { right, .. } => self.iterator_plan(right)?,
            }
        }
        Ok(())
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
            E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit
            | E::Boolean(_) | E::String(_) => {}
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
            E::IteratorFold { plan, initial, folder, .. } => {
                let mark = self.loans.len();
                self.iterator_plan(plan)?;
                self.expression(initial)?;
                self.expression(folder)?;
                self.end_since(mark, &HashSet::new(), "fused iterator fold completed");
            }
            E::IteratorSizeHint { plan, .. } => {
                let mark = self.loans.len();
                self.iterator_plan(plan)?;
                self.end_since(mark, &HashSet::new(), "iterator size hint completed");
            }
            E::Move(id) => {
                self.access(Origin { root: Root::Local(*id), via: None }, Access::Destroy, expr.span)?;
            }
            E::Drop(inner) | E::Cast { value: inner, .. } | E::Upcast { value: inner, .. } => self.expression(inner)?,
            E::New { class, constructor, args } => {
                self.construct(*class, *constructor, args, expr.span, false)?;
            }
            E::Variant { args, .. } => {
                let mark = self.loans.len();
                self.arguments(args)?;
                self.end_since(mark, &HashSet::new(), "enum construction completed");
            }
            E::Call { .. } => { self.call(expr, None)?; }
            E::Try { value, .. } => self.expression(value)?,
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

fn collect_match_bindings(pattern: &t::Pattern, out: &mut Vec<(usize, Option<usize>)>) {
    match &pattern.kind {
        t::PatternKind::Binding { id, guard_id } => out.push((*id, *guard_id)),
        t::PatternKind::Wildcard => {}
        t::PatternKind::Variant { fields, .. } =>
            for field in fields { collect_match_bindings(field, out); },
        t::PatternKind::Record { fields, .. } =>
            for (_, field) in fields { collect_match_bindings(field, out); },
    }
}

pub fn check(program: &t::Program) -> Result<(), Diagnostic> {
    for function in &program.functions {
        if function.native.is_none() { Analyzer::new(program, function, None).run()?; }
    }
    Ok(())
}
pub fn dump(program: &t::Program, source: &str) -> Result<String, Diagnostic> {
    let mut out = String::from("Nacre — control-flow-sensitive borrow analysis\nLocal loan conflicts follow backward last-use liveness across branches and loops; root = owner and its owned fields.\nReceiver reservations precede arguments; activation follows them.\nReturned borrows retain receiver-derived loans.\n\n");
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
