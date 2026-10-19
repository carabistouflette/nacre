use crate::ir::{self as t, NativeMethod as M, Type};
use super::support::{
    native_function, native_function_if, replace_method_ids, replace_placeholders, result_id,
    template_section, text_class, variant,
};

fn input_id(function: &t::Function, buffer: usize) -> usize {
    match function.params.first() {
        Some((id, Type::Borrow { class, mutable: false })) if *class == buffer => *id,
        _ => unreachable!("compression APIs take a shared ByteBuffer borrow"),
    }
}

fn second_id(function: &t::Function, expected: Type) -> usize {
    match function.params.get(1) {
        Some((id, ty)) if *ty == expected => *id,
        _ => unreachable!("compression API parameter type is compiler-owned"),
    }
}


const TEMPLATE: &str = include_str!("../../stdlib/compression.c.in");

pub(super) fn implementation(program: &t::Program, native_method_ids: &mut Option<t::NativeMethodIndex>) -> String {
    let compression = native_function_if(program, |method| method == M::CompressionDeflate).is_some();
    let checksums = native_function_if(program, |method| method == M::ChecksumsCrc32).is_some();
    if !compression && !checksums {
        return String::new();
    }
    let buffer = program.classes.iter()
        .find(|class| matches!(class.native, Some(t::NativeClass::ByteBuffer)))
        .expect("Compression requires the native ByteBuffer class").id;
    let mut body = template_section(TEMPLATE,
        "/* COMPRESSION_GROUP common */", "/* COMPRESSION_GROUP_END common */").to_owned();
    if compression {
        body.push_str(&compression_body(program, buffer));
    }
    if checksums {
        body.push_str(&checksum_body(program, buffer));
    }
    let body = replace_method_ids(body, program, None, native_method_ids);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded Compression placeholder");
    body
}

fn compression_body(program: &t::Program, buffer: usize) -> String {
    let deflate = native_function(program, |method| method == M::CompressionDeflate);
    let inflate = native_function(program, |method| method == M::CompressionInflate);
    let gzip = native_function(program, |method| method == M::CompressionGzip);
    let gunzip = native_function(program, |method| method == M::CompressionGunzip);

    assert_eq!(deflate.return_type, inflate.return_type);
    assert_eq!(deflate.return_type, gzip.return_type);
    assert_eq!(deflate.return_type, gunzip.return_type);
    let result = result_id(deflate);
    for function in [inflate, gzip, gunzip] {
        assert_eq!(result_id(function), result, "compression methods share Result<ByteBuffer, Text>");
    }
    let (ok_tag, ok) = variant(program, result, "Ok");
    assert!(matches!(ok.fields.as_slice(), [Type::Object(class)] if *class == buffer));
    let (_, error) = variant(program, result, "Err");
    assert!(matches!(error.fields.as_slice(), [Type::Object(class)] if Some(*class) == text_class(program)));

    assert_eq!(deflate.params.len(), 2);
    assert_eq!(gzip.params.len(), 2);
    assert_eq!(inflate.params.len(), 2);
    assert_eq!(gunzip.params.len(), 2);
    let deflate_input = input_id(deflate, buffer);
    let deflate_level = second_id(deflate, Type::Int);
    let inflate_input = input_id(inflate, buffer);
    let inflate_max = second_id(inflate, Type::Long);
    let gzip_input = input_id(gzip, buffer);
    let gzip_level = second_id(gzip, Type::Int);
    let gunzip_input = input_id(gunzip, buffer);
    let gunzip_max = second_id(gunzip, Type::Long);

    replace_placeholders(template_section(TEMPLATE,
        "/* COMPRESSION_GROUP compression */", "/* COMPRESSION_GROUP_END compression */").to_owned(), &[
        ("@BUFFER_TYPE@", crate::c_repr::ctype(program, Type::Object(buffer)).into_owned()),
        ("@RESULT_TYPE@", crate::c_repr::ctype(program, deflate.return_type).into_owned()),
        ("@RESULT_CLASS@", result.to_string()),
        ("@RESULT_OK_TAG@", ok_tag.to_string()),
        ("@BUFFER_CLASS_ID@", buffer.to_string()),
        ("@DEFLATE_INPUT_ID@", deflate_input.to_string()),
        ("@DEFLATE_LEVEL_ID@", deflate_level.to_string()),
        ("@INFLATE_INPUT_ID@", inflate_input.to_string()),
        ("@INFLATE_MAX_ID@", inflate_max.to_string()),
        ("@GZIP_INPUT_ID@", gzip_input.to_string()),
        ("@GZIP_LEVEL_ID@", gzip_level.to_string()),
        ("@GUNZIP_INPUT_ID@", gunzip_input.to_string()),
        ("@GUNZIP_MAX_ID@", gunzip_max.to_string()),
    ])
}

fn checksum_body(program: &t::Program, buffer: usize) -> String {
    let crc32 = native_function(program, |method| method == M::ChecksumsCrc32);
    let adler32 = native_function(program, |method| method == M::ChecksumsAdler32);
    assert_eq!(crc32.params.len(), 1);
    assert_eq!(adler32.params.len(), 1);
    assert_eq!(crc32.return_type, Type::Long);
    assert_eq!(adler32.return_type, Type::Long);
    replace_placeholders(template_section(TEMPLATE,
        "/* COMPRESSION_GROUP checksums */", "/* COMPRESSION_GROUP_END checksums */").to_owned(), &[
        ("@BUFFER_TYPE@", crate::c_repr::ctype(program, Type::Object(buffer)).into_owned()),
        ("@CRC32_INPUT_ID@", input_id(crc32, buffer).to_string()),
        ("@ADLER32_INPUT_ID@", input_id(adler32, buffer).to_string()),
    ])
}
