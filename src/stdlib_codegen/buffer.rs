use crate::c_repr::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{replace_placeholders, result_id, text_class, variant, required_indexed_native_method};

pub(super) fn fields() -> String {
    "    unsigned char *bytes;\n    size_t length;\n    size_t capacity;\n".to_owned()
}

pub(super) fn drop_fields() -> String {
    "    ncr_deallocate(p->bytes);\n".to_owned()
}

fn method<'a>(
    program: &'a t::Program, method_ids: &t::NativeMethodIndex, class_id: usize, native: M,
) -> &'a t::Function {
    required_indexed_native_method(program, method_ids, Some(class_id), native)
}

fn result_variant(program: &t::Program, function: &t::Function, name: &str) -> usize {
    let result = result_id(function);
    let (tag, variant) = variant(program, result, name);
    match (name, variant.fields.as_slice()) {
        ("Ok", [Type::Unit]) => {},
        ("Ok", [Type::Object(_)]) => {},
        ("Err", [Type::Object(text)]) if Some(*text) == text_class(program) => {},
        _ => unreachable!("ByteBuffer Result uses an owned Ok payload and Text error"),
    }
    tag
}

pub(super) fn implementation(
    program: &t::Program, method_ids: &t::NativeMethodIndex, class_id: usize,
) -> String {
    assert!(matches!(program.classes[class_id].native, Some(C::ByteBuffer)),
        "ByteBuffer implementation belongs to its native class");
    let factory = required_indexed_native_method(program, method_ids, None, M::ByteBufferWithCapacity);
    assert!(matches!(factory.return_type, Type::Object(result) if matches!(program.classes[result].variants.as_ref(), Some(variants) if variants.iter().any(|v| v.name == "Ok" && matches!(v.fields.as_slice(), [Type::Object(id)] if *id == class_id)))),
        "ByteBuffer.withCapacity returns Result<ByteBuffer, Text>");
    let length = method(program, method_ids, class_id, M::ByteBufferLength);
    let capacity = method(program, method_ids, class_id, M::ByteBufferCapacity);
    let get = method(program, method_ids, class_id, M::ByteBufferGet);
    let set = method(program, method_ids, class_id, M::ByteBufferSet);
    let append = method(program, method_ids, class_id, M::ByteBufferAppend);
    let append_bytes = method(program, method_ids, class_id, M::ByteBufferAppendBytes);
    let clear = method(program, method_ids, class_id, M::ByteBufferClear);
    let truncate = method(program, method_ids, class_id, M::ByteBufferTruncate);

    assert_eq!(length.return_type, Type::Long, "ByteBuffer.length returns long");
    assert_eq!(capacity.return_type, Type::Long, "ByteBuffer.capacity returns long");
    assert_eq!(get.return_type, Type::Byte, "ByteBuffer.get returns byte");
    assert_eq!(append.return_type, Type::Void, "ByteBuffer.append is a command");
    assert_eq!(clear.return_type, Type::Void, "ByteBuffer.clear is a command");
    let Type::Borrow { class: source_class, mutable: false } = append_bytes.params[0].1 else {
        unreachable!("ByteBuffer.appendBytes takes a shared ByteBuffer borrow");
    };
    assert_eq!(source_class, class_id, "ByteBuffer.appendBytes borrows its source");

    let factory_result = result_id(factory);
    let append_result = result_id(append_bytes);
    let truncate_result = result_id(truncate);
    assert_eq!(append_result, truncate_result, "ByteBuffer fallible commands use the same Result<Unit, Text>");
    let (factory_ok, factory_ok_variant) = variant(program, factory_result, "Ok");
    assert!(matches!(factory_ok_variant.fields.as_slice(), [Type::Object(id)] if *id == class_id));
    let (_, factory_error_variant) = variant(program, factory_result, "Err");
    assert!(matches!(factory_error_variant.fields.as_slice(), [Type::Object(id)] if Some(*id) == text_class(program)));
    let append_ok = result_variant(program, append_bytes, "Ok");
    let append_error = result_variant(program, append_bytes, "Err");
    let truncate_ok = result_variant(program, truncate, "Ok");
    let truncate_error = result_variant(program, truncate, "Err");

    let template = include_str!("../../stdlib/byte_buffer.c.in");
    replace_placeholders(template.to_owned(), &[
        ("@BUFFER_TYPE@", ctype(program, Type::Object(class_id)).into_owned()),
        ("@FACTORY_RESULT@", ctype(program, factory.return_type).into_owned()),
        ("@FACTORY_CAPACITY_ID@", factory.params[0].0.to_string()),
        ("@FACTORY_OK_TAG@", factory_ok.to_string()),
        ("@FACTORY_ERROR_RESULT@", factory_result.to_string()),
        ("@LENGTH_RESULT@", ctype(program, length.return_type).into_owned()),
        ("@CAPACITY_RESULT@", ctype(program, capacity.return_type).into_owned()),
        ("@GET_INDEX_ID@", get.params[0].0.to_string()),
        ("@SET_INDEX_ID@", set.params[0].0.to_string()),
        ("@SET_VALUE_ID@", set.params[1].0.to_string()),
        ("@APPEND_VALUE_ID@", append.params[0].0.to_string()),
        ("@APPEND_SOURCE_ID@", append_bytes.params[0].0.to_string()),
        ("@APPEND_RESULT@", ctype(program, append_bytes.return_type).into_owned()),
        ("@APPEND_OK_TAG@", append_ok.to_string()),
        ("@APPEND_ERROR_RESULT@", append_result.to_string()),
        ("@APPEND_ERROR_TAG@", append_error.to_string()),
        ("@TRUNCATE_LENGTH_ID@", truncate.params[0].0.to_string()),
        ("@TRUNCATE_RESULT@", ctype(program, truncate.return_type).into_owned()),
        ("@TRUNCATE_OK_TAG@", truncate_ok.to_string()),
        ("@TRUNCATE_ERROR_RESULT@", truncate_result.to_string()),
        ("@TRUNCATE_ERROR_TAG@", truncate_error.to_string()),
        ("@BYTE_BUFFER_WITH_CAPACITY@", super::support::marker(M::ByteBufferWithCapacity).to_owned()),
        ("@BYTE_BUFFER_LENGTH@", super::support::marker(M::ByteBufferLength).to_owned()),
        ("@BYTE_BUFFER_CAPACITY@", super::support::marker(M::ByteBufferCapacity).to_owned()),
        ("@BYTE_BUFFER_GET@", super::support::marker(M::ByteBufferGet).to_owned()),
        ("@BYTE_BUFFER_SET@", super::support::marker(M::ByteBufferSet).to_owned()),
        ("@BYTE_BUFFER_APPEND@", super::support::marker(M::ByteBufferAppend).to_owned()),
        ("@BYTE_BUFFER_APPEND_BYTES@", super::support::marker(M::ByteBufferAppendBytes).to_owned()),
        ("@BYTE_BUFFER_CLEAR@", super::support::marker(M::ByteBufferClear).to_owned()),
        ("@BYTE_BUFFER_TRUNCATE@", super::support::marker(M::ByteBufferTruncate).to_owned()),
    ])
}
