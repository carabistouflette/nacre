use crate::ir::{self as t, PlaceKind, Type};
use std::collections::HashSet;

pub(crate) fn find_stack_promoted(function: &t::Function, program: &t::Program) -> HashSet<usize> {
    let mut candidates = HashSet::new();
    for stmt in &function.body {
        if let t::Stmt::Let { id, ty: Type::Object(class), value: Some(val) } = stmt
            && matches!(val.kind, t::ExprKind::New { .. })
                && program.classes[*class].native.is_none()
                && program.classes[*class].variants.is_none()
            {
                candidates.insert(*id);
            }
    }
    if candidates.is_empty() {
        return candidates;
    }

    let mut escaping = HashSet::new();
    for stmt in &function.body {
        check_escape_stmt(stmt, &mut escaping);
    }

    candidates.retain(|id| !escaping.contains(id));
    candidates
}

fn check_escape_stmt(stmt: &t::Stmt, escaping: &mut HashSet<usize>) {
    match stmt {
        t::Stmt::Let { value, .. } => {
            if let Some(v) = value {
                check_escape_expr(v, escaping);
            }
        }
        t::Stmt::Assign { target, value } => {
            if let Some(root) = target.root_var() {
                escaping.insert(root);
            }
            if matches!(target.kind, PlaceKind::Field { .. }) {
                collect_moved_vars(value, escaping);
            }
            check_escape_expr(value, escaping);
        }
        t::Stmt::Expr(e) => {
            check_escape_expr(e, escaping);
        }
        t::Stmt::If { condition, then_block, else_block } => {
            check_escape_expr(condition, escaping);
            for s in then_block { check_escape_stmt(s, escaping); }
            if let Some(eb) = else_block {
                for s in eb { check_escape_stmt(s, escaping); }
            }
        }
        t::Stmt::While { condition, body } => {
            check_escape_expr(condition, escaping);
            for s in body { check_escape_stmt(s, escaping); }
        }
        t::Stmt::Return(opt_v) => {
            if let Some(v) = opt_v {
                collect_moved_vars(v, escaping);
                check_escape_expr(v, escaping);
            }
        }
        t::Stmt::Block(inner) => {
            for s in inner { check_escape_stmt(s, escaping); }
        }
        t::Stmt::Match { value, arms } => {
            check_escape_expr(value, escaping);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_moved_vars(guard, escaping);
                    check_escape_expr(guard, escaping);
                }
                for s in &arm.body { check_escape_stmt(s, escaping); }
            }
        }
        t::Stmt::Break | t::Stmt::Continue => {}
    }
}

fn visit_iterator_plan_expressions(
    plan: &t::IteratorPlan,
    visit: &mut impl FnMut(&t::Expr),
) {
    match &plan.source {
        t::IteratorSource::List { collection, .. }
        | t::IteratorSource::Array { collection, .. }
        | t::IteratorSource::Map { collection, .. } => match collection {
            t::IteratorReceiver::Borrowed(_) => {}
            t::IteratorReceiver::BorrowedExpr(expr) | t::IteratorReceiver::Owned(expr) => visit(expr),
        },
    }
    for stage in &plan.stages {
        match stage {
            t::IteratorStage::Map { callback, .. } | t::IteratorStage::Filter { callback, .. } =>
                visit(callback),
            t::IteratorStage::Take { count } => visit(count),
            t::IteratorStage::Enumerate { .. } => {}
            t::IteratorStage::Zip { right, .. } => visit_iterator_plan_expressions(right, visit),
        }
    }
}

fn check_escape_expr(expr: &t::Expr, escaping: &mut HashSet<usize>) {
    use t::ExprKind as E;
    match &expr.kind {
        E::IteratorFold { plan, initial, folder, .. } => {
            visit_iterator_plan_expressions(plan, &mut |nested| check_escape_expr(nested, escaping));
            check_escape_expr(initial, escaping);
            check_escape_expr(folder, escaping);
        }
        E::IteratorSizeHint { plan, .. } => {
            visit_iterator_plan_expressions(plan, &mut |nested| check_escape_expr(nested, escaping));
        }
        E::Move(id) => {
            escaping.insert(*id);
        }
        E::Call { args, .. } | E::New { args, .. } | E::Variant { args, .. } => {
            for a in args {
                check_escape_expr(a, escaping);
            }
        }
        E::Binary { left, right, .. } => {
            check_escape_expr(left, escaping);
            check_escape_expr(right, escaping);
        }
        E::Unary { expr: inner, .. }
        | E::Cast { value: inner }
        | E::Drop(inner)
        | E::Upcast { value: inner, .. }
        | E::Try { value: inner, .. } => {
            check_escape_expr(inner, escaping);
        }
        E::MapSlot { index, .. } => {
            check_escape_expr(index, escaping);
        }
        E::Read(place) => {
            if matches!(expr.ty, Type::Object(_))
                && let Some(root) = place.root_var() {
                    escaping.insert(root);
                }
        }
        E::Borrow { .. }
        | E::MapCapacity(_)
        | E::Char(_)
        | E::Int(_)
        | E::Long(_)
        | E::Double(_)
        | E::Boolean(_)
        | E::Unit
        | E::String(_) => {}
    }
}

fn collect_moved_vars(expr: &t::Expr, escaping: &mut HashSet<usize>) {
    use t::ExprKind as E;
    match &expr.kind {
        E::IteratorFold { plan, initial, folder, .. } => {
            visit_iterator_plan_expressions(plan, &mut |nested| collect_moved_vars(nested, escaping));
            collect_moved_vars(initial, escaping);
            collect_moved_vars(folder, escaping);
        }
        E::IteratorSizeHint { plan, .. } => {
            visit_iterator_plan_expressions(plan, &mut |nested| collect_moved_vars(nested, escaping));
        }
        E::Move(id) => {
            escaping.insert(*id);
        }
        E::Read(place) => {
            if let Some(root) = place.root_var() {
                escaping.insert(root);
            }
        }
        E::Try { value, .. }
        | E::Unary { expr: value, .. }
        | E::Cast { value }
        | E::Drop(value)
        | E::Upcast { value, .. } => {
            collect_moved_vars(value, escaping);
        }
        E::New { args, .. } | E::Call { args, .. } | E::Variant { args, .. } => {
            for a in args {
                collect_moved_vars(a, escaping);
            }
        }
        E::Binary { left, right, .. } => {
            collect_moved_vars(left, escaping);
            collect_moved_vars(right, escaping);
        }
        E::Borrow { place, .. } => {
            if let Some(root) = place.root_var() {
                escaping.insert(root);
            }
        }
        E::MapSlot { index, .. } => {
            collect_moved_vars(index, escaping);
        }
        E::MapCapacity(_)
        | E::Char(_)
        | E::Int(_)
        | E::Long(_)
        | E::Double(_)
        | E::Boolean(_)
        | E::Unit
        | E::String(_) => {}
    }
}
