use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    native_function, replace_method_ids, replace_placeholders, result_id,
    template_section, variant_id,
};

pub(super) fn fields(program: &t::Program, class: C, _class_id: usize) -> String {
    match class {
        C::SocketAddress => "    struct sockaddr_storage address;\n    socklen_t length;\n    int family;\n".to_owned(),
        C::TcpListener | C::TcpStream | C::UdpSocket => "    int fd;\n".to_owned(),
        C::Datagram => {
            let address = native_class_id(program, |native| matches!(native, C::SocketAddress));
            format!("    ncr_obj_{address} *source;\n    int64_t bytes_read;\n    bool truncated;\n")
        }
        _ => unreachable!("network fields requested for a non-network native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::SocketAddress => String::new(),
        C::TcpListener | C::TcpStream | C::UdpSocket =>
            "    if (p->fd >= 0) { int owned = p->fd; p->fd = -1; (void)close(owned); }\n".to_owned(),
        C::Datagram => {
            let address = native_class_id(program, |native| matches!(native, C::SocketAddress));
            format!("    ncr_drop_{address}(p->source);\n    p->source = NULL;\n")
        }
        _ => unreachable!("network drop requested for a non-network native class"),
    }
}

fn native_class_id(program: &t::Program, is: impl Fn(C) -> bool) -> usize {
    program.classes.iter().find(|class| class.native.is_some_and(&is))
        .expect("network signature specializes required native class").id
}

fn method(program: &t::Program, native: M) -> &t::Function {
    native_function(program, |candidate| candidate == native)
}

fn method_if(program: &t::Program, native: M) -> Option<&t::Function> {
    program.functions.iter().find(|function| function.native == Some(native))
}

fn method_for_class(program: &t::Program, class_id: usize, native: M) -> &t::Function {
    program.functions.iter().find(|function| function.receiver == Some(class_id) && function.native == Some(native))
        .expect("compiler-owned specialized network method is present")
}


fn result_type(_program: &t::Program, function: &t::Function) -> String {
    match function.return_type {
        Type::Object(class) => class.to_string(),
        _ => unreachable!("network result signatures return an object enum"),
    }
}

fn param(function: &t::Function, index: usize) -> String {
    function.params[index].0.to_string()
}

fn add(substitutions: &mut Vec<(&'static str, String)>, key: &'static str, value: impl ToString) {
    substitutions.push((key, value.to_string()));
}

fn add_result(substitutions: &mut Vec<(&'static str, String)>, _program: &t::Program, key: &'static str, function: &t::Function) {
    let result = result_id(function);
    add(substitutions, key, result);
}

fn add_result_type(substitutions: &mut Vec<(&'static str, String)>, program: &t::Program, key: &'static str, function: &t::Function) {
    add(substitutions, key, result_type(program, function));
}

fn add_ok_tag(substitutions: &mut Vec<(&'static str, String)>, program: &t::Program, key: &'static str, function: &t::Function) {
    add(substitutions, key, variant_id(program, result_id(function), "Ok"));
}

fn group<'a>(source: &'a str, name: &str) -> &'a str {
    let start = format!("/* NETWORK_GROUP {name} */");
    let end = format!("/* NETWORK_GROUP_END {name} */");
    template_section(source, &start, &end)
}

fn has_any(program: &t::Program, methods: &[M]) -> bool {
    program.functions.iter().any(|function| function.native.is_some_and(|method| methods.contains(&method)))
}

pub(super) fn implementation(program: &t::Program) -> String {
    const NETWORK_METHODS: &[M] = &[
        M::SocketAddressResolve, M::SocketAddressToText,
        M::TcpStreamConnect, M::TcpStreamReadSome, M::TcpStreamWriteSome, M::TcpStreamWriteAll,
        M::TcpStreamRead, M::TcpStreamWrite, M::TcpStreamFlush,
        M::TcpStreamPeerAddress, M::TcpStreamLocalAddress, M::TcpStreamClose,
        M::DatagramSource, M::DatagramBytesRead, M::DatagramTruncated,
        M::UdpSocketBind, M::UdpSocketSendTo, M::UdpSocketReceiveFrom, M::UdpSocketLocalAddress, M::UdpSocketClose,
    ];
    const TLS_WAIT_METHODS: &[M] = &[
        M::TlsStreamConnect, M::TlsStreamAccept,
        M::TlsStreamReadSome, M::TlsStreamWriteSome, M::TlsStreamWriteAll,
        M::TlsStreamCipher, M::TlsStreamProtocol, M::TlsStreamPeerCertificate,
        M::TlsStreamClose, M::TlsStreamRead, M::TlsStreamWrite, M::TlsStreamFlush,
    ];
    const WEBSOCKET_METHODS: &[M] = &[
        M::WebSocketConnect, M::WebSocketConnectTls, M::WebSocketSendText,
        M::WebSocketSendBinary, M::WebSocketPing, M::WebSocketReceive, M::WebSocketClose,
    ];
    const CONNECT_METHODS: &[M] = &[M::TlsStreamConnect, M::WebSocketConnect, M::WebSocketConnectTls];

    let has_network_methods = has_any(program, NETWORK_METHODS);
    let has_tls_io = has_any(program, TLS_WAIT_METHODS);
    let has_websocket_io = has_any(program, WEBSOCKET_METHODS);
    let has_dns_resolve = program.functions.iter().any(|function| function.native == Some(M::DnsResolve));
    let has_shared_connect = has_any(program, CONNECT_METHODS);
    let need_wait = has_network_methods || has_tls_io || has_websocket_io;
    let need_address_helpers = has_network_methods || has_dns_resolve || has_shared_connect;
    let need_descriptor_helper = has_network_methods || has_websocket_io || has_shared_connect;
    let need_connect_helper = has_network_methods || has_shared_connect;
    if !need_wait && !need_address_helpers && !need_descriptor_helper { return String::new(); }

    let address = if need_address_helpers || need_connect_helper {
        Some(native_class_id(program, |native| matches!(native, C::SocketAddress)))
    } else {
        None
    };
    let template = include_str!("../../stdlib/network.c.in");
    let mut body = String::new();
    if has_network_methods || has_dns_resolve { body.push_str(&group(template, "common")); }
    if need_wait { body.push_str(&group(template, "wait")); }
    if need_address_helpers { body.push_str(&group(template, "address_helpers")); }
    if need_descriptor_helper { body.push_str(&group(template, "descriptor_helper")); }
    if need_connect_helper { body.push_str(&group(template, "connect_helper")); }
    let address_query = [M::TcpListenerLocalAddress, M::TcpStreamPeerAddress, M::TcpStreamLocalAddress, M::UdpSocketLocalAddress];
    let close_methods = [M::TcpListenerClose, M::TcpStreamClose, M::UdpSocketClose];
    let resolve = method_if(program, M::SocketAddressResolve);
    let to_text = method_if(program, M::SocketAddressToText);
    let listener_methods = [M::TcpListenerBind, M::TcpListenerAccept, M::TcpListenerLocalAddress, M::TcpListenerClose];
    let stream_methods = [M::TcpStreamConnect, M::TcpStreamReadSome, M::TcpStreamWriteSome, M::TcpStreamWriteAll,
        M::TcpStreamRead, M::TcpStreamWrite, M::TcpStreamFlush,
        M::TcpStreamPeerAddress, M::TcpStreamLocalAddress, M::TcpStreamClose];
    let udp_methods = [M::UdpSocketBind, M::UdpSocketSendTo, M::UdpSocketReceiveFrom, M::UdpSocketLocalAddress, M::UdpSocketClose];
    let datagram_methods = [M::DatagramSource, M::DatagramBytesRead, M::DatagramTruncated];

    if has_any(program, &address_query) { body.push_str(&group(template, "address")); }
    if has_any(program, &close_methods) { body.push_str(&group(template, "close")); }
    if resolve.is_some() { body.push_str(&group(template, "resolve")); }
    if to_text.is_some() { body.push_str(&group(template, "text")); }
    if has_any(program, &listener_methods) { body.push_str(&group(template, "listener")); }
    if has_any(program, &stream_methods) { body.push_str(&group(template, "stream")); }
    if has_any(program, &udp_methods) { body.push_str(&group(template, "udp")); }
    if has_any(program, &datagram_methods) { body.push_str(&group(template, "datagram")); }
    body.push_str(template.split_once("/* NETWORK_GROUP_END datagram */")
        .expect("network template final group").1);

    let mut substitutions = vec![];
    if let Some(address) = address { add(&mut substitutions, "@ADDRESS_ID@", address); }
    if has_any(program, &datagram_methods) || has_any(program, &udp_methods) {
        let datagram = native_class_id(program, |native| matches!(native, C::Datagram));
        add(&mut substitutions, "@DATAGRAM_ID@", datagram);
    }

    if has_any(program, &address_query) {
        let address_function = method_if(program, M::TcpListenerLocalAddress)
            .or_else(|| method_if(program, M::TcpStreamPeerAddress))
            .or_else(|| method_if(program, M::TcpStreamLocalAddress))
            .or_else(|| method_if(program, M::UdpSocketLocalAddress))
            .expect("network address query is present");
        add_result_type(&mut substitutions, program, "@ADDRESS_RESULT_TYPE@", address_function);
        add_result(&mut substitutions, program, "@ADDRESS_RESULT_ID@", address_function);
        add_ok_tag(&mut substitutions, program, "@ADDRESS_RESULT_OK_TAG@", address_function);
    }
    if has_any(program, &close_methods) {
        let close = method_if(program, M::TcpListenerClose)
            .or_else(|| method_if(program, M::TcpStreamClose))
            .or_else(|| method_if(program, M::UdpSocketClose))
            .expect("network close method is present");
        add_result_type(&mut substitutions, program, "@UNIT_RESULT_TYPE@", close);
        add_result(&mut substitutions, program, "@UNIT_RESULT_ID@", close);
        add_ok_tag(&mut substitutions, program, "@UNIT_RESULT_OK_TAG@", close);
    }
    if let Some(function) = resolve {
        let address = address.expect("SocketAddress.resolve requires its native address class");
        let list = native_class_id(program, |native| matches!(native, C::List(Type::Object(element)) if element == address));
        let text = native_class_id(program, |native| matches!(native, C::Text));
        let list_new = method_for_class(program, list, M::ListNew);
        let list_add = method_for_class(program, list, M::ListAdd);
        add(&mut substitutions, "@LIST_ID@", list);
        add(&mut substitutions, "@TEXT_ID@", text);
        add_result_type(&mut substitutions, program, "@RESOLVE_RESULT_TYPE@", function);
        add_result(&mut substitutions, program, "@RESOLVE_RESULT_ID@", function);
        add_ok_tag(&mut substitutions, program, "@RESOLVE_OK_TAG@", function);
        add(&mut substitutions, "@RESOLVE_HOST_ID@", param(function, 0));
        add(&mut substitutions, "@RESOLVE_PORT_ID@", param(function, 1));
        add(&mut substitutions, "@RESOLVE_PASSIVE_ID@", param(function, 2));
        add(&mut substitutions, "@LIST_NEW@", format!("ncr_fn_{}", list_new.id));
        add(&mut substitutions, "@LIST_ADD@", format!("ncr_fn_{}", list_add.id));
    }
    if to_text.is_some() {
        let text = native_class_id(program, |native| matches!(native, C::Text));
        add(&mut substitutions, "@TEXT_ID@", text);
    }
    if has_any(program, &listener_methods) {
        let listener = native_class_id(program, |native| matches!(native, C::TcpListener));
        let stream = native_class_id(program, |native| matches!(native, C::TcpStream));
        let bind = method(program, M::TcpListenerBind);
        let accept = method(program, M::TcpListenerAccept);
        add(&mut substitutions, "@TCP_LISTENER_ID@", listener);
        add(&mut substitutions, "@TCP_STREAM_ID@", stream);
        add_result_type(&mut substitutions, program, "@LISTENER_RESULT_TYPE@", bind);
        add_result(&mut substitutions, program, "@LISTENER_BIND_RESULT_ID@", bind);
        add_ok_tag(&mut substitutions, program, "@LISTENER_RESULT_OK_TAG@", bind);
        add_result_type(&mut substitutions, program, "@ACCEPT_RESULT_TYPE@", accept);
        add_result(&mut substitutions, program, "@ACCEPT_RESULT_ID@", accept);
        add_ok_tag(&mut substitutions, program, "@ACCEPT_RESULT_OK_TAG@", accept);
        add(&mut substitutions, "@LISTENER_BIND_ADDRESS_ID@", param(bind, 0));
        add(&mut substitutions, "@LISTENER_BIND_BACKLOG_ID@", param(bind, 1));
        add(&mut substitutions, "@ACCEPT_TIMEOUT_ID@", param(accept, 0));
    }
    if has_any(program, &stream_methods) {
        let stream = native_class_id(program, |native| matches!(native, C::TcpStream));
        let buffer = native_class_id(program, |native| matches!(native, C::ByteBuffer));
        let connect = method(program, M::TcpStreamConnect);
        let read = method(program, M::TcpStreamReadSome);
        let write = method(program, M::TcpStreamWriteSome);
        let write_all = method(program, M::TcpStreamWriteAll);
        let default_read = method(program, M::TcpStreamRead);
        let default_write = method(program, M::TcpStreamWrite);
        let flush = method(program, M::TcpStreamFlush);
        let text = native_class_id(program, |native| matches!(native, C::Text));
        let address_result = method_if(program, M::TcpStreamPeerAddress)
            .or_else(|| method_if(program, M::TcpListenerLocalAddress))
            .expect("TCP stream address result specialization");
        add(&mut substitutions, "@TCP_STREAM_ID@", stream);
        add(&mut substitutions, "@BUFFER_ID@", buffer);
        add(&mut substitutions, "@BUFFER_RESERVE@", format!("ncr_byte_buffer_reserve_{buffer}"));
        add(&mut substitutions, "@TEXT_ID@", text);
        add_result_type(&mut substitutions, program, "@CONNECT_RESULT_TYPE@", connect);
        add_result(&mut substitutions, program, "@CONNECT_RESULT_ID@", connect);
        add_ok_tag(&mut substitutions, program, "@CONNECT_RESULT_OK_TAG@", connect);
        add(&mut substitutions, "@CONNECT_ADDRESS_ID@", param(connect, 0));
        add(&mut substitutions, "@CONNECT_TIMEOUT_ID@", param(connect, 1));
        add_result_type(&mut substitutions, program, "@LONG_RESULT_TYPE@", read);
        add_result(&mut substitutions, program, "@LONG_RESULT_ID@", read);
        add_ok_tag(&mut substitutions, program, "@LONG_RESULT_OK_TAG@", read);
        add(&mut substitutions, "@READ_DESTINATION_ID@", param(read, 0));
        add(&mut substitutions, "@READ_MAX_ID@", param(read, 1));
        add(&mut substitutions, "@READ_TIMEOUT_ID@", param(read, 2));
        add(&mut substitutions, "@WRITE_SOURCE_ID@", param(write, 0));
        add(&mut substitutions, "@WRITE_OFFSET_ID@", param(write, 1));
        add(&mut substitutions, "@WRITE_MAX_ID@", param(write, 2));
        add(&mut substitutions, "@WRITE_TIMEOUT_ID@", param(write, 3));
        add(&mut substitutions, "@READ_DEFAULT_DESTINATION_ID@", param(default_read, 0));
        add(&mut substitutions, "@READ_DEFAULT_MAX_ID@", param(default_read, 1));
        add(&mut substitutions, "@WRITE_DEFAULT_SOURCE_ID@", param(default_write, 0));
        add(&mut substitutions, "@WRITE_DEFAULT_OFFSET_ID@", param(default_write, 1));
        add(&mut substitutions, "@WRITE_DEFAULT_MAX_ID@", param(default_write, 2));
        assert_eq!(result_id(default_read), result_id(read), "default TCP read shares the readSome result type");
        assert_eq!(result_id(default_write), result_id(write), "default TCP write shares the writeSome result type");
        assert_eq!(result_id(flush), result_id(write_all), "TCP flush shares the writeAll unit result type");
        add_result_type(&mut substitutions, program, "@UNIT_RESULT_TYPE@", write_all);
        add_result(&mut substitutions, program, "@UNIT_RESULT_ID@", write_all);
        add_ok_tag(&mut substitutions, program, "@UNIT_RESULT_OK_TAG@", write_all);
        add(&mut substitutions, "@WRITE_ALL_SOURCE_ID@", param(write_all, 0));
        add(&mut substitutions, "@WRITE_ALL_TIMEOUT_ID@", param(write_all, 1));
        add_result_type(&mut substitutions, program, "@ADDRESS_RESULT_TYPE@", address_result);
        add_result(&mut substitutions, program, "@ADDRESS_RESULT_ID@", address_result);
        add_ok_tag(&mut substitutions, program, "@ADDRESS_RESULT_OK_TAG@", address_result);
    }
    if has_any(program, &udp_methods) {
        let udp = native_class_id(program, |native| matches!(native, C::UdpSocket));
        let buffer = native_class_id(program, |native| matches!(native, C::ByteBuffer));
        let bind = method(program, M::UdpSocketBind);
        let send = method(program, M::UdpSocketSendTo);
        let receive = method(program, M::UdpSocketReceiveFrom);
        let text = native_class_id(program, |native| matches!(native, C::Text));
        let address_result = method_if(program, M::UdpSocketLocalAddress)
            .or_else(|| method_if(program, M::TcpListenerLocalAddress))
            .or_else(|| method_if(program, M::TcpStreamPeerAddress))
            .expect("UDP address result specialization");
        add(&mut substitutions, "@UDP_SOCKET_ID@", udp);
        add(&mut substitutions, "@BUFFER_ID@", buffer);
        add(&mut substitutions, "@BUFFER_RESERVE@", format!("ncr_byte_buffer_reserve_{buffer}"));
        add(&mut substitutions, "@TEXT_ID@", text);
        add_result_type(&mut substitutions, program, "@SOCKET_RESULT_TYPE@", bind);
        add_result(&mut substitutions, program, "@UDP_BIND_RESULT_ID@", bind);
        add_ok_tag(&mut substitutions, program, "@UDP_SOCKET_RESULT_OK_TAG@", bind);
        add(&mut substitutions, "@UDP_BIND_ADDRESS_ID@", param(bind, 0));
        add_result_type(&mut substitutions, program, "@LONG_RESULT_TYPE@", send);
        add_result(&mut substitutions, program, "@UDP_SEND_RESULT_ID@", send);
        add_result(&mut substitutions, program, "@LONG_RESULT_ID@", send);
        add_ok_tag(&mut substitutions, program, "@LONG_RESULT_OK_TAG@", send);
        add(&mut substitutions, "@UDP_SEND_SOURCE_ID@", param(send, 0));
        add(&mut substitutions, "@UDP_SEND_OFFSET_ID@", param(send, 1));
        add(&mut substitutions, "@UDP_SEND_LENGTH_ID@", param(send, 2));
        add(&mut substitutions, "@UDP_SEND_DESTINATION_ID@", param(send, 3));
        add(&mut substitutions, "@UDP_SEND_TIMEOUT_ID@", param(send, 4));
        add_result_type(&mut substitutions, program, "@DATAGRAM_RESULT_TYPE@", receive);
        add_result(&mut substitutions, program, "@UDP_RECEIVE_RESULT_ID@", receive);
        add_ok_tag(&mut substitutions, program, "@UDP_RECEIVE_RESULT_OK_TAG@", receive);
        add(&mut substitutions, "@UDP_RECEIVE_DESTINATION_ID@", param(receive, 0));
        add(&mut substitutions, "@UDP_RECEIVE_MAX_ID@", param(receive, 1));
        add(&mut substitutions, "@UDP_RECEIVE_TIMEOUT_ID@", param(receive, 2));
        add_result_type(&mut substitutions, program, "@ADDRESS_RESULT_TYPE@", address_result);
        add_result(&mut substitutions, program, "@ADDRESS_RESULT_ID@", address_result);
        add_ok_tag(&mut substitutions, program, "@ADDRESS_RESULT_OK_TAG@", address_result);
    }

    let mut body = replace_placeholders(body, &substitutions);
    body = replace_method_ids(body, program, None);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded network placeholder");
    body
}
