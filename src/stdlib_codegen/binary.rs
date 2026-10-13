use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{replace_method_ids, replace_placeholders, result_id, text_class, variant};

fn buffer_class(program: &t::Program) -> usize {
    program.classes.iter().find(|class| matches!(class.native, Some(C::ByteBuffer)))
        .expect("ByteBuffer is in the trusted prelude").id
}

fn text_class_id(program: &t::Program) -> usize {
    text_class(program).expect("Text is in the trusted prelude")
}

fn method(program: &t::Program, receiver: usize, native: M) -> &t::Function {
    program.functions.iter().find(|function| {
        function.receiver == Some(receiver) && function.native == Some(native)
    }).expect("compiler-owned binary method is present")
}

fn static_method(program: &t::Program, native: M) -> &t::Function {
    program.functions.iter().find(|function| {
        function.receiver.is_none() && function.native == Some(native)
    }).expect("compiler-owned binary static method is present")
}

fn param(function: &t::Function, index: usize) -> String {
    function.params[index].0.to_string()
}

fn add(substitutions: &mut Vec<(&'static str, String)>, key: &'static str, value: impl ToString) {
    substitutions.push((key, value.to_string()));
}

fn add_result(
    substitutions: &mut Vec<(&'static str, String)>,
    program: &t::Program,
    function: &t::Function,
    result_key: &'static str,
    ok_key: &'static str,
    error_key: &'static str,
) {
    let id = result_id(function);
    let (_, ok_variant) = variant(program, id, "Ok");
    let (_, error_variant) = variant(program, id, "Err");
    assert_eq!(ok_variant.fields.len(), 1, "binary Result Ok has one payload");
    assert!(matches!(error_variant.fields.as_slice(), [Type::Object(text)] if *text == text_class_id(program)),
        "binary Result Err carries Text");
    add(substitutions, result_key, id);
    add(substitutions, ok_key, ok);
    add(substitutions, error_key, format!("ncr_result_error_{id}"));
}

pub(super) fn fields(program: &t::Program, class: C, _class_id: usize) -> String {
    let buffer = buffer_class(program);
    match class {
        C::BinaryReader => format!("    ncr_obj_{buffer} *bytes;\n    int64_t position;\n    bool little_endian;\n"),
        C::BinaryWriter => format!("    ncr_obj_{buffer} *buffer;\n    bool little_endian;\n"),
        _ => unreachable!("binary fields requested for another native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C, _class_id: usize) -> String {
    let buffer = buffer_class(program);
    match class {
        C::BinaryReader => format!("    ncr_drop_{buffer}(p->bytes);\n"),
        C::BinaryWriter => format!("    if (p->buffer != NULL) {{ ncr_drop_{buffer}(p->buffer); }}\n"),
        _ => unreachable!("binary drop fields requested for another native class"),
    }
}

fn group(source: &str, name: &str) -> String {
    let start = format!("/* BINARY_GROUP_BEGIN {name} */\n");
    let end = format!("/* BINARY_GROUP_END {name} */");
    let body = source.split_once(&start).expect("binary template group begins").1;
    body.split_once(&end).expect("binary template group ends").0.to_owned()
}

pub(super) fn implementation(program: &t::Program, class_id: usize, native: C) -> String {
    let buffer = buffer_class(program);
    let mut substitutions = vec![
        ("@BUFFER_ID@", buffer.to_string()),
    ];
    let mut body = match native {
        C::BinaryReader => {
            let reader = class_id;
            let new = method(program, reader, M::BinaryReaderNew);
            let seek = method(program, reader, M::BinaryReaderSeek);
            let read_byte = method(program, reader, M::BinaryReaderReadByte);
            let read_int16 = method(program, reader, M::BinaryReaderReadInt16);
            let read_int32 = method(program, reader, M::BinaryReaderReadInt32);
            let read_int64 = method(program, reader, M::BinaryReaderReadInt64);
            let read_float32 = method(program, reader, M::BinaryReaderReadFloat32);
            let read_double = method(program, reader, M::BinaryReaderReadDouble);
            let read_bytes = method(program, reader, M::BinaryReaderReadBytes);
            let read_utf8 = method(program, reader, M::BinaryReaderReadUtf8);
            add(&mut substitutions, "@READER_ID@", reader);
            add(&mut substitutions, "@READER_BYTES_ID@", param(new, 0));
            add(&mut substitutions, "@READER_ENDIAN_ID@", param(new, 1));
            add(&mut substitutions, "@READER_SEEK_POSITION_ID@", param(seek, 0));
            add(&mut substitutions, "@READER_BYTES_LENGTH_ID@", param(read_bytes, 0));
            add(&mut substitutions, "@READER_UTF8_LENGTH_ID@", param(read_utf8, 0));
            add_result(&mut substitutions, program, seek, "@READER_SEEK_RESULT@", "@READER_SEEK_OK_TAG@", "@READER_SEEK_ERROR@");
            add_result(&mut substitutions, program, read_byte, "@READER_BYTE_RESULT@", "@READER_BYTE_OK_TAG@", "@READER_BYTE_ERROR@");
            add_result(&mut substitutions, program, read_int16, "@READER_INT16_RESULT@", "@READER_INT16_OK_TAG@", "@READER_INT16_ERROR@");
            add_result(&mut substitutions, program, read_int32, "@READER_INT32_RESULT@", "@READER_INT32_OK_TAG@", "@READER_INT32_ERROR@");
            add_result(&mut substitutions, program, read_int64, "@READER_INT64_RESULT@", "@READER_INT64_OK_TAG@", "@READER_INT64_ERROR@");
            add_result(&mut substitutions, program, read_float32, "@READER_FLOAT32_RESULT@", "@READER_FLOAT32_OK_TAG@", "@READER_FLOAT32_ERROR@");
            add_result(&mut substitutions, program, read_double, "@READER_DOUBLE_RESULT@", "@READER_DOUBLE_OK_TAG@", "@READER_DOUBLE_ERROR@");
            add_result(&mut substitutions, program, read_bytes, "@READER_BYTES_RESULT@", "@READER_BYTES_OK_TAG@", "@READER_BYTES_ERROR@");
            add_result(&mut substitutions, program, read_utf8, "@READER_UTF8_RESULT@", "@READER_UTF8_OK_TAG@", "@READER_UTF8_ERROR@");
            add(&mut substitutions, "@TEXT_ID@", text_class_id(program));
            group(include_str!("../../stdlib/binary.c.in"), "reader")
        }
        C::BinaryWriter => {
            let writer = class_id;
            let new = method(program, writer, M::BinaryWriterNew);
            let write_byte = method(program, writer, M::BinaryWriterWriteByte);
            let write_int16 = method(program, writer, M::BinaryWriterWriteInt16);
            let write_int32 = method(program, writer, M::BinaryWriterWriteInt32);
            let write_int64 = method(program, writer, M::BinaryWriterWriteInt64);
            let write_float32 = method(program, writer, M::BinaryWriterWriteFloat32);
            let write_double = method(program, writer, M::BinaryWriterWriteDouble);
            let write_bytes = method(program, writer, M::BinaryWriterWriteBytes);
            let write_utf8 = method(program, writer, M::BinaryWriterWriteUtf8);
            let finish = static_method(program, M::BinaryWriterFinish);
            add(&mut substitutions, "@WRITER_ID@", writer);
            add(&mut substitutions, "@WRITER_CAPACITY_ID@", param(new, 0));
            add(&mut substitutions, "@WRITER_ENDIAN_ID@", param(new, 1));
            add(&mut substitutions, "@WRITER_WRITE_BYTE_VALUE_ID@", param(write_byte, 0));
            add(&mut substitutions, "@WRITER_WRITE_INT16_VALUE_ID@", param(write_int16, 0));
            add(&mut substitutions, "@WRITER_WRITE_INT32_VALUE_ID@", param(write_int32, 0));
            add(&mut substitutions, "@WRITER_WRITE_INT64_VALUE_ID@", param(write_int64, 0));
            add(&mut substitutions, "@WRITER_WRITE_DOUBLE_VALUE_ID@", param(write_double, 0));
            add(&mut substitutions, "@WRITER_WRITE_FLOAT32_VALUE_ID@", param(write_float32, 0));
            add(&mut substitutions, "@WRITER_WRITE_BYTES_SOURCE_ID@", param(write_bytes, 0));
            add(&mut substitutions, "@WRITER_WRITE_UTF8_TEXT_ID@", param(write_utf8, 0));
            add(&mut substitutions, "@WRITER_FINISH_ID@", param(finish, 0));
            add_result(&mut substitutions, program, write_byte, "@WRITER_RESULT@", "@WRITER_RESULT_OK_TAG@", "@WRITER_RESULT_ERROR@");
            let text = text_class_id(program);
            add(&mut substitutions, "@TEXT_ID@", text);
            group(include_str!("../../stdlib/binary.c.in"), "writer")
        }
        _ => unreachable!("binary implementation requested for another native class"),
    };
    body = replace_placeholders(body, &substitutions);
    body = replace_method_ids(body, program, None);
    debug_assert!(!body.contains('@'), "unexpanded binary template placeholder");
    body
}
