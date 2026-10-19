use crate::ir::{self as t, NativeMethod as M};
use super::support::{
    native_function_if, replace_method_ids, replace_placeholders, result_id,
    text_class, variant_id,
};

pub(super) fn implementation(program: &t::Program, native_method_ids: &mut Option<t::NativeMethodIndex>) -> Option<String> {
    let get = native_function_if(program, |method| matches!(method, M::ArgsGet))?;
    let text = text_class(program).expect("Args requires Text");
    let result = result_id(get);
    let body = replace_placeholders(include_str!("../../stdlib/args.c.in").to_owned(), &[
        ("@TEXT_ID@", text.to_string()),
        ("@TEXT_MAKE@", format!("ncr_text_from_bytes_{text}")),
        ("@RESULT_TEXT@", result.to_string()),
        ("@TEXT_RESULT_OK_TAG@", variant_id(program, result, "Ok").to_string()),
        ("@ERROR_TEXT_RESULT@", format!("ncr_result_error_{result}")),
        ("@ARGS_GET_INDEX@", format!("ncr_v_{}", get.params[0].0)),
    ]);
    let body = replace_method_ids(body, program, None, native_method_ids);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded Args placeholder");
    Some(body)
}
