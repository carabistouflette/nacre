use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    native_class_id, native_function, replace_method_ids, replace_placeholders,
    required_indexed_native_method, result_id, text_class, variant_id, TEXT_INLINE_CAPACITY,
};

pub(super) fn is_used(program: &t::Program) -> bool {
    program.classes.iter().any(|class| matches!(class.native, Some(C::Temporal)))
}

pub(super) fn fields() -> String {
    concat!(
        "    int32_t kind;\n",
        "    int32_t calendar;\n",
        "    int32_t era;\n",
        "    int32_t year;\n",
        "    int32_t extended_year;\n",
        "    int32_t month;\n",
        "    int32_t day;\n",
        "    int32_t day_of_year;\n",
        "    int32_t hour;\n",
        "    int32_t minute;\n",
        "    int32_t second;\n",
        "    int32_t nanosecond;\n",
        "    int32_t offset_seconds;\n",
        "    int32_t period_years;\n",
        "    int32_t period_months;\n",
        "    int32_t period_days;\n",
        "    int64_t epoch_second;\n",
        "    int64_t local_second;\n",
        "    char *zone;\n",
        "    int32_t zone_length;\n",
    ).to_owned()
}

pub(super) fn drop_fields() -> String {
    "    if (p->zone != NULL) { ncr_deallocate(p->zone); }\n".to_owned()
}

pub(super) fn implementation(program: &t::Program, native_method_ids: &mut Option<t::NativeMethodIndex>) -> String {
    let instant = native_function(program, |method| matches!(method, M::TemporalInstant));
    let epoch_second = native_function(program, |method| matches!(method, M::TemporalEpochSecond));
    let offset_seconds = native_function(program, |method| matches!(method, M::TemporalOffsetSeconds));
    let fields = native_function(program, |method| matches!(method, M::TemporalFields));
    let format = native_function(program, |method| matches!(method, M::TemporalFormat));

    let temporal = native_class_id(program, |class| matches!(class, C::Temporal));
    let text = text_class(program).expect("Temporal APIs require native Text");
    let fields_class = program.classes.iter().find(|class| class.name == "CalendarFields")
        .expect("Temporal.fields result CalendarFields is in the trusted prelude").id;
    let list = program.classes.iter().find(|class| {
        matches!(class.native, Some(C::List(Type::Object(element))) if element == text)
    }).expect("Temporal.availableCalendars requires List<Text>").id;
    let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
    let list_new = required_indexed_native_method(program, method_ids, Some(list), M::ListNew);
    let list_add = required_indexed_native_method(program, method_ids, Some(list), M::ListAdd);

    let temporal_result = result_id(instant);
    let fields_result = result_id(fields);
    let long_result = result_id(epoch_second);
    let int_result = result_id(offset_seconds);
    let text_result = result_id(format);
    let body = replace_placeholders(include_str!("../../stdlib/temporal.c.in").to_owned(), &[
        ("@TEMPORAL_FORMAT@", format!("ncr_fn_{}", format.id)),
        ("@TEMPORAL_ID@", temporal.to_string()),
        ("@TEMPORAL_TEXT_ID@", text.to_string()),
        ("@TEMPORAL_TEXT_INLINE_CAPACITY@", TEXT_INLINE_CAPACITY.to_string()),
        ("@TEMPORAL_TEXT_INIT@", format!("ncr_text_init_{text}")),
        ("@TEMPORAL_TEXT_TAKE@", format!("ncr_text_take_bytes_{text}")),
        ("@TEMPORAL_FIELDS_ID@", fields_class.to_string()),
        ("@TEMPORAL_LIST_ID@", list.to_string()),
        ("@TEMPORAL_LIST_NEW@", format!("ncr_fn_{}", list_new.id)),
        ("@TEMPORAL_LIST_ADD@", format!("ncr_fn_{}", list_add.id)),
        ("@TEMPORAL_RESULT@", temporal_result.to_string()),
        ("@TEMPORAL_OK_TAG@", variant_id(program, temporal_result, "Ok").to_string()),
        ("@TEMPORAL_FIELDS_RESULT@", fields_result.to_string()),
        ("@TEMPORAL_FIELDS_OK_TAG@", variant_id(program, fields_result, "Ok").to_string()),
        ("@TEMPORAL_LONG_RESULT@", long_result.to_string()),
        ("@TEMPORAL_LONG_OK_TAG@", variant_id(program, long_result, "Ok").to_string()),
        ("@TEMPORAL_INT_RESULT@", int_result.to_string()),
        ("@TEMPORAL_INT_OK_TAG@", variant_id(program, int_result, "Ok").to_string()),
        ("@TEMPORAL_TEXT_RESULT@", text_result.to_string()),
        ("@TEMPORAL_TEXT_OK_TAG@", variant_id(program, text_result, "Ok").to_string()),
    ]);
    let body = replace_method_ids(body, program, Some(temporal), native_method_ids);
    let body = replace_method_ids(body, program, None, native_method_ids);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded Temporal placeholder");
    body
}
