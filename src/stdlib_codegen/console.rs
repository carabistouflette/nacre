use crate::ir::{self as t, NativeMethod as M, Type};
use super::support::{
    native_function_if, replace_method_ids, replace_placeholders, result_id,
    text_class, variant, variant_id,
};

pub(super) fn implementation(program: &t::Program) -> Option<String> {
    let read_line = native_function_if(program, |method| matches!(method, M::ConsoleReadLine))?;
    let text = text_class(program).expect("Console requires Text");
    let result = result_id(read_line);
    let (ok, ok_variant) = variant(program, result, "Ok");
    let option = match ok_variant.fields.as_slice() {
        [Type::Object(class)] => *class,
        _ => unreachable!("readLine Ok payload is Option<Text>"),
    };
    let (_, some_variant) = variant(program, option, "Some");
    assert!(matches!(some_variant.fields.as_slice(), [Type::Object(id)] if *id == text),
        "Option Some payload must own Text");
    let mut out = include_str!("../../stdlib/io_buffer.c.in")
        .replace("@IO_RESERVE@", "ncr_console_reserve");
    out.push('\n');
    let body = replace_placeholders(include_str!("../../stdlib/console.c.in").to_owned(), &[
        ("@TEXT_ID@", text.to_string()),
        ("@TEXT_TAKE@", format!("ncr_text_take_bytes_{text}")),
        ("@IO_RESERVE@", "ncr_console_reserve".to_owned()),
        ("@RESULT_READLINE@", result.to_string()),
        ("@READLINE_OK_TAG@", ok.to_string()),
        ("@ERROR_READLINE@", format!("ncr_result_error_{result}")),
        ("@OPTION_TEXT@", option.to_string()),
        ("@OPTION_NONE_TAG@", variant_id(program, option, "None").to_string()),
        ("@OPTION_SOME_TAG@", variant_id(program, option, "Some").to_string()),
    ]);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!body.contains('@'), "unexpanded Console placeholder");
    out.push_str(&body);
    Some(out)
}
