//! Definite initialization and affine ownership on an explicit control-flow graph.
//!
//! At each program point, a local has a SET of possible states: uninitialized,
//! ready, moved. Joins are set union. Writes reset to ready; a move requires ready
//! on every incoming path and resets to moved. A monotone worklist computes the
//! least fixed point, including loop back-edges, before any diagnostic is issued.
//! This is not a general borrow checker: call-site receiver locks and the ban on
//! moving/replacing owned fields are still enforced by the type checker.
use std::collections::VecDeque;
use std::fmt::Write;

use crate::ast::BinaryOp;
use crate::diagnostic::{line_column, Diagnostic, Span};
use crate::ir::{self as t, Place, Type};

const UNINIT: u8 = 1;
const READY: u8 = 2;
const MOVED: u8 = 4;
const MAX_NODES: usize = 50_000;
const MAX_STATE_CELLS: usize = 16 * 1024 * 1024;

type NodeId = usize;
type States = Vec<Option<Vec<u8>>>;

#[derive(Clone, Copy, Debug)]
enum Op {
    Nop(&'static str),
    Declare(usize),
    Write(usize),
    Read { id: usize, span: Span, borrow: bool },
    Move { id: usize, span: Span },
    DropTemporary { class: usize, span: Span },
    Cleanup(usize),
    Allocate { class: usize, span: Span },
    ConstructVariant { class: usize, variant: usize, span: Span },
    Call { function: usize, span: Span },
}
#[derive(Debug)]
struct Node { op: Op, next: Vec<NodeId> }
struct Graph { nodes: Vec<Node>, entry: NodeId }
#[derive(Clone, Copy)]
struct Loop { header: NodeId, exit: NodeId, first_scope: usize }
struct Builder {
    graph: Graph,
    loops: Vec<Loop>,
    owners: Vec<Vec<usize>>,
    local_types: Vec<Type>,
    return_exit: NodeId,
}

impl Builder {
    fn new() -> Self {
        Self {
            graph: Graph { nodes: vec![Node { op: Op::Nop("entry"), next: vec![] }], entry: 0 },
            loops: vec![], owners: vec![], local_types: Vec::new(), return_exit: 0,
        }
    }
    fn node(&mut self, op: Op) -> NodeId {
        let id = self.graph.nodes.len();
        self.graph.nodes.push(Node { op, next: vec![] });
        id
    }
    fn edge(&mut self, from: NodeId, to: NodeId) { self.graph.nodes[from].next.push(to); }
    fn step(&mut self, from: NodeId, op: Op) -> NodeId {
        let to = self.node(op); self.edge(from, to); to
    }
    fn cleanup(&mut self, mut at: NodeId, first_scope: usize) -> NodeId {
        let ids: Vec<_> = self.owners[first_scope..].iter().rev()
            .flat_map(|scope| scope.iter().rev().copied()).collect();
        for id in ids { at = self.step(at, Op::Cleanup(id)); }
        at
    }
    fn build(function: &t::Function) -> Graph {
        let mut b = Self::new();
        b.local_types = function.locals.iter().map(|local| local.ty).collect();
        b.return_exit = b.node(Op::Nop("return-exit"));
        let exit = b.node(Op::Nop("fallthrough-exit"));
        b.owners.push(function.params.iter().filter_map(|(id, ty)|
            if ty.is_object() { Some(*id) } else { None }).collect());
        if let Some(at) = b.block(&function.body, 0) {
            let at = b.cleanup(at, 0);
            b.edge(at, exit);
        }
        b.graph
    }
    fn block(&mut self, block: &t::Block, at: NodeId) -> Option<NodeId> {
        self.owners.push(vec![]);
        let mut cursor = Some(at);
        for stmt in block {
            let Some(at) = cursor else { break; };
            cursor = self.statement(stmt, at);
        }
        if let Some(at) = cursor { cursor = Some(self.cleanup(at, self.owners.len() - 1)); }
        self.owners.pop();
        cursor
    }
    fn statement(&mut self, stmt: &t::Stmt, at: NodeId) -> Option<NodeId> {
        use t::Stmt as S;
        match stmt {
            S::Let { id, ty, value } => {
                let mut at = self.step(at, Op::Declare(*id));
                if let Some(value) = value {
                    at = self.expression(value, at);
                    at = self.step(at, Op::Write(*id));
                }
                if ty.is_object() { self.owners.last_mut().expect("local scope").push(*id); }
                Some(at)
            }
            S::Assign { target, value } => {
                // A field receiver is evaluated before the RHS, just as in C codegen.
                let at = match &target.kind {
                    t::PlaceKind::Field { base, .. } => self.read_place(base, value.span, true, at),
                    _ => at,
                };
                let at = self.expression(value, at);
                Some(match &target.kind {
                    t::PlaceKind::Var(id) => self.step(at, Op::Write(*id)),
                    _ => at,
                })
            }
            S::Expr(expr) => {
                let at = self.expression(expr, at);
                Some(if let Type::Object(class) = expr.ty {
                    self.step(at, Op::DropTemporary { class, span: expr.span })
                } else { at })
            }
            S::If { condition, then_block, else_block } => {
                let at = self.expression(condition, at);
                let branch = self.step(at, Op::Nop("if"));
                let then_start = self.step(branch, Op::Nop("then"));
                let else_start = self.step(branch, Op::Nop("else"));
                let then_end = self.block(then_block, then_start);
                let else_end = match else_block {
                    Some(block) => self.block(block, else_start), None => Some(else_start),
                };
                if then_end.is_none() && else_end.is_none() { return None; }
                let join = self.node(Op::Nop("if-join"));
                if let Some(end) = then_end { self.edge(end, join); }
                if let Some(end) = else_end { self.edge(end, join); }
                Some(join)
            }
            S::While { condition, body } => {
                let header = self.step(at, Op::Nop("loop-header"));
                let at = self.expression(condition, header);
                let branch = self.step(at, Op::Nop("while-condition"));
                let exit = self.step(branch, Op::Nop("loop-exit"));
                let body_start = self.step(branch, Op::Nop("loop-body"));
                self.loops.push(Loop { header, exit, first_scope: self.owners.len() });
                if let Some(end) = self.block(body, body_start) { self.edge(end, header); }
                self.loops.pop();
                Some(exit)
            }
            S::Match { value, arms } => {
                let at = self.expression(value, at);
                let branch = self.step(at, Op::Nop("match"));
                let join = self.node(Op::Nop("match-join"));
                let mut has_normal_arm = false;
                for arm in arms {
                    let start = self.step(branch, Op::Nop("match-arm"));
                    self.owners.push(Vec::new());
                    let mut cursor = start;
                    for id in &arm.bindings {
                        cursor = self.step(cursor, Op::Declare(*id));
                        cursor = self.step(cursor, Op::Write(*id));
                        if self.local_types[*id].is_object() {
                            self.owners.last_mut().expect("match arm scope").push(*id);
                        }
                    }
                    if let Some(guard) = &arm.guard {
                        cursor = self.expression(guard, cursor);
                    }
                    if let Some(end) = self.block(&arm.body, cursor) {
                        let end = self.cleanup(end, self.owners.len() - 1);
                        self.edge(end, join);
                        has_normal_arm = true;
                    }
                    self.owners.pop();
                }
                if has_normal_arm { Some(join) } else { None }
            }
            S::Return(value) => {
                let at = value.as_ref().map_or(at, |v| self.expression(v, at));
                let at = self.cleanup(at, 0);
                self.edge(at, self.return_exit);
                None
            }
            S::Break | S::Continue => {
                let target = *self.loops.last().expect("type-checked loop");
                let at = self.cleanup(at, target.first_scope);
                self.edge(at, if matches!(stmt, S::Break) { target.exit } else { target.header });
                None
            }
            S::Block(block) => self.block(block, at),
        }
    }
    fn read_place(&mut self, place: &Place, span: Span, borrow: bool, at: NodeId) -> NodeId {
        match place.root_var() {
            Some(id) => self.step(at, Op::Read { id, span, borrow }),
            None => at, // this is a non-escaping receiver; constructor readiness is checked earlier
        }
    }
    fn expression(&mut self, expr: &t::Expr, at: NodeId) -> NodeId {
        use t::ExprKind as E;
        match &expr.kind {
            E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit
            | E::Boolean(_) | E::String(_) => at,
            E::Read(place) => self.read_place(place, expr.span, false, at),
            E::Borrow { place, .. } => self.read_place(place, expr.span, true, at),
            E::Move(id) => self.step(at, Op::Move { id: *id, span: expr.span }),
            E::MapCapacity(map) => self.read_place(map, expr.span, true, at),
            E::MapSlot { map, index, .. } => {
                let at = self.read_place(map, expr.span, true, at);
                self.expression(index, at)
            }
            E::Drop(inner) => {
                let at = self.expression(inner, at);
                let Type::Object(class) = inner.ty else { unreachable!("checked drop") };
                self.step(at, Op::DropTemporary { class, span: expr.span })
            }
            E::New { class, constructor, args } => {
                let mut at = at;
                for arg in args { at = self.expression(arg, at); }
                let at = self.step(at, Op::Allocate { class: *class, span: expr.span });
                self.step(at, Op::Call { function: *constructor, span: expr.span })
            }
            E::Variant { class, variant, args } => {
                let mut at = at;
                for arg in args { at = self.expression(arg, at); }
                self.step(at, Op::ConstructVariant { class: *class, variant: *variant, span: expr.span })
            }
            E::Call { target, receiver, args } => {
                let mut at = match receiver {
                    Some(place) => self.read_place(place, expr.span, true, at), None => at,
                };
                for arg in args { at = self.expression(arg, at); }
                match target {
                    t::CallTarget::Function(function) => self.step(at, Op::Call { function: *function, span: expr.span }),
                    t::CallTarget::Println => self.step(at, Op::Nop("println")),
                    t::CallTarget::Assert => self.step(at, Op::Nop("assert")),
                }
            }
            E::Unary { expr, .. } | E::Cast { value: expr, .. } | E::Upcast { value: expr, .. } => self.expression(expr, at),
            E::Try { value, .. } => {
                let at = self.expression(value, at);
                let branch = self.step(at, Op::Nop("result-try"));
                let ok = self.step(branch, Op::Nop("result-ok"));
                let error = self.step(branch, Op::Nop("result-error"));
                let error = self.cleanup(error, 0);
                self.edge(error, self.return_exit);
                ok
            }
            E::Binary { op, left, right } => {
                let at = self.expression(left, at);
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let branch = self.step(at, Op::Nop("short-circuit"));
                    let right_start = self.step(branch, Op::Nop("rhs"));
                    let right_end = self.expression(right, right_start);
                    let join = self.node(Op::Nop("short-circuit-join"));
                    self.edge(branch, join); self.edge(right_end, join);
                    join
                } else { self.expression(right, at) }
            }
        }
    }
}

fn transferred(op: Op, id: usize, before: u8) -> u8 {
    match op {
        Op::Declare(v) | Op::Cleanup(v) if v == id => UNINIT,
        Op::Write(v) if v == id => READY,
        Op::Move { id: v, .. } if v == id => MOVED,
        _ => before,
    }
}
fn transfer(op: Op, state: &mut [u8]) {
    let id = match op {
        Op::Declare(id) | Op::Cleanup(id) | Op::Write(id) | Op::Move { id, .. } => id,
        _ => return,
    };
    state[id] = transferred(op, id, state[id]);
}

fn solve(graph: &Graph, function: &t::Function) -> Result<States, Diagnostic> {
    // Bound memory explicitly, including functions with no locals. This is a
    // prototype limit, not an arbitrary loop-iteration limit for the analysis.
    let cells = graph.nodes.len().saturating_mul(function.locals.len());
    if graph.nodes.len() > MAX_NODES || cells > MAX_STATE_CELLS {
        return Err(Diagnostic::new(function.locals.first().map_or(Span::default(), |l| l.span),
            "function is too complex for analysis: limit of 50,000 nodes or 16 MiB of state cells"));
    }
    let mut initial = vec![UNINIT; function.locals.len()];
    for (id, _) in &function.params { initial[*id] = READY; }
    let mut incoming = vec![None; graph.nodes.len()];
    incoming[graph.entry] = Some(initial);
    let mut queue = VecDeque::from([graph.entry]);
    let mut queued = vec![false; graph.nodes.len()];
    queued[graph.entry] = true;
    while let Some(id) = queue.pop_front() {
        queued[id] = false;
        let mut output = incoming[id].as_ref().expect("reachable work item").clone();
        transfer(graph.nodes[id].op, &mut output);
        for &next in &graph.nodes[id].next {
            let slot = &mut incoming[next];
            let changed = match slot {
                None => { *slot = Some(output.clone()); true }
                Some(state) => {
                    let mut changed = false;
                    for (cell, value) in state.iter_mut().zip(&output) {
                        let joined = *cell | *value;
                        changed |= joined != *cell;
                        *cell = joined;
                    }
                    changed
                }
            };
            if changed && !queued[next] { queue.push_back(next); queued[next] = true; }
        }
    }
    Ok(incoming)
}

// Find a contributing move by following only predecessor edges that actually
// carry MOVED at the failing input. Reinitializations stop that propagation.
fn move_origin(graph: &Graph, states: &States, at: NodeId, variable: usize) -> Option<Span> {
    let mut previous = vec![vec![]; graph.nodes.len()];
    for (id, node) in graph.nodes.iter().enumerate() {
        for &next in &node.next { previous[next].push(id); }
    }
    let mut queue: VecDeque<_> = previous[at].iter().copied().collect();
    let mut seen = vec![false; graph.nodes.len()];
    while let Some(id) = queue.pop_front() {
        if seen[id] { continue; }
        seen[id] = true;
        let Some(state) = &states[id] else { continue; };
        let op = graph.nodes[id].op;
        if transferred(op, variable, state[variable]) & MOVED == 0 { continue; }
        if let Op::Move { id: v, span } = op
            && v == variable { return Some(span); }
        queue.extend(previous[id].iter().copied());
    }
    None
}

fn verify(graph: &Graph, function: &t::Function, states: &States) -> Result<(), Diagnostic> {
    for (node_id, node) in graph.nodes.iter().enumerate() {
        let (id, span) = match node.op {
            Op::Read { id, span, .. } | Op::Move { id, span } => (id, span),
            _ => continue,
        };
        let Some(state) = &states[node_id] else { continue; };
        if state[id] == READY { continue; }
        let local = &function.locals[id];
        let mut error = if state[id] & MOVED != 0 {
            Diagnostic::new(span, format!(
                "[E2001] object `{}` is moved or may have been moved; ownership was transferred (no implicit copies)", local.name))
        } else {
            Diagnostic::new(span, format!(
                "[E2002] variable `{}` is uninitialized on all execution paths", local.name))
        };
        if let Some(origin) = move_origin(graph, states, node_id, id) {
            error = error.with_note(origin, "ownership transfer contributing to this state (possibly during a previous loop iteration)");
        }
        error = error.with_note(local.span, format!("declaration of `{}`", local.name))
            .with_help("initialize or reinitialize the variable on every path before this use; `continue` returns to the loop condition");
        return Err(error);
    }
    Ok(())
}

pub fn check(program: &t::Program) -> Result<(), Diagnostic> {
    for function in &program.functions {
        if function.native.is_some() { continue; }
        let graph = Builder::build(function);
        let states = solve(&graph, function)?;
        verify(&graph, function, &states)?;
    }
    Ok(())
}

/// A textual ownership CFG, not an executable or complete computation IR.
pub fn dump(program: &t::Program, source: &str) -> Result<String, Diagnostic> {
    let mut out = String::from("Nacre — ownership CFG\nU=uninitialized, R=ready, M=moved; joins are unions.\nBranches are conservative (no constant-condition proof).\n\n");
    for function in &program.functions {
        if function.native.is_some() { continue; }
        let graph = Builder::build(function);
        let states = solve(&graph, function)?;
        verify(&graph, function, &states)?;
        writeln!(out, "fn #{} {} {{", function.id, function.name).unwrap();
        for (id, local) in function.locals.iter().enumerate() {
            writeln!(out, "  local %{id} {} : {:?}", local.name, local.ty).unwrap();
        }
        for (id, node) in graph.nodes.iter().enumerate() {
            let description = match node.op {
                Op::Nop(name) => name.to_owned(),
                Op::Declare(v) => format!("storage-live %{v} {}", function.locals[v].name),
                Op::Write(v) => format!("initialize/replace %{v} {}", function.locals[v].name),
                Op::Read { id: v, span, borrow } => {
                    let (line, col) = line_column(source, span.start);
                    format!("{} %{v} {} @{line}:{col}", if borrow { "borrow-receiver" } else { "read" }, function.locals[v].name)
                }
                Op::Move { id: v, span } => {
                    let (line, col) = line_column(source, span.start);
                    format!("move %{v} {} @{line}:{col}", function.locals[v].name)
                }
                Op::Cleanup(v) => format!("drop-if-present %{v} {}", function.locals[v].name),
                Op::DropTemporary { class, span } => {
                    let (line, col) = line_column(source, span.start);
                    format!("drop-temporary class#{class} @{line}:{col}")
                }
                Op::Allocate { class, span } => {
                    let (line, col) = line_column(source, span.start);
                    format!("allocate class#{class} @{line}:{col}")
                }
                Op::Call { function, span } => {
                    let (line, col) = line_column(source, span.start);
                    format!("call fn#{function} @{line}:{col}")
                }
                Op::ConstructVariant { class, variant, span } => {
                    let (line, col) = line_column(source, span.start);
                    format!("construct enum#{class} variant#{variant} @{line}:{col}")
                }
            };
            let edges: Vec<_> = node.next.iter().map(|id| format!("bb{id}")).collect();
            let state = match &states[id] {
                None => "unreachable".to_owned(),
                Some(values) => values.iter().enumerate().map(|(v, state)| {
                    let mut flags = String::new();
                    if state & UNINIT != 0 { flags.push('U'); }
                    if state & READY != 0 { flags.push('R'); }
                    if state & MOVED != 0 { flags.push('M'); }
                    format!("%{v}={flags}")
                }).collect::<Vec<_>>().join(" "),
            };
            writeln!(out, "  bb{id}: {description} -> [{}]  in=[{state}]", edges.join(", ")).unwrap();
        }
        out.push_str("}\n\n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn function() -> t::Function {
        t::Function {
            native: None, virtual_slot: None,
            id: 0, name: "test".to_owned(), receiver: None, readonly: false, return_type: Type::Void,
            params: vec![(0, Type::Object(0))], body: vec![],
            locals: (0..2).map(|i| t::Local {
                name: format!("v{i}"), ty: Type::Object(0), span: Span::default(),
            }).collect(),
        }
    }

    #[test]
    fn transfers_are_monotone_on_the_state_lattice() {
        let operations = [Op::Declare(0), Op::Write(0), Op::Move { id: 0, span: Span::default() },
            Op::Cleanup(0), Op::Nop("identity")];
        for op in operations {
            for a in 1u8..8 {
                for b in 1u8..8 {
                    if a | b == b {
                        let x = transferred(op, 0, a);
                        let y = transferred(op, 0, b);
                        assert_eq!(x | y, y, "non-monotone {op:?}: {a} <= {b}");
                    }
                }
            }
        }
    }

    #[test]
    fn fixed_point_matches_exhaustive_concrete_paths() {
        // Small cyclic graphs can be exhaustively interpreted: two locals,
        // each in one of three concrete states. This oracle does NOT reuse the
        // worklist join, so it also checks loop propagation and unreachable nodes.
        let f = function();
        let mut seed = 20261011u64;
        for _ in 0..200 {
            let mut random = || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 32) as usize
            };
            let mut nodes = vec![];
            for _ in 0..7 {
                let variable = random() % 2;
                let op = match random() % 6 {
                    0 => Op::Declare(variable), 1 => Op::Write(variable),
                    2 => Op::Move { id: variable, span: Span::default() },
                    3 => Op::Cleanup(variable),
                    4 => Op::Read { id: variable, span: Span::default(), borrow: false },
                    _ => Op::Nop("pass"),
                };
                let next = (0..random() % 3).map(|_| random() % 7).collect();
                nodes.push(Node { op, next });
            }
            let graph = Graph { nodes, entry: 0 };
            let actual = solve(&graph, &f).unwrap();
            let mut expected: States = vec![None; 7];
            let mut pending = vec![(0usize, vec![READY, UNINIT])];
            let mut seen = HashSet::new();
            while let Some((at, state)) = pending.pop() {
                if !seen.insert((at, state.clone())) { continue; }
                let joined = expected[at].get_or_insert_with(|| vec![0u8; 2]);
                for i in 0..2 { joined[i] |= state[i]; }
                let mut output = state;
                // Independent concrete interpretation of each local update.
                match graph.nodes[at].op {
                    Op::Declare(v) | Op::Cleanup(v) => output[v] = UNINIT,
                    Op::Write(v) => output[v] = READY,
                    Op::Move { id, .. } => output[id] = MOVED,
                    _ => {}
                }
                for &next in &graph.nodes[at].next { pending.push((next, output.clone())); }
            }
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn unreachable_reads_do_not_trigger_diagnostics() {
        let f = function();
        let graph = Graph { entry: 0, nodes: vec![
            Node { op: Op::Nop("entry"), next: vec![] },
            Node { op: Op::Read { id: 1, span: Span::default(), borrow: true }, next: vec![] },
        ] };
        let states = solve(&graph, &f).unwrap();
        assert!(states[1].is_none());
        verify(&graph, &f, &states).unwrap();
    }

    #[test]
    fn invalid_second_iteration_is_detected_after_convergence() {
        let f = function();
        let graph = Graph { entry: 0, nodes: vec![
            Node { op: Op::Nop("entry"), next: vec![1] },
            Node { op: Op::Move { id: 0, span: Span::new(2, 3) }, next: vec![1] },
        ] };
        let states = solve(&graph, &f).unwrap();
        assert_eq!(states[1].as_ref().unwrap()[0], READY | MOVED);
        assert!(verify(&graph, &f, &states).unwrap_err().message.contains("E2001"));
    }
}
