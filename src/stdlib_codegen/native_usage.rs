use std::ops::ControlFlow;
use crate::ir::{self as t, NativeMethod};

/// Query resolved native calls, stopping as soon as a method matches.
pub(super) fn any(program: &t::Program, mut matches: impl FnMut(NativeMethod) -> bool) -> bool {
    visit(program, &mut |method| {
        if matches(method) { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
    }).is_break()
}

/// Inspect every resolved native call without collecting or cloning IR nodes.
pub(super) fn for_each(program: &t::Program, mut inspect: impl FnMut(NativeMethod)) {
    let _ = visit(program, &mut |method| {
        inspect(method);
        ControlFlow::Continue(())
    });
}

// Only calls are usage: unused native declarations must not select helpers.
// All function bodies and branches matter to C emission, even unreachable ones.
fn visit(program: &t::Program, inspect: &mut impl FnMut(NativeMethod) -> ControlFlow<()>) -> ControlFlow<()> {
    for function in &program.functions {
        block(program, &function.body, inspect)?;
    }
    ControlFlow::Continue(())
}

fn block(program: &t::Program, statements: &t::Block,
         inspect: &mut impl FnMut(NativeMethod) -> ControlFlow<()>) -> ControlFlow<()> {
    use t::Stmt as S;
    for statement in statements {
        match statement {
            S::Let { value, .. } | S::Return(value) => {
                if let Some(value) = value { expression(program, value, inspect)?; }
            }
            S::Assign { value, .. } | S::Expr(value) => expression(program, value, inspect)?,
            S::If { condition, then_block, else_block } => {
                expression(program, condition, inspect)?;
                block(program, then_block, inspect)?;
                if let Some(body) = else_block { block(program, body, inspect)?; }
            }
            S::While { condition, body } => {
                expression(program, condition, inspect)?;
                block(program, body, inspect)?;
            }
            S::Match { value, arms } => {
                expression(program, value, inspect)?;
                for arm in arms { block(program, &arm.body, inspect)?; }
            }
            S::Block(body) => block(program, body, inspect)?,
            S::Break | S::Continue => {}
        }
    }
    ControlFlow::Continue(())
}

fn expression(program: &t::Program, value: &t::Expr,
              inspect: &mut impl FnMut(NativeMethod) -> ControlFlow<()>) -> ControlFlow<()> {
    use t::ExprKind as E;
    match &value.kind {
        E::Call { target, args, .. } => {
            if let t::CallTarget::Function(id) = target
                && let Some(method) = program.functions[*id].native {
                inspect(method)?;
            }
            for argument in args { expression(program, argument, inspect)?; }
        }
        E::New { args, .. } | E::Variant { args, .. } => {
            for argument in args { expression(program, argument, inspect)?; }
        }
        E::MapSlot { index, .. } => expression(program, index, inspect)?,
        E::Drop(value) | E::Upcast { value, .. } | E::Unary { expr: value, .. }
        | E::Try { value, .. } | E::Cast { value } => expression(program, value, inspect)?,
        E::Binary { left, right, .. } => {
            expression(program, left, inspect)?;
            expression(program, right, inspect)?;
        }
        E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit | E::Boolean(_)
        | E::String(_) | E::Read(_) | E::Move(_) | E::Borrow { .. } | E::MapCapacity(_) => {}
    }
    ControlFlow::Continue(())
}
