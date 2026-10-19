use crate::ir::{self as t, NativeMethod as M};
use super::support::{native_function, replace_method_ids, replace_placeholders, result_id, variant_id};

pub(super) fn implementation(program: &t::Program, native_method_ids: &mut Option<t::NativeMethodIndex>) -> String {
    let abs_int = native_function(program, |method| matches!(method, M::MathAbsInt));
    let abs_int_value = format!("ncr_v_{}", abs_int.params.first().expect("Math.absInt value parameter").0);
    let abs_long = native_function(program, |method| matches!(method, M::MathAbsLong));
    let abs_long_value = format!("ncr_v_{}", abs_long.params.first().expect("Math.absLong value parameter").0);
    let mut body = include_str!("../../stdlib/math.c.in").to_owned();
    body = replace_placeholders(body, &[
        ("@ABS_INT_RESULT@", result_id(abs_int).to_string()),
        ("@ABS_INT_OK_TAG@", variant_id(program, result_id(abs_int), "Ok").to_string()),
        ("@ABS_INT_VALUE@", abs_int_value),
        ("@ABS_LONG_RESULT@", result_id(abs_long).to_string()),
        ("@ABS_LONG_OK_TAG@", variant_id(program, result_id(abs_long), "Ok").to_string()),
        ("@MATH_INT_RESULT@", result_id(abs_int).to_string()),
        ("@MATH_INT_OK_TAG@", variant_id(program, result_id(abs_int), "Ok").to_string()),
        ("@MATH_LONG_RESULT@", result_id(abs_long).to_string()),
        ("@MATH_LONG_OK_TAG@", variant_id(program, result_id(abs_long), "Ok").to_string()),
        ("@ABS_LONG_VALUE@", abs_long_value),
    ]);
    let body = replace_method_ids(body, program, None, native_method_ids);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded Math placeholder");
    body
}
