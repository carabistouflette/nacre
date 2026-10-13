use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{has_placeholder, native_function, replace_method_ids, replace_placeholders, result_id, template_section, variant_id};

fn class_id(program: &t::Program, native: C) -> Option<usize> {
    program.classes.iter().find_map(|class| {
        class.native.is_some_and(|candidate|
            std::mem::discriminant(&candidate) == std::mem::discriminant(&native))
            .then_some(class.id)
    })
}

pub(super) fn fields(class: C) -> String {
    match class {
        C::XmlReader => "    void *reader;\n    unsigned char *input;\n    uint8_t state;\n".to_owned(),
        C::XmlEvent => "    void *snapshot;\n".to_owned(),
        C::XmlWriter => "    void *writer;\n    void *buffer;\n    void *frames;\n    uint32_t root_count;\n    uint8_t state;\n".to_owned(),
        _ => unreachable!("XML streaming fields requested for another native class"),
    }
}

pub(super) fn drop_fields(class_id: usize, class: C) -> String {
    match class {
        C::XmlReader => format!("    ncr_xml_reader_drop_{class_id}(p);\n"),
        C::XmlEvent => format!("    ncr_xml_event_drop_{class_id}(p);\n"),
        C::XmlWriter => format!("    ncr_xml_writer_drop_{class_id}(p);\n"),
        _ => unreachable!("XML streaming drop requested for another native class"),
    }
}

pub(super) fn drop_helper_prototypes(program: &t::Program) -> String {
    let mut out = String::new();
    for (native, name) in [(C::XmlReader, "reader"), (C::XmlEvent, "event"), (C::XmlWriter, "writer")] {
        if let Some(id) = class_id(program, native) {
            out.push_str(&format!("static void ncr_xml_{name}_drop_{id}(ncr_obj_{id} *value);\n"));
        }
    }
    if class_id(program, C::XmlWriter).is_some() {
        let buffer = class_id(program, C::ByteBuffer).expect("XmlWriter.finish requires ByteBuffer");
        out.push_str(&format!("static void ncr_byte_buffer_reserve_{buffer}(ncr_obj_{buffer} *buffer, size_t required_size, size_t line, size_t column);\n"));
    }
    out
}

fn result_bindings(program: &t::Program, function: &t::Function, keys: [&'static str; 3],
        bindings: &mut Vec<(&'static str, String)>) -> usize {
    let result = result_id(function);
    bindings.extend([
        (keys[0], result.to_string()),
        (keys[1], variant_id(program, result, "Ok").to_string()),
        (keys[2], format!("ncr_result_error_{result}")),
    ]);
    result
}

pub(super) fn implementation(program: &t::Program) -> String {
    let reader = class_id(program, C::XmlReader);
    let event = class_id(program, C::XmlEvent);
    let writer = class_id(program, C::XmlWriter);
    if reader.is_none() && event.is_none() && writer.is_none() { return String::new(); }
    let source = include_str!("../../stdlib/xml_stream.c.in");
    macro_rules! section {
        ($name:literal) => {
            template_section(source, concat!("/* XML_", $name, "_BEGIN */"),
                concat!("/* XML_", $name, "_END */"))
        };
    }
    let mut body = section!("STREAM_HEADERS").to_owned();
    if event.is_some() || reader.is_some() {
        body.push_str(section!("EVENT_TYPES"));
        body.push_str(section!("EVENT_FREE"));
    }
    if reader.is_some() { body.push_str(section!("READER_SNAPSHOT")); }
    if event.is_some() { body.push_str(section!("EVENT_DROP")); }
    if writer.is_some() {
        body.push_str(section!("WRITER_TYPES"));
        body.push_str(section!("WRITER_DROP"));
    }
    if reader.is_some() { body.push_str(section!("READER_METHODS")); }
    if event.is_some() { body.push_str(section!("EVENT_METHODS")); }
    if writer.is_some() { body.push_str(section!("WRITER_METHODS")); }

    let text = super::support::text_class(program).expect("XML streaming requires Text");
    let mut bindings = vec![("@TEXT_ID@", text.to_string())];
    macro_rules! result {
        ($function:expr, $prefix:literal) => {
            result_bindings(program, $function, [
                concat!("@", $prefix, "_RESULT@"), concat!("@", $prefix, "_OK_TAG@"),
                concat!("@", $prefix, "_ERROR@")], &mut bindings)
        };
    }
    for (id, key) in [(reader, "@READER_ID@"), (event, "@EVENT_ID@"), (writer, "@WRITER_ID@"),
        (class_id(program, C::ByteBuffer), "@BUFFER_ID@")] {
        if let Some(id) = id { bindings.push((key, id.to_string())); }
    }
    if reader.is_some() {
        let from_text = native_function(program, |method| method == M::XmlReaderFromText);
        let from_buffer = native_function(program, |method| method == M::XmlReaderFromBuffer);
        let construct = result!(from_text, "CONSTRUCT");
        assert_eq!(result_id(from_buffer), construct, "XmlReader factories share their result");
        let read = native_function(program, |method| method == M::XmlReaderRead);
        let result = result!(read, "READ");
        let (_, ok) = super::support::variant(program, result, "Ok");
        let [Type::Object(option)] = ok.fields.as_slice() else {
            unreachable!("XmlReader.read returns Result<Option<XmlEvent>, Text>");
        };
        let (_, some) = super::support::variant(program, *option, "Some");
        assert_eq!(some.fields.as_slice(), &[Type::Object(event.expect("XmlReader.read requires XmlEvent"))]);
        bindings.extend([
            ("@OPTION_TYPE@", option.to_string()),
            ("@READ_NONE_TAG@", variant_id(program, *option, "None").to_string()),
            ("@READ_SOME_TAG@", variant_id(program, *option, "Some").to_string()),
        ]);
    }
    if event.is_some() {
        let attribute = native_function(program, |method| method == M::XmlEventAttributeName);
        result!(attribute, "ATTRIBUTE");
    }
    if writer.is_some() {
        let create = native_function(program, |method| method == M::XmlWriterCreate);
        result!(create, "WRITER_CREATE");
        let finish = native_function(program, |method| method == M::XmlWriterFinish);
        result!(finish, "FINISH");
    }
    let unit_method = if reader.is_some() { Some(M::XmlReaderClose) }
        else if writer.is_some() { Some(M::XmlWriterStartDocument) } else { None };
    if let Some(method) = unit_method {
        let function = native_function(program, |candidate| candidate == method);
        result!(function, "UNIT");
    }
    for function in &program.functions {
        let names: &[&str] = match function.native {
            Some(M::XmlReaderFromText) => &["@FROM_TEXT_XML_ID@"],
            Some(M::XmlReaderFromBuffer) => &["@FROM_BUFFER_BYTES_ID@"],
            Some(M::XmlEventAttributeName) => &["@EVENT_ATTRIBUTE_NAME_INDEX_ID@"],
            Some(M::XmlEventAttributeNamespaceUri) => &["@EVENT_ATTRIBUTE_NAMESPACE_INDEX_ID@"],
            Some(M::XmlEventAttributeValue) => &["@EVENT_ATTRIBUTE_VALUE_INDEX_ID@"],
            Some(M::XmlWriterStartElement) => &["@START_ELEMENT_NAME_ID@"],
            Some(M::XmlWriterStartElementNs) => &["@START_ELEMENT_NS_PREFIX_ID@", "@START_ELEMENT_NS_LOCAL_ID@", "@START_ELEMENT_NS_URI_ID@"],
            Some(M::XmlWriterWriteAttribute) => &["@WRITE_ATTRIBUTE_NAME_ID@", "@WRITE_ATTRIBUTE_VALUE_ID@"],
            Some(M::XmlWriterWriteAttributeNs) => &["@WRITE_ATTRIBUTE_NS_PREFIX_ID@", "@WRITE_ATTRIBUTE_NS_LOCAL_ID@", "@WRITE_ATTRIBUTE_NS_URI_ID@", "@WRITE_ATTRIBUTE_NS_VALUE_ID@"],
            Some(M::XmlWriterWriteText) => &["@WRITE_TEXT_VALUE_ID@"],
            Some(M::XmlWriterWriteCData) => &["@WRITE_CDATA_VALUE_ID@"],
            Some(M::XmlWriterWriteComment) => &["@WRITE_COMMENT_VALUE_ID@"],
            Some(M::XmlWriterWriteProcessingInstruction) => &["@WRITE_PI_TARGET_ID@", "@WRITE_PI_VALUE_ID@"],
            _ => continue,
        };
        assert_eq!(names.len(), function.params.len(), "XML native parameter contract");
        for (name, (id, _)) in names.iter().zip(&function.params) {
            bindings.push((*name, id.to_string()));
        }
    }
    let body = replace_method_ids(body, program, None);
    let body = replace_placeholders(body, &bindings);
    debug_assert!(!has_placeholder(&body), "unexpanded streaming XML template placeholder");
    body
}
