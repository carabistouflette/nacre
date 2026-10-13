use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{has_placeholder, native_function, replace_method_ids, replace_placeholders, result_id, variant_id};

pub(super) fn fields() -> String {
    "    void *stylesheet;\n".to_owned()
}

pub(super) fn drop_fields(class_id: usize) -> String {
    format!("    ncr_xslt_stylesheet_drop_{class_id}(p);\n")
}

pub(super) fn drop_helper_prototypes(program: &t::Program) -> String {
    let Some(class_id) = xslt_class(program) else { return String::new(); };
    format!("static void ncr_xslt_stylesheet_drop_{class_id}(ncr_obj_{class_id} *stylesheet);\n")
}

pub(super) fn implementation(program: &t::Program) -> String {
    let Some(stylesheet_class) = xslt_class(program) else { return String::new(); };
    let document_class = program.classes.iter()
        .find(|class| matches!(class.native, Some(C::XmlDocument)))
        .expect("XSLT requires XmlDocument")
        .id;
    let text_class = super::support::text_class(program).expect("XSLT requires Text");

    let compile = native_function(program, |method| matches!(method, M::XsltStylesheetCompile));
    let transform = native_function(program, |method| matches!(method, M::XsltStylesheetTransform));
    let transform_bytes = native_function(program, |method| matches!(method, M::XsltStylesheetTransformBytes));
    let buffer_class = program.classes.iter()
        .find(|class| matches!(class.native, Some(C::ByteBuffer)))
        .expect("XSLT serialized output requires ByteBuffer").id;
    assert!(compile.receiver.is_none(), "XsltStylesheet.compile is static");
    assert_eq!(compile.params.len(), 1);
    assert_eq!(compile.params[0].1, Type::Borrow { class: text_class, mutable: false },
        "XsltStylesheet.compile accepts borrowed Text");
    assert_eq!(transform.receiver, Some(stylesheet_class));
    assert_eq!(transform.params.len(), 1);
    assert_eq!(transform.params[0].1, Type::Borrow { class: document_class, mutable: false },
        "XsltStylesheet.transform accepts borrowed XmlDocument");
    assert_eq!(transform_bytes.receiver, Some(stylesheet_class));
    assert_eq!(transform_bytes.params.len(), 1);
    assert_eq!(transform_bytes.params[0].1, Type::Borrow { class: document_class, mutable: false },
        "XsltStylesheet.transformBytes accepts borrowed XmlDocument");
    let compile_result = result_id(compile);
    let transform_result = result_id(transform);
    let compile_ok = variant_id(program, compile_result, "Ok");
    let transform_ok = variant_id(program, transform_result, "Ok");
    let bytes_result = result_id(transform_bytes);
    let bytes_ok = variant_id(program, bytes_result, "Ok");
    assert!(matches!(program.classes[compile_result].result, Some(layout)
        if layout.ok_type == Type::Object(stylesheet_class) && layout.err_type == Type::Object(text_class)));
    assert!(matches!(program.classes[transform_result].result, Some(layout)
        if layout.ok_type == Type::Object(document_class) && layout.err_type == Type::Object(text_class)));
    assert!(matches!(program.classes[bytes_result].result, Some(layout)
        if layout.ok_type == Type::Object(buffer_class) && layout.err_type == Type::Object(text_class)));

    let body = replace_method_ids(include_str!("../../stdlib/xslt.c.in").to_owned(), program, None);
    let body = replace_placeholders(body, &[
        ("@STYLESHEET_ID@", stylesheet_class.to_string()),
        ("@DOCUMENT_ID@", document_class.to_string()),
        ("@TEXT_ID@", text_class.to_string()),
        ("@BUFFER_ID@", buffer_class.to_string()),
        ("@COMPILE_RESULT@", compile_result.to_string()),
        ("@COMPILE_SOURCE_ID@", compile.params[0].0.to_string()),
        ("@COMPILE_OK_TAG@", compile_ok.to_string()),
        ("@COMPILE_ERROR@", format!("ncr_result_error_{compile_result}")),
        ("@TRANSFORM_RESULT@", transform_result.to_string()),
        ("@TRANSFORM_DOCUMENT_ID@", transform.params[0].0.to_string()),
        ("@TRANSFORM_OK_TAG@", transform_ok.to_string()),
        ("@TRANSFORM_ERROR@", format!("ncr_result_error_{transform_result}")),
        ("@BYTES_RESULT@", bytes_result.to_string()),
        ("@BYTES_DOCUMENT_ID@", transform_bytes.params[0].0.to_string()),
        ("@BYTES_OK_TAG@", bytes_ok.to_string()),
        ("@BYTES_ERROR@", format!("ncr_result_error_{bytes_result}")),
    ]);
    debug_assert!(!has_placeholder(&body), "unexpanded XSLT placeholder");
    body
}

fn xslt_class(program: &t::Program) -> Option<usize> {
    program.classes.iter()
        .find(|class| matches!(class.native, Some(C::XsltStylesheet)))
        .map(|class| class.id)
}
