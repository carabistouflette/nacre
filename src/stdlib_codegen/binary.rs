use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    replace_method_ids, replace_placeholders, result_id, text_class, variant,
    required_indexed_native_method,
};

fn buffer_class(program: &t::Program) -> usize {
    program.classes.iter().find(|class| matches!(class.native, Some(C::ByteBuffer)))
        .expect("ByteBuffer is in the trusted prelude").id
}

fn text_class_id(program: &t::Program) -> usize {
    text_class(program).expect("Text is in the trusted prelude")
}

fn method<'a>(
    program: &'a t::Program, method_ids: &t::NativeMethodIndex, receiver: usize, native: M,
) -> &'a t::Function {
    required_indexed_native_method(program, method_ids, Some(receiver), native)
}

fn static_method<'a>(
    program: &'a t::Program, method_ids: &t::NativeMethodIndex, native: M,
) -> &'a t::Function {
    required_indexed_native_method(program, method_ids, None, native)
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
    let (ok, ok_variant) = variant(program, id, "Ok");
    let (_, error_variant) = variant(program, id, "Err");
    assert_eq!(ok_variant.fields.len(), 1, "binary Result Ok has one payload");
    assert!(matches!(error_variant.fields.as_slice(), [Type::Object(text)] if *text == text_class_id(program)),
        "binary Result Err carries Text");
    add(substitutions, result_key, id);
    add(substitutions, ok_key, ok);
    add(substitutions, error_key, format!("ncr_result_error_{id}"));
}

fn interface_class(program: &t::Program, name: &str) -> usize {
    program.classes.iter()
        .find(|class| class.is_interface && class.name == name)
        .expect("ByteSource and ByteSink are in the trusted prelude")
        .id
}

fn interface_method<'a>(program: &'a t::Program, interface: usize, name: &str) -> &'a t::Function {
    let mut matches = program.classes[interface].interface_methods.iter()
        .map(|id| &program.functions[*id])
        .filter(|function| function.name.rsplit_once('.').is_some_and(|(_, method)| method == name));
    let method = matches.next().expect("trusted stream interface declares its method");
    assert!(matches.next().is_none(), "trusted stream interface method is unique");
    method
}

fn add_variant_tag(
    substitutions: &mut Vec<(&'static str, String)>,
    program: &t::Program,
    function: &t::Function,
    variant_name: &str,
    key: &'static str,
) {
    let (tag, _) = variant(program, result_id(function), variant_name);
    add(substitutions, key, tag);
}

fn add_interface_method(
    substitutions: &mut Vec<(&'static str, String)>,
    function: &t::Function,
    slot_key: &'static str,
) {
    add(
        substitutions,
        slot_key,
        function.virtual_slot.expect("stream interface method has a vtable slot"),
    );
}

fn interface_result_tags(program: &t::Program, function: &t::Function) -> (usize, usize) {
    let result = result_id(function);
    let (ok, ok_variant) = variant(program, result, "Ok");
    let (error, error_variant) = variant(program, result, "Err");
    assert!(matches!(ok_variant.fields.as_slice(), [Type::Long] | [Type::Unit]),
        "stream interface Ok carries a count or Unit");
    assert!(matches!(error_variant.fields.as_slice(), [Type::Object(text)] if *text == text_class_id(program)),
        "stream interface Err carries Text");
    (ok, error)
}

pub(super) fn fields(program: &t::Program, class: C, _class_id: usize) -> String {
    let buffer = buffer_class(program);
    let source = interface_class(program, "ByteSource");
    let sink = interface_class(program, "ByteSink");
    match class {
        C::BinaryReader => format!(
            "    ncr_obj_{buffer} *bytes;\n    ncr_obj_{source} source;\n    ncr_obj_{buffer} *stage;\n    int64_t position;\n    int64_t max_bytes;\n    size_t stage_offset;\n    bool little_endian;\n    bool stream_mode;\n    bool source_eof;\n    bool source_failed;\n",
        ),
        C::BinaryWriter => format!(
            "    ncr_obj_{buffer} *buffer;\n    ncr_obj_{sink} sink;\n    int64_t position;\n    int64_t max_bytes;\n    size_t pending_offset;\n    bool little_endian;\n    bool stream_mode;\n",
        ),
        _ => unreachable!("binary fields requested for another native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C, _class_id: usize) -> String {
    let buffer = buffer_class(program);
    let source = interface_class(program, "ByteSource");
    let sink = interface_class(program, "ByteSink");
    match class {
        C::BinaryReader => format!(
            "    if (p->bytes != NULL) {{ ncr_drop_{buffer}(p->bytes); }}\n    if (p->stage != NULL) {{ ncr_drop_{buffer}(p->stage); }}\n    ncr_drop_{source}(p->source);\n",
        ),
        C::BinaryWriter => format!(
            "    if (p->buffer != NULL) {{ ncr_drop_{buffer}(p->buffer); }}\n    ncr_drop_{sink}(p->sink);\n",
        ),
        _ => unreachable!("binary drop fields requested for another native class"),
    }
}

fn group(source: &str, name: &str) -> String {
    let start = format!("/* BINARY_GROUP_BEGIN {name} */\n");
    let end = format!("/* BINARY_GROUP_END {name} */");
    let body = source.split_once(&start).expect("binary template group begins").1;
    body.split_once(&end).expect("binary template group ends").0.to_owned()
}

pub(super) fn implementation(
    program: &t::Program, native_method_ids: &mut Option<t::NativeMethodIndex>, class_id: usize, native: C,
) -> String {
    let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
    let buffer = buffer_class(program);
    let mut substitutions = vec![
        ("@BUFFER_ID@", buffer.to_string()),
    ];
    let mut body = match native {
        C::BinaryReader => {
            let reader = class_id;
            let new = method(program, method_ids, reader, M::BinaryReaderNew);
            let from_source = static_method(program, method_ids, M::BinaryReaderFromSource);
            let seek = method(program, method_ids, reader, M::BinaryReaderSeek);
            let read_byte = method(program, method_ids, reader, M::BinaryReaderReadByte);
            let read_int16 = method(program, method_ids, reader, M::BinaryReaderReadInt16);
            let read_int32 = method(program, method_ids, reader, M::BinaryReaderReadInt32);
            let read_int64 = method(program, method_ids, reader, M::BinaryReaderReadInt64);
            let read_float32 = method(program, method_ids, reader, M::BinaryReaderReadFloat32);
            let read_double = method(program, method_ids, reader, M::BinaryReaderReadDouble);
            let read_bytes = method(program, method_ids, reader, M::BinaryReaderReadBytes);
            let read_utf8 = method(program, method_ids, reader, M::BinaryReaderReadUtf8);
            let source = interface_class(program, "ByteSource");
            let source_read = interface_method(program, source, "read");
            let (source_read_ok, source_read_error) = interface_result_tags(program, source_read);

            add(&mut substitutions, "@READER_ID@", reader);
            add(&mut substitutions, "@READER_BYTES_ID@", param(new, 0));
            add(&mut substitutions, "@READER_ENDIAN_ID@", param(new, 1));
            add(&mut substitutions, "@READER_FROM_SOURCE_SOURCE_ID@", param(from_source, 0));
            add(&mut substitutions, "@READER_FROM_SOURCE_ENDIAN_ID@", param(from_source, 1));
            add(&mut substitutions, "@READER_FROM_SOURCE_MAX_BYTES_ID@", param(from_source, 2));
            add(&mut substitutions, "@SOURCE_ID@", source);
            add(&mut substitutions, "@SOURCE_READ_RESULT@", result_id(source_read));
            add_interface_method(&mut substitutions, source_read, "@SOURCE_READ_SLOT@");
            add(&mut substitutions, "@SOURCE_READ_OK_TAG@", source_read_ok);
            add(&mut substitutions, "@SOURCE_READ_ERROR_TAG@", source_read_error);
            add(&mut substitutions, "@READER_SEEK_POSITION_ID@", param(seek, 0));
            add(&mut substitutions, "@READER_BYTES_LENGTH_ID@", param(read_bytes, 0));
            add(&mut substitutions, "@READER_UTF8_LENGTH_ID@", param(read_utf8, 0));
            add_result(&mut substitutions, program, from_source, "@READER_FACTORY_RESULT@", "@READER_FACTORY_OK_TAG@", "@READER_FACTORY_ERROR@");
            add_variant_tag(&mut substitutions, program, from_source, "Err", "@READER_FACTORY_ERROR_TAG@");
            add_result(&mut substitutions, program, seek, "@READER_SEEK_RESULT@", "@READER_SEEK_OK_TAG@", "@READER_SEEK_ERROR@");
            add_result(&mut substitutions, program, read_byte, "@READER_BYTE_RESULT@", "@READER_BYTE_OK_TAG@", "@READER_BYTE_ERROR@");
            add_result(&mut substitutions, program, read_int16, "@READER_INT16_RESULT@", "@READER_INT16_OK_TAG@", "@READER_INT16_ERROR@");
            add_result(&mut substitutions, program, read_int32, "@READER_INT32_RESULT@", "@READER_INT32_OK_TAG@", "@READER_INT32_ERROR@");
            add_result(&mut substitutions, program, read_int64, "@READER_INT64_RESULT@", "@READER_INT64_OK_TAG@", "@READER_INT64_ERROR@");
            add_result(&mut substitutions, program, read_float32, "@READER_FLOAT32_RESULT@", "@READER_FLOAT32_OK_TAG@", "@READER_FLOAT32_ERROR@");
            add_result(&mut substitutions, program, read_double, "@READER_DOUBLE_RESULT@", "@READER_DOUBLE_OK_TAG@", "@READER_DOUBLE_ERROR@");
            add_result(&mut substitutions, program, read_bytes, "@READER_BYTES_RESULT@", "@READER_BYTES_OK_TAG@", "@READER_BYTES_ERROR@");
            add_result(&mut substitutions, program, read_utf8, "@READER_UTF8_RESULT@", "@READER_UTF8_OK_TAG@", "@READER_UTF8_ERROR@");
            for (function, tag) in [
                (read_byte, "@READER_BYTE_ERROR_TAG@"),
                (read_int16, "@READER_INT16_ERROR_TAG@"),
                (read_int32, "@READER_INT32_ERROR_TAG@"),
                (read_int64, "@READER_INT64_ERROR_TAG@"),
                (read_float32, "@READER_FLOAT32_ERROR_TAG@"),
                (read_double, "@READER_DOUBLE_ERROR_TAG@"),
                (read_bytes, "@READER_BYTES_ERROR_TAG@"),
                (read_utf8, "@READER_UTF8_ERROR_TAG@"),
            ] {
                add_variant_tag(&mut substitutions, program, function, "Err", tag);
            }
            add(&mut substitutions, "@TEXT_ID@", text_class_id(program));
            group(include_str!("../../stdlib/binary.c.in"), "reader")
        }
        C::BinaryWriter => {
            let writer = class_id;
            let new = method(program, method_ids, writer, M::BinaryWriterNew);
            let to_sink = static_method(program, method_ids, M::BinaryWriterToSink);
            let flush = method(program, method_ids, writer, M::BinaryWriterFlush);
            let write_byte = method(program, method_ids, writer, M::BinaryWriterWriteByte);
            let write_int16 = method(program, method_ids, writer, M::BinaryWriterWriteInt16);
            let write_int32 = method(program, method_ids, writer, M::BinaryWriterWriteInt32);
            let write_int64 = method(program, method_ids, writer, M::BinaryWriterWriteInt64);
            let write_float32 = method(program, method_ids, writer, M::BinaryWriterWriteFloat32);
            let write_double = method(program, method_ids, writer, M::BinaryWriterWriteDouble);
            let write_bytes = method(program, method_ids, writer, M::BinaryWriterWriteBytes);
            let write_utf8 = method(program, method_ids, writer, M::BinaryWriterWriteUtf8);
            let finish = static_method(program, method_ids, M::BinaryWriterFinish);
            let sink = interface_class(program, "ByteSink");
            let sink_write = interface_method(program, sink, "write");
            let sink_flush = interface_method(program, sink, "flush");
            let (sink_write_ok, sink_write_error) = interface_result_tags(program, sink_write);
            let (sink_flush_ok, sink_flush_error) = interface_result_tags(program, sink_flush);

            add(&mut substitutions, "@WRITER_ID@", writer);
            add(&mut substitutions, "@WRITER_CAPACITY_ID@", param(new, 0));
            add(&mut substitutions, "@WRITER_ENDIAN_ID@", param(new, 1));
            add(&mut substitutions, "@WRITER_TO_SINK_SINK_ID@", param(to_sink, 0));
            add(&mut substitutions, "@WRITER_TO_SINK_ENDIAN_ID@", param(to_sink, 1));
            add(&mut substitutions, "@WRITER_TO_SINK_MAX_BYTES_ID@", param(to_sink, 2));
            add(&mut substitutions, "@SINK_ID@", sink);
            add_interface_method(&mut substitutions, sink_write, "@SINK_WRITE_SLOT@");
            add(&mut substitutions, "@SINK_WRITE_RESULT@", result_id(sink_write));
            add(&mut substitutions, "@SINK_WRITE_OK_TAG@", sink_write_ok);
            add(&mut substitutions, "@SINK_WRITE_ERROR_TAG@", sink_write_error);
            add_interface_method(&mut substitutions, sink_flush, "@SINK_FLUSH_SLOT@");
            add(&mut substitutions, "@SINK_FLUSH_RESULT@", result_id(sink_flush));
            add(&mut substitutions, "@SINK_FLUSH_OK_TAG@", sink_flush_ok);
            add(&mut substitutions, "@SINK_FLUSH_ERROR_TAG@", sink_flush_error);
            add(&mut substitutions, "@WRITER_WRITE_BYTE_VALUE_ID@", param(write_byte, 0));
            add(&mut substitutions, "@WRITER_WRITE_INT16_VALUE_ID@", param(write_int16, 0));
            add(&mut substitutions, "@WRITER_WRITE_INT32_VALUE_ID@", param(write_int32, 0));
            add(&mut substitutions, "@WRITER_WRITE_INT64_VALUE_ID@", param(write_int64, 0));
            add(&mut substitutions, "@WRITER_WRITE_DOUBLE_VALUE_ID@", param(write_double, 0));
            add(&mut substitutions, "@WRITER_WRITE_FLOAT32_VALUE_ID@", param(write_float32, 0));
            add(&mut substitutions, "@WRITER_WRITE_BYTES_SOURCE_ID@", param(write_bytes, 0));
            add(&mut substitutions, "@WRITER_WRITE_UTF8_TEXT_ID@", param(write_utf8, 0));
            add(&mut substitutions, "@WRITER_FINISH_ID@", param(finish, 0));
            add_result(&mut substitutions, program, to_sink, "@WRITER_FACTORY_RESULT@", "@WRITER_FACTORY_OK_TAG@", "@WRITER_FACTORY_ERROR@");
            add_variant_tag(&mut substitutions, program, to_sink, "Err", "@WRITER_FACTORY_ERROR_TAG@");
            add_result(&mut substitutions, program, write_byte, "@WRITER_RESULT@", "@WRITER_RESULT_OK_TAG@", "@WRITER_RESULT_ERROR@");
            add_variant_tag(&mut substitutions, program, write_byte, "Err", "@WRITER_RESULT_ERROR_TAG@");
            add_result(&mut substitutions, program, flush, "@WRITER_FLUSH_RESULT@", "@WRITER_FLUSH_OK_TAG@", "@WRITER_FLUSH_ERROR@");
            add_variant_tag(&mut substitutions, program, flush, "Err", "@WRITER_FLUSH_ERROR_TAG@");
            let text = text_class_id(program);
            add(&mut substitutions, "@TEXT_ID@", text);
            group(include_str!("../../stdlib/binary.c.in"), "writer")
        }
        _ => unreachable!("binary implementation requested for another native class"),
    };
    body = replace_placeholders(body, &substitutions);
    body = replace_method_ids(body, program, None, native_method_ids);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded binary template placeholder");
    body
}
