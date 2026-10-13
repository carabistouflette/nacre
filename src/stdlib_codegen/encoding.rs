use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    marker, native_function, replace_method_ids, replace_placeholders, result_id, text_class,
    variant, TEXT_INLINE_CAPACITY,
};

fn function(program: &t::Program, method: M) -> &t::Function {
    native_function(program, |candidate| candidate == method)
}

fn byte_buffer_id(program: &t::Program) -> usize {
    program.classes.iter()
        .find(|class| matches!(class.native, Some(C::ByteBuffer)))
        .map(|class| class.id)
        .expect("Encoding requires ByteBuffer")
}

fn parameter(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params[index].0)
}

fn signature(program: &t::Program, method: M) -> String {
    let function = function(program, method);
    assert_eq!(function.native, Some(method), "Encoding signature has the expected native method");
    assert!(function.receiver.is_none(), "Encoding methods are static");
    let mut parameters = function.params.iter()
        .map(|(id, ty)| format!("{} ncr_v_{id}", ctype(program, *ty)))
        .collect::<Vec<_>>();
    parameters.extend(["size_t ncr_line".to_owned(), "size_t ncr_column".to_owned()]);
    format!("static {} {}({})", ctype(program, function.return_type), marker(method), parameters.join(", "))
}

fn assert_borrow(function: &t::Function, index: usize, expected: usize) {
    assert!(matches!(function.params.get(index),
        Some((_, Type::Borrow { class, mutable: false })) if *class == expected),
        "Encoding takes the specified shared borrow");
}

fn result(program: &t::Program, function: &t::Function, ok_type: Type, text: usize) -> (usize, usize) {
    let result = result_id(function);
    let (ok_tag, ok) = variant(program, result, "Ok");
    assert_eq!(ok.fields.as_slice(), &[ok_type], "Encoding Result has its specialized Ok payload");
    let (_, error) = variant(program, result, "Err");
    assert!(matches!(error.fields.as_slice(), [Type::Object(class)] if *class == text),
        "Encoding Result owns an error Text");
    (result, ok_tag)
}

pub(super) fn implementation(program: &t::Program, class: usize) -> String {
    assert!(matches!(program.classes[class].native, Some(C::Encoding)),
        "Encoding implementation belongs to its native class");
    let text = text_class(program).expect("Encoding requires Text");
    let buffer = byte_buffer_id(program);

    let hex_encode = function(program, M::EncodingHexEncode);
    let hex_decode = function(program, M::EncodingHexDecode);
    let base64_encode = function(program, M::EncodingBase64Encode);
    let base64_decode = function(program, M::EncodingBase64Decode);
    let base64_url_encode = function(program, M::EncodingBase64UrlEncode);
    let base64_url_decode = function(program, M::EncodingBase64UrlDecode);
    let utf8_encode = function(program, M::EncodingUtf8Encode);
    let utf8_decode = function(program, M::EncodingUtf8Decode);

    for method in [hex_encode, base64_encode, base64_url_encode] {
        assert_borrow(method, 0, buffer);
        assert_eq!(method.return_type, Type::Object(text), "Encoding encoders return owned Text");
    }
    for method in [hex_decode, base64_decode, base64_url_decode] {
        assert_borrow(method, 0, text);
    }
    assert_borrow(utf8_encode, 0, text);
    assert_eq!(utf8_encode.return_type, Type::Object(buffer), "Encoding.utf8Encode returns an owned ByteBuffer");
    assert_borrow(utf8_decode, 0, buffer);

    let (hex_result, hex_ok) = result(program, hex_decode, Type::Object(buffer), text);
    let (base64_result, base64_ok) = result(program, base64_decode, Type::Object(buffer), text);
    let (base64_url_result, base64_url_ok) = result(program, base64_url_decode, Type::Object(buffer), text);
    assert_eq!(base64_result, base64_url_result,
        "base64 decoders share their specialized Result<ByteBuffer, Text>");
    assert_eq!(base64_ok, base64_url_ok, "base64 decoders share the Result Ok tag");
    let (utf8_result, utf8_ok) = result(program, utf8_decode, Type::Object(text), text);

    let mut body = include_str!("../../stdlib/encoding.c.in").to_owned();
    body = replace_placeholders(body, &[
        ("@TEXT_ID@", text.to_string()),
        ("@BUFFER_ID@", buffer.to_string()),
        ("@TEXT_INLINE_CAPACITY@", TEXT_INLINE_CAPACITY.to_string()),
        ("@TEXT_INIT@", format!("ncr_text_init_{text}")),
        ("@TEXT_TAKE_BYTES@", format!("ncr_text_take_bytes_{text}")),
        ("@BUFFER_RESERVE@", format!("ncr_byte_buffer_reserve_{buffer}")),
        ("@HEX_ENCODE_SIGNATURE@", signature(program, M::EncodingHexEncode)),
        ("@HEX_DECODE_SIGNATURE@", signature(program, M::EncodingHexDecode)),
        ("@BASE64_ENCODE_SIGNATURE@", signature(program, M::EncodingBase64Encode)),
        ("@BASE64_DECODE_SIGNATURE@", signature(program, M::EncodingBase64Decode)),
        ("@BASE64_URL_ENCODE_SIGNATURE@", signature(program, M::EncodingBase64UrlEncode)),
        ("@BASE64_URL_DECODE_SIGNATURE@", signature(program, M::EncodingBase64UrlDecode)),
        ("@UTF8_ENCODE_SIGNATURE@", signature(program, M::EncodingUtf8Encode)),
        ("@UTF8_DECODE_SIGNATURE@", signature(program, M::EncodingUtf8Decode)),
        ("@HEX_ENCODE_BYTES@", parameter(hex_encode, 0)),
        ("@HEX_DECODE_TEXT@", parameter(hex_decode, 0)),
        ("@BASE64_ENCODE_BYTES@", parameter(base64_encode, 0)),
        ("@BASE64_DECODE_TEXT@", parameter(base64_decode, 0)),
        ("@BASE64_URL_ENCODE_BYTES@", parameter(base64_url_encode, 0)),
        ("@BASE64_URL_DECODE_TEXT@", parameter(base64_url_decode, 0)),
        ("@UTF8_ENCODE_TEXT@", parameter(utf8_encode, 0)),
        ("@UTF8_DECODE_BYTES@", parameter(utf8_decode, 0)),
        ("@HEX_DECODE_RESULT@", hex_result.to_string()),
        ("@HEX_DECODE_OK_TAG@", hex_ok.to_string()),
        ("@HEX_DECODE_ERROR@", format!("ncr_result_error_{hex_result}")),
        ("@BASE64_DECODE_RESULT@", base64_result.to_string()),
        ("@BASE64_DECODE_OK_TAG@", base64_ok.to_string()),
        ("@BASE64_DECODE_ERROR@", format!("ncr_result_error_{base64_result}")),
        ("@UTF8_DECODE_RESULT@", utf8_result.to_string()),
        ("@UTF8_DECODE_OK_TAG@", utf8_ok.to_string()),
        ("@UTF8_DECODE_ERROR@", format!("ncr_result_error_{utf8_result}")),
    ]);
    body = body.replace("@ID@", &class.to_string());
    body = replace_method_ids(body, program, None);
    debug_assert!(!body.contains('@'), "unexpanded Encoding placeholder");
    body
}
