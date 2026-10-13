use nacre::{compile_to_c, native, validate};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

fn execute(source: &str, flags: &[&str]) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("concurrency_alpha.ncr", source)));
    native::compile_c_with_flags(&c, flags)
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_clean(output: &Output, expected_stdout: &str) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected_stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let fields = stderr.split_whitespace().collect::<Vec<_>>();
    assert_eq!(fields.len(), 4, "{stderr}");
    assert_eq!(fields[0], "NACRE_MEMORY");
    let allocations = fields[1].strip_prefix("allocations=").unwrap().parse::<usize>().unwrap();
    let frees = fields[2].strip_prefix("frees=").unwrap().parse::<usize>().unwrap();
    assert_eq!(allocations, frees, "{stderr}");
    assert_eq!(fields[3], "live=0", "{stderr}");
    assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
    assert!(!stderr.contains("runtime error:"), "{stderr}");
}

fn channel_wait_observer(c: &str) -> String {
    let sender_wait = "pthread_cond_wait(&control->not_full, &control->mutex)";
    let receiver_wait = "pthread_cond_wait(&control->not_empty, &control->mutex)";
    assert!(c.contains(sender_wait), "generated sender condition wait was not found");
    assert!(c.contains(receiver_wait), "generated receiver condition wait was not found");
    let instrumented = c
        .replace(
            sender_wait,
            "ncr_test_sender_cond_wait(&control->not_full, &control->mutex)",
        )
        .replace(
            receiver_wait,
            "ncr_test_receiver_cond_wait(&control->not_empty, &control->mutex)",
        );
    let mut observed = r#"
#define _GNU_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <pthread.h>
#include <stdatomic.h>
#include <unistd.h>
static atomic_flag ncr_test_wait_reported = ATOMIC_FLAG_INIT;
static int ncr_test_sender_cond_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (!atomic_flag_test_and_set_explicit(&ncr_test_wait_reported, memory_order_relaxed)) {
        static const char marker[] = "NCR_TEST_WAIT_ENTRY:SENDER\n";
        (void)write(STDOUT_FILENO, marker, sizeof(marker) - 1);
    }
    return pthread_cond_wait(condition, mutex);
}
static int ncr_test_receiver_cond_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (!atomic_flag_test_and_set_explicit(&ncr_test_wait_reported, memory_order_relaxed)) {
        static const char marker[] = "NCR_TEST_WAIT_ENTRY:RECEIVER\n";
        (void)write(STDOUT_FILENO, marker, sizeof(marker) - 1);
    }
    return pthread_cond_wait(condition, mutex);
}
"#
    .to_owned();
    observed.push_str(&instrumented);
    observed
}

struct NativeWaitProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_rx: Receiver<String>,
    stderr_rx: Receiver<String>,
    stdout: String,
    stdout_reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
}

impl NativeWaitProcess {
    fn start(program: &native::NativeProgram) -> Self {
        let mut child = Command::new(program.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdin = child.stdin.take();
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let stdout_reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if stdout_tx.send(line).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let (stderr_tx, stderr_rx) = mpsc::channel();
        let stderr_reader = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut output = String::new();
            let _ = reader.read_to_string(&mut output);
            let _ = stderr_tx.send(output);
        });
        Self {
            child,
            stdin,
            stdout_rx,
            stderr_rx,
            stdout: String::new(),
            stdout_reader: Some(stdout_reader),
            stderr_reader: Some(stderr_reader),
        }
    }

    fn wait_for_entry(&mut self, expected: &str) {
        let line = self
            .stdout_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("native channel operation did not enter pthread_cond_wait");
        assert_eq!(line.trim_end_matches(['\r', '\n']), expected);
    }

    fn release(&mut self) {
        let mut stdin = self.stdin.take().expect("native stdin is piped");
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn finish_for(mut self, timeout: Duration) -> Output {
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "native channel operation exceeded its finite test deadline"
            );
            thread::sleep(Duration::from_millis(10));
        };
        loop {
            match self.stdout_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(line) => self.stdout.push_str(&line),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    panic!("native stdout reader did not finish")
                }
            }
        }
        let stderr = self
            .stderr_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("native stderr reader did not finish");
        if let Some(reader) = self.stdout_reader.take() {
            reader.join().unwrap();
        }
        if let Some(reader) = self.stderr_reader.take() {
            reader.join().unwrap();
        }
        Output {
            status,
            stdout: std::mem::take(&mut self.stdout).into_bytes(),
            stderr: stderr.into_bytes(),
        }
    }
}

impl Drop for NativeWaitProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(reader) = self.stdout_reader.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }
}

fn execute_after_channel_wait(source: &str, expected_wait: &str) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("concurrency_alpha.ncr", source)));
    let c = channel_wait_observer(&c);
    let program = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"));
    let mut process = NativeWaitProcess::start(&program);
    process.wait_for_entry(expected_wait);
    process.release();
    process.finish_for(Duration::from_secs(10))
}

const THREAD_LIFETIME: &str = r#"
class OutputTask implements Task<Text> {
    private Text message;
    public OutputTask(Text message) { this.message = message; }
    public Text run() {
        this.message.print();
        return new Text("transferred result");
    }
}

Result<Unit, Text> exerciseThreads() {
    Thread<Text> joined = Thread<Text>.spawn(new OutputTask(new Text("worker joined")))?;
    match (joined.join()) {
        Ok(value) { value.print(); }
        Err(error) { return new Result<Unit, Text>.Err(error); }
    }
    match (joined.join()) {
        Ok(value) { value.print(); }
        Err(error) { println("repeat join rejected"); }
    }
    Thread<Text> dropped = Thread<Text>.spawn(new OutputTask(new Text("joined during drop")))?;
    drop(dropped);
    println("drop completed");
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exerciseThreads()) {
        Ok(value) {}
        Err(error) { error.print(); }
    }
}
"#;

#[test]
fn thread_join_transfers_results_and_drop_joins() {
    let output = execute(THREAD_LIFETIME, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "worker joined\ntransferred result\nrepeat join rejected\njoined during drop\ndrop completed\n");
}

#[test]
fn thread_result_can_carry_an_application_error_as_its_value() {
    let source = r#"
class FailedTask implements Task<Result<int, Text>> {
    public Result<int, Text> run() {
        return new Result<int, Text>.Err(new Text("task failure"));
    }
}
void main() {
    match (Thread<Result<int, Text>>.spawn(new FailedTask())) {
        Ok(thread) {
            match (thread.join()) {
                Ok(outcome) {
                    match (outcome) {
                        Ok(value) { println(value); }
                        Err(error) { println("task returned an application error"); error.print(); }
                    }
                }
                Err(error) { println("pthread join failed"); error.print(); }
            }
        }
        Err(error) { println("pthread creation failed"); error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "task returned an application error\ntask failure\n");
}

#[test]
fn failed_thread_creation_drops_task_context_and_thread_storage() {
    let source = r#"
class NumberTask implements Task<int> {
    public int run() { return 7; }
}
void main() {
    match (Thread<int>.spawn(new NumberTask())) {
        Ok(thread) { println("unexpected success"); drop(thread); }
        Err(error) { println("thread creation failed"); error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS", "-DNCR_TEST_FAIL_PTHREAD_CREATE"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("thread creation failed\n"), "{stdout}");
    assert!(stdout.contains("pthread_create"), "{stdout}");
    let stats = String::from_utf8_lossy(&output.stderr);
    assert!(stats.contains("NACRE_MEMORY allocations="), "{stats}");
    assert!(stats.contains(" live=0\n"), "{stats}");
    assert!(!stats.contains("AddressSanitizer"), "{stats}");
    assert!(!stats.contains("runtime error:"), "{stats}");
}

const MUTEX_STRESS: &str = r#"
class IncrementTask implements Task<Unit> {
    private Mutex<int> counter;
    private int rounds;
    public IncrementTask(Mutex<int> counter, int rounds) {
        this.counter = counter;
        this.rounds = rounds;
    }
    public Unit run() {
        int i = 0;
        while (i < this.rounds) {
            match (this.counter.lock()) {
                Ok(guard) {
                    int current = guard.read();
                    match (guard.write(current + 1)) {
                        Ok(unit) {}
                        Err(error) { error.print(); return (); }
                    }
                }
                Err(error) { error.print(); return (); }
            }
            i = i + 1;
        }
        return ();
    }
}

Result<int, Text> exerciseMutex() {
    Mutex<int> shared = Mutex<int>.create(0)?;
    Thread<Unit> first = Thread<Unit>.spawn(new IncrementTask(shared.cloneHandle()?, 3000))?;
    Thread<Unit> second = Thread<Unit>.spawn(new IncrementTask(shared.cloneHandle()?, 3000))?;
    Thread<Unit> third = Thread<Unit>.spawn(new IncrementTask(shared.cloneHandle()?, 3000))?;
    Unit firstDone = first.join()?;
    Unit secondDone = second.join()?;
    Unit thirdDone = third.join()?;
    Guard<int> guard = shared.lock()?;
    return new Result<int, Text>.Ok(guard.read());
}

void main() {
    match (exerciseMutex()) {
        Ok(value) { println(value); }
        Err(error) { error.print(); }
    }
}
"#;

#[test]
fn cloned_mutex_handles_serialize_shared_updates() {
    let output = execute(MUTEX_STRESS, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "9000\n");
}

const CHANNEL_STRESS: &str = r#"
class Producer implements Task<Unit> {
    private Sender<int> sender;
    public Producer(Sender<int> sender) { this.sender = sender; }
    public Unit run() {
        int value = 1;
        while (value <= 1000) {
            match (this.sender.send(value)) {
                Ok(unit) {}
                Err(error) {
                    match (error) {
                        Closed(original) { println(original); }
                        System(message, original) { message.print(); println(original); }
                    }
                    return ();
                }
            }
            value = value + 1;
        }
        match (this.sender.close()) {
            Ok(unit) {}
            Err(error) { error.print(); }
        }
        return ();
    }
}

Result<Unit, Text> exerciseChannel() {
    Channel<int> channel = Channel<int>.bounded(7L)?;
    Sender<int> sender = channel.takeSender()?;
    Receiver<int> receiver = channel.takeReceiver()?;
    Thread<Unit> producer = Thread<Unit>.spawn(new Producer(sender))?;
    int expected = 1;
    boolean fifo = true;
    int total = 0;
    while (true) {
        match (receiver.receive()) {
            Ok(value) {
                match (value) {
                    Some(item) {
                        if (item != expected) { fifo = false; }
                        expected = expected + 1;
                        total = total + item;
                    }
                    None { break; }
                }
            }
            Err(error) { return new Result<Unit, Text>.Err(error); }
        }
    }
    Unit producerDone = producer.join()?;
    if (!fifo || expected != 1001 || total != 500500) {
        return new Result<Unit, Text>.Err(new Text("channel order or total was incorrect"));
    }
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exerciseChannel()) {
        Ok(value) { println("channel FIFO and EOF passed"); }
        Err(error) { error.print(); }
    }
}
"#;

#[test]
fn bounded_channel_preserves_fifo_and_reports_eof_after_sender_close() {
    let output = execute(CHANNEL_STRESS, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "channel FIFO and EOF passed\n");
}

const CHANNEL_ABORT: &str = r#"
class WaitingSender implements Task<Unit> {
    private Sender<Text> sender;
    public WaitingSender(Sender<Text> sender) { this.sender = sender; }
    public Unit run() {
        match (this.sender.send(new Text("second"))) {
            Ok(value) { println("unexpected send success"); }
            Err(error) {
                match (error) {
                    Closed(original) { original.print(); }
                    System(message, original) { message.print(); original.print(); }
                }
            }
        }
        return ();
    }
}

void main() {
    Channel<Text> channel;
    match (Channel<Text>.bounded(1L)) {
        Ok(value) { channel = value; }
        Err(error) { error.print(); return; }
    }
    Sender<Text> sender;
    Receiver<Text> receiver;
    match (channel.takeSender()) {
        Ok(value) { sender = value; }
        Err(error) { error.print(); return; }
    }
    match (channel.takeReceiver()) {
        Ok(value) { receiver = value; }
        Err(error) { error.print(); return; }
    }
    match (sender.send(new Text("queued"))) {
        Ok(value) {}
        Err(error) { println("unexpected first send error"); }
    }
    Sender<Text> workerSender;
    match (sender.cloneSender()) {
        Ok(value) { workerSender = value; }
        Err(error) { error.print(); return; }
    }
    Thread<Unit> worker;
    match (Thread<Unit>.spawn(new WaitingSender(workerSender))) {
        Ok(value) { worker = value; }
        Err(error) { error.print(); return; }
    }
    match (Console.readLine()) {
        Ok(value) {
            match (value) {
                Some(line) { drop(line); }
                None { println("unexpected release EOF"); }
            }
        }
        Err(error) { error.print(); }
    }
    match (receiver.close()) {
        Ok(value) {}
        Err(error) { error.print(); return; }
    }
    match (worker.join()) {
        Ok(value) { println("blocked sender woke"); }
        Err(error) { error.print(); }
    }
    match (sender.send(new Text("third"))) {
        Ok(value) { println("unexpected send after abort"); }
        Err(error) {
            match (error) {
                Closed(original) { original.print(); }
                System(message, original) { message.print(); original.print(); }
            }
        }
    }
}
"#;

#[test]
fn receiver_abort_wakes_senders_and_returns_unsent_values() {
    let output = execute_after_channel_wait(CHANNEL_ABORT, "NCR_TEST_WAIT_ENTRY:SENDER");
    assert_clean(&output, "second\nblocked sender woke\nthird\n");
}

const BLOCKED_RECEIVER: &str = r#"
class WaitingReceiver implements Task<Unit> {
    private Receiver<int> receiver;
    public WaitingReceiver(Receiver<int> receiver) { this.receiver = receiver; }
    public Unit run() {
        match (this.receiver.receive()) {
            Ok(value) {
                match (value) {
                    Some(item) { println("unexpected item"); }
                    None { println("receiver observed EOF"); }
                }
            }
            Err(error) { error.print(); }
        }
        return ();
    }
}

void waitForHostRelease() {
    match (Console.readLine()) {
        Ok(value) {
            match (value) {
                Some(line) { drop(line); }
                None { println("unexpected release EOF"); }
            }
        }
        Err(error) { error.print(); }
    }
}

void main() {
    Channel<int> channel;
    match (Channel<int>.bounded(1L)) {
        Ok(value) { channel = value; }
        Err(error) { error.print(); return; }
    }
    Sender<int> sender;
    Receiver<int> receiver;
    match (channel.takeSender()) {
        Ok(value) { sender = value; }
        Err(error) { error.print(); return; }
    }
    match (channel.takeReceiver()) {
        Ok(value) { receiver = value; }
        Err(error) { error.print(); return; }
    }
    Thread<Unit> worker;
    match (Thread<Unit>.spawn(new WaitingReceiver(receiver))) {
        Ok(value) { worker = value; }
        Err(error) { error.print(); return; }
    }
    waitForHostRelease();
    match (sender.close()) {
        Ok(value) {}
        Err(error) { error.print(); return; }
    }
    match (worker.join()) {
        Ok(value) {}
        Err(error) { error.print(); }
    }
}
"#;

#[test]
fn last_sender_close_wakes_a_receiver_blocked_in_pthread_cond_wait() {
    let output =
        execute_after_channel_wait(BLOCKED_RECEIVER, "NCR_TEST_WAIT_ENTRY:RECEIVER");
    assert_clean(&output, "receiver observed EOF\n");
}

#[test]
fn condition_wait_failure_returns_system_error_with_original_value() {
    let source = r#"
void main() {
    Channel<Text> channel;
    match (Channel<Text>.bounded(1L)) {
        Ok(value) { channel = value; }
        Err(error) { error.print(); return; }
    }
    Sender<Text> sender;
    Receiver<Text> receiver;
    match (channel.takeSender()) {
        Ok(value) { sender = value; }
        Err(error) { error.print(); return; }
    }
    match (channel.takeReceiver()) {
        Ok(value) { receiver = value; }
        Err(error) { error.print(); return; }
    }
    match (sender.send(new Text("queued"))) {
        Ok(value) {}
        Err(error) { println("unexpected first send error"); }
    }
    match (sender.send(new Text("returned"))) {
        Ok(value) { println("unexpected send success"); }
        Err(error) {
            match (error) {
                Closed(original) { println("unexpected closed error"); original.print(); }
                System(message, original) {
                    println("system failure returned value");
                    original.print();
                }
            }
        }
    }
    match (receiver.close()) {
        Ok(value) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(
        source,
        &["-DNACRE_MEMORY_STATS", "-DNCR_TEST_FAIL_PTHREAD_COND_WAIT"],
    );
    assert_clean(&output, "system failure returned value\nreturned\n");
}

#[test]
fn cloned_senders_keep_channel_open_until_the_last_close() {
    let source = r#"
void main() {
    Channel<int> channel;
    match (Channel<int>.bounded(1L)) {
        Ok(value) { channel = value; }
        Err(error) { error.print(); return; }
    }
    Sender<int> first;
    Receiver<int> receiver;
    match (channel.takeSender()) {
        Ok(value) { first = value; }
        Err(error) { error.print(); return; }
    }
    match (channel.takeReceiver()) {
        Ok(value) { receiver = value; }
        Err(error) { error.print(); return; }
    }
    boolean senderTakeRejected = false;
    boolean receiverTakeRejected = false;
    match (channel.takeSender()) {
        Ok(extra) { drop(extra); }
        Err(error) { senderTakeRejected = true; }
    }
    match (channel.takeReceiver()) {
        Ok(extra) { drop(extra); }
        Err(error) { receiverTakeRejected = true; }
    }
    if (senderTakeRejected && receiverTakeRejected) { println("endpoint takes are one-shot"); }
    Sender<int> second;
    match (first.cloneSender()) {
        Ok(value) { second = value; }
        Err(error) { error.print(); return; }
    }
    match (first.close()) {
        Ok(value) {}
        Err(error) { error.print(); return; }
    }
    match (second.send(42)) {
        Ok(value) {}
        Err(error) { println("unexpected closed sender"); }
    }
    match (second.close()) {
        Ok(value) {}
        Err(error) { error.print(); return; }
    }
    match (receiver.receive()) {
        Ok(value) {
            match (value) {
                Some(item) { println(item); }
                None { println("unexpected early EOF"); }
            }
        }
        Err(error) { error.print(); }
    }
    match (receiver.receive()) {
        Ok(value) {
            match (value) {
                Some(item) { println("unexpected extra value"); }
                None { println("last sender closed"); }
            }
        }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "endpoint takes are one-shot\n42\nlast sender closed\n");
}

#[test]
fn concurrent_last_sender_drops_wake_receiver_once() {
    let source = r#"
class DropSenderTask implements Task<Unit> {
    private Sender<int> sender;
    public DropSenderTask(Sender<int> sender) { this.sender = sender; }
    public Unit run() { return (); }
}
Result<Unit, Text> exercise() {
    Channel<int> channel = Channel<int>.bounded(1L)?;
    Sender<int> sender = channel.takeSender()?;
    Receiver<int> receiver = channel.takeReceiver()?;
    Sender<int> firstSender = sender.cloneSender()?;
    Sender<int> secondSender = sender.cloneSender()?;
    Sender<int> thirdSender = sender.cloneSender()?;
    Sender<int> fourthSender = sender.cloneSender()?;
    Unit originalSenderClosed = sender.close()?;
    Thread<Unit> first = Thread<Unit>.spawn(new DropSenderTask(firstSender))?;
    Thread<Unit> second = Thread<Unit>.spawn(new DropSenderTask(secondSender))?;
    Thread<Unit> third = Thread<Unit>.spawn(new DropSenderTask(thirdSender))?;
    Thread<Unit> fourth = Thread<Unit>.spawn(new DropSenderTask(fourthSender))?;
    while (true) {
        match (receiver.receive()) {
            Ok(value) {
                match (value) {
                    Some(item) { return new Result<Unit, Text>.Err(new Text("unexpected queued item")); }
                    None { break; }
                }
            }
            Err(error) { return new Result<Unit, Text>.Err(error); }
        }
    }
    Unit firstDone = first.join()?;
    Unit secondDone = second.join()?;
    Unit thirdDone = third.join()?;
    Unit fourthDone = fourth.join()?;
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(value) { println("last concurrent sender closed"); }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "last concurrent sender closed\n");
}

#[test]
fn channel_pair_drop_aborts_and_returns_the_original_send_value() {
    let source = r#"
void main() {
    Sender<int> sender;
    {
        Channel<int> channel;
        match (Channel<int>.bounded(1L)) {
            Ok(value) { channel = value; }
            Err(error) { error.print(); return; }
        }
        match (channel.takeSender()) {
            Ok(value) { sender = value; }
            Err(error) { error.print(); return; }
        }
    }
    match (sender.send(19)) {
        Ok(value) { println("unexpected send success"); }
        Err(error) {
            match (error) {
                Closed(original) { println(original); }
                System(message, original) { message.print(); println(original); }
            }
        }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "19\n");
}

#[test]
fn mutex_try_lock_reports_busy_then_succeeds_after_guard_drop() {
    let source = r#"
Result<int, Text> exercise() {
    Mutex<int> mutex = Mutex<int>.create(11)?;
    Guard<int> guard = mutex.lock()?;
    boolean recursiveRejected = false;
    match (mutex.lock()) {
        Ok(unexpected) { drop(unexpected); }
        Err(error) { recursiveRejected = true; }
    }
    boolean busy = false;
    match (mutex.tryLock()) {
        Ok(option) {
            match (option) {
                Some(unexpected) { drop(unexpected); }
                None { busy = true; }
            }
        }
        Err(error) { return new Result<int, Text>.Err(error); }
    }
    drop(guard);
    int value = -1;
    match (mutex.tryLock()) {
        Ok(option) {
            match (option) {
                Some(released) { value = released.read(); }
                None {}
            }
        }
        Err(error) { return new Result<int, Text>.Err(error); }
    }
    if (!busy || !recursiveRejected || value != 11) {
        return new Result<int, Text>.Err(new Text("tryLock did not observe the lock state"));
    }
    return new Result<int, Text>.Ok(value);
}
void main() {
    match (exercise()) {
        Ok(value) { println("tryLock ownership passed"); }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "tryLock ownership passed\n");
}

#[test]
fn channel_capacity_is_positive_and_allocation_arithmetic_is_checked() {
    let source = r#"
void main() {
    boolean rejectedZero = false;
    boolean rejectedOverflow = false;
    match (Channel<int>.bounded(0L)) {
        Ok(channel) { drop(channel); }
        Err(error) { rejectedZero = true; }
    }
    match (Channel<int>.bounded(9223372036854775807L)) {
        Ok(channel) { drop(channel); }
        Err(error) { rejectedOverflow = true; }
    }
    if (rejectedZero && rejectedOverflow) { println("channel capacity bounds passed"); }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "channel capacity bounds passed\n");
}

#[test]
fn abort_drops_nested_endpoints_after_unlocking_the_channel() {
    let source = r#"
class Holder {
    private Sender<Holder> nestedSender;
    public Holder(Sender<Holder> nestedSender) { this.nestedSender = nestedSender; }
}
void main() {
    Channel<Holder> channel;
    match (Channel<Holder>.bounded(1L)) {
        Ok(value) { channel = value; }
        Err(error) { error.print(); return; }
    }
    Sender<Holder> sender;
    Receiver<Holder> receiver;
    match (channel.takeSender()) {
        Ok(value) { sender = value; }
        Err(error) { error.print(); return; }
    }
    match (channel.takeReceiver()) {
        Ok(value) { receiver = value; }
        Err(error) { error.print(); return; }
    }
    Sender<Holder> nested;
    match (sender.cloneSender()) {
        Ok(value) { nested = value; }
        Err(error) { error.print(); return; }
    }
    match (sender.send(new Holder(nested))) {
        Ok(value) {}
        Err(error) { println("unexpected send error"); }
    }
    match (receiver.close()) {
        Ok(value) { println("queued endpoint was dropped"); }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "queued endpoint was dropped\n");
}

fn assert_sendability_rejected(source: &str) {
    let error = validate(source).unwrap_err();
    let rendered = error.render("sendability.ncr", source);
    assert!(rendered.contains("[E7101]"), "{rendered}");
}

#[test]
fn sendability_rejects_guard_handles_and_results() {
    assert_sendability_rejected(r#"
class GuardTask implements Task<int> {
    Guard<int> guard;
    GuardTask(Guard<int> guard) { this.guard = guard; }
    public int run() { return this.guard.read(); }
}
void main() {
    match (Mutex<int>.create(1)) {
        Ok(mutex) {
            match (mutex.lock()) {
                Ok(guard) { Thread<int>.spawn(new GuardTask(guard)); }
                Err(error) {}
            }
        }
        Err(error) {}
    }
}
"#);

    assert_sendability_rejected(r#"
class GuardResultTask implements Task<Result<Guard<int>, Text>> {
    Mutex<int> mutex;
    GuardResultTask(Mutex<int> mutex) { this.mutex = mutex; }
    public Result<Guard<int>, Text> run() { return this.mutex.lock(); }
}
void main() {
    match (Mutex<int>.create(1)) {
        Ok(mutex) { Thread<Result<Guard<int>, Text>>.spawn(new GuardResultTask(mutex)); }
        Err(error) {}
    }
}
"#);

    assert_sendability_rejected(r#"
enum GuardEnvelope {
    Held(Guard<int>);
    Empty;
}
class EmptyEnvelopeTask implements Task<GuardEnvelope> {
    public GuardEnvelope run() { return new GuardEnvelope.Empty(); }
}
void main() {
    Thread<GuardEnvelope>.spawn(new EmptyEnvelopeTask());
}
"#);

    assert_sendability_rejected(r#"
class GuardArray {
    Array<Guard<int>> values;
}
void main() {
    Channel<GuardArray>.bounded(1L);
}
"#);

    assert_sendability_rejected(r#"
interface NumberSource { int value(); }
class SafeSource implements NumberSource {
    public int value() { return 1; }
}
class GuardSource implements NumberSource {
    Guard<int> guard;
    GuardSource(Guard<int> guard) { this.guard = guard; }
    public int value() { return this.guard.read(); }
}
class InterfaceTask implements Task<int> {
    NumberSource source;
    InterfaceTask(NumberSource source) { this.source = source; }
    public int run() { return this.source.value(); }
}
void main() {
    NumberSource source = new SafeSource();
    Thread<int>.spawn(new InterfaceTask(source));
}
"#);
}

#[test]
fn guard_borrows_remain_rooted_in_the_guard() {
    let source = r#"
class Box { int value; Box(int value) { this.value = value; } }
Result<Unit, Text> invalidBorrow() {
    Mutex<Box> mutex = Mutex<Box>.create(new Box(1))?;
    Guard<Box> guard = mutex.lock()?;
    borrow mut Box alias = guard.getMut();
    drop(guard);
    alias.value = 2;
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (invalidBorrow()) {
        Ok(value) {}
        Err(error) { error.print(); }
    }
}
"#;
    let error = validate(source).unwrap_err();
    let rendered = error.render("guard_borrow.ncr", source);
    assert!(rendered.contains("borrow") || rendered.contains("loan"), "{rendered}");
}

#[test]
fn concurrency_systems_example_runs_as_a_native_program() {
    let output = execute(
        include_str!("../examples/systems/concurrency.ncr"),
        &["-DNACRE_MEMORY_STATS"],
    );
    assert_clean(&output, "2000\n15\n");
}

#[test]
fn concurrency_systems_example_runs_under_selected_native_sanitizer() {
    let sanitizer = std::env::var("NACRE_SANITIZER")
        .unwrap_or_else(|_| "address,undefined".to_owned());
    let sanitizer_flag = format!("-fsanitize={sanitizer}");
    let output = execute(
        include_str!("../examples/systems/concurrency.ncr"),
        &[&sanitizer_flag, "-fno-omit-frame-pointer", "-DNACRE_MEMORY_STATS"],
    );
    assert_clean(&output, "2000\n15\n");
}

#[test]
fn standalone_guard_and_endpoint_types_resolve_their_control_blocks() {
    let source = r#"
class HandleHolder {
    Sender<int> sender;
    Guard<int> guard;
    HandleHolder(Sender<int> sender, Guard<int> guard) {
        this.sender = sender;
        this.guard = guard;
    }
}
void main() {}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "");
}

#[test]
fn guards_borrow_heap_objects_inline_enums_and_interfaces_without_losing_owners() {
    let source = r#"
interface Value {
    readonly int read();
    void write(int value);
}
class Box implements Value {
    private int number;
    private Text marker;
    Box(int number) { this.number = number; this.marker = new Text("owned"); }
    public readonly int read() { return this.number; }
    public void write(int value) { this.number = value; }
}
enum Payload { Item(Text); }
Result<Unit, Text> exercise() {
    Mutex<Box> box = Mutex<Box>.create(new Box(42))?;
    {
        Guard<Box> guard = box.lock()?;
        { borrow Box shared = guard.getRef(); println(shared.read()); }
        { borrow mut Box exclusive = guard.getMut(); exclusive.write(99); }
        { borrow Box shared = guard.getRef(); println(shared.read()); }
    }
    Mutex<Payload> payload = Mutex<Payload>.create(new Payload.Item(new Text("enum")))?;
    {
        Guard<Payload> guard = payload.lock()?;
        { borrow mut Payload exclusive = guard.getMut(); match (exclusive) { Item(text) { text.append("!"); } } }
        { borrow Payload shared = guard.getRef(); match (shared) { Item(text) { println(text.byteLength()); } } }
    }
    Mutex<Value> value = Mutex<Value>.create(new Box(7))?;
    {
        Guard<Value> guard = value.lock()?;
        { borrow Value shared = guard.getRef(); println(shared.read()); }
        { borrow mut Value exclusive = guard.getMut(); exclusive.write(8); }
        { borrow Value shared = guard.getRef(); println(shared.read()); }
    }
    return new Result<Unit, Text>.Ok(());
}
void main() { match (exercise()) { Ok(unit) {} Err(error) { error.print(); } } }
"#;
    let output = execute(source, &[
        "-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined", "-fno-omit-frame-pointer",
    ]);
    assert_clean(&output, "42\n99\n5\n7\n8\n");
}

#[test]
fn deque_hidden_guard_payload_is_not_sendable_even_when_empty() {
    let source = "void main() { Mutex<Deque<Guard<int>>>.create(new Deque<Guard<int>>()); }";
    let error = validate(source).expect_err("a non-sendable Guard container was accepted");
    assert!(error.message.contains("E7101"), "{}", error.render("deque_guard.ncr", source));
}
