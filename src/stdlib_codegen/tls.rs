use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{has_placeholder, replace_placeholders, result_id, variant_id};

fn class_id(program: &t::Program, is: impl Fn(C) -> bool) -> usize {
    program.classes.iter().find(|class| class.native.is_some_and(&is))
        .expect("TLS signature specializes required native class").id
}
fn class_id_if(program: &t::Program, is: impl Fn(C) -> bool) -> Option<usize> {
    program.classes.iter().find(|class| class.native.is_some_and(&is)).map(|class| class.id)
}


fn method_if(program: &t::Program, native: M) -> Option<&t::Function> {
    program.functions.iter().find(|function| function.native == Some(native))
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

fn has_any(program: &t::Program, methods: &[M]) -> bool {
    program.functions.iter().any(|function| function.native.is_some_and(|method| methods.contains(&method)))
}

fn group(source: &str, name: &str) -> String {
    let start = format!("/* TLS_GROUP {name} */");
    let end = format!("/* TLS_GROUP_END {name} */");
    source.split_once(&start).expect("TLS template group opener").1
        .split_once(&end).expect("TLS template group closer").0.to_owned()
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
    let has_config = program.classes.iter().any(|class| class.native == Some(C::TlsConfig));
    let has_stream = program.classes.iter().any(|class| class.native == Some(C::TlsStream));
    let has_websocket = program.classes.iter().any(|class| class.native == Some(C::WebSocket));
    let mut out = String::new();
    if has_stream || has_websocket {
        out.push_str("static void ncr_tls_ssl_release(void *ssl);\n");
    }
    if has_config {
        let config = class_id(program, |native| matches!(native, C::TlsConfig));
        out.push_str(&format!("static void ncr_tls_config_release(ncr_obj_{config} *config);\n"));
    }
    if has_stream {
        let stream = class_id(program, |native| matches!(native, C::TlsStream));
        out.push_str(&format!("static int ncr_tls_stream_release(ncr_obj_{stream} *stream);\n"));
    }
    out
}

pub(super) fn implementation(program: &t::Program) -> String {
    const CONFIG_METHODS: &[M] = &[
        M::TlsConfigClient, M::TlsConfigClientWithCa, M::TlsConfigClientWithIdentity,
        M::TlsConfigServer, M::TlsConfigServerWithClientAuth,
    ];
    const STREAM_METHODS: &[M] = &[
        M::TlsStreamConnect, M::TlsStreamAccept,
        M::TlsStreamReadSome, M::TlsStreamWriteSome, M::TlsStreamWriteAll,
        M::TlsStreamCipher, M::TlsStreamProtocol, M::TlsStreamPeerCertificate,
        M::TlsStreamClose, M::TlsStreamRead, M::TlsStreamWrite, M::TlsStreamFlush,
    ];
    const TLS_WEBSOCKET_METHODS: &[M] = &[M::WebSocketConnectTls];
    const CLIENT_HOST_METHODS: &[M] = &[M::TlsStreamConnect];
    const CLIENT_HOST_ASCII_METHODS: &[M] = &[M::TlsStreamConnect, M::WebSocketConnectTls];

    let has_config = program.classes.iter().any(|class| class.native == Some(C::TlsConfig));
    let has_stream = program.classes.iter().any(|class| class.native == Some(C::TlsStream));
    let has_websocket = program.classes.iter().any(|class| class.native == Some(C::WebSocket));
    let has_secure_type = has_config || has_stream || has_websocket;
    if !has_secure_type { return String::new(); }

    let has_config_methods = has_any(program, CONFIG_METHODS);
    let has_stream_methods = has_any(program, STREAM_METHODS);
    let has_tls_helpers = has_stream_methods || has_any(program, TLS_WEBSOCKET_METHODS);
    let has_client_host = has_any(program, CLIENT_HOST_METHODS);
    let has_client_host_ascii = has_any(program, CLIENT_HOST_ASCII_METHODS);

    let template = include_str!("../../stdlib/tls.c.in");
    let mut body = template.split_once("/* TLS_GROUP common */")
        .expect("TLS template common group").0.to_owned();
    if has_config_methods || has_stream_methods { body.push_str(&group(template, "common")); }
    if has_stream || has_websocket { body.push_str(&group(template, "ssl_release")); }
    if has_config { body.push_str(&group(template, "config_drop")); }
    if has_stream { body.push_str(&group(template, "stream_drop")); }
    if has_client_host_ascii { body.push_str(&group(template, "client_host_ascii")); }
    if has_client_host { body.push_str(&group(template, "client_host")); }
    if has_config_methods {
        body.push_str(&group(template, "config_core"));
        if has_any(program, &[M::TlsConfigClientWithIdentity, M::TlsConfigServer, M::TlsConfigServerWithClientAuth]) {
            body.push_str(&group(template, "config_identity"));
        }
        for (native, group_name) in [
            (M::TlsConfigClient, "config_client"),
            (M::TlsConfigClientWithCa, "config_client_with_ca"),
            (M::TlsConfigClientWithIdentity, "config_client_with_identity"),
            (M::TlsConfigServer, "config_server"),
            (M::TlsConfigServerWithClientAuth, "config_server_with_client_auth"),
        ] {
            if method_if(program, native).is_some() { body.push_str(&group(template, group_name)); }
        }
    }
    if has_stream_methods {
        if has_any(program, &[M::TlsStreamWriteSome, M::TlsStreamWrite, M::TlsStreamWriteAll]) {
            body.push_str(&group(template, "stream_dispose"));
        }
        if has_any(program, &[M::TlsStreamReadSome, M::TlsStreamRead]) {
            body.push_str(&group(template, "stream_read_core"));
        }
        if has_any(program, &[M::TlsStreamWriteSome, M::TlsStreamWrite]) {
            body.push_str(&group(template, "stream_write_core"));
        }
        if has_any(program, &[M::TlsStreamCipher, M::TlsStreamProtocol]) {
            body.push_str(&group(template, "stream_text_query"));
        }
        for (native, group_name) in [
            (M::TlsStreamConnect, "stream_connect"),
            (M::TlsStreamAccept, "stream_accept"),
            (M::TlsStreamReadSome, "stream_read_some"),
            (M::TlsStreamRead, "stream_read"),
            (M::TlsStreamWriteSome, "stream_write_some"),
            (M::TlsStreamWrite, "stream_write"),
            (M::TlsStreamWriteAll, "stream_write_all"),
            (M::TlsStreamCipher, "stream_cipher"),
            (M::TlsStreamProtocol, "stream_protocol"),
            (M::TlsStreamPeerCertificate, "stream_peer_certificate"),
            (M::TlsStreamFlush, "stream_flush"),
            (M::TlsStreamClose, "stream_close"),
        ] {
            if method_if(program, native).is_some() { body.push_str(&group(template, group_name)); }
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
        let factory = CONFIG_METHODS.iter().find_map(|native| method_if(program, *native))
            .expect("a TLS config method is active");
        add_result_type(&mut replacements, "@CONFIG_RESULT_TYPE@", factory);
        add_result(&mut replacements, "@CONFIG_RESULT_ID@", factory);
        add_ok_tag_for_program(program, &mut replacements, "@CONFIG_OK_TAG@", factory);
        for (native, marker) in [
            (M::TlsConfigClient, "@TLS_CONFIG_CLIENT_FN@"),
            (M::TlsConfigClientWithCa, "@TLS_CONFIG_CLIENT_WITH_CA_FN@"),
            (M::TlsConfigClientWithIdentity, "@TLS_CONFIG_CLIENT_WITH_IDENTITY_FN@"),
            (M::TlsConfigServer, "@TLS_CONFIG_SERVER_FN@"),
            (M::TlsConfigServerWithClientAuth, "@TLS_CONFIG_SERVER_WITH_CLIENT_AUTH_FN@"),
        ] {
            if let Some(function) = method_if(program, native) {
                add_function(&mut replacements, marker, function);
            }
        }
        if let Some(function) = method_if(program, M::TlsConfigClientWithCa) {
            add(&mut replacements, "@PATH_ID@", class_id(program, |native| matches!(native, C::Path)));
            add(&mut replacements, "@CLIENT_WITH_CA_PATH_ID@", param(function, 0));
        }
        if let Some(function) = method_if(program, M::TlsConfigClientWithIdentity) {
            add(&mut replacements, "@CERTIFICATE_ID@", class_id(program, |native| matches!(native, C::Certificate)));
            add(&mut replacements, "@CRYPTO_KEY_ID@", class_id(program, |native| matches!(native, C::CryptoKey)));
            add(&mut replacements, "@PATH_ID@", class_id(program, |native| matches!(native, C::Path)));
            add(&mut replacements, "@CLIENT_WITH_IDENTITY_CERT_ID@", param(function, 0));
            add(&mut replacements, "@CLIENT_WITH_IDENTITY_KEY_ID@", param(function, 1));
            add(&mut replacements, "@CLIENT_WITH_IDENTITY_CA_ID@", param(function, 2));
        }
        if let Some(function) = method_if(program, M::TlsConfigServer) {
            add(&mut replacements, "@CERTIFICATE_ID@", class_id(program, |native| matches!(native, C::Certificate)));
            add(&mut replacements, "@CRYPTO_KEY_ID@", class_id(program, |native| matches!(native, C::CryptoKey)));
            add(&mut replacements, "@SERVER_CERT_ID@", param(function, 0));
            add(&mut replacements, "@SERVER_KEY_ID@", param(function, 1));
        }
        if let Some(function) = method_if(program, M::TlsConfigServerWithClientAuth) {
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

        let connect = method_if(program, M::TlsStreamConnect);
        let accept = method_if(program, M::TlsStreamAccept);
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

        let read_some = method_if(program, M::TlsStreamReadSome);
        let read = method_if(program, M::TlsStreamRead);
        let write_some = method_if(program, M::TlsStreamWriteSome);
        let write = method_if(program, M::TlsStreamWrite);
        let write_all = method_if(program, M::TlsStreamWriteAll);
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
        if let Some(function) = write_all.or_else(|| method_if(program, M::TlsStreamFlush)) {
            add_result_type(&mut replacements, "@UNIT_RESULT_TYPE@", function);
            add_result(&mut replacements, "@UNIT_RESULT_ID@", function);
            add_ok_tag_for_program(program, &mut replacements, "@UNIT_OK_TAG@", function);
        }
        if let Some(function) = method_if(program, M::TlsStreamFlush) {
            add_function(&mut replacements, "@TLS_STREAM_FLUSH_FN@", function);
        }

        let cipher = method_if(program, M::TlsStreamCipher);
        let protocol = method_if(program, M::TlsStreamProtocol);
        if let Some(function) = cipher.or(protocol) {
            add_result_type(&mut replacements, "@TEXT_RESULT_TYPE@", function);
            add_result(&mut replacements, "@TEXT_RESULT_ID@", function);
            add_ok_tag_for_program(program, &mut replacements, "@TEXT_OK_TAG@", function);
        }
        if let Some(function) = cipher { add_function(&mut replacements, "@TLS_STREAM_CIPHER_FN@", function); }
        if let Some(function) = protocol { add_function(&mut replacements, "@TLS_STREAM_PROTOCOL_FN@", function); }

        if let Some(function) = method_if(program, M::TlsStreamPeerCertificate) {
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
        if let Some(function) = method_if(program, M::TlsStreamClose) {
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
