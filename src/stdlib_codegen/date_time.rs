use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    required_first_native_method, required_indexed_native_method, replace_method_ids,
    replace_placeholders, result_id, text_class, variant_id,
};


pub(super) fn is_used(program: &t::Program) -> bool {
    program.classes.iter().any(|class| matches!(class.native, Some(C::DateTime)))
}

pub(super) fn common_helpers(program: &t::Program) -> String {
    let text = text_class(program).expect("temporal APIs require native Text");
    replace_placeholders(include_str!("../../stdlib/date_time_common.c.in").to_owned(), &[
        ("@DATE_TIME_TEXT_ID@", text.to_string()),
        ("@DATE_TIME_TEXT_TAKE@", format!("ncr_text_take_bytes_{text}")),
    ])
}

pub(super) fn implementation(program: &t::Program, native_method_ids: &mut Option<t::NativeMethodIndex>) -> String {
    let from_parts = required_first_native_method(program, native_method_ids, M::DateTimeFromParts);
    let parts = required_first_native_method(program, native_method_ids, M::DateTimeParts);
    let format = required_first_native_method(program, native_method_ids, M::DateTimeFormat);
    let parse = required_first_native_method(program, native_method_ids, M::DateTimeParse);
    let days_in_month = required_first_native_method(program, native_method_ids, M::DateTimeDaysInMonth);
    let add_calendar = required_first_native_method(program, native_method_ids, M::DateTimeAddCalendar);
    let duration = required_first_native_method(program, native_method_ids, M::DateTimeDurationMillis);

    let text = text_class(program).expect("DateTime APIs require native Text");
    let fields = program.classes.iter().find(|class| class.name == "DateFields")
        .expect("DateTime.parts result DateFields is in the trusted prelude").id;
    let list = program.classes.iter().find(|class| {
        matches!(class.native, Some(C::List(Type::Object(element))) if element == text)
    }).expect("DateTime.availableZones requires List<Text>").id;
    let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
    let list_new = required_indexed_native_method(program, method_ids, Some(list), M::ListNew);
    let list_add = required_indexed_native_method(program, method_ids, Some(list), M::ListAdd);

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
    let body = replace_method_ids(body, program, None, native_method_ids);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded DateTime placeholder");
    body
}
