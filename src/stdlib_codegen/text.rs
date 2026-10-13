use crate::ir::{self as t, NativeMethod as M, Type};
use super::support::{
    native_function, replace_placeholders, result_id, variant_id, TEXT_INLINE_CAPACITY,
};
fn text_method_placeholders(body: String, program: &t::Program, text: usize) -> String {
    let parse = native_function(program, |method| matches!(method, M::TextParseInt));
    let parse_long = native_function(program, |method| matches!(method, M::TextParseLong));
    let parse_double = native_function(program, |method| matches!(method, M::TextParseDouble));
    let slice_bytes = native_function(program, |method| matches!(method, M::TextSliceBytes));
    let slice = native_function(program, |method| matches!(method, M::TextSlice));
    let split = native_function(program, |method| matches!(method, M::TextSplit));
    let list_class = match split.return_type {
        Type::Object(class) => class,
        _ => unreachable!("Text.split returns a concrete List<Text>"),
    };
    let list_element = match program.classes[list_class].native {
        Some(t::NativeClass::List(element)) => element,
        _ => unreachable!("Text.split specializes its result to native List<Text>"),
    };
    assert_eq!(list_element, Type::Object(text), "Text.split list element is Text");
    let list_new = program.functions.iter().find(|function| {
        function.receiver == Some(list_class) && matches!(function.native, Some(M::ListNew))
    }).expect("Text.split List<Text> constructor");
    let list_add = program.functions.iter().find(|function| {
        function.receiver == Some(list_class) && matches!(function.native, Some(M::ListAdd))
    }).expect("Text.split List<Text>.add");
    let parse_id = result_id(parse);
    let parse_long_id = result_id(parse_long);
    let parse_double_id = result_id(parse_double);
    let slice_bytes_id = result_id(slice_bytes);
    let slice_id = result_id(slice);
    replace_placeholders(body, &[
        ("@TEXT_INIT@", format!("ncr_text_init_{text}")),
        ("@PARSE_RESULT@", parse_id.to_string()),
        ("@PARSE_OK_TAG@", variant_id(program, parse_id, "Ok").to_string()),
        ("@ERROR_PARSE_RESULT@", format!("ncr_result_error_{parse_id}")),
        ("@PARSE_LONG_RESULT@", parse_long_id.to_string()),
        ("@PARSE_LONG_OK_TAG@", variant_id(program, parse_long_id, "Ok").to_string()),
        ("@ERROR_PARSE_LONG_RESULT@", format!("ncr_result_error_{parse_long_id}")),
        ("@PARSE_DOUBLE_RESULT@", parse_double_id.to_string()),
        ("@PARSE_DOUBLE_OK_TAG@", variant_id(program, parse_double_id, "Ok").to_string()),
        ("@ERROR_PARSE_DOUBLE_RESULT@", format!("ncr_result_error_{parse_double_id}")),
        ("@SLICE_BYTES_RESULT@", slice_bytes_id.to_string()),
        ("@SLICE_BYTES_OK_TAG@", variant_id(program, slice_bytes_id, "Ok").to_string()),
        ("@ERROR_SLICE_BYTES_RESULT@", format!("ncr_result_error_{slice_bytes_id}")),
        ("@SLICE_RESULT@", slice_id.to_string()),
        ("@SLICE_OK_TAG@", variant_id(program, slice_id, "Ok").to_string()),
        ("@ERROR_SLICE_RESULT@", format!("ncr_result_error_{slice_id}")),
        ("@TEXT_MAKE@", format!("ncr_text_from_bytes_{text}")),
        ("@TEXT_TAKE@", format!("ncr_text_take_bytes_{text}")),
        ("@LIST_TYPE@", crate::codegen::ctype(program, split.return_type)),
        ("@LIST_NEW@", format!("ncr_fn_{}", list_new.id)),
        ("@LIST_ADD@", format!("ncr_fn_{}", list_add.id)),
    ])
}

pub(super) fn fields() -> String {
    format!(
        "    unsigned char *bytes;\n    int32_t length;\n    int32_t capacity;\n    unsigned char inline_storage[{TEXT_INLINE_CAPACITY}];\n"
    )
}

pub(super) fn drop_fields() -> String {
    "    if (p->bytes != NULL && p->bytes != p->inline_storage) { ncr_deallocate(p->bytes); }\n".to_owned()
}

pub(super) fn implementation(program: &t::Program, text: usize) -> String {
    text_method_placeholders(include_str!("../../stdlib/text.c.in").to_owned(), program, text)
}
