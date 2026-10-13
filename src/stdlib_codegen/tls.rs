use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    has_placeholder, native_class_id, native_class_id_if, native_function_if,
    replace_placeholders, result_id, template_section, variant_id,
};

fn method_if(program: &t::Program, native: M) -> Option<&t::Function> {
    native_function_if(program, |candidate| candidate == native)
}
fn class_id(program: &t::Program, is: impl Fn(C) -> bool) -> usize {
    native_class_id(program, is)
}

fn class_id_if(program: &t::Program, is: impl Fn(C) -> bool) -> Option<usize> {
    native_class_id_if(program, is)
}


fn group<'a>(source: &'a str, name: &str) -> &'a str {
    let (start, end) = match name {
        "common" => ("/* TLS_GROUP common */", "/* TLS_GROUP_END common */"),
        "ssl_release" => ("/* TLS_GROUP ssl_release */", "/* TLS_GROUP_END ssl_release */"),
        "config_drop" => ("/* TLS_GROUP config_drop */", "/* TLS_GROUP_END config_drop */"),
        "stream_drop" => ("/* TLS_GROUP stream_drop */", "/* TLS_GROUP_END stream_drop */"),
        "client_host_ascii" => ("/* TLS_GROUP client_host_ascii */", "/* TLS_GROUP_END client_host_ascii */"),
        "client_host" => ("/* TLS_GROUP client_host */", "/* TLS_GROUP_END client_host */"),
        "stream_helpers" => ("/* TLS_GROUP stream_helpers */", "/* TLS_GROUP_END stream_helpers */"),
        "config_core" => ("/* TLS_GROUP config_core */", "/* TLS_GROUP_END config_core */"),
        "config_identity" => ("/* TLS_GROUP config_identity */", "/* TLS_GROUP_END config_identity */"),
        "config_client" => ("/* TLS_GROUP config_client */", "/* TLS_GROUP_END config_client */"),
        "config_client_with_ca" => ("/* TLS_GROUP config_client_with_ca */", "/* TLS_GROUP_END config_client_with_ca */"),
        "config_client_with_identity" => ("/* TLS_GROUP config_client_with_identity */", "/* TLS_GROUP_END config_client_with_identity */"),
        "config_server" => ("/* TLS_GROUP config_server */", "/* TLS_GROUP_END config_server */"),
        "config_server_with_client_auth" => ("/* TLS_GROUP config_server_with_client_auth */", "/* TLS_GROUP_END config_server_with_client_auth */"),
        "stream_dispose" => ("/* TLS_GROUP stream_dispose */", "/* TLS_GROUP_END stream_dispose */"),
        "stream_read_core" => ("/* TLS_GROUP stream_read_core */", "/* TLS_GROUP_END stream_read_core */"),
        "stream_write_core" => ("/* TLS_GROUP stream_write_core */", "/* TLS_GROUP_END stream_write_core */"),
        "stream_text_query" => ("/* TLS_GROUP stream_text_query */", "/* TLS_GROUP_END stream_text_query */"),
        "stream_connect" => ("/* TLS_GROUP stream_connect */", "/* TLS_GROUP_END stream_connect */"),
        "stream_accept" => ("/* TLS_GROUP stream_accept */", "/* TLS_GROUP_END stream_accept */"),
        "stream_read_some" => ("/* TLS_GROUP stream_read_some */", "/* TLS_GROUP_END stream_read_some */"),
        "stream_read" => ("/* TLS_GROUP stream_read */", "/* TLS_GROUP_END stream_read */"),
        "stream_write_some" => ("/* TLS_GROUP stream_write_some */", "/* TLS_GROUP_END stream_write_some */"),
        "stream_write" => ("/* TLS_GROUP stream_write */", "/* TLS_GROUP_END stream_write */"),
        "stream_write_all" => ("/* TLS_GROUP stream_write_all */", "/* TLS_GROUP_END stream_write_all */"),
        "stream_cipher" => ("/* TLS_GROUP stream_cipher */", "/* TLS_GROUP_END stream_cipher */"),
        "stream_protocol" => ("/* TLS_GROUP stream_protocol */", "/* TLS_GROUP_END stream_protocol */"),
        "stream_peer_certificate" => ("/* TLS_GROUP stream_peer_certificate */", "/* TLS_GROUP_END stream_peer_certificate */"),
        "stream_flush" => ("/* TLS_GROUP stream_flush */", "/* TLS_GROUP_END stream_flush */"),
        "stream_close" => ("/* TLS_GROUP stream_close */", "/* TLS_GROUP_END stream_close */"),
        _ => unreachable!("unknown TLS template group"),
    };
    template_section(source, start, end)
}

fn add(replacements: &mut Vec<(&'static str, String)>, key: &'static str, value: impl ToString) {
    replacements.push((key, value.to_string()));
}

fn add_function(replacements: &mut Vec<(&'static str, String)>, key: &'static str, function: &t::Function) {
    add(replacements, key, format!("ncr_fn_{}", function.id));
}

fn add_result(replacements: &mut Vec<(&'static str, String)>, key: &'static str, function: &t::Function) {
    add(replacements, key, result_id(function));
}

fn add_result_type(replacements: &mut Vec<(&'static str, String)>, key: &'static str, function: &t::Function) {
    let Type::Object(id) = function.return_type else {
        unreachable!("TLS native result returns an object enum")
    };
    add(replacements, key, id);
}

fn add_ok_tag_for_program(
    program: &t::Program,
    replacements: &mut Vec<(&'static str, String)>,
    key: &'static str,
    function: &t::Function,
) {
    add(replacements, key, variant_id(program, result_id(function), "Ok"));
}

fn param(function: &t::Function, index: usize) -> usize {
    function.params[index].0
}


pub(super) fn fields(program: &t::Program, class: C, _class_id: usize) -> String {
    match class {
        C::TlsConfig => "    void *context;\n    void *certificate;\n    void *key;\n    bool server;\n".to_owned(),
        C::TlsStream => {
            let config = class_id(program, |native| matches!(native, C::TlsConfig));
            let mutex = class_id(program, |native| matches!(native, C::Mutex(Type::Object(value)) if value == config));
            format!("    int fd;\n    void *ssl;\n    ncr_obj_{mutex} *config;\n")
        }
        _ => unreachable!("TLS storage requested for another native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::TlsConfig => {
            let mut fields = "    ncr_tls_config_release(p);\n".to_owned();
            if let Some(certificate) = class_id_if(program, |native| matches!(native, C::Certificate)) {
                fields.push_str(&format!("    ncr_drop_{certificate}(p->certificate);\n    p->certificate = NULL;\n"));
            }
            if let Some(key) = class_id_if(program, |native| matches!(native, C::CryptoKey)) {
                fields.push_str(&format!("    ncr_drop_{key}(p->key);\n    p->key = NULL;\n"));
            }
            fields
        }
        C::TlsStream => "    (void)ncr_tls_stream_release(p);\n".to_owned(),
        _ => unreachable!("TLS drop requested for another native class"),
    }
}

pub(crate) fn drop_helper_prototypes(program: &t::Program) -> String {
    let config = class_id_if(program, |class| matches!(class, C::TlsConfig));
    let stream = class_id_if(program, |class| matches!(class, C::TlsStream));
    let websocket = class_id_if(program, |class| matches!(class, C::WebSocket));
    let mut out = String::new();
    if stream.is_some() || websocket.is_some() {
        out.push_str("static void ncr_tls_ssl_release(void *ssl);\n");
    }
    if let Some(config) = config {
        out.push_str(&format!("static void ncr_tls_config_release(ncr_obj_{config} *config);\n"));
    }
    if let Some(stream) = stream {
        out.push_str(&format!("static int ncr_tls_stream_release(ncr_obj_{stream} *stream);\n"));
    }
    out
}

pub(super) fn implementation(program: &t::Program) -> String {
    let config_class = class_id_if(program, |native| matches!(native, C::TlsConfig));
    let stream_class = class_id_if(program, |native| matches!(native, C::TlsStream));
    let websocket_class = class_id_if(program, |native| matches!(native, C::WebSocket));
    if config_class.is_none() && stream_class.is_none() && websocket_class.is_none() {
        return String::new();
    }

    let config_client = method_if(program, M::TlsConfigClient);
    let config_client_with_ca = method_if(program, M::TlsConfigClientWithCa);
    let config_client_with_identity = method_if(program, M::TlsConfigClientWithIdentity);
    let config_server = method_if(program, M::TlsConfigServer);
    let config_server_with_client_auth = method_if(program, M::TlsConfigServerWithClientAuth);
    let stream_connect = method_if(program, M::TlsStreamConnect);
    let stream_accept = method_if(program, M::TlsStreamAccept);
    let stream_read_some = method_if(program, M::TlsStreamReadSome);
    let stream_read = method_if(program, M::TlsStreamRead);
    let stream_write_some = method_if(program, M::TlsStreamWriteSome);
    let stream_write = method_if(program, M::TlsStreamWrite);
    let stream_write_all = method_if(program, M::TlsStreamWriteAll);
    let stream_cipher = method_if(program, M::TlsStreamCipher);
    let stream_protocol = method_if(program, M::TlsStreamProtocol);
    let stream_peer_certificate = method_if(program, M::TlsStreamPeerCertificate);
    let stream_flush = method_if(program, M::TlsStreamFlush);
    let stream_close = method_if(program, M::TlsStreamClose);
    let websocket_connect_tls = method_if(program, M::WebSocketConnectTls);

    let has_config = config_class.is_some();
    let has_stream = stream_class.is_some();
    let has_websocket = websocket_class.is_some();
    let has_config_methods = config_client.is_some();
    let has_stream_methods = stream_connect.is_some() || stream_accept.is_some();
    let has_tls_helpers = has_stream_methods || websocket_connect_tls.is_some();
    let has_client_host = stream_connect.is_some();
    let has_client_host_ascii = has_client_host || websocket_connect_tls.is_some();

    let template = include_str!("../../stdlib/tls.c.in");
    let mut body = String::new();
    body.push_str(template.split_once("/* TLS_GROUP common */")
        .expect("TLS template common group").0);

    if has_config_methods || has_stream_methods { body.push_str(group(template, "common")); }
    if has_stream || has_websocket { body.push_str(group(template, "ssl_release")); }
    if has_config { body.push_str(group(template, "config_drop")); }
    if has_stream { body.push_str(group(template, "stream_drop")); }
    if has_client_host_ascii { body.push_str(group(template, "client_host_ascii")); }
    if has_client_host { body.push_str(group(template, "client_host")); }
    if has_tls_helpers { body.push_str(group(template, "stream_helpers")); }
    if has_config_methods {
        body.push_str(group(template, "config_core"));
        if config_client_with_identity.is_some()
            || config_server.is_some()
            || config_server_with_client_auth.is_some()
        {
            body.push_str(group(template, "config_identity"));
        }
        for (function, group_name) in [
            (config_client, "config_client"),
            (config_client_with_ca, "config_client_with_ca"),
            (config_client_with_identity, "config_client_with_identity"),
            (config_server, "config_server"),
            (config_server_with_client_auth, "config_server_with_client_auth"),
        ] {
            if function.is_some() { body.push_str(group(template, group_name)); }
        }
    }
    if has_stream_methods {
        if stream_write_some.is_some() || stream_write.is_some() || stream_write_all.is_some() {
            body.push_str(group(template, "stream_dispose"));
        }
        if stream_read_some.is_some() || stream_read.is_some() {
            body.push_str(group(template, "stream_read_core"));
        }
        if stream_write_some.is_some() || stream_write.is_some() {
            body.push_str(group(template, "stream_write_core"));
        }
        if stream_cipher.is_some() || stream_protocol.is_some() {
            body.push_str(group(template, "stream_text_query"));
        }
        for (function, group_name) in [
            (stream_connect, "stream_connect"),
            (stream_accept, "stream_accept"),
            (stream_read_some, "stream_read_some"),
            (stream_read, "stream_read"),
            (stream_write_some, "stream_write_some"),
            (stream_write, "stream_write"),
            (stream_write_all, "stream_write_all"),
            (stream_cipher, "stream_cipher"),
            (stream_protocol, "stream_protocol"),
            (stream_peer_certificate, "stream_peer_certificate"),
            (stream_flush, "stream_flush"),
            (stream_close, "stream_close"),
        ] {
            if function.is_some() { body.push_str(group(template, group_name)); }
        }
    }

    let mut replacements = vec![];
    if has_config {
        add(&mut replacements, "@TLS_CONFIG_ID@", class_id(program, |native| matches!(native, C::TlsConfig)));
    }
    if has_stream {
        let stream = class_id(program, |native| matches!(native, C::TlsStream));
        add(&mut replacements, "@TLS_STREAM_ID@", stream);
    }
    if has_stream || has_tls_helpers {
        let config = class_id(program, |native| matches!(native, C::TlsConfig));
        let mutex = class_id(program, |native| matches!(native, C::Mutex(Type::Object(value)) if value == config));
        add(&mut replacements, "@TLS_CONFIG_ID@", config);
        add(&mut replacements, "@CONFIG_MUTEX_ID@", mutex);
    }
    if has_client_host_ascii || has_client_host || has_stream_methods || has_config_methods {
        add(&mut replacements, "@TEXT_ID@", class_id(program, |native| matches!(native, C::Text)));
    }
    if has_config_methods {
        let factory = config_client.expect("TlsConfig.client is a native method");
        add_result_type(&mut replacements, "@CONFIG_RESULT_TYPE@", factory);
        add_result(&mut replacements, "@CONFIG_RESULT_ID@", factory);
        add_ok_tag_for_program(program, &mut replacements, "@CONFIG_OK_TAG@", factory);
        if let Some(function) = config_client { add_function(&mut replacements, "@TLS_CONFIG_CLIENT_FN@", function); }
        if let Some(function) = config_client_with_ca { add_function(&mut replacements, "@TLS_CONFIG_CLIENT_WITH_CA_FN@", function); }
        if let Some(function) = config_client_with_identity { add_function(&mut replacements, "@TLS_CONFIG_CLIENT_WITH_IDENTITY_FN@", function); }
        if let Some(function) = config_server { add_function(&mut replacements, "@TLS_CONFIG_SERVER_FN@", function); }
        if let Some(function) = config_server_with_client_auth {
            add_function(&mut replacements, "@TLS_CONFIG_SERVER_WITH_CLIENT_AUTH_FN@", function);
        }
        if let Some(function) = config_client_with_ca {
            add(&mut replacements, "@PATH_ID@", class_id(program, |native| matches!(native, C::Path)));
            add(&mut replacements, "@CLIENT_WITH_CA_PATH_ID@", param(function, 0));
        }
        if let Some(function) = config_client_with_identity {
            add(&mut replacements, "@CERTIFICATE_ID@", class_id(program, |native| matches!(native, C::Certificate)));
            add(&mut replacements, "@CRYPTO_KEY_ID@", class_id(program, |native| matches!(native, C::CryptoKey)));
            add(&mut replacements, "@PATH_ID@", class_id(program, |native| matches!(native, C::Path)));
            add(&mut replacements, "@CLIENT_WITH_IDENTITY_CERT_ID@", param(function, 0));
            add(&mut replacements, "@CLIENT_WITH_IDENTITY_KEY_ID@", param(function, 1));
            add(&mut replacements, "@CLIENT_WITH_IDENTITY_CA_ID@", param(function, 2));
        }
        if let Some(function) = config_server {
            add(&mut replacements, "@CERTIFICATE_ID@", class_id(program, |native| matches!(native, C::Certificate)));
            add(&mut replacements, "@CRYPTO_KEY_ID@", class_id(program, |native| matches!(native, C::CryptoKey)));
            add(&mut replacements, "@SERVER_CERT_ID@", param(function, 0));
            add(&mut replacements, "@SERVER_KEY_ID@", param(function, 1));
        }
        if let Some(function) = config_server_with_client_auth {
            add(&mut replacements, "@CERTIFICATE_ID@", class_id(program, |native| matches!(native, C::Certificate)));
            add(&mut replacements, "@CRYPTO_KEY_ID@", class_id(program, |native| matches!(native, C::CryptoKey)));
            add(&mut replacements, "@PATH_ID@", class_id(program, |native| matches!(native, C::Path)));
            add(&mut replacements, "@SERVER_CLIENT_AUTH_CERT_ID@", param(function, 0));
            add(&mut replacements, "@SERVER_CLIENT_AUTH_KEY_ID@", param(function, 1));
            add(&mut replacements, "@SERVER_CLIENT_AUTH_CA_ID@", param(function, 2));
        }
    }
    if has_stream_methods {
        let stream = class_id(program, |native| matches!(native, C::TlsStream));
        let config = class_id(program, |native| matches!(native, C::TlsConfig));
        let mutex = class_id(program, |native| matches!(native, C::Mutex(Type::Object(value)) if value == config));
        add(&mut replacements, "@TLS_STREAM_ID@", stream);
        add(&mut replacements, "@TLS_CONFIG_ID@", config);
        add(&mut replacements, "@CONFIG_MUTEX_ID@", mutex);
        add(&mut replacements, "@TEXT_ID@", class_id(program, |native| matches!(native, C::Text)));

        let connect = stream_connect;
        let accept = stream_accept;
        if let Some(stream_result) = connect.or(accept) {
            add_result_type(&mut replacements, "@STREAM_RESULT_TYPE@", stream_result);
            add_result(&mut replacements, "@STREAM_RESULT_ID@", stream_result);
            add_ok_tag_for_program(program, &mut replacements, "@STREAM_OK_TAG@", stream_result);
        }
        if let Some(function) = connect {
            add(&mut replacements, "@ADDRESS_ID@", class_id(program, |native| matches!(native, C::SocketAddress)));
            add_function(&mut replacements, "@TLS_STREAM_CONNECT_FN@", function);
            add(&mut replacements, "@CONNECT_ADDRESS_ID@", param(function, 0));
            add(&mut replacements, "@CONNECT_CONFIG_ID@", param(function, 1));
            add(&mut replacements, "@CONNECT_HOST_ID@", param(function, 2));
            add(&mut replacements, "@CONNECT_TIMEOUT_ID@", param(function, 3));
        }
        if let Some(function) = accept {
            add(&mut replacements, "@LISTENER_ID@", class_id(program, |native| matches!(native, C::TcpListener)));
            add_function(&mut replacements, "@TLS_STREAM_ACCEPT_FN@", function);
            add(&mut replacements, "@ACCEPT_LISTENER_ID@", param(function, 0));
            add(&mut replacements, "@ACCEPT_CONFIG_ID@", param(function, 1));
            add(&mut replacements, "@ACCEPT_TIMEOUT_ID@", param(function, 2));
        }

        let read_some = stream_read_some;
        let read = stream_read;
        let write_some = stream_write_some;
        let write = stream_write;
        let write_all = stream_write_all;
        let long_result = read_some.or(read).or(write_some).or(write);
        if let Some(function) = long_result {
            add_result_type(&mut replacements, "@LONG_RESULT_TYPE@", function);
            add_result(&mut replacements, "@LONG_RESULT_ID@", function);
            add_ok_tag_for_program(program, &mut replacements, "@LONG_OK_TAG@", function);
        }
        if read_some.is_some() || read.is_some() || write_some.is_some() || write.is_some() || write_all.is_some() {
            let buffer = class_id(program, |native| matches!(native, C::ByteBuffer));
            add(&mut replacements, "@BUFFER_ID@", buffer);
        }
        if read_some.is_some() || read.is_some() {
            let buffer = class_id(program, |native| matches!(native, C::ByteBuffer));
            add(&mut replacements, "@BUFFER_RESERVE@", format!("ncr_byte_buffer_reserve_{buffer}"));
            let function = read_some.or(read).expect("TLS read method is present");
            add(&mut replacements, "@READ_DESTINATION_ID@", param(function, 0));
            add(&mut replacements, "@READ_MAX_ID@", param(function, 1));
        }
        if let Some(function) = read_some {
            add_function(&mut replacements, "@TLS_STREAM_READ_SOME_FN@", function);
            add(&mut replacements, "@READ_TIMEOUT_ID@", param(function, 2));
        }
        if let Some(function) = read {
            add_function(&mut replacements, "@TLS_STREAM_READ_FN@", function);
        }
        if write_some.is_some() || write.is_some() {
            let function = write_some.or(write).expect("TLS write method is present");
            add(&mut replacements, "@WRITE_SOURCE_ID@", param(function, 0));
            add(&mut replacements, "@WRITE_OFFSET_ID@", param(function, 1));
            add(&mut replacements, "@WRITE_MAX_ID@", param(function, 2));
        }
        if let Some(function) = write_some {
            add_function(&mut replacements, "@TLS_STREAM_WRITE_SOME_FN@", function);
            add(&mut replacements, "@WRITE_TIMEOUT_ID@", param(function, 3));
        }
        if let Some(function) = write {
            add_function(&mut replacements, "@TLS_STREAM_WRITE_FN@", function);
        }
        if let Some(function) = write_all {
            add_function(&mut replacements, "@TLS_STREAM_WRITE_ALL_FN@", function);
            add(&mut replacements, "@WRITE_ALL_SOURCE_ID@", param(function, 0));
            add(&mut replacements, "@WRITE_ALL_TIMEOUT_ID@", param(function, 1));
        }
        if let Some(function) = write_all.or(stream_flush) {
            add_result_type(&mut replacements, "@UNIT_RESULT_TYPE@", function);
            add_result(&mut replacements, "@UNIT_RESULT_ID@", function);
            add_ok_tag_for_program(program, &mut replacements, "@UNIT_OK_TAG@", function);
        }
        if let Some(function) = stream_flush {
            add_function(&mut replacements, "@TLS_STREAM_FLUSH_FN@", function);
        }

        let cipher = stream_cipher;
        let protocol = stream_protocol;
        if let Some(function) = cipher.or(protocol) {
            add_result_type(&mut replacements, "@TEXT_RESULT_TYPE@", function);
            add_result(&mut replacements, "@TEXT_RESULT_ID@", function);
            add_ok_tag_for_program(program, &mut replacements, "@TEXT_OK_TAG@", function);
        }
        if let Some(function) = cipher { add_function(&mut replacements, "@TLS_STREAM_CIPHER_FN@", function); }
        if let Some(function) = protocol { add_function(&mut replacements, "@TLS_STREAM_PROTOCOL_FN@", function); }

        if let Some(function) = stream_peer_certificate {
            let peer_result = result_id(function);
            let Type::Object(option) = program.classes[peer_result].result
                .expect("peer certificate Result layout").ok_type
                else { unreachable!("peerCertificate returns Option<Certificate>") };
            add(&mut replacements, "@CERTIFICATE_ID@", class_id(program, |native| matches!(native, C::Certificate)));
            add_result_type(&mut replacements, "@PEER_RESULT_TYPE@", function);
            add_result(&mut replacements, "@PEER_RESULT_ID@", function);
            add_ok_tag_for_program(program, &mut replacements, "@PEER_OK_TAG@", function);
            add(&mut replacements, "@PEER_OPTION_ID@", option);
            add(&mut replacements, "@PEER_OPTION_SOME_TAG@", variant_id(program, option, "Some"));
            add(&mut replacements, "@PEER_OPTION_NONE_TAG@", variant_id(program, option, "None"));
            add_function(&mut replacements, "@TLS_STREAM_PEER_CERTIFICATE_FN@", function);
        }
        if let Some(function) = stream_close {
            add_result_type(&mut replacements, "@CLOSE_RESULT_TYPE@", function);
            add_result(&mut replacements, "@CLOSE_RESULT_ID@", function);
            add_ok_tag_for_program(program, &mut replacements, "@CLOSE_OK_TAG@", function);
            add_function(&mut replacements, "@TLS_STREAM_CLOSE_FN@", function);
        }
    }

    let output = replace_placeholders(body, &replacements);
    debug_assert!(!has_placeholder(&output), "unexpanded TLS placeholder");
    output
}
