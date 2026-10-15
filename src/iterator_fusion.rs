//! Compiler-side construction and invariant maintenance for fused iterator plans.
//!
//! The plan contains no runtime adapter node. A terminal fold or size-hint
//! expression consumes the complete chain directly in one backend loop.
use crate::ir::{
    Expr, IteratorPlan, IteratorReceiver, IteratorSizeHint, IteratorSource, IteratorStage, Type,
};
use crate::diagnostic::Span;

/// Creates a pipeline rooted at one finite List, Array, or Map.
pub(crate) fn source(
    source: IteratorSource,
    item_type: Type,
    span: Span,
) -> IteratorPlan {
    IteratorPlan {
        source,
        stages: Vec::new(),
        item_type,
        item_owned: false,
        size_hint: IteratorSizeHint::Exact,
        span,
    }
}

/// Appends a mapper and changes the pipeline element type without allocating an adapter.
pub(crate) fn map(
    plan: &mut IteratorPlan,
    callback: Expr,
    callback_method: usize,
    output_type: Type,
) {
    let input_type = plan.item_type;
    let input_owned = plan.item_owned;
    plan.stages.push(IteratorStage::Map {
        callback,
        callback_method,
        input_type,
        output_type,
        input_owned,
    });
    plan.item_type = output_type;
    plan.item_owned = output_type.is_object();
}

/// Appends a lazy predicate; any filter makes the lower size bound zero.
pub(crate) fn filter(plan: &mut IteratorPlan, callback: Expr, callback_method: usize) {
    let item_type = plan.item_type;
    plan.stages.push(IteratorStage::Filter { callback, callback_method, item_type });
    plan.size_hint = IteratorSizeHint::Bounded;
}

/// Limits runtime length; exact input stays exact after evaluating the count.
/// Negative counts are rejected by the backend.
pub(crate) fn take(plan: &mut IteratorPlan, count: Expr) {
    plan.stages.push(IteratorStage::Take { count });
}

/// Adds an inline Pair view while preserving the ownership of the input item in the plan.
pub(crate) fn enumerate(plan: &mut IteratorPlan, pair_class: usize) {
    let input_type = plan.item_type;
    let input_owned = plan.item_owned;
    plan.stages.push(IteratorStage::Enumerate { pair_class, input_type, input_owned });
    plan.item_type = Type::Borrow { class: pair_class, mutable: false };
    plan.item_owned = false;
}

/// Zips two finite pipelines to the shorter one, building only a stack Pair value per item.
pub(crate) fn zip(plan: &mut IteratorPlan, right: IteratorPlan, pair_class: usize) {
    let left_type = plan.item_type;
    let right_type = right.item_type;
    if right.size_hint == IteratorSizeHint::Bounded {
        plan.size_hint = IteratorSizeHint::Bounded;
    }
    plan.stages.push(IteratorStage::Zip {
        right: Box::new(right),
        pair_class,
        left_type,
        right_type,
    });
    plan.item_type = Type::Borrow { class: pair_class, mutable: false };
    plan.item_owned = false;
}

/// Borrowed source receivers remain live only through the terminal consumer.
pub(crate) fn borrowed_receiver(place: crate::ir::Place) -> IteratorReceiver {
    IteratorReceiver::Borrowed(place)
}

/// A borrowed temporary receiver remains live through the terminal consumer.
pub(crate) fn borrowed_expression(expr: Expr) -> IteratorReceiver {
    IteratorReceiver::BorrowedExpr(Box::new(expr))
}

/// A temporary collection is kept owned by the terminal and released on every exit.
pub(crate) fn owned_receiver(expr: Expr) -> IteratorReceiver {
    IteratorReceiver::Owned(Box::new(expr))
}
