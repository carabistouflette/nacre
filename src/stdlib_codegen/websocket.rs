use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    has_placeholder, native_class_id, native_class_id_if, native_function_if,
    replace_placeholders, result_id, template_section, variant, variant_id,
};

fn method_if(program: &t::Program, native: M) -> Option<&t::Function> {
    native_function_if(program, |candidate| candidate == native)
}

fn method(program: &t::Program, native: M) -> &t::Function {
    method_if(program, native).expect("native WebSocket dependency is present")
}

fn class_id_if(program: &t::Program, is: impl Fn(C) -> bool) -> Option<usize> {
    native_class_id_if(program, is)
}

fn class_id(program: &t::Program, is: impl Fn(C) -> bool) -> usize {
    native_class_id(program, is)
}

fn group<'a>(source: &'a str, name: &str) -> &'a str {
    let (start, end) = match name {
        "standard_includes_1" => ("/* WEBSOCKET_GROUP standard_includes_1 */", "/* WEBSOCKET_GROUP_END standard_includes_1 */"),
        "standard_includes_2" => ("/* WEBSOCKET_GROUP standard_includes_2 */", "/* WEBSOCKET_GROUP_END standard_includes_2 */"),
        "ssl_includes" => ("/* WEBSOCKET_GROUP ssl_includes */", "/* WEBSOCKET_GROUP_END ssl_includes */"),
        "constants" => ("/* WEBSOCKET_GROUP constants */", "/* WEBSOCKET_GROUP_END constants */"),
        "connect_text" => ("/* WEBSOCKET_GROUP connect_text */", "/* WEBSOCKET_GROUP_END connect_text */"),
        "connect_uri" => ("/* WEBSOCKET_GROUP connect_uri */", "/* WEBSOCKET_GROUP_END connect_uri */"),
        "transport_read" => ("/* WEBSOCKET_GROUP transport_read */", "/* WEBSOCKET_GROUP_END transport_read */"),
        "transport_write" => ("/* WEBSOCKET_GROUP transport_write */", "/* WEBSOCKET_GROUP_END transport_write */"),
        "transport_frame" => ("/* WEBSOCKET_GROUP transport_frame */", "/* WEBSOCKET_GROUP_END transport_frame */"),
        "http" => ("/* WEBSOCKET_GROUP http */", "/* WEBSOCKET_GROUP_END http */"),
        "cleanup" => ("/* WEBSOCKET_GROUP cleanup */", "/* WEBSOCKET_GROUP_END cleanup */"),
        "utf8" => ("/* WEBSOCKET_GROUP utf8 */", "/* WEBSOCKET_GROUP_END utf8 */"),
        "tls_shutdown" => ("/* WEBSOCKET_GROUP tls_shutdown */", "/* WEBSOCKET_GROUP_END tls_shutdown */"),
        "close_core" => ("/* WEBSOCKET_GROUP close_core */", "/* WEBSOCKET_GROUP_END close_core */"),
        "receive_core" => ("/* WEBSOCKET_GROUP receive_core */", "/* WEBSOCKET_GROUP_END receive_core */"),
        "connect_core" => ("/* WEBSOCKET_GROUP connect_core */", "/* WEBSOCKET_GROUP_END connect_core */"),
        "connect_method" => ("/* WEBSOCKET_GROUP connect_method */", "/* WEBSOCKET_GROUP_END connect_method */"),
        "connect_tls_method" => ("/* WEBSOCKET_GROUP connect_tls_method */", "/* WEBSOCKET_GROUP_END connect_tls_method */"),
        "send_text_method" => ("/* WEBSOCKET_GROUP send_text_method */", "/* WEBSOCKET_GROUP_END send_text_method */"),
        "send_binary_method" => ("/* WEBSOCKET_GROUP send_binary_method */", "/* WEBSOCKET_GROUP_END send_binary_method */"),
        "ping_method" => ("/* WEBSOCKET_GROUP ping_method */", "/* WEBSOCKET_GROUP_END ping_method */"),
        "receive_method" => ("/* WEBSOCKET_GROUP receive_method */", "/* WEBSOCKET_GROUP_END receive_method */"),
        "close_method" => ("/* WEBSOCKET_GROUP close_method */", "/* WEBSOCKET_GROUP_END close_method */"),
        "message_is_text_method" => ("/* WEBSOCKET_GROUP message_is_text_method */", "/* WEBSOCKET_GROUP_END message_is_text_method */"),
        "message_body_method" => ("/* WEBSOCKET_GROUP message_body_method */", "/* WEBSOCKET_GROUP_END message_body_method */"),
        "message_as_text_method" => ("/* WEBSOCKET_GROUP message_as_text_method */", "/* WEBSOCKET_GROUP_END message_as_text_method */"),
        _ => unreachable!("unknown WebSocket template group"),
    };
    template_section(source, start, end)
}

fn add(replacements: &mut Vec<(&'static str, String)>, key: &'static str, value: impl ToString) {
    replacements.push((key, value.to_string()));
}

fn arg(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params[index].0)
}

fn arg_type(program: &t::Program, function: &t::Function, index: usize) -> String {
    ctype(program, function.params[index].1)
}

fn result_type(program: &t::Program, function: &t::Function) -> String {
    ctype(program, function.return_type)
}

fn method_name(function: &t::Function) -> String {
    format!("ncr_fn_{}", function.id)
}

fn add_method(replacements: &mut Vec<(&'static str, String)>, key: &'static str, function: &t::Function) {
    add(replacements, key, method_name(function));
}

fn add_result(
    replacements: &mut Vec<(&'static str, String)>,
    program: &t::Program,
    key_type: &'static str,
    key_id: &'static str,
    key_ok: &'static str,
    key_err: &'static str,
    function: &t::Function,
) {
    let id = result_id(function);
    add(replacements, key_type, result_type(program, function));
    add(replacements, key_id, id);
    add(replacements, key_ok, variant_id(program, id, "Ok"));
    add(replacements, key_err, variant_id(program, id, "Err"));
}
fn add_args(
    replacements: &mut Vec<(&'static str, String)>,
    function: &t::Function,
    bindings: &[(&'static str, usize)],
) {
    for &(key, index) in bindings { add(replacements, key, arg(function, index)); }
}

fn add_param_types(
    replacements: &mut Vec<(&'static str, String)>,
    program: &t::Program,
    function: &t::Function,
    bindings: &[(&'static str, usize)],
) {
    for &(key, index) in bindings { add(replacements, key, arg_type(program, function, index)); }
}

pub(super) fn fields(program: &t::Program, class: C, _class_id: usize) -> String {
    match class {
        C::WebSocket => {
            let tls_config_field = if let Some(connect_tls) = method_if(program, M::WebSocketConnectTls) {
                let Type::Object(mutex) = connect_tls.params[1].1 else {
                    unreachable!("WebSocket.connectTls owns a Mutex<TlsConfig> handle")
                };
                format!("    ncr_obj_{mutex} *tls_config;\n")
            } else {
                String::new()
            };
            format!(
                "    int fd;\n    void *ssl;\n{tls_config_field}    bool closing;\n    bool closed;\n    size_t max_message_bytes;\n    unsigned char read_buffer[4096];\n    size_t read_buffer_offset;\n    size_t read_buffer_length;\n"
            )
        }
        C::WebSocketMessage => {
            let buffer = class_id(program, |native| matches!(native, C::ByteBuffer));
            format!("    bool is_text;\n    ncr_obj_{buffer} *body;\n")
        }
        _ => unreachable!("WebSocket fields requested for another native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::WebSocket => {
            let tls_config_drop = if let Some(connect_tls) = method_if(program, M::WebSocketConnectTls) {
                let Type::Object(mutex) = connect_tls.params[1].1 else {
                    unreachable!("WebSocket.connectTls owns a Mutex<TlsConfig> handle")
                };
                format!(
                    "    if (p->tls_config != NULL) {{ ncr_obj_{mutex} *owned_config = p->tls_config; p->tls_config = NULL; ncr_drop_{mutex}(owned_config); }}\n"
                )
            } else {
                String::new()
            };
            format!(
                "    if (p->ssl != NULL) {{ ncr_tls_ssl_release(p->ssl); p->ssl = NULL; }}\n{tls_config_drop}    if (p->fd >= 0) {{ int owned_fd = p->fd; p->fd = -1; (void)close(owned_fd); }}\n"
            )
        }
        C::WebSocketMessage => {
            let buffer = class_id(program, |native| matches!(native, C::ByteBuffer));
            format!(
                "    if (p->body != NULL) {{ ncr_obj_{buffer} *owned_body = p->body; p->body = NULL; ncr_drop_{buffer}(owned_body); }}\n"
            )
        }
        _ => unreachable!("WebSocket drop requested for another native class"),
    }
}

pub(super) fn implementation(program: &t::Program) -> String {
    let connect = method_if(program, M::WebSocketConnect);
    let connect_tls = method_if(program, M::WebSocketConnectTls);
    let send_text = method_if(program, M::WebSocketSendText);
    let send_binary = method_if(program, M::WebSocketSendBinary);
    let ping = method_if(program, M::WebSocketPing);
    let receive = method_if(program, M::WebSocketReceive);
    let close = method_if(program, M::WebSocketClose);
    let is_text = method_if(program, M::WebSocketMessageIsText);
    let body = method_if(program, M::WebSocketMessageBody);
    let as_text = method_if(program, M::WebSocketMessageAsText);
    let websocket = class_id_if(program, |native| matches!(native, C::WebSocket));
    let message = class_id_if(program, |native| matches!(native, C::WebSocketMessage));

    if websocket.is_none() && message.is_none() { return String::new(); }
    let has_ws_io = connect.is_some() || connect_tls.is_some() || send_text.is_some()
        || send_binary.is_some() || ping.is_some() || receive.is_some() || close.is_some();
    let has_message_methods = is_text.is_some() || body.is_some() || as_text.is_some();
    let has_connect = connect.is_some() || connect_tls.is_some();
    let has_receive_core = receive.is_some() || close.is_some();
    let has_ws_instance_methods = send_text.is_some() || send_binary.is_some()
        || ping.is_some() || receive.is_some() || close.is_some();
    let needs_utf8 = has_receive_core || as_text.is_some();
    let has_read_transport = has_connect || has_receive_core;
    let has_frame_transport = send_text.is_some() || send_binary.is_some()
        || ping.is_some() || has_receive_core;

    let template = include_str!("../../stdlib/websocket.c.in");
    let mut output = String::new();
    if has_ws_io || has_message_methods {
        output.push_str(&group(template, "standard_includes_1"));
        output.push_str(&group(template, "standard_includes_2"));
    }
    if has_ws_io {
        output.push_str(&group(template, "ssl_includes"));
        output.push_str(&group(template, "constants"));
    }
    if has_connect {
        output.push_str(&group(template, "connect_text"));
        output.push_str(&group(template, "connect_uri"));
    }
    if has_read_transport { output.push_str(&group(template, "transport_read")); }
    if has_ws_io { output.push_str(&group(template, "transport_write")); }
    if has_frame_transport { output.push_str(&group(template, "transport_frame")); }
    if has_connect { output.push_str(&group(template, "http")); }
    if has_ws_io { output.push_str(&group(template, "cleanup")); }
    if needs_utf8 { output.push_str(&group(template, "utf8")); }
    if has_receive_core {
        output.push_str(&group(template, "tls_shutdown"));
        output.push_str(&group(template, "close_core"));
        output.push_str(&group(template, "receive_core"));
    }
    if has_connect { output.push_str(&group(template, "connect_core")); }
    if connect.is_some() { output.push_str(&group(template, "connect_method")); }
    if connect_tls.is_some() { output.push_str(&group(template, "connect_tls_method")); }
    if send_text.is_some() { output.push_str(&group(template, "send_text_method")); }
    if send_binary.is_some() { output.push_str(&group(template, "send_binary_method")); }
    if ping.is_some() { output.push_str(&group(template, "ping_method")); }
    if receive.is_some() { output.push_str(&group(template, "receive_method")); }
    if close.is_some() { output.push_str(&group(template, "close_method")); }
    if is_text.is_some() { output.push_str(&group(template, "message_is_text_method")); }
    if body.is_some() { output.push_str(&group(template, "message_body_method")); }
    if as_text.is_some() { output.push_str(&group(template, "message_as_text_method")); }

    let mut replacements = Vec::new();
    if let Some(websocket) = websocket {
        add(&mut replacements, "@WEBSOCKET_TYPE@", ctype(program, Type::Object(websocket)));
    }
    if has_ws_instance_methods {
        add(&mut replacements, "@WEBSOCKET_SELF_TYPE@", ctype(program, Type::Object(
            websocket.expect("WebSocket instance methods require WebSocket class"),
        )));
    }

    if let Some(config) = connect_tls {
        let Type::Object(mutex) = config.params[1].1 else {
            unreachable!("WebSocket.connectTls owns a Mutex<TlsConfig> handle")
        };
        add(&mut replacements, "@INIT_TLS_CONFIG@", "websocket->tls_config = NULL;");
        add(&mut replacements, "@DROP_TLS_CONFIG@", format!(
            "    if (ws->tls_config != NULL) {{ ncr_obj_{mutex} *owned_config = ws->tls_config; ws->tls_config = NULL; ncr_drop_{mutex}(owned_config); }}\n"
        ));
        add(&mut replacements, "@DROP_UNMOVED_TLS_CONFIG@", format!(
            "    if (config != NULL) {{ ncr_obj_{mutex} *owned_config = config; ncr_drop_{mutex}(owned_config); }}\n"
        ));
    } else {
        if websocket.is_some() { add(&mut replacements, "@DROP_TLS_CONFIG@", ""); }
        if has_connect {
            add(&mut replacements, "@INIT_TLS_CONFIG@", "");
            add(&mut replacements, "@DROP_UNMOVED_TLS_CONFIG@", "");
        }
    }
    if connect_tls.is_some() {
        add(&mut replacements, "@TLS_READ_BRANCH@", r#"        bool tls_timed_out = false;
        ssize_t amount = ncr_tls_read(ws->ssl, ws->fd, ws->read_buffer, sizeof(ws->read_buffer),
            finite, deadline, &tls_timed_out, failure);
        if (tls_timed_out) { *timed_out = true; return 0; }
        if (amount < 0) { return -1; }
        if (amount == 0) { *failure = ECONNRESET; return -1; }
        ws->read_buffer_length = (size_t)amount;
        return 1;"#);
        add(&mut replacements, "@TLS_WRITE_BRANCH@", r#"            bool tls_timed_out = false;
            ssize_t amount = ncr_tls_write(ws->ssl, ws->fd, source + offset, length - offset,
                finite, deadline, &tls_timed_out, failure);
            if (tls_timed_out) { *timed_out = true; return false; }
            if (amount < 0) { return false; }
            if (amount == 0) { *failure = EPIPE; return false; }
            offset += (size_t)amount;
            continue;"#);
        add(&mut replacements, "@TLS_SHUTDOWN_BRANCH@", r#"    bool tls_timed_out = false;
    int status = ncr_tls_shutdown(ws->ssl, ws->fd, finite, deadline, &tls_timed_out, failure);
    if (tls_timed_out) { *timed_out = true; }
    return status == 1;"#);
        add(&mut replacements, "@TLS_UPGRADE@", r#"    if (secure) {
        websocket->ssl = ncr_tls_ssl_for_config(config, websocket->fd, true, &failure);
        if (websocket->ssl == NULL) {
            error_message = "WebSocket TLS setup failed";
            goto failed;
        }
        websocket->tls_config = config;
        config = NULL;
        if (!ncr_tls_set_client_host_ascii(websocket->ssl, connect_host, line, column, &failure)) {
            error_message = "WebSocket TLS setup failed";
            goto failed;
        }
        if (ncr_tls_handshake(websocket->ssl, websocket->fd, true, finite, deadline, &timed_out, &failure) != 1) {
            error_message = timed_out ? "WebSocket TLS handshake timed out" : "WebSocket TLS handshake failed";
            goto failed;
        }
    }"#);
    } else {
        add(&mut replacements, "@TLS_READ_BRANCH@", "*failure = EPROTONOSUPPORT; return -1;");
        add(&mut replacements, "@TLS_WRITE_BRANCH@", "*failure = EPROTONOSUPPORT; return false;");
        add(&mut replacements, "@TLS_SHUTDOWN_BRANCH@", "*failure = EPROTONOSUPPORT; return false;");
        add(&mut replacements, "@TLS_UPGRADE@", "");
    }

    if has_connect {
        let core_result = connect.or(connect_tls).expect("WebSocket connect method is present");
        if let (Some(plain), Some(tls)) = (connect, connect_tls) {
            assert_eq!(result_id(plain), result_id(tls), "WebSocket connect variants share Result<WebSocket,Text>");
        }
        add(&mut replacements, "@CONNECT_CORE_RESULT_TYPE@", result_type(program, core_result));
        add(&mut replacements, "@CONNECT_CORE_RESULT_ID@", result_id(core_result));
        add(&mut replacements, "@CONNECT_CORE_OK_TAG@", variant_id(program, result_id(core_result), "Ok"));

        let uri_scheme = method(program, M::UriScheme);
        let uri_user_info = method(program, M::UriUserInfo);
        let uri_host = method(program, M::UriHost);
        let uri_path = method(program, M::UriPath);
        let uri_port = method(program, M::UriPort);
        let uri_query = method(program, M::UriQuery);
        let uri_fragment = method(program, M::UriFragment);
        let uri = uri_scheme.receiver.expect("Uri.scheme is an instance getter");
        let Type::Object(option_text) = uri_scheme.return_type else {
            unreachable!("Uri text getters return Option<Text>")
        };
        let text = class_id(program, |native| matches!(native, C::Text));
        let address = class_id(program, |native| matches!(native, C::SocketAddress));
        let list = class_id(program, |native| matches!(native, C::List(Type::Object(element)) if element == address));
        let port_result_id = result_id(uri_port);
        let (_, port_ok) = variant(program, port_result_id, "Ok");
        let Type::Object(option_port) = port_ok.fields[0] else {
            unreachable!("Uri.port Ok payload is Option<int>")
        };
        let resolver_result = result_id(method(program, M::SocketAddressResolve));
        let resolver_ok_tag = variant_id(program, resolver_result, "Ok");

        add(&mut replacements, "@URI_TYPE@", ctype(program, Type::Object(uri)));
        add(&mut replacements, "@TEXT_ID@", text);
        add(&mut replacements, "@TEXT_TYPE@", ctype(program, Type::Object(text)));
        add(&mut replacements, "@OPTION_TEXT_ID@", option_text);
        add(&mut replacements, "@OPTION_TEXT_TYPE@", ctype(program, Type::Object(option_text)));
        add(&mut replacements, "@ADDRESS_LIST_TYPE@", ctype(program, Type::Object(list)));
        add(&mut replacements, "@OPTION_PORT_ID@", option_port);
        add(&mut replacements, "@OPTION_PORT_TYPE@", ctype(program, Type::Object(option_port)));
        add_result(
            &mut replacements, program,
            "@URI_PORT_RESULT_TYPE@", "@URI_PORT_RESULT_ID@", "@URI_PORT_OK_TAG@", "@URI_PORT_ERR_TAG@",
            uri_port,
        );
        add(&mut replacements, "@URI_TEXT_NONE_TAG@", variant_id(program, option_text, "None"));
        add(&mut replacements, "@URI_TEXT_SOME_TAG@", variant_id(program, option_text, "Some"));
        add(&mut replacements, "@URI_PORT_NONE_TAG@", variant_id(program, option_port, "None"));
        add(&mut replacements, "@URI_PORT_SOME_TAG@", variant_id(program, option_port, "Some"));
        add(&mut replacements, "@DNS_RESOLVE_RESULT_ID@", resolver_result);
        add(&mut replacements, "@DNS_RESOLVE_RESULT_TYPE@", ctype(program, Type::Object(resolver_result)));
        add(&mut replacements, "@DNS_RESOLVE_OK_TAG@", resolver_ok_tag);

        for (key, function) in [
            ("@URI_SCHEME_FN@", uri_scheme),
            ("@URI_USER_INFO_FN@", uri_user_info),
            ("@URI_HOST_FN@", uri_host),
            ("@URI_PATH_FN@", uri_path),
            ("@URI_PORT_FN@", uri_port),
            ("@URI_QUERY_FN@", uri_query),
            ("@URI_FRAGMENT_FN@", uri_fragment),
        ] {
            add_method(&mut replacements, key, function);
        }
        add(&mut replacements, "@TEXT_UTF8_VALID@", "ncr_utf8_valid");
    }

    if has_receive_core {
        let buffer = class_id(program, |native| matches!(native, C::ByteBuffer));
        add(&mut replacements, "@BUFFER_ID@", buffer);
        add(&mut replacements, "@BUFFER_TYPE@", ctype(program, Type::Object(buffer)));
        add(&mut replacements, "@BUFFER_RESERVE@", format!("ncr_byte_buffer_reserve_{buffer}"));
        add(&mut replacements, "@TEXT_UTF8_VALID@", "ncr_utf8_valid");
    }
    if as_text.is_some() {
        let text = class_id(program, |native| matches!(native, C::Text));
        add(&mut replacements, "@TEXT_ID@", text);
        add(&mut replacements, "@TEXT_TYPE@", ctype(program, Type::Object(text)));
        add(&mut replacements, "@TEXT_FROM_BYTES@", format!("ncr_text_from_bytes_{text}"));
        add(&mut replacements, "@TEXT_UTF8_VALID@", "ncr_utf8_valid");
    }

    if let Some(function) = connect {
        add_result(&mut replacements, program, "@CONNECT_RESULT_TYPE@", "@CONNECT_RESULT_ID@", "@CONNECT_OK_TAG@", "@CONNECT_ERR_TAG@", function);
        add_method(&mut replacements, "@CONNECT_FN@", function);
        add_args(&mut replacements, function, &[
            ("@CONNECT_URI_ARG@", 0), ("@CONNECT_TIMEOUT_ARG@", 1), ("@CONNECT_MAX_MESSAGE_ARG@", 2),
        ]);
        add_param_types(&mut replacements, program, function, &[
            ("@CONNECT_URI_PARAM_TYPE@", 0), ("@CONNECT_TIMEOUT_PARAM_TYPE@", 1),
            ("@CONNECT_MAX_MESSAGE_PARAM_TYPE@", 2),
        ]);
    }
    if let Some(function) = connect_tls {
        add_result(&mut replacements, program, "@CONNECT_TLS_RESULT_TYPE@", "@CONNECT_TLS_RESULT_ID@", "@CONNECT_TLS_OK_TAG@", "@CONNECT_TLS_ERR_TAG@", function);
        add_method(&mut replacements, "@CONNECT_TLS_FN@", function);
        add_args(&mut replacements, function, &[
            ("@CONNECT_TLS_URI_ARG@", 0), ("@CONNECT_TLS_CONFIG_ARG@", 1),
            ("@CONNECT_TLS_TIMEOUT_ARG@", 2), ("@CONNECT_TLS_MAX_MESSAGE_ARG@", 3),
        ]);
        add_param_types(&mut replacements, program, function, &[
            ("@CONNECT_TLS_URI_PARAM_TYPE@", 0), ("@CONNECT_TLS_CONFIG_PARAM_TYPE@", 1),
            ("@CONNECT_TLS_TIMEOUT_PARAM_TYPE@", 2), ("@CONNECT_TLS_MAX_MESSAGE_PARAM_TYPE@", 3),
        ]);
    }
    if let Some(function) = send_text {
        add_result(&mut replacements, program, "@SEND_TEXT_RESULT_TYPE@", "@SEND_TEXT_RESULT_ID@", "@SEND_TEXT_OK_TAG@", "@SEND_TEXT_ERR_TAG@", function);
        add_method(&mut replacements, "@SEND_TEXT_FN@", function);
        add_args(&mut replacements, function, &[("@SEND_TEXT_ARG@", 0), ("@SEND_TEXT_TIMEOUT_ARG@", 1)]);
        add_param_types(&mut replacements, program, function, &[
            ("@SEND_TEXT_PARAM_TYPE@", 0), ("@SEND_TEXT_TIMEOUT_PARAM_TYPE@", 1),
        ]);
    }
    if let Some(function) = send_binary {
        add_result(&mut replacements, program, "@SEND_BINARY_RESULT_TYPE@", "@SEND_BINARY_RESULT_ID@", "@SEND_BINARY_OK_TAG@", "@SEND_BINARY_ERR_TAG@", function);
        add_method(&mut replacements, "@SEND_BINARY_FN@", function);
        add_args(&mut replacements, function, &[("@SEND_BINARY_ARG@", 0), ("@SEND_BINARY_TIMEOUT_ARG@", 1)]);
        add_param_types(&mut replacements, program, function, &[
            ("@SEND_BINARY_PARAM_TYPE@", 0), ("@SEND_BINARY_TIMEOUT_PARAM_TYPE@", 1),
        ]);
    }
    if let Some(function) = ping {
        add_result(&mut replacements, program, "@PING_RESULT_TYPE@", "@PING_RESULT_ID@", "@PING_OK_TAG@", "@PING_ERR_TAG@", function);
        add_method(&mut replacements, "@PING_FN@", function);
        add_args(&mut replacements, function, &[("@PING_ARG@", 0), ("@PING_TIMEOUT_ARG@", 1)]);
        add_param_types(&mut replacements, program, function, &[
            ("@PING_PARAM_TYPE@", 0), ("@PING_TIMEOUT_PARAM_TYPE@", 1),
        ]);
    }
    if let Some(function) = receive {
        add_result(&mut replacements, program, "@RECEIVE_RESULT_TYPE@", "@RECEIVE_RESULT_ID@", "@RECEIVE_OK_TAG@", "@RECEIVE_ERR_TAG@", function);
        add_method(&mut replacements, "@RECEIVE_FN@", function);
        add_args(&mut replacements, function, &[("@RECEIVE_TIMEOUT_ARG@", 0)]);
        add_param_types(&mut replacements, program, function, &[("@RECEIVE_TIMEOUT_PARAM_TYPE@", 0)]);
        let (_, ok) = variant(program, result_id(function), "Ok");
        let Type::Object(option_message) = ok.fields[0] else {
            unreachable!("WebSocket.receive Ok payload is Option<WebSocketMessage>")
        };
        add(&mut replacements, "@MESSAGE_TYPE@", ctype(program, Type::Object(
            message.expect("WebSocket.receive uses WebSocketMessage"),
        )));
        add(&mut replacements, "@OPTION_MESSAGE_TYPE@", ctype(program, Type::Object(option_message)));
        add(&mut replacements, "@RECEIVE_NONE_TAG@", variant_id(program, option_message, "None"));
        add(&mut replacements, "@RECEIVE_SOME_TAG@", variant_id(program, option_message, "Some"));
    }
    if let Some(function) = close {
        add_result(&mut replacements, program, "@CLOSE_RESULT_TYPE@", "@CLOSE_RESULT_ID@", "@CLOSE_OK_TAG@", "@CLOSE_ERR_TAG@", function);
        add_method(&mut replacements, "@CLOSE_FN@", function);
        add_args(&mut replacements, function, &[
            ("@CLOSE_CODE_ARG@", 0), ("@CLOSE_REASON_ARG@", 1), ("@CLOSE_TIMEOUT_ARG@", 2),
        ]);
        add_param_types(&mut replacements, program, function, &[
            ("@CLOSE_CODE_PARAM_TYPE@", 0), ("@CLOSE_REASON_PARAM_TYPE@", 1),
            ("@CLOSE_TIMEOUT_PARAM_TYPE@", 2),
        ]);
    }
    if is_text.is_some() || body.is_some() || as_text.is_some() {
        add(&mut replacements, "@MESSAGE_SELF_TYPE@", ctype(program, Type::Object(
            message.expect("WebSocket message methods require WebSocketMessage class"),
        )));
    }
    if let Some(function) = is_text { add_method(&mut replacements, "@MESSAGE_IS_TEXT_FN@", function); }
    if let Some(function) = body {
        add_method(&mut replacements, "@MESSAGE_BODY_FN@", function);
        add(&mut replacements, "@MESSAGE_BODY_RETURN_TYPE@", ctype(program, function.return_type));
    }
    if let Some(function) = as_text {
        add_result(&mut replacements, program, "@AS_TEXT_RESULT_TYPE@", "@AS_TEXT_RESULT_ID@", "@AS_TEXT_OK_TAG@", "@AS_TEXT_ERR_TAG@", function);
        add_method(&mut replacements, "@MESSAGE_AS_TEXT_FN@", function);
    }

    let output = replace_placeholders(output, &replacements);
    debug_assert!(!has_placeholder(&output), "unexpanded WebSocket placeholder");
    output
}
