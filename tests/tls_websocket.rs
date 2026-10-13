use nacre::{compile_to_c, native};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const CERT: &str = include_str!("fixtures/tls_loopback_cert.pem");
const KEY: &str = include_str!("fixtures/tls_loopback_key.pem");
const INTERMEDIATE: &str = include_str!("fixtures/crypto_intermediate.pem");

fn compile(source: &str) -> native::NativeProgram {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("tls_websocket.ncr", source)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS", "-pthread"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
}

fn nacre_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for character in value.chars() {
        match character {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            '\0' => literal.push_str("\\0"),
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

fn assert_live_zero(stderr: &str) {
    assert!(
        stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "native ownership report did not show zero live objects:\n{stderr}"
    );
}

struct Running {
    child: Child,
    stdout_rx: Receiver<String>,
    stderr_rx: Receiver<String>,
    stdout: String,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl Running {
    fn start(program: &native::NativeProgram, args: &[&str]) -> Self {
        let mut child = Command::new(program.path())
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let stdout_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => if stdout_tx.send(line).is_err() { break; },
                }
            }
        });
        let (stderr_tx, stderr_rx) = mpsc::channel();
        let stderr_thread = thread::spawn(move || {
            let mut output = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut output);
            let _ = stderr_tx.send(output);
        });
        Self { child, stdout_rx, stderr_rx, stdout: String::new(), stdout_thread: Some(stdout_thread), stderr_thread: Some(stderr_thread) }
    }

    fn next_line(&mut self) -> String {
        let line = self.stdout_rx.recv_timeout(Duration::from_secs(10))
            .expect("native TLS server did not publish its endpoint");
        self.stdout.push_str(&line);
        line.trim_end_matches(['\r', '\n']).to_owned()
    }

    fn finish_for(mut self, timeout: Duration) -> (ExitStatus, String, String) {
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() { break status; }
            assert!(Instant::now() < deadline, "native helper exceeded its finite test deadline");
            thread::sleep(Duration::from_millis(10));
        };
        loop {
            match self.stdout_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(line) => self.stdout.push_str(&line),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => panic!("native stdout reader did not finish"),
            }
        }
        let stderr = self.stderr_rx.recv_timeout(Duration::from_secs(5))
            .expect("native stderr reader did not finish");
        if let Some(reader) = self.stdout_thread.take() { reader.join().unwrap(); }
        if let Some(reader) = self.stderr_thread.take() { reader.join().unwrap(); }
        (status, std::mem::take(&mut self.stdout), stderr)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() { let _ = self.child.kill(); }
    }
}

fn run(program: &native::NativeProgram, args: &[&str]) -> String {
    let (status, stdout, stderr) = Running::start(program, args).finish_for(Duration::from_secs(15));
    assert!(status.success(), "native client failed:\n{stderr}");
    assert_live_zero(&stderr);
    stdout
}

#[test]
fn tls_loopback_verifies_identity_and_transfers_bytes_with_owned_config() {
    let server_source = format!(r#"
void main() {{
    Text certificate_text = new Text({});
    match (Certificate.parsePemChain(certificate_text)) {{
        Err(error) {{ println("certificate-error"); }}
        Ok(certificate) {{
            Text key_text = new Text({});
            match (CryptoKey.parsePrivatePem(key_text)) {{
                Err(error) {{ println("key-error"); }}
                Ok(key) {{
                    match (TlsConfig.server(certificate, key)) {{
                        Err(error) {{ println("config-error"); }}
                        Ok(config) {{
                            match (Mutex<TlsConfig>.create(config)) {{
                                Err(error) {{ println("mutex-error"); }}
                                Ok(config_handle) {{
                                    Text host = new Text("127.0.0.1");
                                    match (SocketAddress.resolve(host, 0, false)) {{
                                        Err(error) {{ println("resolve-error"); }}
                                        Ok(addresses) {{
                                            match (TcpListener.bind(addresses.getRef(0), 8)) {{
                                                Err(error) {{ println("bind-error"); }}
                                                Ok(listener) {{
                                                    match (listener.localAddress()) {{
                                                        Err(error) {{ println("local-error"); }}
                                                        Ok(local) {{
                                                            Text endpoint = local.toText();
                                                            Console.printText(endpoint);
                                                            Console.print("\n");
                                                            match (TlsStream.accept(listener, config_handle, 5000L)) {{
                                                                Err(error) {{ println("accept-error"); }}
                                                                Ok(stream) {{
                                                                    println("server-accepted");
                                                                    match (ByteBuffer.withCapacity(0L)) {{
                                                                        Err(error) {{ println("buffer-error"); }}
                                                                        Ok(received) {{
                                                                            long total = 0L;
                                                                            boolean okay = true;
                                                                            while (total < 5L && okay) {{
                                                                                match (stream.readSome(received, 5L - total, 5000L)) {{
                                                                                    Err(error) {{ okay = false; }}
                                                                                    Ok(count) {{ if (count <= 0L) {{ okay = false; }} else {{ total = total + count; }} }}
                                                                                }}
                                                                            }}
                                                                            println(okay && total == 5L && received.get(0L) == (byte)104
                                                                                && received.get(1L) == (byte)101 && received.get(2L) == (byte)108
                                                                                && received.get(3L) == (byte)108 && received.get(4L) == (byte)111);
                                                                        }}
                                                                    }}
                                                                    match (Time.sleepMillis(200L)) {{
                                                                        Err(error) {{ println("server-delay-error"); }}
                                                                        Ok(unit) {{ println("server-delay-ok"); }}
                                                                    }}
                                                                    Text response_text = new Text("world");
                                                                    ByteBuffer response = Encoding.utf8Encode(response_text);
                                                                    match (stream.writeAll(response, 5000L)) {{
                                                                        Err(error) {{ println("server-write-error"); }}
                                                                        Ok(unit) {{ println("server-write-ok"); }}
                                                                    }}
                                                                    match (stream.close()) {{
                                                                        Err(error) {{ println("server-close-error"); }}
                                                                        Ok(unit) {{ println("server-close-ok"); }}
                                                                    }}
                                                                }}
                                                            }}
                                                        }}
                                                    }}
                                                }}
                                            }}
                                        }}
                                    }}
                                }}
                            }}
                        }}
                    }}
                }}
            }}
        }}
    }}
}}
    "#, nacre_literal(&format!("{CERT}{INTERMEDIATE}")), nacre_literal(KEY));

    let ca_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tls_loopback_cert.pem");
    let client_source = format!(r#"
boolean tlsTextEquals(Result<Text, Text> result, String expected) {{
    match (result) {{ Err(error) {{ return false; }} Ok(value) {{ return value.equalsString(expected); }} }}
}}
boolean tlsUnitOk(Result<Unit, Text> result) {{
    match (result) {{ Err(error) {{ return false; }} Ok(value) {{ return true; }} }}
}}
boolean peerChainSurvivesStreamClose(borrow Certificate certificate, String expected, borrow Path caFile) {{
    match (certificate.toPemChain()) {{
        Err(error) {{ return false; }}
        Ok(pem) {{
            if (!pem.equalsString(expected)) {{ return false; }}
            match (Certificate.parsePemChain(pem)) {{
                Err(error) {{ return false; }}
                Ok(restored) {{
                    Text host = new Text("localhost");
                    match (Time.unixMillis()) {{
                        Err(error) {{ return false; }}
                        Ok(nowMillis) {{
                            return tlsTextEquals(certificate.serialText(), "471755676793627701306629492919834469958365017893")
                                && tlsTextEquals(restored.subject(), "CN=localhost")
                                && tlsTextEquals(restored.serialText(), "471755676793627701306629492919834469958365017893")
                                && tlsUnitOk(restored.verify(host, caFile, nowMillis / 1000L));
                        }}
                    }}
                }}
            }}
        }}
    }}
}}
void main() {{
    match (Args.get(0)) {{
        Err(error) {{ println("argument-error"); }}
        Ok(port_text) {{
            match (port_text.parseInt()) {{
                Err(error) {{ println("port-error"); }}
                Ok(port) {{
                    Text address_host = new Text("127.0.0.1");
                    match (SocketAddress.resolve(address_host, port, false)) {{
                        Err(error) {{ println("resolve-error"); }}
                        Ok(addresses) {{
                            Text ca_path = new Text({});
                            match (Path.fromText(ca_path)) {{
                                Err(error) {{ println("path-error"); }}
                                Ok(ca_file) {{
                                    match (TlsConfig.clientWithCa(ca_file)) {{
                                        Err(error) {{ println("config-error"); }}
                                        Ok(config) {{
                                            match (Mutex<TlsConfig>.create(config)) {{
                                                Err(error) {{ println("mutex-error"); }}
                                                Ok(config_handle) {{
                                            Text host = new Text("localhost");
                                            match (TlsStream.connect(addresses.getRef(0), config_handle, host, 5000L)) {{
                                                Err(error) {{ println("connect-error"); }}
                                                Ok(stream) {{
                                                    Result<Option<Certificate>, Text> peerCertificate = stream.peerCertificate();
                                                    Text request_text = new Text("hello");
                                                    ByteBuffer request = Encoding.utf8Encode(request_text);
                                                    match (stream.writeAll(request, 5000L)) {{
                                                        Err(error) {{ println("client-write-error"); }}
                                                        Ok(unit) {{ println("client-write-ok"); }}
                                                    }}
                                                    Text missing_path = new Text("/nacre-test-ca-that-does-not-exist.pem");
                                                    match (Path.fromText(missing_path)) {{
                                                        Err(error) {{ println(false); }}
                                                        Ok(missing_ca) {{
                                                            match (TlsConfig.clientWithCa(missing_ca)) {{
                                                                Err(error) {{ println(true); }}
                                                                Ok(unexpected) {{ println(false); }}
                                                            }}
                                                        }}
                                                    }}
                                                    match (ByteBuffer.withCapacity(0L)) {{
                                                        Err(error) {{ println("buffer-error"); }}
                                                        Ok(received) {{
                                                            long total = 0L;
                                                            boolean okay = true;
                                                            while (total < 5L && okay) {{
                                                                match (stream.readSome(received, 5L - total, 5000L)) {{
                                                                    Err(error) {{ okay = false; }}
                                                                    Ok(count) {{ if (count <= 0L) {{ okay = false; }} else {{ total = total + count; }} }}
                                                                }}
                                                            }}
                                                            println(okay && total == 5L && received.get(0L) == (byte)119
                                                                && received.get(1L) == (byte)111 && received.get(2L) == (byte)114
                                                                && received.get(3L) == (byte)108 && received.get(4L) == (byte)100);
                                                        }}
                                                    }}
                                                    match (stream.close()) {{
                                                        Err(error) {{ println("client-close-error"); }}
                                                        Ok(unit) {{ println("client-close-ok"); }}
                                                    }}
                                                    match (peerCertificate) {{
                                                        Err(error) {{ println(false); }}
                                                        Ok(peer) {{
                                                            match (peer) {{
                                                                None {{ println(false); }}
                                                                Some(certificate) {{
                                                                    println(peerChainSurvivesStreamClose(certificate, {}, ca_file));
                                                                }}
                                                            }}
                                                        }}
                                                    }}
                                                }}
                                            }}
                                        }}
                                        }}
                                    }}
                                    }}
                                }}
                            }}
                        }}
                    }}
                }}
            }}
        }}
    }}
}}
    "#, nacre_literal(ca_path), nacre_literal(&format!("{CERT}{INTERMEDIATE}")));

    let server = compile(&server_source);
    let client = compile(&client_source);
    let mut server_process = Running::start(&server, &[]);
    let endpoint = server_process.next_line();
    assert!(endpoint.starts_with("127.0.0.1:"), "{endpoint}");
    let port = endpoint.rsplit(':').next().unwrap();
    let client_output = run(&client, &[port]);
    assert!(client_output.contains("client-write-ok\ntrue\ntrue\nclient-close-ok\ntrue\n"), "{client_output}");
    let (status, stdout, stderr) = server_process.finish_for(Duration::from_secs(15));
    assert!(status.success(), "TLS server failed:\n{stderr}");
    assert_live_zero(&stderr);
    assert!(stdout.contains("server-accepted\ntrue\nserver-delay-ok\nserver-write-ok\nserver-close-ok\n"), "{stdout}");
}

fn sha1(input: &[u8]) -> [u8; 20] {
    let mut bytes = input.to_vec();
    let bit_length = (bytes.len() as u64).wrapping_mul(8);
    bytes.push(0x80);
    while bytes.len() % 64 != 56 { bytes.push(0); }
    bytes.extend_from_slice(&bit_length.to_be_bytes());
    let mut state = [0x6745_2301u32, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476, 0xc3d2_e1f0];
    for block in bytes.chunks_exact(64) {
        let mut words = [0u32; 80];
        for (index, word) in block.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes(word.try_into().unwrap());
        }
        for index in 16..80 {
            words[index] = (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.into_iter().enumerate() {
            let (function, constant) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let next = a.rotate_left(5).wrapping_add(function).wrapping_add(e)
                .wrapping_add(constant).wrapping_add(word);
            e = d; d = c; c = b.rotate_left(30); b = a; a = next;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
    }
    let mut digest = [0u8; 20];
    for (index, word) in state.into_iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 { ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char } else { '=' });
        output.push(if chunk.len() > 2 { ALPHABET[(c & 63) as usize] as char } else { '=' });
    }
    output
}

fn websocket_accept(key: &str) -> String {
    let input = format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64(&sha1(input.as_bytes()))
}

fn read_http_headers(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while bytes.len() < 16 * 1024 {
        stream.read_exact(&mut byte)?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") { return Ok(String::from_utf8_lossy(&bytes).into_owned()); }
    }
    Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "oversized WebSocket handshake"))
}

fn read_client_frame(stream: &mut TcpStream) -> std::io::Result<(u8, Vec<u8>)> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header)?;
    let marker = header[1] & 0x7f;
    let length = match marker {
        126 => { let mut extended = [0u8; 2]; stream.read_exact(&mut extended)?; u16::from_be_bytes(extended) as usize }
        127 => { let mut extended = [0u8; 8]; stream.read_exact(&mut extended)?; usize::try_from(u64::from_be_bytes(extended)).unwrap() }
        value => value as usize,
    };
    assert!(header[1] & 0x80 != 0, "client frame was not masked");
    assert!(length <= 1024, "unexpected oversized test frame");
    let mut mask = [0u8; 4];
    stream.read_exact(&mut mask)?;
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload)?;
    for (index, byte) in payload.iter_mut().enumerate() { *byte ^= mask[index % 4]; }
    Ok((header[0] & 0x0f, payload))
}

fn write_server_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
    assert!(payload.len() < 126, "test server only sends short frames");
    stream.write_all(&[0x80 | opcode, payload.len() as u8])?;
    stream.write_all(payload)?;
    stream.flush()
}

fn serve_websocket(listener: TcpListener) -> JoinHandle<()> {
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("WebSocket loopback accept");
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let request = read_http_headers(&mut stream).expect("WebSocket HTTP request");
        assert!(request.starts_with("GET /echo?x=1 HTTP/1.1\r\n"), "{request}");
        let key = request.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("Sec-WebSocket-Key").then(|| value.trim())
        }).expect("Sec-WebSocket-Key");
        let accept = websocket_accept(key);
        write!(stream,
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        ).unwrap();
        let (opcode, payload) = read_client_frame(&mut stream).expect("client text frame");
        assert_eq!(opcode, 1);
        assert_eq!(payload, b"hello");
        write_server_frame(&mut stream, 1, b"world").unwrap();
        let (opcode, payload) = read_client_frame(&mut stream).expect("client close frame");
        assert_eq!(opcode, 8);
        assert!(payload.starts_with(&[0x03, 0xe8]));
        write_server_frame(&mut stream, 8, &payload).unwrap();
        drop(stream);
        let (mut tls_stream, _) = listener.accept().expect("WSS failure-path accept");
        tls_stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        tls_stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut client_hello = [0u8; 5];
        tls_stream.read_exact(&mut client_hello).expect("TLS client hello");
        tls_stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").unwrap();
        tls_stream.flush().unwrap();
    })
}

#[test]
fn websocket_loopback_exchanges_frames_and_completes_close_handshake() {
    let source = r#"
void main() {
    match (Args.get(0)) {
        Err(error) { println("argument-error"); }
        Ok(port_text) {
            match (port_text.parseInt()) {
                Err(error) { println("port-error"); }
                Ok(port) {
                    Text source = new Text("ws://127.0.0.1:");
                    source.appendText(port_text);
                    source.append("/echo?x=1");
                    match (Uri.parse(source)) {
                        Err(error) { println("uri-error"); }
                        Ok(uri) {
                            match (WebSocket.connect(uri, 5000L, 1024L)) {
                                Err(error) { println("connect-error"); }
                                Ok(socket) {
                                    Text outgoing = new Text("hello");
                                    match (socket.sendText(outgoing, 5000L)) {
                                        Err(error) { println("send-error"); }
                                        Ok(unit) { println("send-ok"); }
                                    }
                                    match (socket.receive(5000L)) {
                                        Err(error) { println("receive-error"); }
                                        Ok(message_option) {
                                            match (message_option) {
                                                None { println("missing-message"); }
                                                Some(message) {
                                                    println(message.isText());
                                                    match (message.asText()) {
                                                        Err(error) { println("decode-error"); }
                                                        Ok(text) { println(text.equalsString("world")); }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Text reason = new Text("done");
                                    match (socket.close(1000, reason, 5000L)) {
                                        Err(error) { println("close-error"); }
                                        Ok(unit) { println("close-ok"); }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let ca_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tls_loopback_cert.pem");
    let wss_source = r#"
void main() {
    match (Args.get(0)) {
        Err(error) { println(false); }
        Ok(port_text) {
            Text address = new Text("wss://127.0.0.1:");
            address.appendText(port_text);
            address.append("/echo");
            match (Uri.parse(address)) {
                Err(error) { println(false); }
                Ok(uri) {
                    Text ca_path = new Text(@CA_PATH@);
                    match (Path.fromText(ca_path)) {
                        Err(error) { println(false); }
                        Ok(ca_file) {
                            match (TlsConfig.clientWithCa(ca_file)) {
                                Err(error) { println(false); }
                                Ok(config) {
                                    match (Mutex<TlsConfig>.create(config)) {
                                        Err(error) { println(false); }
                                        Ok(config_handle) {
                                            match (WebSocket.connectTls(uri, config_handle, 5000L, 1024L)) {
                                                Err(error) { println(true); }
                                                Ok(socket) {
                                                    println(false);
                                                    Text reason = new Text("done");
                                                    match (socket.close(1000, reason, 1000L)) {
                                                        Err(close_error) { println("close-error"); }
                                                        Ok(unit) { println("unexpected-close"); }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#.replace("@CA_PATH@", &nacre_literal(ca_path));
    let wss_program = compile(&wss_source);
    let program = compile(source);
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let server = serve_websocket(listener);
    let output = run(&program, &[&port]);
    assert!(output.contains("send-ok\ntrue\ntrue\nclose-ok\n"), "{output}");
    let wss_output = run(&wss_program, &[&port]);
    assert!(wss_output.contains("true\n"), "{wss_output}");
    server.join().unwrap();
}
