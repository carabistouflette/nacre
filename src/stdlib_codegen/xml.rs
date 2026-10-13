use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{native_function, replace_method_ids, replace_placeholders, result_id, variant_id};

fn xml_class(program: &t::Program) -> Option<usize> {
    program.classes.iter().find_map(|class|
        matches!(class.native, Some(C::XmlDocument)).then_some(class.id))
}

fn used(program: &t::Program) -> bool {
    xml_class(program).is_some()
}

pub(super) fn fields() -> String {
    "    void *document;\n    void *xpath_context;\n".to_owned()
}

pub(super) fn drop_fields(program: &t::Program, class_id: usize) -> String {
    if used(program) {
        format!("    ncr_xml_document_drop_{class_id}(p->document, p->xpath_context);\n")
    } else {
        String::new()
    }
}

pub(super) fn drop_helper_prototypes(program: &t::Program) -> String {
    if let Some(class_id) = xml_class(program) {
        format!("static void ncr_xml_document_drop_{class_id}(void *document, void *xpath_context);\n")
    } else {
        String::new()
    }
}

fn method(program: &t::Program, native: M) -> &t::Function {
    native_function(program, |candidate| candidate == native)
}


fn parameter(function: &t::Function, index: usize) -> String {
    function.params[index].0.to_string()
}

pub(super) fn implementation(program: &t::Program) -> Option<String> {
    if !used(program) { return None; }
    let xml = xml_class(program).expect("XmlDocument native class");
    let text = program.classes.iter().find(|class| matches!(class.native, Some(C::Text)))
        .expect("Text native class").id;
    let parse = method(program, M::XmlDocumentParse);
    let serialize = method(program, M::XmlDocumentSerialize);
    let query_text = method(program, M::XmlDocumentQueryText);
    let query_xml = method(program, M::XmlDocumentQueryXml);
    let set_text = method(program, M::XmlDocumentSetText);
    let set_attribute = method(program, M::XmlDocumentSetAttribute);
    let remove = method(program, M::XmlDocumentRemove);
    let append_xml = method(program, M::XmlDocumentAppendXml);
    let validate = method(program, M::XmlDocumentValidate);
    let register_namespace = method(program, M::XmlDocumentRegisterNamespace);
    let escape = method(program, M::XmlDocumentEscape);

    let query_text_result = result_id(query_text);
    let Type::Object(list) = program.classes[query_text_result].result
        .expect("XmlDocument.queryText returns Result").ok_type else {
        unreachable!("XmlDocument.queryText Result payload is List<Text>");
    };
    assert_eq!(query_xml.return_type, query_text.return_type,
        "XmlDocument query methods share Result<List<Text>, Text>");
    assert!(matches!(program.classes[list].native, Some(C::List(Type::Object(element))) if element == text),
        "XmlDocument query results specialize to native List<Text>");
    let list_new = program.functions.iter().find(|function|
        function.receiver == Some(list) && function.native == Some(M::ListNew))
        .expect("XmlDocument List<Text> constructor");
    let list_add = program.functions.iter().find(|function|
        function.receiver == Some(list) && function.native == Some(M::ListAdd))
        .expect("XmlDocument List<Text>.add");

    let parse_result = result_id(parse);
    let serialize_result = result_id(serialize);
    let query_result = query_text_result;
    let long_result = result_id(set_text);
    assert_eq!(result_id(set_attribute), long_result);
    assert_eq!(result_id(remove), long_result);
    assert_eq!(result_id(append_xml), long_result);
    let unit_result = result_id(validate);
    assert_eq!(result_id(register_namespace), unit_result);

    let template = replace_placeholders(include_str!("../../stdlib/xml.c.in").to_owned(), &[
        ("@LIST_NEW@", format!("ncr_fn_{}", list_new.id)),
        ("@LIST_ADD@", format!("ncr_fn_{}", list_add.id)),
    ]);
    let template = replace_method_ids(template, program, None);
    let body = replace_placeholders(template, &[
        ("@XML_DOCUMENT_ID@", xml.to_string()),
        ("@TEXT_ID@", text.to_string()),
        ("@LIST_ID@", list.to_string()),
        ("@PARSE_INPUT@", parameter(parse, 0)),
        ("@PARSE_RESULT@", parse_result.to_string()),
        ("@PARSE_OK_TAG@", variant_id(program, parse_result, "Ok").to_string()),
        ("@PARSE_ERROR@", format!("ncr_result_error_{parse_result}")),
        ("@SERIALIZE_RESULT@", serialize_result.to_string()),
        ("@SERIALIZE_OK_TAG@", variant_id(program, serialize_result, "Ok").to_string()),
        ("@SERIALIZE_ERROR@", format!("ncr_result_error_{serialize_result}")),
        ("@QUERY_RESULT@", query_result.to_string()),
        ("@QUERY_OK_TAG@", variant_id(program, query_result, "Ok").to_string()),
        ("@QUERY_ERROR@", format!("ncr_result_error_{query_result}")),
        ("@LONG_RESULT@", long_result.to_string()),
        ("@LONG_OK_TAG@", variant_id(program, long_result, "Ok").to_string()),
        ("@LONG_ERROR@", format!("ncr_result_error_{long_result}")),
        ("@UNIT_RESULT@", unit_result.to_string()),
        ("@UNIT_OK_TAG@", variant_id(program, unit_result, "Ok").to_string()),
        ("@UNIT_ERROR@", format!("ncr_result_error_{unit_result}")),
        ("@QUERY_TEXT_XPATH@", parameter(query_text, 0)),
        ("@QUERY_XML_XPATH@", parameter(query_xml, 0)),
        ("@SET_TEXT_XPATH@", parameter(set_text, 0)),
        ("@SET_TEXT_VALUE@", parameter(set_text, 1)),
        ("@SET_ATTRIBUTE_XPATH@", parameter(set_attribute, 0)),
        ("@SET_ATTRIBUTE_NAME@", parameter(set_attribute, 1)),
        ("@SET_ATTRIBUTE_VALUE@", parameter(set_attribute, 2)),
        ("@REMOVE_XPATH@", parameter(remove, 0)),
        ("@APPEND_XPATH@", parameter(append_xml, 0)),
        ("@APPEND_FRAGMENT@", parameter(append_xml, 1)),
        ("@VALIDATE_XSD@", parameter(validate, 0)),
        ("@NAMESPACE_PREFIX@", parameter(register_namespace, 0)),
        ("@NAMESPACE_URI@", parameter(register_namespace, 1)),
        ("@ESCAPE_TEXT@", parameter(escape, 0)),
    ]);
    debug_assert!(!body.contains('@'), "unexpanded XML template placeholder");
    Some(body)
}
