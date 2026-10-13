use crate::ir::{self as t, NativeMethod as M};
use super::support::{native_function, replace_placeholders, result_id, variant_id};

fn text_method_placeholders(body: String, program: &t::Program, text: usize) -> String {
    let parse = native_function(program, |method| matches!(method, M::TextParseInt));
    let slice = native_function(program, |method| matches!(method, M::TextSliceBytes));
    let parse_id = result_id(parse);
    let slice_id = result_id(slice);
    replace_placeholders(body, &[
        ("@PARSE_RESULT@", parse_id.to_string()),
        ("@PARSE_OK_TAG@", variant_id(program, parse_id, "Ok").to_string()),
        ("@ERROR_PARSE_RESULT@", format!("ncr_io_error_{parse_id}")),
        ("@SLICE_RESULT@", slice_id.to_string()),
        ("@SLICE_OK_TAG@", variant_id(program, slice_id, "Ok").to_string()),
        ("@ERROR_SLICE_RESULT@", format!("ncr_io_error_{slice_id}")),
        ("@TEXT_MAKE@", format!("ncr_io_text_from_bytes_{text}")),
    ])
}

pub(super) fn fields() -> String {
    "    unsigned char *bytes;\n    int32_t length;\n    int32_t capacity;\n".to_owned()
}

pub(super) fn drop_fields() -> String {
    "    ncr_deallocate(p->bytes);\n".to_owned()
}

pub(super) fn implementation(program: &t::Program, text: usize) -> String {
    text_method_placeholders(include_str!("../../stdlib/text.c.in").to_owned(), program, text)
}
