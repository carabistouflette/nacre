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

fn barrier_generation_observer(c: &str) -> String {
    let timed_wait = "pthread_cond_timedwait(&ncr_self->condition->condition, &control->mutex, &deadline)";
    let broadcast = "pthread_cond_broadcast(&ncr_self->condition->condition)";
    assert!(c.contains(timed_wait), "generated barrier condition wait was not found");
    assert!(c.contains(broadcast), "generated barrier notification was not found");
    let instrumented = c
        .replace(
            timed_wait,
            "ncr_test_barrier_timedwait(&ncr_self->condition->condition, &control->mutex, &deadline)",
        )
        .replace(
            broadcast,
            "ncr_test_barrier_broadcast(&ncr_self->condition->condition)",
        );
    let mut observed = r#"
#define _GNU_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <time.h>
static atomic_flag ncr_test_delayed_waiter = ATOMIC_FLAG_INIT;
static _Atomic bool ncr_test_next_generation_broken = false;
static _Atomic unsigned ncr_test_barrier_broadcasts = 0;
static int ncr_test_barrier_timedwait(pthread_cond_t *condition, pthread_mutex_t *mutex,
        const struct timespec *deadline) {
    int error = pthread_cond_timedwait(condition, mutex, deadline);
    if (error == 0 &&
            !atomic_flag_test_and_set_explicit(&ncr_test_delayed_waiter, memory_order_relaxed)) {
        error = pthread_mutex_unlock(mutex);
        if (error != 0) { return error; }
        while (!atomic_load_explicit(&ncr_test_next_generation_broken, memory_order_acquire)) {
            (void)sched_yield();
        }
        error = pthread_mutex_lock(mutex);
    }
    return error;
}
static int ncr_test_barrier_broadcast(pthread_cond_t *condition) {
    int error = pthread_cond_broadcast(condition);
    if (error == 0 &&
            atomic_fetch_add_explicit(&ncr_test_barrier_broadcasts, 1, memory_order_relaxed) == 1) {
        atomic_store_explicit(&ncr_test_next_generation_broken, true, memory_order_release);
    }
    return error;
}
"#
    .to_owned();
    observed.push_str(&instrumented);
    observed
}

fn fail_future_condition_initialization(c: &str) -> String {
    let initializer = "pthread_cond_init(&node->condition, &attributes)";
    assert!(c.contains(initializer), "generated condition initialization was not found");
    let instrumented = c.replace(
        initializer,
        "ncr_test_fail_condition_init(&node->condition, &attributes)",
    );
    let mut observed = r#"
#define _GNU_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <pthread.h>
static int ncr_test_fail_condition_init(pthread_cond_t *condition,
        const pthread_condattr_t *attributes) {
    (void)condition;
    (void)attributes;
    return EAGAIN;
}
"#
    .to_owned();
    observed.push_str(&instrumented);
    observed
}
fn observe_unbounded_sync_waits(c: &str) -> String {
    let mutex_wait = "pthread_cond_wait(&ncr_self->condition->condition, &control->mutex)";
    let future_wait = "pthread_cond_wait(&ncr_self->state_condition->condition, &control->mutex)";
    let instrumented = c
        .replace(
            mutex_wait,
            "ncr_test_sync_wait(&ncr_self->condition->condition, &control->mutex)",
        )
        .replace(
            future_wait,
            "ncr_test_sync_wait(&ncr_self->state_condition->condition, &control->mutex)",
        );
    let mut observed = r#"
#define _GNU_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <pthread.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdio.h>
static _Atomic unsigned ncr_test_sync_wait_count = 0;
static _Thread_local bool ncr_test_sync_wait_reported = false;
static int ncr_test_sync_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (!ncr_test_sync_wait_reported) {
        unsigned index = atomic_fetch_add_explicit(&ncr_test_sync_wait_count, 1, memory_order_relaxed);
        ncr_test_sync_wait_reported = true;
        printf("NCR_TEST_SYNC_WAIT:%u\n", index);
        fflush(stdout);
    }
    return pthread_cond_wait(condition, mutex);
}
"#
    .to_owned();
    observed.push_str(&instrumented);
    observed
}

fn observe_condition_node_lifetimes(c: &str) -> String {
    let initializer = "pthread_cond_init(&node->condition, &attributes)";
    let instrumented = c
        .replace(
            initializer,
            "ncr_test_condition_init(&node->condition, &attributes)",
        )
        .replace("pthread_cond_destroy(", "ncr_test_condition_destroy(");
    let mut observed = r#"
#define _GNU_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
static _Atomic size_t ncr_test_live_conditions = 0;
static int ncr_test_condition_init(pthread_cond_t *condition,
        const pthread_condattr_t *attributes) {
    int error = pthread_cond_init(condition, attributes);
    if (error == 0) {
        atomic_fetch_add_explicit(&ncr_test_live_conditions, 1, memory_order_relaxed);
    }
    return error;
}
static int ncr_test_condition_destroy(pthread_cond_t *condition) {
    int error = pthread_cond_destroy(condition);
    if (error == 0) {
        size_t live = atomic_fetch_sub_explicit(&ncr_test_live_conditions, 1,
            memory_order_relaxed) - 1;
        printf("NCR_TEST_CONDITION_LIVE:%zu\n", live);
        fflush(stdout);
    }
    return error;
}
"#
    .to_owned();
    observed.push_str(&instrumented);
    observed
}


fn fail_second_pool_worker_creation(c: &str) -> String {
    let create = "return pthread_create(thread, NULL, start, context);";
    let wait = "pthread_cond_wait(&state->ready->condition, &control->mutex)";
    let broadcast = "pthread_cond_broadcast(&state->ready->condition)";
    assert!(c.contains(create), "generated pthread creation helper was not found");
    assert!(c.contains(wait), "generated pool worker wait was not found");
    assert!(c.contains(broadcast), "generated pool condition broadcast was not found");
    let instrumented = c
        .replace(create, "return ncr_test_pool_thread_create(thread, start, context);")
        .replace(
            wait,
            "ncr_test_pool_cond_wait(&state->ready->condition, &control->mutex)",
        )
        .replace(
            broadcast,
            "ncr_test_pool_broadcast(&state->ready->condition)",
        );
    let mut observed = r#"
#define _GNU_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <time.h>
static _Atomic unsigned ncr_test_create_count = 0;
static _Atomic bool ncr_test_worker_waiting = false;
static _Atomic bool ncr_test_second_create_failed = false;
static _Atomic bool ncr_test_cleanup_broadcast = false;
static int ncr_test_pool_thread_create(pthread_t *thread, void *(*start)(void *),
        void *context) {
    unsigned call = atomic_fetch_add_explicit(&ncr_test_create_count, 1, memory_order_relaxed);
    if (call == 0) { return pthread_create(thread, NULL, start, context); }
    if (call == 1) {
        while (!atomic_load_explicit(&ncr_test_worker_waiting, memory_order_acquire)) {
            (void)sched_yield();
        }
        atomic_store_explicit(&ncr_test_second_create_failed, true, memory_order_release);
    }
    return EAGAIN;
}
static int ncr_test_pool_cond_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    atomic_store_explicit(&ncr_test_worker_waiting, true, memory_order_release);
    while (!atomic_load_explicit(&ncr_test_second_create_failed, memory_order_acquire)) {
        (void)sched_yield();
    }
    const struct timespec pause = { .tv_sec = 0, .tv_nsec = 1000000L };
    for (unsigned attempt = 0; attempt < 100 &&
            !atomic_load_explicit(&ncr_test_cleanup_broadcast, memory_order_acquire); ++attempt) {
        (void)nanosleep(&pause, NULL);
    }
    return pthread_cond_wait(condition, mutex);
}
static int ncr_test_pool_broadcast(pthread_cond_t *condition) {
    int error = pthread_cond_broadcast(condition);
    if (error == 0) {
        atomic_store_explicit(&ncr_test_cleanup_broadcast, true, memory_order_release);
    }
    return error;
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
            .expect("native operation did not enter pthread_cond_wait");
        assert_eq!(line.trim_end_matches(['\r', '\n']), expected);
    }

    fn release(&mut self) {
        let stdin = self.stdin.as_mut().expect("native stdin is piped");
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
                "native operation exceeded its finite test deadline"
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

#[test]
fn native_atomics_semaphores_and_latches_cover_values_bounds_and_transitions() {
    let source = r#"
Result<Unit, Text> exercise() {
    AtomicInt counter = AtomicInt.create(4)?;
    AtomicInt shared = counter.cloneHandle()?;
    int added = counter.fetchAdd(3);
    int failed = shared.compareExchange(6, 10);
    int exchanged = shared.compareExchange(7, 11);
    int old = counter.exchange(12);
    counter.store(13);
    println(added);
    println(failed);
    println(exchanged);
    println(old);
    println(counter.load());
    drop(shared);

    AtomicLong wide = AtomicLong.create(-2L)?;
    println(wide.fetchSubtract(5L));
    println(wide.load());
    AtomicBoolean flag = AtomicBoolean.create(false)?;
    println(flag.exchange(true));
    println(flag.compareExchange(false, false));
    println(flag.load());

    Semaphore permits = Semaphore.create(0L)?;
    println(permits.acquire(0L)?);
    println(permits.acquire(1L)?);
    Semaphore alias = permits.cloneHandle()?;
    drop(permits);
    alias.release(2L)?;
    println(alias.acquire(0L)?);
    println(alias.acquire(0L)?);
    println(alias.acquire(0L)?);
    boolean negativeRelease = false;
    match (alias.release(-1L)) {
        Ok(unit) {}
        Err(error) { negativeRelease = true; drop(error); }
    }
    if (negativeRelease) { println("negative release rejected"); }
    drop(alias);
    boolean negativePermits = false;
    match (Semaphore.create(-1L)) {
        Ok(value) { drop(value); }
        Err(error) { negativePermits = true; drop(error); }
    }
    if (negativePermits) { println("negative permits rejected"); }

    CountDownLatch latch = CountDownLatch.create(2L)?;
    CountDownLatch sharedLatch = latch.cloneHandle()?;
    drop(latch);
    println(sharedLatch.wait(0L)?);
    sharedLatch.countDown()?;
    println(sharedLatch.count());
    sharedLatch.countDown()?;
    sharedLatch.countDown()?;
    println(sharedLatch.count());
    println(sharedLatch.wait(0L)?);
    drop(sharedLatch);
    boolean negativeCount = false;
    match (CountDownLatch.create(-1L)) {
        Ok(value) { drop(value); }
        Err(error) { negativeCount = true; drop(error); }
    }
    if (negativeCount) { println("negative latch rejected"); }
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(
        &output,
        "4\n7\n7\n11\n13\n-2\n-7\nfalse\ntrue\ntrue\nfalse\nfalse\ntrue\ntrue\nfalse\nnegative release rejected\nnegative permits rejected\nfalse\n1\n0\ntrue\nnegative latch rejected\n",
    );
}

#[test]
fn concurrency_dependencies_are_requested_during_transitive_generic_expansion() {
    let source = r#"
class AtomicProbe<T> {
    public static Result<int, Text> run() {
        AtomicInt counter = AtomicInt.create(40)?;
        counter.fetchAdd(2);
        return new Result<int, Text>.Ok(counter.load());
    }
}

class ForwardProbe<T> {
    public static Result<int, Text> run() {
        return AtomicProbe<T>.run();
    }
}

void main() {
    match (ForwardProbe<int>.run()) {
        Ok(value) { println(value); }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "42\n");
}

#[test]
fn barriers_break_reset_and_reuse_generations() {
    let source = r#"
class BarrierWaitTask implements Task<Unit> {
    private Barrier barrier;
    private AtomicInt arrivals;
    public BarrierWaitTask(Barrier barrier, AtomicInt arrivals) {
        this.barrier = barrier;
        this.arrivals = arrivals;
    }
    public Unit run() {
        match (this.barrier.wait(1000L)) {
            Ok(index) { int previous = this.arrivals.fetchAdd(1); }
            Err(error) { error.print(); }
        }
        return ();
    }
}

Result<Unit, Text> exercise() {
    boolean invalidParties = false;
    match (Barrier.create(0)) {
        Ok(value) { drop(value); }
        Err(error) { invalidParties = true; drop(error); }
    }
    if (invalidParties) { println("zero parties rejected"); }

    Barrier barrier = Barrier.create(2)?;
    boolean prematureReset = false;
    match (barrier.reset()) {
        Ok(unit) { println("unexpected barrier reset"); }
        Err(error) { prematureReset = true; drop(error); }
    }
    if (prematureReset) { println("unbroken barrier reset rejected"); }
    match (barrier.wait(0L)) {
        Ok(index) { println("unexpected barrier pass"); }
        Err(error) { println("barrier generation timed out"); drop(error); }
    }
    match (barrier.wait(0L)) {
        Ok(index) { println("unexpected broken barrier pass"); }
        Err(error) { println("barrier generation broken"); drop(error); }
    }
    barrier.reset()?;

    AtomicInt arrivals = AtomicInt.create(0)?;
    Barrier workerBarrier = barrier.cloneHandle()?;
    Barrier mainBarrier = barrier.cloneHandle()?;
    AtomicInt workerArrivals = arrivals.cloneHandle()?;
    drop(barrier);
    Thread<Unit> worker = Thread<Unit>.spawn(new BarrierWaitTask(workerBarrier, workerArrivals))?;
    match (mainBarrier.wait(1000L)) {
        Ok(index) { int previous = arrivals.fetchAdd(1); }
        Err(error) { return new Result<Unit, Text>.Err(error); }
    }
    worker.join()?;
    if (arrivals.load() == 2) { println("barrier passed"); }
    drop(mainBarrier);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(
        &output,
        "zero parties rejected\nunbroken barrier reset rejected\nbarrier generation timed out\nbarrier generation broken\nbarrier passed\n",
    );
}

#[test]
fn successful_barrier_generation_is_not_broken_by_a_later_timeout() {
    let source = r#"
class BoundaryTask implements Task<Unit> {
    private Barrier barrier;
    private AtomicInt outcome;
    public BoundaryTask(Barrier barrier, AtomicInt outcome) {
        this.barrier = barrier;
        this.outcome = outcome;
    }
    public Unit run() {
        int state = 0;
        match (this.barrier.wait(5000L)) {
            Ok(index) { state = 2; }
            Err(error) { drop(error); }
        }
        boolean nextGenerationBroken = false;
        match (this.barrier.wait(0L)) {
            Ok(index) {}
            Err(error) { nextGenerationBroken = true; drop(error); }
        }
        if (nextGenerationBroken) { state = state + 1; }
        this.outcome.store(state);
        return ();
    }
}

Result<Unit, Text> exercise() {
    Barrier barrier = Barrier.create(2)?;
    AtomicInt outcome = AtomicInt.create(0)?;
    AtomicInt workerOutcome = outcome.cloneHandle()?;
    Barrier workerBarrier = barrier.cloneHandle()?;
    Thread<Unit> worker = Thread<Unit>.spawn(new BoundaryTask(workerBarrier, workerOutcome))?;

    int state = 0;
    match (barrier.wait(5000L)) {
        Ok(index) { state = 2; }
        Err(error) { drop(error); }
    }
    boolean nextGenerationBroken = false;
    match (barrier.wait(0L)) {
        Ok(index) {}
        Err(error) { nextGenerationBroken = true; drop(error); }
    }
    worker.join()?;
    boolean resetSucceeded = false;
    match (barrier.reset()) {
        Ok(unit) { resetSucceeded = true; }
        Err(error) { drop(error); }
    }
    println(state == 2 && nextGenerationBroken && outcome.load() == 3 && resetSucceeded);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("concurrency_alpha.ncr", source)));
    let c = barrier_generation_observer(&c);
    let output = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"));
    assert_clean(&output, "true\n");
}

#[test]
fn condition_wait_releases_and_reacquires_its_associated_guard() {
    let source = r#"
class SignalTask implements Task<Unit> {
    private Mutex<int> mutex;
    private Condition<int> condition;
    public SignalTask(Mutex<int> mutex, Condition<int> condition) {
        this.mutex = mutex;
        this.condition = condition;
    }
    public Unit run() {
        Guard<int> guard;
        match (this.mutex.lock()) {
            Ok(value) { guard = value; }
            Err(error) { error.print(); return (); }
        }
        match (guard.write(1)) {
            Ok(unit) {}
            Err(error) { error.print(); }
        }
        match (this.condition.signal()) {
            Ok(unit) {}
            Err(error) { error.print(); }
        }
        drop(guard);
        return ();
    }
}

Result<Unit, Text> exercise() {
    Mutex<int> mutex = Mutex<int>.create(0)?;
    Condition<int> condition = Condition<int>.create(mutex)?;
    Mutex<int> unrelated = Mutex<int>.create(0)?;
    Guard<int> unrelatedGuard = unrelated.lock()?;
    boolean rejected = false;
    match (condition.wait(unrelatedGuard, 0L)) {
        Ok(value) { println("unexpected unrelated guard"); }
        Err(error) { rejected = true; drop(error); }
    }
    drop(unrelatedGuard);
    Mutex<int> producerMutex = mutex.cloneHandle()?;
    Mutex<int> waiterMutex = mutex.cloneHandle()?;
    Condition<int> producerCondition = condition.cloneHandle()?;
    Condition<int> waiterCondition = condition.cloneHandle()?;
    drop(condition);
    drop(mutex);
    Guard<int> guard = waiterMutex.lock()?;
    Thread<Unit> worker = Thread<Unit>.spawn(new SignalTask(producerMutex, producerCondition))?;
    while (guard.read() == 0) {
        boolean notified = waiterCondition.wait(guard, 1000L)?;
        if (!notified) { return new Result<Unit, Text>.Err(new Text("condition timed out")); }
    }
    boolean observed = guard.read() == 1;
    drop(guard);
    worker.join()?;
    drop(waiterMutex);
    boolean conditionRetainedControl = false;
    match (waiterCondition.signal()) {
        Ok(unit) { conditionRetainedControl = true; }
        Err(error) { drop(error); }
    }
    drop(waiterCondition);
    println(rejected && observed && conditionRetainedControl);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "true\n");
}

#[test]
fn futures_transfer_results_once_and_cancellation_is_cooperative() {
    let source = r#"
class GateTask implements CancelableTask<int> {
    private CountDownLatch gate;
    public GateTask(CountDownLatch gate) { this.gate = gate; }
    public Result<int, Text> run(borrow CancellationToken token) {
        boolean opened = this.gate.wait(2000L)?;
        if (!opened) { return new Result<int, Text>.Err(new Text("gate timed out")); }
        return new Result<int, Text>.Ok(42);
    }
}

class CooperativeTask implements CancelableTask<int> {
    private CountDownLatch gate;
    public CooperativeTask(CountDownLatch gate) { this.gate = gate; }
    public Result<int, Text> run(borrow CancellationToken token) {
        boolean opened = this.gate.wait(2000L)?;
        if (!opened) { return new Result<int, Text>.Err(new Text("gate timed out")); }
        boolean cancelled = token.isCancelled()?;
        if (cancelled) { return new Result<int, Text>.Ok(1); }
        return new Result<int, Text>.Ok(0);
    }
}

class DropObservedTask implements CancelableTask<int> {
    private CountDownLatch started;
    private AtomicInt finished;
    public DropObservedTask(CountDownLatch started, AtomicInt finished) {
        this.started = started;
        this.finished = finished;
    }
    public Result<int, Text> run(borrow CancellationToken token) {
        this.started.countDown()?;
        boolean cancelled = token.isCancelled()?;
        while (!cancelled) { cancelled = token.isCancelled()?; }
        this.finished.fetchAdd(1);
        return new Result<int, Text>.Ok(99);
    }
}

Result<Unit, Text> exercise() {
    CancellationToken explicitToken = CancellationToken.create()?;
    CancellationToken tokenAlias = explicitToken.cloneHandle()?;
    boolean tokenInitiallyCancelled = tokenAlias.isCancelled()?;
    explicitToken.request()?;
    boolean tokenRequestShared = tokenAlias.isCancelled()?;
    drop(tokenAlias);
    drop(explicitToken);

    CountDownLatch gate = CountDownLatch.create(1L)?;
    CountDownLatch workerGate = gate.cloneHandle()?;
    Future<int> future = Future<int>.spawn(new GateTask(workerGate))?;
    boolean unfinishedAtStart = !future.isDone();
    Option<int> poll = future.await(0L)?;
    boolean pollTimedOut = false;
    match (poll) {
        None { pollTimedOut = true; }
        Some(value) { return new Result<Unit, Text>.Err(new Text("future completed before gate release")); }
    }
    gate.countDown()?;
    Option<int> result = future.await(1000L)?;
    int resultValue = -1;
    match (result) {
        Some(value) { resultValue = value; }
        None { return new Result<Unit, Text>.Err(new Text("future did not complete")); }
    }
    boolean repeatRejected = false;
    match (future.await(1000L)) {
        Ok(value) {}
        Err(error) { repeatRejected = true; drop(error); }
    }
    boolean futureDone = future.isDone();
    drop(future);

    CountDownLatch dropStarted = CountDownLatch.create(1L)?;
    CountDownLatch workerDropStarted = dropStarted.cloneHandle()?;
    AtomicInt finished = AtomicInt.create(0)?;
    AtomicInt workerFinished = finished.cloneHandle()?;
    Future<int> dropped = Future<int>.spawn(new DropObservedTask(workerDropStarted, workerFinished))?;
    boolean dropTaskStarted = dropStarted.wait(1000L)?;
    drop(dropped);
    boolean dropJoinedWorker = finished.load() == 1;
    drop(finished);

    CountDownLatch cancelGate = CountDownLatch.create(1L)?;
    CountDownLatch workerCancelGate = cancelGate.cloneHandle()?;
    Future<int> cancelled = Future<int>.spawn(new CooperativeTask(workerCancelGate))?;
    cancelled.cancel()?;
    cancelGate.countDown()?;
    boolean cancellationObserved = false;
    match (cancelled.await(1000L)) {
        Ok(value) {
            match (value) {
                Some(outcome) { cancellationObserved = outcome == 1; }
                None {}
            }
        }
        Err(error) { drop(error); }
    }
    drop(cancelled);
    println(!tokenInitiallyCancelled && tokenRequestShared && unfinishedAtStart &&
        pollTimedOut && resultValue == 42 && repeatRejected && futureDone &&
        dropTaskStarted && dropJoinedWorker && cancellationObserved);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "true\n");
}

#[test]
fn failed_future_synchronization_setup_drops_the_consumed_task() {
    let source = r#"
class PayloadTask implements CancelableTask<int> {
    private Text payload;
    public PayloadTask(Text payload) { this.payload = payload; }
    public Result<int, Text> run(borrow CancellationToken token) {
        return new Result<int, Text>.Ok(this.payload.byteLength());
    }
}

void main() {
    boolean rejected = false;
    match (Future<int>.spawn(new PayloadTask(new Text("owned")))) {
        Ok(future) { drop(future); }
        Err(error) { rejected = true; drop(error); }
    }
    println(rejected);
}
"#;
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("concurrency_alpha.ncr", source)));
    let c = fail_future_condition_initialization(&c);
    let output = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"));
    assert_clean(&output, "true\n");
}

#[test]
fn partial_pool_startup_synchronizes_worker_shutdown() {
    let source = r#"
void main() {
    boolean rejected = false;
    match (ThreadPool<int>.create(2, 1L)) {
        Ok(pool) { drop(pool); }
        Err(error) { rejected = true; drop(error); }
    }
    println(rejected);
}
"#;
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("concurrency_alpha.ncr", source)));
    let c = fail_second_pool_worker_creation(&c);
    let output = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"));
    assert_clean(&output, "true\n");
}

#[test]
fn thread_pools_recover_rejected_tasks_cancel_queued_work_and_drain_delays() {
    let source = r#"
class ImmediateCancelableTask implements CancelableTask<int> {
    private int value;
    public ImmediateCancelableTask(int value) { this.value = value; }
    public Result<int, Text> run(borrow CancellationToken token) {
        return new Result<int, Text>.Ok(this.value);
    }
}

class QueueGateTask implements CancelableTask<int> {
    private CountDownLatch started;
    private CountDownLatch release;
    public QueueGateTask(CountDownLatch started, CountDownLatch release) {
        this.started = started;
        this.release = release;
    }
    public Result<int, Text> run(borrow CancellationToken token) {
        this.started.countDown()?;
        boolean opened = this.release.wait(5000L)?;
        if (!opened) { return new Result<int, Text>.Err(new Text("pool gate timed out")); }
        return new Result<int, Text>.Ok(1);
    }
}

Result<Future<int>, Text> requiredSubmit(borrow mut ThreadPool<int> pool, CancelableTask<int> task) {
    match (pool.submit(task)) {
        Ok(future) { return new Result<Future<int>, Text>.Ok(future); }
        Err(error) {
            match (error) {
                Closed(original) { drop(original); return new Result<Future<int>, Text>.Err(new Text("unexpected closed pool")); }
                Full(original) { drop(original); return new Result<Future<int>, Text>.Err(new Text("unexpected full pool")); }
                System(message, original) { drop(original); return new Result<Future<int>, Text>.Err(message); }
            }
        }
    }
}

Result<Unit, Text> exercise() {
    boolean zeroWorkers = false;
    match (ThreadPool<int>.create(0, 1L)) {
        Ok(value) { drop(value); }
        Err(error) { zeroWorkers = true; drop(error); }
    }
    boolean zeroCapacity = false;
    match (ThreadPool<int>.create(1, 0L)) {
        Ok(value) { drop(value); }
        Err(error) { zeroCapacity = true; drop(error); }
    }
    if (zeroWorkers && zeroCapacity) { println("invalid pool bounds rejected"); }

    ThreadPool<int> pool = ThreadPool<int>.create(1, 2L)?;
    CountDownLatch started = CountDownLatch.create(1L)?;
    CountDownLatch release = CountDownLatch.create(1L)?;
    CountDownLatch workerStarted = started.cloneHandle()?;
    CountDownLatch workerRelease = release.cloneHandle()?;
    Future<int> running = requiredSubmit(pool, new QueueGateTask(workerStarted, workerRelease))?;
    if (!started.wait(1000L)?) { return new Result<Unit, Text>.Err(new Text("pool worker did not start")); }
    Future<int> cancelled = requiredSubmit(pool, new ImmediateCancelableTask(2))?;
    Future<int> queued = requiredSubmit(pool, new ImmediateCancelableTask(3))?;

    match (pool.submit(new ImmediateCancelableTask(4))) {
        Ok(future) { drop(future); println("unexpected pool capacity"); }
        Err(error) {
            match (error) {
                Full(original) { println("pool full preserved task"); drop(original); }
                Closed(original) { drop(original); return new Result<Unit, Text>.Err(new Text("pool closed before full")); }
                System(message, original) { drop(original); return new Result<Unit, Text>.Err(message); }
            }
        }
    }
    match (pool.schedule(new ImmediateCancelableTask(5), -1L)) {
        Ok(future) { drop(future); println("unexpected negative schedule"); }
        Err(error) {
            match (error) {
                System(message, original) { println("negative delay preserved task"); drop(message); drop(original); }
                Closed(original) { drop(original); return new Result<Unit, Text>.Err(new Text("negative schedule reported closed")); }
                Full(original) { drop(original); return new Result<Unit, Text>.Err(new Text("negative schedule reported full")); }
            }
        }
    }

    cancelled.cancel()?;
    release.countDown()?;
    pool.shutdown(true)?;
    match (running.await(0L)?) {
        Some(value) { println(value); }
        None { return new Result<Unit, Text>.Err(new Text("running pool task did not finish")); }
    }
    match (cancelled.await(1000L)) {
        Ok(value) { println("unexpected cancelled task success"); }
        Err(error) { println("queued task cancelled"); drop(error); }
    }
    match (queued.await(1000L)?) {
        Some(value) { println(value); }
        None { return new Result<Unit, Text>.Err(new Text("drained task did not finish")); }
    }
    match (pool.submit(new ImmediateCancelableTask(6))) {
        Ok(future) { drop(future); println("unexpected submit after shutdown"); }
        Err(error) {
            match (error) {
                Closed(original) { println("closed pool preserved task"); drop(original); }
                Full(original) { drop(original); return new Result<Unit, Text>.Err(new Text("shutdown reported full")); }
                System(message, original) { drop(original); return new Result<Unit, Text>.Err(message); }
            }
        }
    }
    drop(running);
    drop(cancelled);
    drop(queued);
    drop(pool);

    ThreadPool<int> delayedPool = ThreadPool<int>.create(1, 1L)?;
    Future<int> delayed;
    match (delayedPool.schedule(new ImmediateCancelableTask(9), 50L)) {
        Ok(value) { delayed = value; }
        Err(error) {
            match (error) {
                Closed(original) { drop(original); return new Result<Unit, Text>.Err(new Text("new pool unexpectedly closed")); }
                Full(original) { drop(original); return new Result<Unit, Text>.Err(new Text("new pool unexpectedly full")); }
                System(message, original) { drop(original); return new Result<Unit, Text>.Err(message); }
            }
        }
    }
    match (delayed.await(1000L)?) {
        Some(value) { println(value); }
        None { return new Result<Unit, Text>.Err(new Text("delayed pool task did not run")); }
    }
    delayedPool.shutdown(true)?;
    drop(delayed);
    drop(delayedPool);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(
        &output,
        "invalid pool bounds rejected\npool full preserved task\nnegative delay preserved task\n1\nqueued task cancelled\n3\nclosed pool preserved task\n9\n",
    );
}

#[test]
fn future_and_pool_result_payloads_must_be_sendable() {
    let pool_source = "void main() { ThreadPool<Guard<int>>.create(1, 1L); }";
    assert!(validate(pool_source).is_err());

    let future_source = r#"
class GuardResultTask implements CancelableTask<Guard<int>> {
    public Result<Guard<int>, Text> run(borrow CancellationToken token) {
        return new Result<Guard<int>, Text>.Err(new Text("no guard result"));
    }
}
void main() { Future<Guard<int>>.spawn(new GuardResultTask()); }
"#;
    assert!(validate(future_source).is_err());
}


#[test]
fn pool_cancel_shutdown_completes_queued_futures_before_joining_workers() {
    let source = r#"
class BlockUntilReleased implements CancelableTask<int> {
    private CountDownLatch started;
    private CountDownLatch release;
    public BlockUntilReleased(CountDownLatch started, CountDownLatch release) {
        this.started = started;
        this.release = release;
    }
    public Result<int, Text> run(borrow CancellationToken token) {
        this.started.countDown()?;
        boolean opened = this.release.wait(5000L)?;
        if (!opened) { return new Result<int, Text>.Err(new Text("shutdown gate timed out")); }
        return new Result<int, Text>.Ok(1);
    }
}

class NeverStarted implements CancelableTask<int> {
    public Result<int, Text> run(borrow CancellationToken token) {
        return new Result<int, Text>.Ok(2);
    }
}

class ObserveShutdownFuture implements Task<Unit> {
    private Future<int> queued;
    private CountDownLatch release;
    public ObserveShutdownFuture(Future<int> queued, CountDownLatch release) {
        this.queued = queued;
        this.release = release;
    }
    public Unit run() {
        match (this.queued.await(3000L)) {
            Ok(value) {
                match (value) {
                    None { println("shutdown cancellation timed out"); }
                    Some(result) { println("queued task ran during cancel shutdown"); }
                }
            }
            Err(error) { println("shutdown canceled queued future"); drop(error); }
        }
        match (this.release.countDown()) {
            Ok(unit) {}
            Err(error) { error.print(); }
        }
        return ();
    }
}

Result<Future<int>, Text> requiredSubmit(borrow mut ThreadPool<int> pool, CancelableTask<int> task) {
    match (pool.submit(task)) {
        Ok(future) { return new Result<Future<int>, Text>.Ok(future); }
        Err(error) {
            match (error) {
                Closed(original) { drop(original); return new Result<Future<int>, Text>.Err(new Text("unexpected closed pool")); }
                Full(original) { drop(original); return new Result<Future<int>, Text>.Err(new Text("unexpected full pool")); }
                System(message, original) { drop(original); return new Result<Future<int>, Text>.Err(message); }
            }
        }
    }
}

Result<Unit, Text> exercise() {
    ThreadPool<int> pool = ThreadPool<int>.create(1, 1L)?;
    CountDownLatch started = CountDownLatch.create(1L)?;
    CountDownLatch release = CountDownLatch.create(1L)?;
    CountDownLatch workerStarted = started.cloneHandle()?;
    CountDownLatch workerRelease = release.cloneHandle()?;
    Future<int> running = requiredSubmit(pool, new BlockUntilReleased(workerStarted, workerRelease))?;
    if (!started.wait(1000L)?) { return new Result<Unit, Text>.Err(new Text("pool worker did not start")); }
    Future<int> queued = requiredSubmit(pool, new NeverStarted())?;
    CountDownLatch observerRelease = release.cloneHandle()?;
    Thread<Unit> observer = Thread<Unit>.spawn(new ObserveShutdownFuture(queued, observerRelease))?;
    pool.shutdown(false)?;
    observer.join()?;
    match (running.await(0L)?) {
        Some(value) { println(value); }
        None { return new Result<Unit, Text>.Err(new Text("running shutdown task did not finish")); }
    }
    drop(running);
    drop(pool);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "shutdown canceled queued future\n1\n");
}

#[test]
fn recurring_pool_cadence_cancellation_shutdown_and_cleanup() {
    let source = r#"
class CadenceTask implements CancelableTask<int> {
    private AtomicInt runs;
    private AtomicInt active;
    private AtomicInt overlaps;
    private CountDownLatch started;
    private CountDownLatch release;
    private CountDownLatch secondStarted;
    public CadenceTask(AtomicInt runs, AtomicInt active, AtomicInt overlaps,
        CountDownLatch started, CountDownLatch release, CountDownLatch secondStarted) {
        this.runs = runs;
        this.active = active;
        this.overlaps = overlaps;
        this.started = started;
        this.release = release;
        this.secondStarted = secondStarted;
    }
    public Result<int, Text> run(borrow CancellationToken token) {
        int previous = this.active.fetchAdd(1);
        if (previous != 0) { this.overlaps.fetchAdd(1); }
        int index = this.runs.fetchAdd(1);
        if (index == 0) {
            this.started.countDown()?;
            boolean opened = this.release.wait(5000L)?;
            this.active.fetchSubtract(1);
            if (!opened) { return new Result<int, Text>.Err(new Text("cadence gate timed out")); }
            return new Result<int, Text>.Ok(index);
        }
        this.secondStarted.countDown()?;
        this.active.fetchSubtract(1);
        return new Result<int, Text>.Err(new Text("cadence complete"));
    }
}

class OwnedRecurringTextTask implements CancelableTask<Text> {
    private AtomicInt runs;
    private CountDownLatch started;
    private Text owned;
    public OwnedRecurringTextTask(AtomicInt runs, CountDownLatch started, Text owned) {
        this.runs = runs;
        this.started = started;
        this.owned = owned;
    }
    public Result<Text, Text> run(borrow CancellationToken token) {
        int index = this.runs.fetchAdd(1);
        if (index == 0) { this.started.countDown()?; }
        if (index < 2) { return new Result<Text, Text>.Ok(new Text("discarded recurring result")); }
        boolean cancelled = false;
        while (!cancelled) {
            cancelled = token.isCancelled()?;
            if (!cancelled) { Time.sleepMillis(1L)?; }
        }
        return new Result<Text, Text>.Ok(new Text("cancelled invocation result"));
    }
}

class ImmediateTextTask implements CancelableTask<Text> {
    public Result<Text, Text> run(borrow CancellationToken token) {
        return new Result<Text, Text>.Ok(new Text("unexpected task run"));
    }
}

class CountTask implements CancelableTask<int> {
    private AtomicInt runs;
    public CountTask(AtomicInt runs) { this.runs = runs; }
    public Result<int, Text> run(borrow CancellationToken token) {
        this.runs.fetchAdd(1);
        return new Result<int, Text>.Ok(1);
    }
}

Result<Future<int>, Text> scheduleRate(borrow mut ThreadPool<int> pool, CancelableTask<int> task, long initialDelay, long interval) {
    match (pool.scheduleAtFixedRate(task, initialDelay, interval)) {
        Ok(future) { return new Result<Future<int>, Text>.Ok(future); }
        Err(error) {
            match (error) {
                Closed(original) { drop(original); return new Result<Future<int>, Text>.Err(new Text("rate pool closed")); }
                Full(original) { drop(original); return new Result<Future<int>, Text>.Err(new Text("rate pool full")); }
                System(message, original) { drop(original); return new Result<Future<int>, Text>.Err(message); }
            }
        }
    }
}

Result<Future<int>, Text> scheduleDelay(borrow mut ThreadPool<int> pool, CancelableTask<int> task, long initialDelay, long interval) {
    match (pool.scheduleWithFixedDelay(task, initialDelay, interval)) {
        Ok(future) { return new Result<Future<int>, Text>.Ok(future); }
        Err(error) {
            match (error) {
                Closed(original) { drop(original); return new Result<Future<int>, Text>.Err(new Text("delay pool closed")); }
                Full(original) { drop(original); return new Result<Future<int>, Text>.Err(new Text("delay pool full")); }
                System(message, original) { drop(original); return new Result<Future<int>, Text>.Err(message); }
            }
        }
    }
}

Result<Unit, Text> exercise() {
    ThreadPool<int> cadencePool = ThreadPool<int>.create(2, 1L)?;
    AtomicInt rateRuns = AtomicInt.create(0)?;
    AtomicInt rateActive = AtomicInt.create(0)?;
    AtomicInt rateOverlaps = AtomicInt.create(0)?;
    CountDownLatch rateStarted = CountDownLatch.create(1L)?;
    CountDownLatch rateRelease = CountDownLatch.create(1L)?;
    CountDownLatch rateSecond = CountDownLatch.create(1L)?;
    Future<int> rate = scheduleRate(cadencePool,
        new CadenceTask(rateRuns.cloneHandle()?, rateActive.cloneHandle()?, rateOverlaps.cloneHandle()?,
            rateStarted.cloneHandle()?, rateRelease.cloneHandle()?, rateSecond.cloneHandle()?), 500L, 200L)?;
    if (rateStarted.wait(0L)?) { return new Result<Unit, Text>.Err(new Text("fixed-rate task ignored initial delay")); }
    if (!rateStarted.wait(1000L)?) { return new Result<Unit, Text>.Err(new Text("fixed-rate task did not start")); }
    Time.sleepMillis(450L)?;
    rateRelease.countDown()?;
    if (!rateSecond.wait(100L)?) { return new Result<Unit, Text>.Err(new Text("fixed-rate task did not catch up")); }
    match (rate.await(1000L)) {
        Err(error) {
            boolean expected = error.equalsString("cadence complete");
            drop(error);
            if (!expected) { return new Result<Unit, Text>.Err(new Text("fixed-rate task error was lost")); }
            println("fixed rate catches up");
        }
        Ok(value) { drop(value); return new Result<Unit, Text>.Err(new Text("fixed-rate future succeeded")); }
    }
    if (rateOverlaps.load() != 0) { return new Result<Unit, Text>.Err(new Text("fixed-rate task overlapped itself")); }

    AtomicInt delayRuns = AtomicInt.create(0)?;
    AtomicInt delayActive = AtomicInt.create(0)?;
    AtomicInt delayOverlaps = AtomicInt.create(0)?;
    CountDownLatch delayStarted = CountDownLatch.create(1L)?;
    CountDownLatch delayRelease = CountDownLatch.create(1L)?;
    CountDownLatch delaySecond = CountDownLatch.create(1L)?;
    Future<int> delayed = scheduleDelay(cadencePool,
        new CadenceTask(delayRuns.cloneHandle()?, delayActive.cloneHandle()?, delayOverlaps.cloneHandle()?,
            delayStarted.cloneHandle()?, delayRelease.cloneHandle()?, delaySecond.cloneHandle()?), 500L, 200L)?;
    if (delayStarted.wait(0L)?) { return new Result<Unit, Text>.Err(new Text("fixed-delay task ignored initial delay")); }
    if (!delayStarted.wait(1000L)?) { return new Result<Unit, Text>.Err(new Text("fixed-delay task did not start")); }
    Time.sleepMillis(450L)?;
    delayRelease.countDown()?;
    if (delaySecond.wait(100L)?) { return new Result<Unit, Text>.Err(new Text("fixed-delay task skipped its interval")); }
    match (delayed.await(1000L)) {
        Err(error) {
            boolean expected = error.equalsString("cadence complete");
            drop(error);
            if (!expected) { return new Result<Unit, Text>.Err(new Text("fixed-delay task error was lost")); }
            println("fixed delay waits");
        }
        Ok(value) { drop(value); return new Result<Unit, Text>.Err(new Text("fixed-delay future succeeded")); }
    }
    if (delayOverlaps.load() != 0) { return new Result<Unit, Text>.Err(new Text("fixed-delay task overlapped itself")); }
    cadencePool.shutdown(true)?;
    drop(rate);
    drop(delayed);
    drop(cadencePool);

    ThreadPool<Text> cancelPool = ThreadPool<Text>.create(1, 1L)?;
    AtomicInt completedRuns = AtomicInt.create(0)?;
    CountDownLatch cancelStarted = CountDownLatch.create(1L)?;
    Future<Text> recurring;
    match (cancelPool.scheduleWithFixedDelay(
        new OwnedRecurringTextTask(completedRuns.cloneHandle()?, cancelStarted.cloneHandle()?,
            new Text("task-owned payload")), 0L, 1L)) {
        Ok(future) { recurring = future; }
        Err(error) {
            match (error) {
                Closed(original) { drop(original); return new Result<Unit, Text>.Err(new Text("cancel pool closed")); }
                Full(original) { drop(original); return new Result<Unit, Text>.Err(new Text("cancel pool full")); }
                System(message, original) { drop(original); return new Result<Unit, Text>.Err(message); }
            }
        }
    }
    if (!cancelStarted.wait(1000L)?) { return new Result<Unit, Text>.Err(new Text("recurring task did not start")); }
    int polls = 0;
    while (completedRuns.load() < 3 && polls < 5000) {
        Time.sleepMillis(1L)?;
        polls = polls + 1;
    }
    if (completedRuns.load() < 3) { return new Result<Unit, Text>.Err(new Text("recurring task did not repeat")); }
    match (recurring.await(0L)?) {
        None {}
        Some(value) { drop(value); return new Result<Unit, Text>.Err(new Text("recurring future exposed a run value")); }
    }
    match (cancelPool.submit(new ImmediateTextTask())) {
        Ok(extra) { drop(extra); return new Result<Unit, Text>.Err(new Text("active recurrence lost its queue reservation")); }
        Err(error) {
            match (error) {
                Full(original) { println("active recurrence reserves queue capacity"); drop(original); }
                Closed(original) { drop(original); return new Result<Unit, Text>.Err(new Text("active recurrence pool closed")); }
                System(message, original) { drop(original); return new Result<Unit, Text>.Err(message); }
            }
        }
    }
    match (cancelPool.scheduleAtFixedRate(new ImmediateTextTask(), 0L, 0L)) {
        Ok(future) { drop(future); return new Result<Unit, Text>.Err(new Text("zero recurrence interval accepted")); }
        Err(error) {
            match (error) {
                System(message, original) {
                    println("nonpositive recurring interval rejected");
                    drop(message);
                    drop(original);
                }
                Closed(original) { drop(original); return new Result<Unit, Text>.Err(new Text("invalid interval reported closed")); }
                Full(original) { drop(original); return new Result<Unit, Text>.Err(new Text("invalid interval reported full")); }
            }
        }
    }
    match (cancelPool.scheduleWithFixedDelay(new ImmediateTextTask(), 0L, 9223372036855L)) {
        Ok(future) { drop(future); return new Result<Unit, Text>.Err(new Text("overflowing recurring interval accepted")); }
        Err(error) {
            match (error) {
                System(message, original) {
                    println("recurring interval overflow rejected");
                    drop(message);
                    drop(original);
                }
                Closed(original) { drop(original); return new Result<Unit, Text>.Err(new Text("overflow reported closed")); }
                Full(original) { drop(original); return new Result<Unit, Text>.Err(new Text("overflow reported full")); }
            }
        }
    }
    recurring.cancel()?;
    match (recurring.await(5000L)) {
        Err(error) {
            boolean cancelled = error.equalsString("recurring task cancelled");
            drop(error);
            if (!cancelled) { return new Result<Unit, Text>.Err(new Text("wrong recurrence cancellation error")); }
            println("recurring task cancellation observed");
        }
        Ok(value) { drop(value); return new Result<Unit, Text>.Err(new Text("cancelled recurrence succeeded")); }
    }
    cancelPool.shutdown(false)?;
    drop(recurring);
    drop(cancelPool);

    ThreadPool<int> drainPool = ThreadPool<int>.create(1, 1L)?;
    AtomicInt drainRuns = AtomicInt.create(0)?;
    Future<int> draining = scheduleRate(drainPool, new CountTask(drainRuns.cloneHandle()?), 60000L, 200L)?;
    drainPool.shutdown(true)?;
    match (draining.await(1000L)) {
        Err(error) {
            boolean cancelled = error.equalsString("recurring task cancelled");
            drop(error);
            if (!cancelled) { return new Result<Unit, Text>.Err(new Text("wrong drained recurrence error")); }
        }
        Ok(value) { drop(value); return new Result<Unit, Text>.Err(new Text("drained recurrence succeeded")); }
    }
    if (drainRuns.load() != 0) { return new Result<Unit, Text>.Err(new Text("drain ran a queued recurrence")); }
    println("draining cancels queued recurrence");
    drop(draining);
    drop(drainPool);
    ThreadPool<int> immediatePool = ThreadPool<int>.create(1, 1L)?;
    AtomicInt immediateRuns = AtomicInt.create(0)?;
    Future<int> immediate = scheduleDelay(immediatePool, new CountTask(immediateRuns.cloneHandle()?), 60000L, 200L)?;
    immediatePool.shutdown(false)?;
    match (immediate.await(1000L)) {
        Err(error) {
            boolean cancelled = error.equalsString("recurring task cancelled");
            drop(error);
            if (!cancelled) { return new Result<Unit, Text>.Err(new Text("wrong immediate recurrence error")); }
        }
        Ok(value) { drop(value); return new Result<Unit, Text>.Err(new Text("immediate shutdown recurrence succeeded")); }
    }
    if (immediateRuns.load() != 0) { return new Result<Unit, Text>.Err(new Text("immediate shutdown ran a queued recurrence")); }
    println("immediate shutdown cancels queued recurrence");
    drop(immediate);
    drop(immediatePool);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output,
        "fixed rate catches up\nfixed delay waits\nactive recurrence reserves queue capacity\nnonpositive recurring interval rejected\nrecurring interval overflow rejected\nrecurring task cancellation observed\ndraining cancels queued recurrence\nimmediate shutdown cancels queued recurrence\n");
}

#[test]
fn fixed_delay_is_anchored_before_synchronous_owned_result_disposal() {
    let source = r#"
class BlockingResult implements CancelableTask<Unit> {
    private CountDownLatch started;
    private CountDownLatch release;
    public BlockingResult(CountDownLatch started, CountDownLatch release) {
        this.started = started; this.release = release;
    }
    public Result<Unit, Text> run(borrow CancellationToken token) {
        this.started.countDown()?;
        this.release.wait(5000L)?;
        return new Result<Unit, Text>.Ok(());
    }
}
class ImmediateResult implements CancelableTask<Unit> {
    public Result<Unit, Text> run(borrow CancellationToken token) {
        return new Result<Unit, Text>.Ok(());
    }
}
class RecurringResults implements CancelableTask<Future<Unit>> {
    private CountDownLatch started;
    private CountDownLatch release;
    private CountDownLatch second;
    private AtomicInt runs;
    public RecurringResults(CountDownLatch started, CountDownLatch release,
                           CountDownLatch second, AtomicInt runs) {
        this.started = started; this.release = release;
        this.second = second; this.runs = runs;
    }
    public Result<Future<Unit>, Text> run(borrow CancellationToken token) {
        int previous = this.runs.fetchAdd(1);
        Future<Unit> result;
        if (previous == 0) {
            result = Future<Unit>.spawn(new BlockingResult(
                this.started.cloneHandle()?, this.release.cloneHandle()?))?;
        } else {
            this.second.countDown()?;
            result = Future<Unit>.spawn(new ImmediateResult())?;
        }
        return new Result<Future<Unit>, Text>.Ok(result);
    }
}
Result<Unit, Text> exercise() {
    CountDownLatch started = CountDownLatch.create(1L)?;
    CountDownLatch release = CountDownLatch.create(1L)?;
    CountDownLatch second = CountDownLatch.create(1L)?;
    AtomicInt runs = AtomicInt.create(0)?;
    ThreadPool<Future<Unit>> pool = ThreadPool<Future<Unit>>.create(1, 1L)?;
    Future<Future<Unit>> scheduled;
    match (pool.scheduleWithFixedDelay(new RecurringResults(
            started.cloneHandle()?, release.cloneHandle()?,
            second.cloneHandle()?, runs.cloneHandle()?), 0L, 1000L)) {
        Ok(future) { scheduled = future; }
        Err(error) {
            match (error) {
                Closed(task) { return new Result<Unit, Text>.Err(new Text("closed")); }
                Full(task) { return new Result<Unit, Text>.Err(new Text("full")); }
                System(message, task) { return new Result<Unit, Text>.Err(message); }
            }
        }
    }
    if (!started.wait(5000L)?) {
        return new Result<Unit, Text>.Err(new Text("result worker did not start"));
    }
    Time.sleepMillis(1200L)?;
    release.countDown()?;
    println(second.wait(500L)?);
    scheduled.cancel()?;
    pool.shutdown(false)?;
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "true\n");
}
#[test]
fn unbounded_synchronizer_waits_block_then_wake_on_real_transitions() {
    let source = r#"
class SemaphoreWaitTask implements Task<boolean> {
    private Semaphore semaphore;
    public SemaphoreWaitTask(Semaphore semaphore) { this.semaphore = semaphore; }
    public boolean run() {
        match (this.semaphore.acquire(-1L)) {
            Ok(value) { return value; }
            Err(error) { drop(error); return false; }
        }
    }
}

class LatchWaitTask implements Task<boolean> {
    private CountDownLatch latch;
    public LatchWaitTask(CountDownLatch latch) { this.latch = latch; }
    public boolean run() {
        match (this.latch.wait(-1L)) {
            Ok(value) { return value; }
            Err(error) { drop(error); return false; }
        }
    }
}

class BarrierWaitTask implements Task<int> {
    private Barrier barrier;
    public BarrierWaitTask(Barrier barrier) { this.barrier = barrier; }
    public int run() {
        match (this.barrier.wait(-1L)) {
            Ok(index) { return index; }
            Err(error) { drop(error); return -1; }
        }
    }
}

class ConditionWaitTask implements Task<boolean> {
    private Mutex<int> mutex;
    private Condition<int> condition;
    public ConditionWaitTask(Mutex<int> mutex, Condition<int> condition) {
        this.mutex = mutex;
        this.condition = condition;
    }
    public boolean run() {
        Guard<int> guard;
        match (this.mutex.lock()) {
            Ok(value) { guard = value; }
            Err(error) { drop(error); return false; }
        }
        while (guard.read() == 0) {
            boolean notified = false;
            match (this.condition.wait(guard, -1L)) {
                Ok(value) { notified = value; }
                Err(error) { drop(error); drop(guard); return false; }
            }
            if (!notified) { drop(guard); return false; }
        }
        boolean observed = guard.read() == 7;
        drop(guard);
        return observed;
    }
}

class ReadLineTask implements CancelableTask<int> {
    public Result<int, Text> run(borrow CancellationToken token) {
        Option<Text> line = Console.readLine()?;
        drop(line);
        return new Result<int, Text>.Ok(42);
    }
}

Result<Unit, Text> awaitHarnessRelease() {
    Option<Text> line = Console.readLine()?;
    drop(line);
    return new Result<Unit, Text>.Ok(());
}

Result<Unit, Text> exercise() {
    Semaphore semaphore = Semaphore.create(0L)?;
    Semaphore semaphoreHandle = semaphore.cloneHandle()?;
    Thread<boolean> semaphoreWaiter =
        Thread<boolean>.spawn(new SemaphoreWaitTask(semaphoreHandle))?;
    awaitHarnessRelease()?;
    semaphore.release(1L)?;
    boolean acquired = false;
    match (semaphoreWaiter.join()) {
        Ok(value) { acquired = value; }
        Err(error) { drop(error); }
    }
    drop(semaphoreWaiter);
    drop(semaphore);

    CountDownLatch latch = CountDownLatch.create(1L)?;
    CountDownLatch latchHandle = latch.cloneHandle()?;
    Thread<boolean> latchWaiter = Thread<boolean>.spawn(new LatchWaitTask(latchHandle))?;
    awaitHarnessRelease()?;
    latch.countDown()?;
    boolean countedDown = false;
    match (latchWaiter.join()) {
        Ok(value) { countedDown = value; }
        Err(error) { drop(error); }
    }
    drop(latchWaiter);
    drop(latch);

    Barrier barrier = Barrier.create(2)?;
    Barrier barrierHandle = barrier.cloneHandle()?;
    Thread<int> barrierWaiter = Thread<int>.spawn(new BarrierWaitTask(barrierHandle))?;
    awaitHarnessRelease()?;
    int mainIndex = -1;
    match (barrier.wait(-1L)) {
        Ok(index) { mainIndex = index; }
        Err(error) { drop(error); }
    }
    int workerIndex = -1;
    match (barrierWaiter.join()) {
        Ok(index) { workerIndex = index; }
        Err(error) { drop(error); }
    }
    drop(barrierWaiter);
    drop(barrier);

    Mutex<int> mutex = Mutex<int>.create(0)?;
    Condition<int> condition = Condition<int>.create(mutex)?;
    Mutex<int> waiterMutex = mutex.cloneHandle()?;
    Condition<int> waiterCondition = condition.cloneHandle()?;
    Thread<boolean> conditionWaiter = Thread<boolean>.spawn(
        new ConditionWaitTask(waiterMutex, waiterCondition))?;
    awaitHarnessRelease()?;
    Guard<int> guard = mutex.lock()?;
    guard.write(7)?;
    condition.signal()?;
    drop(guard);
    boolean observed = false;
    match (conditionWaiter.join()) {
        Ok(value) { observed = value; }
        Err(error) { drop(error); }
    }
    drop(conditionWaiter);
    drop(condition);
    drop(mutex);

    Future<int> future = Future<int>.spawn(new ReadLineTask())?;
    Option<int> result = future.await(-1L)?;
    int value = -1;
    match (result) {
        Some(resultValue) { value = resultValue; }
        None {}
    }
    drop(future);

    if (!acquired || !countedDown || mainIndex < 0 || workerIndex < 0 ||
            mainIndex == workerIndex || !observed || value != 42) {
        return new Result<Unit, Text>.Err(new Text("unbounded wait transition failed"));
    }
    println("unbounded synchronizer waits passed");
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("concurrency_alpha.ncr", source)));
    let c = observe_unbounded_sync_waits(&c);
    let program = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"));
    let mut process = NativeWaitProcess::start(&program);
    for index in 0..5 {
        process.wait_for_entry(&format!("NCR_TEST_SYNC_WAIT:{index}"));
        process.release();
    }
    let output = process.finish_for(Duration::from_secs(10));
    assert_clean(&output, "unbounded synchronizer waits passed\n");
}

#[test]
fn synchronizer_timeout_sentinel_and_zero_poll_boundaries_are_distinct() {
    let source = r#"
class PendingTask implements CancelableTask<int> {
    private CountDownLatch gate;
    public PendingTask(CountDownLatch gate) { this.gate = gate; }
    public Result<int, Text> run(borrow CancellationToken token) {
        boolean opened = this.gate.wait(-1L)?;
        if (!opened) { return new Result<int, Text>.Err(new Text("gate wait failed")); }
        return new Result<int, Text>.Ok(42);
    }
}

Result<Unit, Text> exercise() {
    Semaphore semaphore = Semaphore.create(0L)?;
    boolean semaphoreInvalidAccepted = false;
    match (semaphore.acquire(-2L)) {
        Ok(value) { semaphoreInvalidAccepted = true; }
        Err(error) { drop(error); }
    }
    boolean semaphoreEmptyPoll = !semaphore.acquire(0L)?;
    semaphore.release(1L)?;
    boolean semaphoreReadyPoll = semaphore.acquire(0L)?;

    CountDownLatch latch = CountDownLatch.create(1L)?;
    boolean latchInvalidAccepted = false;
    match (latch.wait(-2L)) {
        Ok(value) { latchInvalidAccepted = true; }
        Err(error) { drop(error); }
    }
    boolean latchEmptyPoll = !latch.wait(0L)?;
    latch.countDown()?;
    boolean latchReadyPoll = latch.wait(0L)?;

    Barrier barrier = Barrier.create(2)?;
    boolean barrierInvalidAccepted = false;
    match (barrier.wait(-2L)) {
        Ok(index) { barrierInvalidAccepted = true; }
        Err(error) { drop(error); }
    }
    boolean invalidWaitLeftBarrierUnbroken = false;
    match (barrier.reset()) {
        Ok(unit) {}
        Err(error) { invalidWaitLeftBarrierUnbroken = true; drop(error); }
    }
    boolean barrierPollBrokeGeneration = false;
    match (barrier.wait(0L)) {
        Ok(index) {}
        Err(error) { barrierPollBrokeGeneration = true; drop(error); }
    }
    barrier.reset()?;

    Mutex<int> mutex = Mutex<int>.create(17)?;
    Condition<int> condition = Condition<int>.create(mutex)?;
    Guard<int> guard = mutex.lock()?;
    boolean conditionInvalidAccepted = false;
    match (condition.wait(guard, -2L)) {
        Ok(value) { conditionInvalidAccepted = true; }
        Err(error) { drop(error); }
    }
    boolean conditionGuardRetained = guard.read() == 17;
    condition.wait(guard, 0L)?;
    boolean conditionGuardReacquired = guard.read() == 17;
    drop(guard);

    CountDownLatch gate = CountDownLatch.create(1L)?;
    CountDownLatch workerGate = gate.cloneHandle()?;
    Future<int> future = Future<int>.spawn(new PendingTask(workerGate))?;
    boolean futureInvalidAccepted = false;
    match (future.await(-2L)) {
        Ok(value) { futureInvalidAccepted = true; drop(value); }
        Err(error) { drop(error); }
    }
    boolean futureEmptyPoll = false;
    Option<int> pending = future.await(0L)?;
    match (pending) {
        None { futureEmptyPoll = true; }
        Some(value) {}
    }
    gate.countDown()?;
    int futureValue = -1;
    match (future.await(-1L)?) {
        Some(value) { futureValue = value; }
        None {}
    }

    println(!semaphoreInvalidAccepted && semaphoreEmptyPoll && semaphoreReadyPoll &&
        !latchInvalidAccepted && latchEmptyPoll && latchReadyPoll &&
        !barrierInvalidAccepted && invalidWaitLeftBarrierUnbroken &&
        barrierPollBrokeGeneration && !conditionInvalidAccepted &&
        conditionGuardRetained && conditionGuardReacquired &&
        !futureInvalidAccepted && futureEmptyPoll && futureValue == 42);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS"]);
    assert_clean(&output, "true\n");
}

#[test]
fn condition_nodes_follow_last_clone_lifetime_not_mutex_lifetime() {
    let source = r#"
Result<Unit, Text> exercise() {
    Mutex<int> mutex = Mutex<int>.create(0)?;
    int iterations = 0;
    while (iterations < 512) {
        Condition<int> condition = Condition<int>.create(mutex)?;
        drop(condition);
        iterations = iterations + 1;
    }

    Condition<int> original = Condition<int>.create(mutex)?;
    Condition<int> lastClone = original.cloneHandle()?;
    drop(original);
    Guard<int> held = mutex.lock()?;
    drop(lastClone);
    int protectedValue = held.read();
    drop(held);
    println(iterations == 512 && protectedValue == 0);
    return new Result<Unit, Text>.Ok(());
}

void main() {
    match (exercise()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#;
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("concurrency_alpha.ncr", source)));
    let c = observe_condition_node_lifetimes(&c);
    let program = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"));
    let mut process = NativeWaitProcess::start(&program);
    for _ in 0..513 {
        let line = process
            .stdout_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("condition node destruction was not observed");
        assert_eq!(line.trim_end_matches(['\r', '\n']), "NCR_TEST_CONDITION_LIVE:0");
    }
    let output = process.finish_for(Duration::from_secs(10));
    assert_clean(&output, "true\n");
}
