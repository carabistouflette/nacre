//! Native end-to-end tests for bounded HTTP requests, response ownership, and TLS verification.
use nacre::{compile_to_c, native};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Output, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

fn compile(source: &str) -> native::NativeProgram {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("http_native.ncr", source)));
    native::compile_c_with_flags(&c, &["-pthread", "-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("{error}"))
}

fn run(program: &native::NativeProgram) -> Output {
    let output = Command::new(program.path())
        .env("NO_PROXY", "*")
        .env("no_proxy", "*")
        .output()
        .unwrap();
    assert!(output.status.success(), "native HTTP program failed:\n{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "native ownership report did not show zero live objects:\n{stderr}");
    output
}
fn stdout_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout).lines().map(str::to_owned).collect()
}


#[derive(Debug)]
struct CapturedRequest {
    headers: String,
    body: Vec<u8>,
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn read_request(stream: &mut TcpStream) -> CapturedRequest {
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let end = find_header_end(&bytes);
        if let Some(end) = end {
            let headers = std::str::from_utf8(&bytes[..end]).expect("HTTP request headers are ASCII/UTF-8");
            let content_length = headers.lines().skip(1).find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().unwrap())
            }).unwrap_or(0);
            if bytes.len() >= end + 4 + content_length {
                return CapturedRequest {
                    headers: headers.to_owned(),
                    body: bytes[end + 4..end + 4 + content_length].to_vec(),
                };
            }
        }
        let count = stream.read(&mut chunk).unwrap();
        assert_ne!(count, 0, "client closed before sending a complete HTTP request");
        bytes.extend_from_slice(&chunk[..count]);
    }
}

fn accept_loopback(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "HTTP client did not connect to loopback server");
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("loopback accept failed: {error}"),
        }
    }
}

fn one_response_server(response: Vec<u8>, delay: Duration) -> (String, JoinHandle<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let mut stream = accept_loopback(&listener);
        let request = read_request(&mut stream);
        if !delay.is_zero() { thread::sleep(delay); }
        let _ = stream.write_all(&response);
        let _ = stream.flush();
        request
    });
    (endpoint, worker)
}
fn two_response_server(responses: Vec<Vec<u8>>) -> (String, JoinHandle<Vec<CapturedRequest>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let mut requests = Vec::with_capacity(responses.len());
        for response in responses {
            let mut stream = accept_loopback(&listener);
            requests.push(read_request(&mut stream));
            let _ = stream.write_all(&response);
            let _ = stream.flush();
        }
        requests
    });
    (endpoint, worker)
}

#[test]
fn head_with_explicit_empty_body_does_not_wait_for_response_content() {
    let (url, server) = one_response_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 123\r\nConnection: close\r\n\r\n".to_vec(),
        Duration::ZERO,
    );
    let source = r#"
Result<Unit, Text> exchange() {
    Text method = new Text("HEAD");
    Text url = new Text("@URL@");
    HttpRequest request = HttpRequest.create(method, url)?;
    request.setTimeoutMillis(1000L)?;
    ByteBuffer empty = ByteBuffer.withCapacity(0L)?;
    request.setBody(empty);
    HttpResponse response = request.send()?;
    println(response.status());
    borrow ByteBuffer body = response.body();
    println(body.length());
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exchange()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = run(&compile(&source.replace("@URL@", &url)));
    assert_eq!(stdout_lines(&output), vec!["200".to_owned(), "0".to_owned()]);
    let request = server.join().unwrap();
    assert!(request.headers.starts_with("HEAD / HTTP/1.1"));
    assert!(request.body.is_empty());
}



#[test]
fn sends_binary_patch_and_keeps_non_2xx_response_data() {
    let response = b"HTTP/1.1 100 Continue\r\nX-Duplicate: interim\r\n\r\nHTTP/1.1 418 Teapot\r\nContent-Length: 3\r\nX-Duplicate: first\r\nx-duplicate: second\r\nConnection: close\r\n\r\n\x00\xffA".to_vec();
    let (url, server) = one_response_server(response, Duration::ZERO);
    let source = r#"
Result<Unit, Text> exchange(borrow Text url) {
    Text method = new Text("PATCH");
    HttpRequest request = HttpRequest.create(method, url)?;
    Text headerName = new Text("X-Nacre");
    Text headerValue = new Text("loopback");
    request.setHeader(headerName, headerValue)?;
    request.setTimeoutMillis(2000L)?;
    request.setMaxResponseBytes(3L)?;
    ByteBuffer payload = ByteBuffer.withCapacity(3L)?;
    payload.append((byte)65);
    payload.append((byte)66);
    payload.append((byte)67);
    request.setBody(payload);
    payload.set(0L, (byte)90);
    HttpResponse response = request.send()?;
    println(response.status() == 418);
    Text duplicateName = new Text("x-duplicate");
    match (response.header(duplicateName)) {
        None { println("HEADER_MISSING"); }
        Some(value) { println(value.equalsString("second")); }
    }
    borrow ByteBuffer received = response.body();
    println(received.length() == 3L
        && received.get(0L) == (byte)0
        && received.get(1L) == (byte)255
        && received.get(2L) == (byte)65);
    match (response.bodyText()) {
        Ok(value) { println("UTF8_UNEXPECTED"); }
        Err(error) { println("INVALID_UTF8"); }
    }
    Text finalUrl = response.finalUrl();
    println(finalUrl.equalsString("@URL@"));
    return new Result<Unit, Text>.Ok(());
}
void main() {
    Text url = new Text("@URL@");
    match (exchange(url)) {
        Ok(unit) { println("DONE"); }
        Err(error) { println("REQUEST_ERROR"); }
    }
}
"#;
    let program = compile(&source.replace("@URL@", &url));
    let output = run(&program);
    assert_eq!(stdout_lines(&output), vec![
        "true".to_owned(), "true".to_owned(), "true".to_owned(),
        "INVALID_UTF8".to_owned(), "true".to_owned(), "DONE".to_owned(),
    ]);
    let request = server.join().unwrap();
    assert!(request.headers.starts_with("PATCH / HTTP/1.1"), "{}", request.headers);
    assert!(request.headers.lines().any(|line| line.eq_ignore_ascii_case("X-Nacre: loopback")), "{}", request.headers);
    assert_eq!(request.body, b"ABC");
}

#[test]
fn reuses_a_request_without_replacing_prior_response_data() {
    let responses = vec![
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\none".to_vec(),
        b"HTTP/1.1 201 Created\r\nContent-Length: 3\r\nConnection: close\r\n\r\ntwo".to_vec(),
    ];
    let (url, server) = two_response_server(responses);
    let source = r#"
Result<Unit, Text> exchange(borrow Text url) {
    Text method = new Text("GET");
    HttpRequest request = HttpRequest.create(method, url)?;
    HttpResponse first = request.send()?;
    HttpResponse second = request.send()?;
    println(first.status() == 200 && second.status() == 201);
    borrow ByteBuffer firstBody = first.body();
    borrow ByteBuffer secondBody = second.body();
    println(firstBody.get(0L) == (byte)111 && secondBody.get(0L) == (byte)116);
    return new Result<Unit, Text>.Ok(());
}
void main() {
    Text url = new Text("@URL@");
    match (exchange(url)) {
        Ok(unit) { println("REUSED"); }
        Err(error) { println("REQUEST_ERROR"); }
    }
}
"#;
    let output = run(&compile(&source.replace("@URL@", &url)));
    assert_eq!(stdout_lines(&output), vec!["true".to_owned(), "true".to_owned(), "REUSED".to_owned()]);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request.headers.starts_with("GET / HTTP/1.1")));
}


#[test]
fn rejects_invalid_methods_headers_userinfo_and_non_http_schemes() {
    let source = r#"
void main() {
    Text url = new Text("http://127.0.0.1:1/");
    Text invalidMethod = new Text("GE T");
    match (HttpRequest.create(invalidMethod, url)) {
        Ok(request) { println("METHOD_ACCEPTED"); }
        Err(error) { println("BAD_METHOD"); }
    }
    Text method = new Text("GET");
    match (HttpRequest.create(method, url)) {
        Err(error) { println("CREATE_ERROR"); }
        Ok(request) {
            Text name = new Text("X-Injected");
            Text value = new Text("safe\r\nInjected: yes");
            match (request.setHeader(name, value)) {
                Ok(unit) { println("HEADER_ACCEPTED"); }
                Err(error) { println("BAD_HEADER_VALUE"); }
            }
            Text badName = new Text("X Bad");
            Text ordinaryValue = new Text("value");
            match (request.setHeader(badName, ordinaryValue)) {
                Ok(unit) { println("NAME_ACCEPTED"); }
                Err(error) { println("BAD_HEADER_NAME"); }
            }
            match (request.setTimeoutMillis(-1L)) {
                Ok(unit) { println("NEGATIVE_TIMEOUT_ACCEPTED"); }
                Err(error) { println("NEGATIVE_TIMEOUT_REJECTED"); }
            }
            match (request.setMaxResponseBytes(-1L)) {
                Ok(unit) { println("NEGATIVE_LIMIT_ACCEPTED"); }
                Err(error) { println("NEGATIVE_LIMIT_REJECTED"); }
            }
        }
    }
    Text credentials = new Text("http://user:secret@127.0.0.1:80/");
    match (HttpRequest.create(method, credentials)) {
        Ok(request) { println("USERINFO_ACCEPTED"); }
        Err(error) { println("USERINFO_REJECTED"); }
    }
    Text otherScheme = new Text("file:///etc/passwd");
    match (HttpRequest.create(method, otherScheme)) {
        Ok(request) { println("SCHEME_ACCEPTED"); }
        Err(error) { println("SCHEME_REJECTED"); }
    }
    Text nulUrl = new Text("http://127.0.0.1\0:80/");
    match (HttpRequest.create(method, nulUrl)) {
        Ok(request) { println("NUL_URL_ACCEPTED"); }
        Err(error) { println("NUL_URL_REJECTED"); }
    }
}
"#;
    let output = run(&compile(source));
    assert_eq!(stdout_lines(&output), vec![
        "BAD_METHOD".to_owned(), "BAD_HEADER_VALUE".to_owned(), "BAD_HEADER_NAME".to_owned(),
        "NEGATIVE_TIMEOUT_REJECTED".to_owned(), "NEGATIVE_LIMIT_REJECTED".to_owned(),
        "USERINFO_REJECTED".to_owned(), "SCHEME_REJECTED".to_owned(), "NUL_URL_REJECTED".to_owned(),
    ]);
}

#[test]
fn enforces_response_body_limit_and_timeout() {
    let (url, server) = one_response_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabcdef".to_vec(),
        Duration::ZERO,
    );
    let source = r#"
void main() {
    Text method = new Text("GET");
    Text url = new Text("@URL@");
    match (HttpRequest.create(method, url)) {
        Err(error) { println("CREATE_ERROR"); }
        Ok(request) {
            request.setMaxResponseBytes(2L);
            match (request.send()) {
                Ok(response) { println("LIMIT_IGNORED"); }
                Err(error) { println("BODY_LIMIT"); }
            }
        }
    }
}
"#;
    let output = run(&compile(&source.replace("@URL@", &url)));
    assert_eq!(stdout_lines(&output), vec!["BODY_LIMIT".to_owned()]);
    server.join().unwrap();

    let (url, server) = one_response_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_vec(),
        Duration::from_millis(300),
    );
    let source = r#"
void main() {
    Text method = new Text("GET");
    Text url = new Text("@URL@");
    match (HttpRequest.create(method, url)) {
        Err(error) { println("CREATE_ERROR"); }
        Ok(request) {
            request.setTimeoutMillis(40L);
            match (request.send()) {
                Ok(response) { println("TIMEOUT_IGNORED"); }
                Err(error) { println("TIMED_OUT"); }
            }
        }
    }
}
"#;
    let output = run(&compile(&source.replace("@URL@", &url)));
    assert_eq!(stdout_lines(&output), vec!["TIMED_OUT".to_owned()]);
    server.join().unwrap();
}

#[test]
fn rejects_malformed_response_headers_without_returning_partial_response() {
    let (url, server) = one_response_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nnot-a-header\r\n\r\n".to_vec(),
        Duration::ZERO,
    );
    let source = r#"
void main() {
    Text method = new Text("GET");
    Text url = new Text("@URL@");
    match (HttpRequest.create(method, url)) {
        Err(error) { println("CREATE_ERROR"); }
        Ok(request) {
            match (request.send()) {
                Ok(response) { println("PARTIAL_RESPONSE_ESCAPED"); }
                Err(error) { println("MALFORMED_HEADERS"); }
            }
        }
    }
}
"#;
    let output = run(&compile(&source.replace("@URL@", &url)));
    assert_eq!(stdout_lines(&output), vec!["MALFORMED_HEADERS".to_owned()]);
    server.join().unwrap();
}

const TLS_TEST_CA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/http-test-ca.pem");
const TLS_TEST_KEY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/http-test-key.pem");

struct TlsServer(Child);

impl Drop for TlsServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_loopback_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn start_tls_server() -> Option<(String, TlsServer)> {
    if Command::new("openssl").arg("version").output().is_err() {
        eprintln!("skipping HTTPS verification test: openssl executable is unavailable");
        return None;
    }
    let address = free_loopback_address();
    let address_text = address.to_string();
    let mut child = Command::new("openssl")
        .args(["s_server", "-quiet", "-accept"])
        .arg(&address_text)
        .args(["-cert", TLS_TEST_CA, "-key", TLS_TEST_KEY, "-www"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("openssl s_server failed to start");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("openssl s_server exited during startup: {status}");
        }
        if TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "openssl s_server did not listen on {address}");
        thread::sleep(Duration::from_millis(10));
    }
    Some((address.to_string(), TlsServer(child)))
}

#[test]
fn https_uses_ca_file_and_rejects_wrong_host_or_untrusted_certificates() {
    let Some((address, _server)) = start_tls_server() else { return; };
    let ca_path = TLS_TEST_CA.replace('\\', "\\\\").replace('"', "\\\"");
    let source = r#"
Result<Unit, Text> attempt(borrow Text url, borrow Text caPath, boolean trustCa) {
    Text method = new Text("GET");
    HttpRequest request = HttpRequest.create(method, url)?;
    if (trustCa) {
        Path ca = Path.fromText(caPath.clone())?;
        request.setCaFile(ca)?;
    }
    match (request.send()) {
        Ok(response) { println("TLS_OK"); }
        Err(error) { println("TLS_REJECTED"); }
    }
    return new Result<Unit, Text>.Ok(());
}
void main() {
    Text caPath = new Text("@CA_PATH@");
    Text trusted = new Text("https://127.0.0.1:@PORT@/");
    Text wrongHost = new Text("https://localhost:@PORT@/");
    match (attempt(trusted, caPath, true)) {
        Ok(unit) {}
        Err(error) { println("SETUP_ERROR"); }
    }
    match (attempt(wrongHost, caPath, true)) {
        Ok(unit) {}
        Err(error) { println("SETUP_ERROR"); }
    }
    match (attempt(trusted, caPath, false)) {
        Ok(unit) {}
        Err(error) { println("SETUP_ERROR"); }
    }
}
"#;
    let source = source.replace("@CA_PATH@", &ca_path).replace("@PORT@", address.rsplit(':').next().unwrap());
    let output = run(&compile(&source));
    assert_eq!(stdout_lines(&output), vec![
        "TLS_OK".to_owned(), "TLS_REJECTED".to_owned(), "TLS_REJECTED".to_owned(),
    ]);
}
