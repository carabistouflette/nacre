use crate::ir::{self as t, NativeMethod as M};
use super::support::{
    replace_method_ids, replace_placeholders, required_first_native_method, result_id, variant_id,
};

pub(super) fn implementation(program: &t::Program, native_method_ids: &mut Option<t::NativeMethodIndex>) -> String {
    let monotonic = required_first_native_method(program, native_method_ids, M::TimeMonotonicNanos);
    let monotonic_result = result_id(monotonic);
    let unix_millis = required_first_native_method(program, native_method_ids, M::TimeUnixMillis);
    let sleep_millis = required_first_native_method(program, native_method_ids, M::TimeSleepMillis);
    let deadline = required_first_native_method(program, native_method_ids, M::TimeDeadlineAfterMillis);
    let unix_result = result_id(unix_millis);
    let sleep_result = result_id(sleep_millis);
    let deadline_result = result_id(deadline);

    let mut body = include_str!("../../stdlib/time.c.in").to_owned();
    body = replace_placeholders(body, &[
        ("@TIME_MONOTONIC_RESULT@", monotonic_result.to_string()),
        ("@TIME_MONOTONIC_OK_TAG@", variant_id(program, monotonic_result, "Ok").to_string()),
        ("@TIME_UNIX_RESULT@", unix_result.to_string()),
        ("@TIME_UNIX_OK_TAG@", variant_id(program, unix_result, "Ok").to_string()),
        ("@TIME_SLEEP_RESULT@", sleep_result.to_string()),
        ("@TIME_SLEEP_OK_TAG@", variant_id(program, sleep_result, "Ok").to_string()),
        ("@TIME_DEADLINE_RESULT@", deadline_result.to_string()),
        ("@TIME_DEADLINE_OK_TAG@", variant_id(program, deadline_result, "Ok").to_string()),
    ]);
    let body = replace_method_ids(body, program, None, native_method_ids);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded Time placeholder");
    body
}
