//! Native end-to-end tests for TCP, UDP, address resolution, deadlines, and descriptor ownership.
mod support;

use nacre::native;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

fn compile(source: &str) -> native::NativeProgram {
    let mut flags = vec!["-DNACRE_MEMORY_STATS", "-pthread"];
    if std::env::var_os("NACRE_NETWORK_SANITIZERS").is_some() {
        flags.extend(["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]);
    }
    support::compile(source, &flags)
        .unwrap_or_else(|error| panic!("{error}"))
}

fn assert_live_zero(stderr: &str) {
    assert!(
        stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "native ownership report did not show zero live objects:\n{stderr}"
    );
}

fn run(program: &native::NativeProgram, args: &[&str]) -> Output {
    let process = Running::start(program, args, false);
    let (status, stdout, stderr) = process.finish_for(Duration::from_secs(15));
    assert!(
        status.success(),
        "native program failed:\n{}",
        stderr
    );
    assert_live_zero(&stderr);
    Output { status, stdout: stdout.into_bytes(), stderr: stderr.into_bytes() }
}

struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_rx: Receiver<String>,
    stderr_rx: Receiver<String>,
    stdout: String,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl Running {
    fn start(program: &native::NativeProgram, args: &[&str], piped_stdin: bool) -> Self {
        let mut child = Command::new(program.path())
            .args(args)
            .stdin(if piped_stdin { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdin = child.stdin.take();
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let stdout_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if stdout_tx.send(line).is_err() { break; }
                    }
                }
            }
        });
        let (stderr_tx, stderr_rx) = mpsc::channel();
        let stderr_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut output = String::new();
            let _ = reader.read_to_string(&mut output);
            let _ = stderr_tx.send(output);
        });
        Self { child, stdin, stdout_rx, stderr_rx, stdout: String::new(), stdout_thread: Some(stdout_thread), stderr_thread: Some(stderr_thread) }
    }

    fn next_line(&mut self) -> String {
        let line = self.stdout_rx.recv_timeout(Duration::from_secs(10))
            .expect("native helper did not produce its next line");
        self.stdout.push_str(&line);
        line.trim_end_matches(['\r', '\n']).to_owned()
    }

    fn write_line(&mut self, line: &str) {
        self.stdin.as_mut().expect("helper stdin is piped").write_all(line.as_bytes()).unwrap();
        self.stdin.as_mut().unwrap().flush().unwrap();
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
        let _ = self.child.wait();
        if let Some(reader) = self.stdout_thread.take() { let _ = reader.join(); }
        if let Some(reader) = self.stderr_thread.take() { let _ = reader.join(); }
    }
}

#[test]
fn address_resolution_validates_inputs_and_reports_ipv6_availability() {
    let source = r#"
void report(borrow Text host, int port, boolean passive) {
    match (SocketAddress.resolve(host, port, passive)) {
        Ok(addresses) { println(addresses.size() > 0); }
        Err(error) { println(false); }
    }
}
void main() {
    Text ipv4 = new Text("127.0.0.1");
    report(ipv4, 0, false);
    report(ipv4, -1, false);
    report(ipv4, 65536, false);
    Text nul_host = new Text("127.0.0.1\0suffix");
    report(nul_host, 80, false);
    Text empty = new Text("");
    report(empty, 0, false);
    report(empty, 0, true);
    Text missing = new Text("nacre-network-test-does-not-exist.invalid");
    report(missing, 80, false);
    Text ipv6 = new Text("::1");
    match (SocketAddress.resolve(ipv6, 0, false)) {
        Ok(addresses) {
            println("IPV6_AVAILABLE");
            Text opening = new Text("[");
            borrow SocketAddress first = addresses.getRef(0);
            Text numeric = first.toText();
            println(numeric.startsWith(opening));
        }
        Err(error) { println("IPV6_UNAVAILABLE"); }
    }
}
"#;
    let program = compile(source);
    let process = Running::start(&program, &[], false);
    let (status, stdout, stderr) = process.finish_for(Duration::from_secs(15));
    assert!(status.success(), "address resolution test failed:\n{stderr}");
    assert_live_zero(&stderr);
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(&lines[..7], ["true", "false", "false", "false", "false", "true", "false"]);
    assert!(lines.iter().any(|line| *line == "IPV6_AVAILABLE" || *line == "IPV6_UNAVAILABLE"), "{stdout}");
    eprintln!("IPv6 loopback {}", if lines.contains(&"IPV6_AVAILABLE") { "available" } else { "unavailable" });
    if lines.contains(&"IPV6_AVAILABLE") { assert_eq!(lines.last(), Some(&"true")); }
}

const TCP_SERVER: &str = r#"
void main() {
    Text host = new Text("127.0.0.1");
    match (SocketAddress.resolve(host, 0, false)) {
        Err(error) { println("resolve-error"); }
        Ok(addresses) {
            match (TcpListener.bind(addresses.getRef(0), 8)) {
                Err(error) { println("bind-error"); }
                Ok(listener) {
                    match (listener.localAddress()) {
                        Err(error) { println("local-error"); }
                        Ok(local) {
                            Text endpoint = local.toText();
                            Console.printText(endpoint);
                            Console.print("\n");
                            match (listener.accept(0L)) {
                                Err(error) { println("accept-error"); }
                                Ok(stream) {
                                    match (stream.peerAddress()) {
                                        Ok(peer) {
                                            Text peer_text = peer.toText();
                                            println(peer_text.charCount() > 0);
                                        }
                                        Err(error) { println(false); }
                                    }
                                    match (stream.localAddress()) {
                                        Ok(bound) {
                                            Text bound_text = bound.toText();
                                            println(bound_text.charCount() > 0);
                                        }
                                        Err(error) { println(false); }
                                    }
                                    match (ByteBuffer.withCapacity(0L)) {
                                        Err(error) { println("buffer-error"); }
                                        Ok(received) {
                                            received.append((byte)90);
                                            match (stream.readSome(received, 0L, 0L)) {
                                                Ok(count) { println(count == 0L); }
                                                Err(error) { println(false); }
                                            }
                                            long consumed = 0L;
                                            while (consumed < 5L) {
                                                match (stream.readSome(received, 2L, 0L)) {
                                                    Ok(count) {
                                                        if (count == 0L) { consumed = 5L; }
                                                        else { consumed = consumed + count; }
                                                    }
                                                    Err(error) { consumed = 5L; }
                                                }
                                            }
                                            println(received.length() == 6L
                                                && received.get(0L) == (byte)90
                                                && received.get(1L) == (byte)65
                                                && received.get(2L) == (byte)66
                                                && received.get(3L) == (byte)67
                                                && received.get(4L) == (byte)68
                                                && received.get(5L) == (byte)69);
                                            match (stream.readSome(received, 1L, 0L)) {
                                                Ok(count) { println(count == 0L); }
                                                Err(error) { println(false); }
                                            }
                                        }
                                    }
                                    match (stream.close()) {
                                        Ok(unit) { println("stream-closed"); }
                                        Err(error) { println("stream-close-error"); }
                                    }
                                    match (stream.peerAddress()) {
                                        Err(error) { println("closed-stream-error"); }
                                        Ok(address) { println("closed-stream-open"); }
                                    }
                                }
                            }
                            match (listener.close()) {
                                Ok(unit) { println("listener-closed"); }
                                Err(error) { println("listener-close-error"); }
                            }
                            match (listener.localAddress()) {
                                Err(error) { println("closed-listener-error"); }
                                Ok(address) { println("closed-listener-open"); }
                            }
                            match (listener.close()) {
                                Err(error) { println("repeated-close-error"); }
                                Ok(unit) { println("repeated-close-open"); }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;

const TCP_CLIENT: &str = r#"
void main() {
    match (Args.get(0)) {
        Err(error) { println("argument-error"); }
        Ok(port_text) {
            match (port_text.parseInt()) {
                Err(error) { println("port-error"); }
                Ok(port) {
                    Text host = new Text("127.0.0.1");
                    match (SocketAddress.resolve(host, port, false)) {
                        Err(error) { println("resolve-error"); }
                        Ok(addresses) {
                            match (TcpStream.connect(addresses.getRef(0), 0L)) {
                                Err(error) { println("connect-error"); }
                                Ok(stream) {
                                    match (ByteBuffer.withCapacity(0L)) {
                                        Err(error) { println("buffer-error"); }
                                        Ok(prefix) {
                                            prefix.append((byte)65);
                                            prefix.append((byte)66);
                                            match (stream.writeSome(prefix, 0L, 0L, -1L)) {
                                                Err(error) { println("write-some-negative-rejected"); }
                                                Ok(count) { println("write-some-negative-unexpected"); }
                                            }
                                            match (stream.writeAll(prefix, -1L)) {
                                                Err(error) { println("write-all-negative-rejected"); }
                                                Ok(unit) { println("write-all-negative-unexpected"); }
                                            }
                                            boolean sent_ok = true;
                                            long offset = 0L;
                                            while (offset < 2L) {
                                                match (stream.writeSome(prefix, offset, 2L - offset, 0L)) {
                                                    Ok(count) { if (count > 0L) { offset = offset + count; } }
                                                    Err(error) { offset = 2L; sent_ok = false; }
                                                }
                                            }
                                            println(sent_ok && offset == 2L);
                                    }
                                    }
                                    match (ByteBuffer.withCapacity(0L)) {
                                        Err(error) { println("tail-buffer-error"); }
                                        Ok(tail) {
                                            tail.append((byte)67);
                                            tail.append((byte)68);
                                            tail.append((byte)69);
                                            match (stream.writeAll(tail, 0L)) {
                                                Ok(unit) { println("write-all-ok"); }
                                                Err(error) { println("write-all-error"); }
                                            }
                                        }
                                    }
                                    match (stream.close()) {
                                        Ok(unit) { println("client-closed"); }
                                        Err(error) { println("client-close-error"); }
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

#[test]
fn tcp_loopback_fragments_reads_and_writes_and_observes_orderly_eof() {
    let server = compile(TCP_SERVER);
    let client = compile(TCP_CLIENT);
    let mut process = Running::start(&server, &[], false);
    let endpoint = process.next_line();
    assert!(endpoint.starts_with("127.0.0.1:"), "{endpoint}");
    let port = endpoint.rsplit(':').next().unwrap();
    let client_output = run(&client, &[port]);
    assert!(String::from_utf8_lossy(&client_output.stdout).contains("write-some-negative-rejected\nwrite-all-negative-rejected\ntrue\nwrite-all-ok\nclient-closed\n"));
    let (status, stdout, stderr) = process.finish_for(Duration::from_secs(10));
    assert!(status.success(), "server exited unsuccessfully:\n{stderr}");
    assert_live_zero(&stderr);
    assert!(stdout.contains("true\ntrue\ntrue\ntrue\nstream-closed\n"), "{stdout}");
    assert!(stdout.contains("listener-closed\nclosed-listener-error\nrepeated-close-error\n"), "{stdout}");
}

#[test]
fn tcp_accept_and_read_deadlines_return_errors() {
    let accept_source = r#"
void main() {
    Text host = new Text("127.0.0.1");
    match (SocketAddress.resolve(host, 0, false)) {
        Err(error) { println("resolve-error"); }
        Ok(addresses) {
            match (TcpListener.bind(addresses.getRef(0), 8)) {
                Err(error) { println("bind-error"); }
                Ok(listener) {
                    match (listener.accept(-1L)) {
                        Err(error) { println("accept-negative-rejected"); }
                        Ok(stream) { println("accept-negative-unexpected"); drop(stream); }
                    }
                    match (listener.accept(25L)) {
                        Err(error) { println("accept-deadline"); }
                        Ok(stream) { println("accept-unexpected"); drop(stream); }
                    }
                }
            }
        }
    }
}
"#;
    let accept = compile(accept_source);
    let output = run(&accept, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("accept-negative-rejected\n"), "{stdout}");
    assert!(stdout.contains("accept-deadline\n"), "{stdout}");

    let read_server_source = r#"
void main() {
    Text host = new Text("127.0.0.1");
    match (SocketAddress.resolve(host, 0, false)) {
        Err(error) { println("resolve-error"); }
        Ok(addresses) {
            match (TcpListener.bind(addresses.getRef(0), 8)) {
                Err(error) { println("bind-error"); }
                Ok(listener) {
                    match (listener.localAddress()) {
                        Err(error) { println("local-error"); }
                        Ok(local) {
                            Text endpoint = local.toText();
                            Console.printText(endpoint);
                            Console.print("\n");
                            match (listener.accept(5000L)) {
                                Err(error) { println("accept-error"); }
                                Ok(stream) {
                                    match (ByteBuffer.withCapacity(0L)) {
                                        Err(error) { println("buffer-error"); }
                                        Ok(buffer) {
                                            match (stream.readSome(buffer, 10L, -1L)) {
                                                Err(error) { println("read-negative-rejected"); }
                                                Ok(count) { println("read-negative-unexpected"); }
                                            }
                                            match (stream.readSome(buffer, 1048576L, 50L)) {
                                                Err(error) { println("read-deadline"); }
                                                Ok(count) { println("read-unexpected"); }
                                            }
                                            if (buffer.capacity() == 0L) { println("read-capacity-untouched"); }
                                            else { println("read-capacity-grew"); }
                                        }
                                    }
                                    match (stream.close()) { Ok(unit) { println("stream-closed"); } Err(error) { println("stream-close-error"); } }
                                }
                            }
                            match (listener.close()) { Ok(unit) { println("listener-closed"); } Err(error) { println("listener-close-error"); } }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let server = compile(read_server_source);
    let mut process = Running::start(&server, &[], false);
    let endpoint = process.next_line();
    let port: u16 = endpoint.rsplit(':').next().unwrap().parse().unwrap();
    let _peer = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    assert_eq!(process.next_line(), "read-negative-rejected");
    assert_eq!(process.next_line(), "read-deadline");
    assert_eq!(process.next_line(), "read-capacity-untouched");
    let (status, stdout, stderr) = process.finish_for(Duration::from_secs(10));
    assert!(status.success(), "read-deadline server failed:\n{stderr}");
    assert_live_zero(&stderr);
    assert!(stdout.contains("read-deadline\n"), "{stdout}");
    assert!(stdout.contains("read-capacity-untouched\n"), "{stdout}");
}

#[test]
fn tcp_read_some_bounds_large_max_bytes() {
    let source = r#"
void main() {
    Text host = new Text("127.0.0.1");
    match (SocketAddress.resolve(host, 0, false)) {
        Err(error) { println("resolve-error"); }
        Ok(addresses) {
            match (TcpListener.bind(addresses.getRef(0), 8)) {
                Err(error) { println("bind-error"); }
                Ok(listener) {
                    match (listener.localAddress()) {
                        Err(error) { println("local-error"); }
                        Ok(local) {
                            Text endpoint = local.toText();
                            Console.printText(endpoint);
                            Console.print("\n");
                            match (listener.accept(5000L)) {
                                Err(error) { println("accept-error"); }
                                Ok(stream) {
                                    match (ByteBuffer.withCapacity(0L)) {
                                        Err(error) { println("buffer-error"); }
                                        Ok(buffer) {
                                            match (stream.readSome(buffer, 1048576L, 5000L)) {
                                                Ok(count) {
                                                    if (count == 1L && buffer.length() == 1L
                                                        && buffer.capacity() <= 131072L
                                                        && buffer.get(0L) == (byte)73) {
                                                        println("read-bounded");
                                                    } else { println("read-unbounded"); }
                                                }
                                                Err(error) { println("read-error"); }
                                            }
                                        }
                                    }
                                    match (stream.close()) { Ok(unit) { println("stream-closed"); } Err(error) { println("stream-close-error"); } }
                                }
                            }
                            match (listener.close()) { Ok(unit) { println("listener-closed"); } Err(error) { println("listener-close-error"); } }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let server = compile(source);
    let mut process = Running::start(&server, &[], false);
    let endpoint = process.next_line();
    let port: u16 = endpoint.rsplit(':').next().unwrap().parse().unwrap();
    let mut peer = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    peer.write_all(&[73]).unwrap();
    let (status, stdout, stderr) = process.finish_for(Duration::from_secs(10));
    assert!(status.success(), "TCP large-read server failed:\n{stderr}");
    assert_live_zero(&stderr);
    assert!(stdout.contains("read-bounded\n"), "{stdout}");
}

#[test]
fn tcp_peer_close_is_reported_as_a_write_error_without_sigpipe() {
    let server_source = r#"
void main() {
    Text host = new Text("127.0.0.1");
    match (SocketAddress.resolve(host, 0, false)) {
        Err(error) { println("resolve-error"); }
        Ok(addresses) {
            match (TcpListener.bind(addresses.getRef(0), 8)) {
                Err(error) { println("bind-error"); }
                Ok(listener) {
                    match (listener.localAddress()) {
                        Err(error) { println("local-error"); }
                        Ok(local) {
                            Text endpoint = local.toText();
                            Console.printText(endpoint);
                            Console.print("\n");
                            match (listener.accept(5000L)) {
                                Err(error) { println("accept-error"); }
                                Ok(stream) {
                                    Console.print("ACCEPTED\n");
                                    match (Console.readLine()) {
                                        Err(error) { println("input-error"); }
                                        Ok(input) {
                                            match (stream.close()) {
                                                Ok(unit) { Console.print("PEER_CLOSED\n"); }
                                                Err(error) { println("stream-close-error"); }
                                            }
                                        }
                                    }
                                }
                            }
                            match (listener.close()) { Ok(unit) { println("listener-closed"); } Err(error) { println("listener-close-error"); } }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let client_source = r#"
void main() {
    match (Args.get(0)) {
        Err(error) { println("argument-error"); }
        Ok(port_text) {
            match (port_text.parseInt()) {
                Err(error) { println("port-error"); }
                Ok(port) {
                    Text host = new Text("127.0.0.1");
                    match (SocketAddress.resolve(host, port, false)) {
                        Err(error) { println("resolve-error"); }
                        Ok(addresses) {
                            match (TcpStream.connect(addresses.getRef(0), 0L)) {
                                Err(error) { println("connect-error"); }
                                Ok(stream) {
                                    Console.print("CONNECTED\n");
                                    match (Console.readLine()) {
                                        Err(error) { println("input-error"); }
                                        Ok(input) {
                                            match (ByteBuffer.withCapacity(0L)) {
                                                Err(error) { println("read-buffer-error"); }
                                                Ok(received) {
                                                    match (stream.readSome(received, 1L, 5000L)) {
                                                        Err(error) { println("peer-read-error"); }
                                                        Ok(count) {
                                                            if (count == 0L) {
                                                                match (ByteBuffer.withCapacity(1048576L)) {
                                                                    Err(error) { println("write-buffer-error"); }
                                                                    Ok(payload) {
                                                                        int index = 0;
                                                                        while (index < 1048576) {
                                                                            payload.append((byte)88);
                                                                            index = index + 1;
                                                                        }
                                                                        match (stream.writeAll(payload, 1000L)) {
                                                                            Err(error) { println("PEER_CLOSE_WRITE_ERR"); }
                                                                            Ok(unit) { println("PEER_CLOSE_WRITE_UNEXPECTED"); }
                                                                        }
                                                                    }
                                                                }
                                                            } else { println("peer-data-unexpected"); }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    match (stream.close()) { Ok(unit) { println("client-closed"); } Err(error) { println("client-close-error"); } }
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
    let server = compile(server_source);
    let client = compile(client_source);
    let mut server_process = Running::start(&server, &[], true);
    let endpoint = server_process.next_line();
    let port = endpoint.rsplit(':').next().unwrap();
    let mut client_process = Running::start(&client, &[port], true);
    assert_eq!(client_process.next_line(), "CONNECTED");
    assert_eq!(server_process.next_line(), "ACCEPTED");
    server_process.write_line("\n");
    assert_eq!(server_process.next_line(), "PEER_CLOSED");
    client_process.write_line("\n");
    let (client_status, client_stdout, client_stderr) = client_process.finish_for(Duration::from_secs(10));
    assert!(client_status.success(), "peer-close client failed:\n{client_stderr}");
    assert_live_zero(&client_stderr);
    assert!(client_stdout.contains("PEER_CLOSE_WRITE_ERR\n"), "{client_stdout}");
    let (server_status, server_stdout, server_stderr) = server_process.finish_for(Duration::from_secs(10));
    assert!(server_status.success(), "peer-close server failed:\n{server_stderr}");
    assert_live_zero(&server_stderr);
    assert!(server_stdout.contains("listener-closed\n"), "{server_stdout}");
}


#[test]
fn tcp_connect_uses_a_single_finite_deadline_with_a_full_accept_queue() {
    let helper_source = r#"
#define _GNU_SOURCE
#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>
int main(void) {
    int listener = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (listener < 0) { return 1; }
    struct sockaddr_in address;
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address.sin_port = 0;
    if (bind(listener, (struct sockaddr *)&address, sizeof(address)) != 0 || listen(listener, 1) != 0) { return 2; }
    socklen_t address_length = sizeof(address);
    if (getsockname(listener, (struct sockaddr *)&address, &address_length) != 0) { return 3; }
    printf("%u\n", (unsigned)ntohs(address.sin_port));
    fflush(stdout);
    int filler = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (filler < 0 || connect(filler, (struct sockaddr *)&address, sizeof(address)) != 0) { return 4; }
    int filler2 = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (filler2 < 0 || connect(filler2, (struct sockaddr *)&address, sizeof(address)) != 0) { return 5; }
    puts("READY");
    fflush(stdout);
    sleep(5);
    close(filler2);
    close(filler);
    close(listener);
    return 0;
}
"#;
    let helper = native::compile_c(helper_source).unwrap();
    let mut listener = Running::start(&helper, &[], false);
    let port = listener.next_line();
    assert_eq!(listener.next_line(), "READY");
    let source = r#"
void main() {
    match (Args.get(0)) {
        Err(error) { println("argument-error"); }
        Ok(port_text) {
            match (port_text.parseInt()) {
                Err(error) { println("port-error"); }
                Ok(port) {
                    Text host = new Text("127.0.0.1");
                    match (SocketAddress.resolve(host, port, false)) {
                        Err(error) { println("resolve-error"); }
                        Ok(addresses) {
                            match (TcpStream.connect(addresses.getRef(0), -1L)) {
                                Err(error) { println("connect-negative-rejected"); }
                                Ok(stream) { println("connect-negative-unexpected"); drop(stream); }
                            }
                            match (TcpStream.connect(addresses.getRef(0), 50L)) {
                                Err(error) { println("connect-deadline"); }
                                Ok(stream) { println("connect-unexpected"); drop(stream); }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
    let client = compile(source);
    let output = run(&client, &[&port]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("connect-negative-rejected\n"), "{stdout}");
    assert!(stdout.contains("connect-deadline\n"), "{stdout}");
    drop(listener);
}

#[test]
fn udp_loopback_reports_zero_and_truncated_datagrams_and_closed_handles() {
    let source = r#"
void main() {
    Text host = new Text("127.0.0.1");
    match (SocketAddress.resolve(host, 0, false)) {
        Err(error) { println("resolve-error"); }
        Ok(addresses) {
            match (UdpSocket.bind(addresses.getRef(0))) {
                Err(error) { println("bind-error"); }
                Ok(socket) {
                    match (socket.localAddress()) {
                        Err(error) { println("local-error"); }
                        Ok(local) {
                            match (ByteBuffer.withCapacity(0L)) {
                                Err(error) { println("empty-error"); }
                                Ok(empty) {
                                    match (socket.sendTo(empty, 0L, 0L, local, -1L)) {
                                        Err(error) { println("udp-send-negative-rejected"); }
                                        Ok(count) { println("udp-send-negative-unexpected"); }
                                    }
                                    match (socket.receiveFrom(empty, 0L, -1L)) {
                                        Err(error) { println("udp-receive-negative-rejected"); }
                                        Ok(datagram) { println("udp-receive-negative-unexpected"); }
                                    }
                                    match (socket.sendTo(empty, 0L, 0L, local, 0L)) {
                                        Err(error) { println("zero-send-error"); }
                                        Ok(count) { println(count == 0L); }
                                    }
                                }
                            }
                            match (ByteBuffer.withCapacity(0L)) {
                                Err(error) { println("receive-buffer-error"); }
                                Ok(received) {
                                    received.append((byte)90);
                                    match (socket.receiveFrom(received, 0L, 0L)) {
                                        Err(error) { println("zero-receive-error"); }
                                        Ok(datagram) {
                                            println(datagram.bytesRead() == 0L && !datagram.truncated());
                                            println(received.length() == 1L && received.get(0L) == (byte)90);
                                            borrow SocketAddress source_address = datagram.source();
                                            Text sender = source_address.toText();
                                            Text endpoint = local.toText();
                                            println(sender.equals(endpoint));
                                        }
                                    }
                                    match (ByteBuffer.withCapacity(0L)) {
                                        Err(error) { println("payload-error"); }
                                        Ok(payload) {
                                            payload.append((byte)65);
                                            payload.append((byte)66);
                                            payload.append((byte)67);
                                            payload.append((byte)68);
                                            match (socket.sendTo(payload, 0L, 4L, local, 0L)) {
                                                Err(error) { println("payload-send-error"); }
                                                Ok(count) { println(count == 4L); }
                                            }
                                        }
                                    }
                                    match (socket.receiveFrom(received, 2L, 0L)) {
                                        Err(error) { println("oversize-receive-error"); }
                                        Ok(datagram) {
                                            println(datagram.bytesRead() == 2L && datagram.truncated());
                                            println(received.length() == 3L
                                                && received.get(0L) == (byte)90
                                                && received.get(1L) == (byte)65
                                                && received.get(2L) == (byte)66);
                                        }
                                    }
                                    match (socket.sendTo(received, 0L, 1L, local, 0L)) {
                                        Err(error) { println("bounded-send-error"); }
                                        Ok(count) {
                                            if (count == 1L) { println("bounded-send-ok"); }
                                            else { println("bounded-send-error"); }
                                        }
                                    }
                                    match (ByteBuffer.withCapacity(0L)) {
                                        Err(error) { println("bounded-buffer-error"); }
                                        Ok(bounded) {
                                            match (socket.receiveFrom(bounded, 1048576L, 1000L)) {
                                                Err(error) { println("bounded-receive-error"); }
                                                Ok(datagram) {
                                                    if (datagram.bytesRead() == 1L && !datagram.truncated()
                                                        && bounded.length() == 1L && bounded.capacity() <= 131072L
                                                        && bounded.get(0L) == (byte)90) {
                                                        println("udp-read-bounded");
                                                    } else { println("udp-read-unbounded"); }
                                                }
                                            }
                                        }
                                    }
                                    match (ByteBuffer.withCapacity(0L)) {
                                        Err(error) { println("timeout-buffer-error"); }
                                        Ok(timeout_buffer) {
                                            match (socket.receiveFrom(timeout_buffer, 1048576L, 20L)) {
                                                Err(error) { println("udp-deadline"); }
                                                Ok(datagram) { println("udp-unexpected"); }
                                            }
                                            if (timeout_buffer.capacity() == 0L) {
                                                println("udp-timeout-unallocated");
                                            } else { println("udp-timeout-allocation"); }
                                        }
                                    }
                                    match (socket.close()) {
                                        Ok(unit) { println("udp-closed"); }
                                        Err(error) { println("udp-close-error"); }
                                    }
                                    match (socket.localAddress()) {
                                        Err(error) { println("closed-udp-address-error"); }
                                        Ok(address) { println("closed-udp-address-open"); }
                                    }
                                    match (socket.sendTo(received, 0L, 0L, local, 0L)) {
                                        Err(error) { println("closed-udp-send-error"); }
                                        Ok(count) { println("closed-udp-send-open"); }
                                    }
                                    match (socket.receiveFrom(received, 0L, 0L)) {
                                        Err(error) { println("closed-udp-receive-error"); }
                                        Ok(datagram) { println("closed-udp-receive-open"); }
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
    let program = compile(source);
    let output = run(&program, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for marker in [
        "udp-send-negative-rejected\nudp-receive-negative-rejected\n",
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
        "bounded-send-ok\nudp-read-bounded\n",
        "udp-deadline\nudp-timeout-unallocated\nudp-closed\nclosed-udp-address-error\nclosed-udp-send-error\nclosed-udp-receive-error\n",
    ] {
        assert!(stdout.contains(marker), "missing {marker:?} in {stdout}");
    }
}

#[test]
fn udp_ipv6_loopback_is_exercised_when_available() {
    let source = r#"
void main() {
    Text host = new Text("::1");
    match (SocketAddress.resolve(host, 0, false)) {
        Err(error) { println("IPV6_UNAVAILABLE"); }
        Ok(addresses) {
            match (UdpSocket.bind(addresses.getRef(0))) {
                Err(error) { println("IPV6_UNAVAILABLE"); }
                Ok(socket) {
                    match (socket.localAddress()) {
                        Err(error) { println(false); }
                        Ok(local) {
                            match (ByteBuffer.withCapacity(0L)) {
                                Err(error) { println(false); }
                                Ok(payload) {
                                    payload.append((byte)86);
                                    match (socket.sendTo(payload, 0L, 1L, local, 1000L)) {
                                        Err(error) { println(false); }
                                        Ok(sent) {
                                            match (ByteBuffer.withCapacity(0L)) {
                                                Err(error) { println(false); }
                                                Ok(received) {
                                                    match (socket.receiveFrom(received, 1L, 1000L)) {
                                                        Err(error) { println(false); }
                                                        Ok(datagram) {
                                                            println(sent == 1L && datagram.bytesRead() == 1L
                                                                && received.get(0L) == (byte)86
                                                                && !datagram.truncated());
                                                            println("IPV6_AVAILABLE");
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
}
"#;
    let program = compile(source);
    let output = run(&program, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim() == "true\nIPV6_AVAILABLE" || stdout.trim() == "IPV6_UNAVAILABLE", "{stdout}");
}

#[test]
fn failed_factories_and_dropped_handles_release_real_descriptors() {
    let source = r#"
void main() {
    Text host = new Text("127.0.0.1");
    int attempt = 0;
    while (attempt < 24) {
        match (SocketAddress.resolve(host, 0, false)) {
            Err(error) { println("resolve-error"); }
            Ok(addresses) {
                match (TcpListener.bind(addresses.getRef(0), 8)) {
                    Err(error) { println("listener-error"); }
                    Ok(listener) {
                        match (listener.localAddress()) {
                            Err(error) { println("listener-address-error"); }
                            Ok(bound) {
                                match (TcpListener.bind(bound, 8)) {
                                    Err(error) { println("tcp-bind-rejected"); }
                                    Ok(duplicate) { println("tcp-bind-unexpected"); drop(duplicate); }
                                }
                            }
                        }
                    }
                }
                match (UdpSocket.bind(addresses.getRef(0))) {
                    Err(error) { println("udp-error"); }
                    Ok(socket) {
                        match (socket.localAddress()) {
                            Err(error) { println("udp-address-error"); }
                            Ok(bound) {
                                match (UdpSocket.bind(bound)) {
                                    Err(error) { println("udp-bind-rejected"); }
                                    Ok(duplicate) { println("udp-bind-unexpected"); drop(duplicate); }
                                }
                            }
                        }
                    }
                }
            }
        }
        attempt = attempt + 1;
    }
    println("READY");
    Console.print("");
    match (Console.readLine()) {
        Ok(option) { println("finished"); }
        Err(error) { println("input-error"); }
    }
}
"#;
    let baseline_program = compile(r#"
void main() {
    Console.print("READY\n");
    match (Console.readLine()) {
        Ok(option) { println("finished"); }
        Err(error) { println("input-error"); }
    }
}
"#);
    let mut baseline = Running::start(&baseline_program, &[], true);
    assert_eq!(baseline.next_line(), "READY");
    let baseline_descriptors = std::fs::read_dir(format!("/proc/{}/fd", baseline.child.id())).unwrap().count();
    baseline.write_line("\n");
    let (baseline_status, _, baseline_stderr) = baseline.finish_for(Duration::from_secs(10));
    assert!(baseline_status.success(), "descriptor baseline failed:\n{baseline_stderr}");
    assert_live_zero(&baseline_stderr);
    let program = compile(source);
    let mut process = Running::start(&program, &[], true);
    loop {
        let line = process.next_line();
        if line == "READY" { break; }
        assert!(line == "tcp-bind-rejected" || line == "udp-bind-rejected", "{line}");
    }
    let pid = process.child.id();
    let descriptors: Vec<_> = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .unwrap()
        .map(|entry| std::fs::read_link(entry.unwrap().path()).unwrap().display().to_string())
        .collect();
    assert_eq!(descriptors.len(), baseline_descriptors, "network descriptors remained open after factory failures and drops: {descriptors:?}");
    process.write_line("\n");
    let (status, stdout, stderr) = process.finish_for(Duration::from_secs(10));
    assert!(status.success(), "resource test failed:\n{stderr}");
    assert_live_zero(&stderr);
    assert!(stdout.contains("tcp-bind-rejected\nudp-bind-rejected\nREADY\nfinished\n"), "{stdout}");
}
