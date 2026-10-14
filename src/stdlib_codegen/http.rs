use crate::c_repr::ctype;
use crate::ir::{self as t, CallTarget, Expr, ExprKind, NativeClass as C, NativeMethod as M, Stmt, Type};
use super::support::{
    native_function, replace_method_ids, replace_placeholders, result_id, text_class, variant_id,
};

const HTTP_METHODS: &[M] = &[
    M::HttpRequestCreate,
    M::HttpRequestSetHeader,
    M::HttpRequestSetBody,
    M::HttpRequestSetTimeoutMillis,
    M::HttpRequestSetMaxResponseBytes,
    M::HttpRequestSetCaFile,
    M::HttpRequestSend,
    M::HttpResponseStatus,
    M::HttpResponseBody,
    M::HttpResponseHeader,
    M::HttpResponseFinalUrl,
    M::HttpResponseBodyText,
];

fn class_id(program: &t::Program, is: impl Fn(C) -> bool) -> usize {
    program.classes.iter()
        .find(|class| class.native.is_some_and(&is))
        .expect("HTTP signatures specialize their required native class").id
}

fn method(program: &t::Program, native: M) -> &t::Function {
    native_function(program, |candidate| candidate == native)
}

fn http_call(program: &t::Program, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Call { target, args, .. } => {
            let direct = matches!(target, CallTarget::Function(id)
                if program.functions.get(*id).and_then(|function| function.native)
                    .is_some_and(|native| HTTP_METHODS.contains(&native)));
            direct
                || args.iter().any(|argument| http_call(program, argument))
        }
        ExprKind::Drop(value) | ExprKind::Upcast { value, .. } |
        ExprKind::Unary { expr: value, .. } | ExprKind::Try { value, .. } |
        ExprKind::Cast { value } => http_call(program, value),
        ExprKind::MapSlot { index, .. } => http_call(program, index),
        ExprKind::New { args, .. } | ExprKind::Variant { args, .. } =>
            args.iter().any(|argument| http_call(program, argument)),
        ExprKind::Binary { left, right, .. } =>
            http_call(program, left) || http_call(program, right),
        ExprKind::Char(_) | ExprKind::Int(_) | ExprKind::Long(_) | ExprKind::Double(_) |
        ExprKind::Unit | ExprKind::Boolean(_) | ExprKind::String(_) | ExprKind::Read(_) |
        ExprKind::Move(_) | ExprKind::Borrow { .. } | ExprKind::MapCapacity(_) => false,
    }
}

fn http_block(program: &t::Program, block: &[Stmt]) -> bool {
    block.iter().any(|statement| match statement {
        Stmt::Let { value, .. } => value.as_ref().is_some_and(|value| http_call(program, value)),
        Stmt::Assign { value, .. } | Stmt::Expr(value) => http_call(program, value),
        Stmt::If { condition, then_block, else_block } => http_call(program, condition)
            || http_block(program, then_block)
            || else_block.as_ref().is_some_and(|block| http_block(program, block)),
        Stmt::While { condition, body } => http_call(program, condition) || http_block(program, body),
        Stmt::Match { value, arms } => http_call(program, value)
            || arms.iter().any(|arm| http_block(program, &arm.body)),
        Stmt::Return(value) => value.as_ref().is_some_and(|value| http_call(program, value)),
        Stmt::Block(block) => http_block(program, block),
        Stmt::Break | Stmt::Continue => false,
    })
}

pub(super) fn uses_http(program: &t::Program) -> bool {
    program.functions.iter().any(|function| http_block(program, &function.body))
}

pub(super) fn fields(program: &t::Program, class: C) -> String {
    match class {
        C::HttpRequest => concat!(
            "    void *easy;\n",
            "    void *header_list;\n",
            "    unsigned char *method;\n    size_t method_length;\n",
            "    unsigned char *url;\n    size_t url_length;\n",
            "    unsigned char *body;\n    size_t body_length;\n    bool body_set;\n",
            "    unsigned char *ca_file;\n    size_t ca_file_length;\n",
            "    int64_t timeout_millis;\n    size_t max_response_bytes;\n",
        ).to_owned(),
        C::HttpResponse => {
            let buffer = class_id(program, |native| matches!(native, C::ByteBuffer));
            format!(concat!(
                "    ncr_obj_{buffer} *body;\n",
                "    void *headers;\n",
                "    size_t header_count;\n    size_t header_capacity;\n    size_t header_bytes;\n",
                "    unsigned char *final_url;\n    size_t final_url_length;\n",
                "    int32_t status;\n",
            ), buffer = buffer)
        }
        _ => unreachable!("HTTP storage requested for a non-HTTP native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C) -> String {
    if !uses_http(program) { return String::new(); }
    match class {
        C::HttpRequest => "    ncr_http_request_destroy((void *)p);\n".to_owned(),
        C::HttpResponse => "    ncr_http_response_destroy((void *)p);\n".to_owned(),
        _ => unreachable!("HTTP drop requested for a non-HTTP native class"),
    }
}

pub(crate) fn drop_helper_prototypes(program: &t::Program) -> String {
    if !uses_http(program) { return String::new(); }
    "static void ncr_http_request_destroy(void *value);\nstatic void ncr_http_response_destroy(void *value);\n".to_owned()
}

pub(super) fn implementation(program: &t::Program) -> String {
    if !uses_http(program) { return String::new(); }

    let request = class_id(program, |native| matches!(native, C::HttpRequest));
    let response = class_id(program, |native| matches!(native, C::HttpResponse));
    let text = text_class(program).expect("HTTP requires Text");
    let buffer = class_id(program, |native| matches!(native, C::ByteBuffer));
    let path = class_id(program, |native| matches!(native, C::Path));

    let create = method(program, M::HttpRequestCreate);
    let set_header = method(program, M::HttpRequestSetHeader);
    let set_body = method(program, M::HttpRequestSetBody);
    let set_timeout = method(program, M::HttpRequestSetTimeoutMillis);
    let set_max = method(program, M::HttpRequestSetMaxResponseBytes);
    let set_ca = method(program, M::HttpRequestSetCaFile);
    let send = method(program, M::HttpRequestSend);
    let status = method(program, M::HttpResponseStatus);
    let body = method(program, M::HttpResponseBody);
    let header = method(program, M::HttpResponseHeader);
    let final_url = method(program, M::HttpResponseFinalUrl);
    let body_text = method(program, M::HttpResponseBodyText);

    assert_eq!(result_id(set_header), result_id(set_timeout), "HTTP setters share Result<Unit, Text>");
    assert_eq!(result_id(set_header), result_id(set_max), "HTTP setters share Result<Unit, Text>");
    assert_eq!(result_id(set_header), result_id(set_ca), "HTTP setters share Result<Unit, Text>");
    let unit_result = result_id(set_header);
    let create_result = result_id(create);
    let send_result = result_id(send);
    let body_text_result = result_id(body_text);
    let option_result = result_id(header);
    assert!(matches!(program.classes[option_result].variants.as_ref(), Some(variants)
        if variants.iter().any(|variant| variant.name == "Some"
            && matches!(variant.fields.as_slice(), [Type::Object(id)] if *id == text))));

    let parameter = |function: &t::Function, index: usize| format!("ncr_v_{}", function.params[index].0);
    let template = include_str!("../../stdlib/http.c.in");
    let substitutions = vec![
        ("@HTTP_REQUEST_ID@", request.to_string()),
        ("@HTTP_RESPONSE_ID@", response.to_string()),
        ("@TEXT_ID@", text.to_string()),
        ("@BUFFER_ID@", buffer.to_string()),
        ("@PATH_ID@", path.to_string()),
        ("@CREATE_RESULT_TYPE@", ctype(program, create.return_type).into_owned()),
        ("@CREATE_RESULT_ID@", create_result.to_string()),
        ("@CREATE_OK_TAG@", variant_id(program, create_result, "Ok").to_string()),
        ("@UNIT_RESULT_TYPE@", ctype(program, set_header.return_type).into_owned()),
        ("@UNIT_RESULT_ID@", unit_result.to_string()),
        ("@UNIT_OK_TAG@", variant_id(program, unit_result, "Ok").to_string()),
        ("@SEND_RESULT_TYPE@", ctype(program, send.return_type).into_owned()),
        ("@SEND_RESULT_ID@", send_result.to_string()),
        ("@SEND_OK_TAG@", variant_id(program, send_result, "Ok").to_string()),
        ("@OPTION_RESULT_TYPE@", ctype(program, header.return_type).into_owned()),
        ("@OPTION_NONE_TAG@", variant_id(program, option_result, "None").to_string()),
        ("@OPTION_SOME_TAG@", variant_id(program, option_result, "Some").to_string()),
        ("@BODY_TEXT_RESULT_TYPE@", ctype(program, body_text.return_type).into_owned()),
        ("@BODY_TEXT_RESULT_ID@", body_text_result.to_string()),
        ("@BODY_TEXT_OK_TAG@", variant_id(program, body_text_result, "Ok").to_string()),
        ("@TEXT_FROM_BYTES@", format!("ncr_text_from_bytes_{text}")),
        ("@CREATE_METHOD_ID@", parameter(create, 0)),
        ("@CREATE_URL_ID@", parameter(create, 1)),
        ("@SET_HEADER_NAME_ID@", parameter(set_header, 0)),
        ("@SET_HEADER_VALUE_ID@", parameter(set_header, 1)),
        ("@SET_BODY_SOURCE_ID@", parameter(set_body, 0)),
        ("@SET_TIMEOUT_VALUE_ID@", parameter(set_timeout, 0)),
        ("@SET_MAX_VALUE_ID@", parameter(set_max, 0)),
        ("@SET_CA_PATH_ID@", parameter(set_ca, 0)),
        ("@RESPONSE_STATUS_RESULT_TYPE@", ctype(program, status.return_type).into_owned()),
        ("@RESPONSE_BODY_RESULT_TYPE@", ctype(program, body.return_type).into_owned()),
        ("@RESPONSE_HEADER_NAME_ID@", parameter(header, 0)),
        ("@RESPONSE_FINAL_URL_RESULT_TYPE@", ctype(program, final_url.return_type).into_owned()),
    ];
    let body = replace_placeholders(template.to_owned(), &substitutions);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded HTTP placeholder");
    body
}
