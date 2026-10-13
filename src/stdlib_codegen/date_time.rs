use crate::ir::{self as t, CallTarget, Expr, ExprKind, NativeClass as C, NativeMethod as M, Stmt, Type};
use super::support::{
    native_function, replace_method_ids, replace_placeholders, result_id, text_class, variant_id,
};

fn is_date_time_method(method: M) -> bool {
    matches!(
        method,
        M::DateTimeFromParts
            | M::DateTimeParts
            | M::DateTimeFormat
            | M::DateTimeParse
            | M::DateTimeIsLeapYear
            | M::DateTimeDaysInMonth
            | M::DateTimeAddCalendar
            | M::DateTimeDurationMillis
            | M::DateTimeAvailableZones
    )
}

fn expression_uses_date_time(program: &t::Program, expression: &Expr) -> bool {
    let nested = match &expression.kind {
        ExprKind::Drop(value)
        | ExprKind::Unary { expr: value, .. }
        | ExprKind::Try { value, .. }
        | ExprKind::Cast { value } => expression_uses_date_time(program, value),
        ExprKind::Upcast { value, .. } => expression_uses_date_time(program, value),
        ExprKind::MapSlot { index, .. } => expression_uses_date_time(program, index),
        ExprKind::New { args, .. } | ExprKind::Variant { args, .. } => {
            args.iter().any(|argument| expression_uses_date_time(program, argument))
        }
        ExprKind::Call { target, args, .. } => {
            let is_date_time_call = match target {
                CallTarget::Function(id) => program.functions[*id]
                    .native
                    .is_some_and(is_date_time_method),
                CallTarget::Println => false,
            };
            is_date_time_call
                || args.iter().any(|argument| expression_uses_date_time(program, argument))
        }
        ExprKind::Binary { left, right, .. } => {
            expression_uses_date_time(program, left)
                || expression_uses_date_time(program, right)
        }
        ExprKind::Char(_)
        | ExprKind::Int(_)
        | ExprKind::Long(_)
        | ExprKind::Double(_)
        | ExprKind::Unit
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Read(_)
        | ExprKind::Move(_)
        | ExprKind::Borrow { .. } => false,
    };
    nested
}

fn block_uses_date_time(program: &t::Program, block: &[Stmt]) -> bool {
    block.iter().any(|statement| match statement {
        Stmt::Let { value, .. } => value
            .as_ref()
            .is_some_and(|value| expression_uses_date_time(program, value)),
        Stmt::Assign { value, .. } | Stmt::Expr(value) => {
            expression_uses_date_time(program, value)
        }
        Stmt::If { condition, then_block, else_block } => {
            expression_uses_date_time(program, condition)
                || block_uses_date_time(program, then_block)
                || else_block
                    .as_ref()
                    .is_some_and(|block| block_uses_date_time(program, block))
        }
        Stmt::While { condition, body } => {
            expression_uses_date_time(program, condition)
                || block_uses_date_time(program, body)
        }
        Stmt::Match { value, arms } => {
            expression_uses_date_time(program, value)
                || arms.iter().any(|arm| block_uses_date_time(program, &arm.body))
        }
        Stmt::Return(value) => value
            .as_ref()
            .is_some_and(|value| expression_uses_date_time(program, value)),
        Stmt::Block(block) => block_uses_date_time(program, block),
        Stmt::Break | Stmt::Continue => false,
    })
}

pub(super) fn is_used(program: &t::Program) -> bool {
    program.functions.iter().any(|function| {
        function.native.is_none() && block_uses_date_time(program, &function.body)
    })
}

pub(super) fn implementation(program: &t::Program) -> String {
    let from_parts = native_function(program, |method| matches!(method, M::DateTimeFromParts));
    let parts = native_function(program, |method| matches!(method, M::DateTimeParts));
    let format = native_function(program, |method| matches!(method, M::DateTimeFormat));
    let parse = native_function(program, |method| matches!(method, M::DateTimeParse));
    let days_in_month = native_function(program, |method| matches!(method, M::DateTimeDaysInMonth));
    let add_calendar = native_function(program, |method| matches!(method, M::DateTimeAddCalendar));
    let duration = native_function(program, |method| matches!(method, M::DateTimeDurationMillis));

    let text = text_class(program).expect("DateTime APIs require native Text");
    let fields = program.classes.iter().find(|class| class.name == "DateFields")
        .expect("DateTime.parts result DateFields is in the trusted prelude").id;
    let list = program.classes.iter().find(|class| {
        matches!(class.native, Some(C::List(Type::Object(element))) if element == text)
    }).expect("DateTime.availableZones requires List<Text>").id;
    let list_new = program.functions.iter().find(|function| {
        function.receiver == Some(list) && matches!(function.native, Some(M::ListNew))
    }).expect("DateTime.availableZones List<Text> constructor");
    let list_add = program.functions.iter().find(|function| {
        function.receiver == Some(list) && matches!(function.native, Some(M::ListAdd))
    }).expect("DateTime.availableZones List<Text>.add");

    let from_parts_result = result_id(from_parts);
    let parts_result = result_id(parts);
    let format_result = result_id(format);
    let parse_result = result_id(parse);
    let days_result = result_id(days_in_month);
    let add_result = result_id(add_calendar);
    let duration_result = result_id(duration);

    let body = replace_placeholders(include_str!("../../stdlib/date_time.c.in").to_owned(), &[
        ("@DATE_TIME_FROM_PARTS_RESULT@", from_parts_result.to_string()),
        ("@DATE_TIME_FROM_PARTS_OK_TAG@", variant_id(program, from_parts_result, "Ok").to_string()),
        ("@DATE_TIME_PARTS_RESULT@", parts_result.to_string()),
        ("@DATE_TIME_PARTS_OK_TAG@", variant_id(program, parts_result, "Ok").to_string()),
        ("@DATE_TIME_FIELDS_ID@", fields.to_string()),
        ("@DATE_TIME_FORMAT_RESULT@", format_result.to_string()),
        ("@DATE_TIME_FORMAT_OK_TAG@", variant_id(program, format_result, "Ok").to_string()),
        ("@DATE_TIME_PARSE_RESULT@", parse_result.to_string()),
        ("@DATE_TIME_PARSE_OK_TAG@", variant_id(program, parse_result, "Ok").to_string()),
        ("@DATE_TIME_DAYS_RESULT@", days_result.to_string()),
        ("@DATE_TIME_DAYS_OK_TAG@", variant_id(program, days_result, "Ok").to_string()),
        ("@DATE_TIME_ADD_RESULT@", add_result.to_string()),
        ("@DATE_TIME_ADD_OK_TAG@", variant_id(program, add_result, "Ok").to_string()),
        ("@DATE_TIME_DURATION_RESULT@", duration_result.to_string()),
        ("@DATE_TIME_DURATION_OK_TAG@", variant_id(program, duration_result, "Ok").to_string()),
        ("@DATE_TIME_TEXT_ID@", text.to_string()),
        ("@DATE_TIME_TEXT_TAKE@", format!("ncr_text_take_bytes_{text}")),
        ("@DATE_TIME_LIST_ID@", list.to_string()),
        ("@DATE_TIME_LIST_NEW@", format!("ncr_fn_{}", list_new.id)),
        ("@DATE_TIME_LIST_ADD@", format!("ncr_fn_{}", list_add.id)),
    ]);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!body.contains('@'), "unexpanded DateTime placeholder");
    body
}
