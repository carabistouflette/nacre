#![cfg(target_os = "linux")]

use nacre::{compile_to_c, native};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nacre-process-alpha-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create process test directory");
        Self(path)
    }

    fn path(&self) -> &Path { &self.0 }
}

impl Drop for TempDir {

    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

fn source_path(path: &Path) -> String {
    let path = path.to_str().expect("ASCII process test path");
    format!(
        "\"{}\"",
        path.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}

fn compile_program(source: &str, flags: &[&str]) -> native::NativeProgram {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("process_alpha.ncr", source)));
    native::compile_c_with_flags(&c, flags)
        .unwrap_or_else(|error| panic!("native process test compilation failed:\n{error}"))
}

fn execute(program: &native::NativeProgram, cwd: &Path, environment: &[(&str, &OsStr)]) -> Output {
    let output = Command::new("timeout")
        .arg("20s")
        .arg(program.path())
        .current_dir(cwd)
        .envs(environment.iter().copied())
        .output()
        .expect("run Nacre process test under a finite host timeout");
    assert!(output.status.success(), "native consumer failed or timed out:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "native process test leaked Nacre-owned allocations:\n{stderr}");
    output
}
fn execute_with_input(program: &native::NativeProgram, cwd: &Path, input: &[u8]) -> Output {
    let mut child = Command::new("timeout")
        .arg("20s")
        .arg(program.path())
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run Nacre process test under a finite host timeout");
    child.stdin.take().expect("piped test input").write_all(input)
        .expect("send process test input");
    let output = child.wait_with_output().expect("collect timed Nacre test output");
    assert!(output.status.success(), "native consumer failed or timed out:\\n{}\\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "native process test leaked Nacre-owned allocations:\\n{stderr}");
    output
}


fn run(source: &str, cwd: &Path) -> Output {
    let program = compile_program(source, &["-DNACRE_MEMORY_STATS"]);
    execute(&program, cwd, &[])
}

fn build_helper(directory: &Path) -> PathBuf {
    let source = directory.join("process_helper.c");
    let executable = directory.join("process-helper");
    fs::write(&source, r#"
#define _GNU_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

static int write_all(int fd, const unsigned char *bytes, size_t length) {
    while (length > 0) {
        ssize_t count = write(fd, bytes, length);
        if (count > 0) { bytes += (size_t)count; length -= (size_t)count; continue; }
        if (count < 0 && errno == EINTR) { continue; }
        return -1;
    }
    return 0;
}

static void sleep_millis(long millis) {
    struct timespec request = {
        .tv_sec = millis / 1000,
        .tv_nsec = (millis % 1000) * 1000000L,
    };
    while (nanosleep(&request, &request) != 0 && errno == EINTR) { }
}

int main(int argc, char **argv) {
    if (argc < 2) { return 90; }
    if (strcmp(argv[1], "large") == 0) {
        unsigned char out[8192];
        unsigned char err[8192];
        memset(out, 'O', sizeof(out));
        memset(err, 'E', sizeof(err));
        for (int index = 0; index < 128; ++index) {
            if (write_all(STDOUT_FILENO, out, sizeof(out)) != 0) { return 91; }
            if (write_all(STDERR_FILENO, err, sizeof(err)) != 0) { return 92; }
        }
        return 3;
    }
    if (strcmp(argv[1], "fixed") == 0) {
        if (write_all(STDOUT_FILENO, (const unsigned char *)"ABCD", 4) != 0) { return 93; }
        if (write_all(STDERR_FILENO, (const unsigned char *)"xyz", 3) != 0) { return 94; }
        return 0;
    }
    if (strcmp(argv[1], "inherit") == 0) {
        if (write_all(STDOUT_FILENO, (const unsigned char *)"INHERITED_STDOUT\n", 17) != 0) { return 101; }
        if (write_all(STDERR_FILENO, (const unsigned char *)"INHERITED_STDERR\n", 17) != 0) { return 102; }
        return 0;
    }
    if (strcmp(argv[1], "inherit-input") == 0) {
        unsigned char input[64];
        ssize_t count;
        do { count = read(STDIN_FILENO, input, sizeof(input)); } while (count < 0 && errno == EINTR);
        if (count < 0) { return 105; }
        if (write_all(STDOUT_FILENO, (const unsigned char *)"INHERITED_INPUT:", 16) != 0) { return 106; }
        if (count > 0 && write_all(STDOUT_FILENO, input, (size_t)count) != 0) { return 107; }
        return 0;
    }
    if (strcmp(argv[1], "prefix-hang") == 0) {
        if (write_all(STDOUT_FILENO, (const unsigned char *)"prefix", 6) != 0) { return 103; }
        if (write_all(STDERR_FILENO, (const unsigned char *)"error", 5) != 0) { return 104; }
        for (;;) { sleep_millis(1000); }
    }
    if (strcmp(argv[1], "redirect") == 0) {
        unsigned char input = '-';
        ssize_t count;
        do { count = read(STDIN_FILENO, &input, 1); } while (count < 0 && errno == EINTR);
        if (count < 0) { return 95; }
        if (write_all(STDOUT_FILENO, (const unsigned char *)"O", 1) != 0) { return 96; }
        if (count > 0 && write_all(STDOUT_FILENO, &input, 1) != 0) { return 97; }
        if (count == 0 && write_all(STDOUT_FILENO, (const unsigned char *)"-", 1) != 0) { return 98; }
        if (write_all(STDERR_FILENO, (const unsigned char *)"E", 1) != 0) { return 99; }
        return 7;
    }
    if (strcmp(argv[1], "delayed") == 0) { sleep_millis(250); return 0; }
    if (strcmp(argv[1], "hang") == 0) { for (;;) { sleep_millis(1000); } }
    if (strcmp(argv[1], "self-term") == 0) { (void)raise(SIGTERM); return 90; }
    if (strcmp(argv[1], "pid-sleeper") == 0) {
        if (signal(SIGTERM, SIG_IGN) == SIG_ERR) { return 108; }
        printf("%ld\n", (long)getpid());
        (void)fflush(stdout);
        for (;;) { sleep_millis(1000); }
    }
    if (strcmp(argv[1], "fork-writer") == 0) {
        pid_t child = fork();
        if (child < 0) { return 100; }
        if (child == 0) {
            unsigned char bytes[4096];
            memset(bytes, 'D', sizeof(bytes));
            for (;;) {
                if (write_all(STDOUT_FILENO, bytes, sizeof(bytes)) != 0) { _exit(0); }
                if (write_all(STDERR_FILENO, bytes, sizeof(bytes)) != 0) { _exit(0); }
            }
        }
        return 0;
    }
    return 90;
}
"#).expect("write process helper");
    let compiler = native::find_c_compiler().expect("find C compiler for process fixture");
    let output = Command::new(&compiler)
        .args(["-std=c11", "-O2", "-Wall", "-Wextra"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile process helper without a shell");
    assert!(output.status.success(), "process helper compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr));
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("mark process helper executable");
    executable
}

fn helper_command(helper: &Path, mode: &str, variable: &str) -> String {
    format!(
        "Command {variable} = Command.create(Path.fromText(new Text({}))?);\n    {variable}.addArg(new Text({}))?;",
        source_path(helper), source_path(Path::new(mode))
    )
}

#[test]
fn command_arguments_are_literal_and_capture_keeps_exact_bytes() {
    let directory = TempDir::new();
    let marker = directory.path().join("shell-must-not-create-this");
    let marker_text = marker.to_str().unwrap();
    let literal = format!("left space ; $(touch {marker_text}) 'single' \"double\"\nlast line");
    let literal_nacre = source_path(Path::new(&literal));
    let expected_nacre = source_path(Path::new(&literal));
    let source = format!(r#"
boolean bytesEqual(borrow ByteBuffer actual, borrow Text expected) {{
    if (actual.length() != expected.byteLength()) {{ return false; }}
    long index = 0L;
    while (index < actual.length()) {{
        if ((int)actual.get(index) != expected.byteAt((int)index)) {{ return false; }}
        index = index + 1L;
    }}
    return true;
}}

Result<Unit, Text> testLiteral() {{
    Text expected = new Text({expected_nacre});
    Command command = Command.create(Path.fromText(new Text("/usr/bin/printf"))?);
    command.addArg(new Text("%s"))?;
    command.addArg(new Text({literal_nacre}))?;
    ProcessOutput captured = command.output(1024L, 5000L)?;
    if (bytesEqual(captured.stdout(), expected)) {{ println("literal argument bytes preserved"); }}
    else {{ println("literal argument bytes changed"); }}
    if (!captured.stdoutTruncated() && !captured.stderrTruncated() && !captured.timedOut()) {{
        println("capture flags independent and clear");
    }}
    match (captured.status()) {{
        Exited(code) {{ if (code == 0L) {{ println("literal child exited normally"); }} }}
        Signaled(signal) {{ println("literal child unexpectedly signaled"); println(signal); }}
    }}
    return new Result<Unit, Text>.Ok(());
}}

void main() {{
    match (testLiteral()) {{
        Ok(_) {{ println("literal argument test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#);
    let output = run(&source, directory.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("literal argument bytes preserved\n"), "{stdout}");
    assert!(stdout.contains("capture flags independent and clear\n"), "{stdout}");
    assert!(stdout.contains("literal child exited normally\n"), "{stdout}");
    assert!(!marker.exists(), "literal command-substitution text was evaluated");
}

#[test]
fn environment_api_emits_without_command_api() {
    let directory = TempDir::new();
    let name = format!("NACRE_PROCESS_ENV_ONLY_{}", std::process::id());
    let source = format!(r#"
Result<Unit, Text> testEnvironment() {{
    Text name = new Text({name});
    Text value = new Text("environment value");
    Env.set(name, value)?;
    match (Env.get(name)?) {{
        Some(found) {{ if (found.equalsString("environment value")) {{ println("environment API emitted without command support"); }} }}
        None {{ println("environment API value missing"); }}
    }}
    Env.unset(name)?;
    return new Result<Unit, Text>.Ok(());
}}
void main() {{
    match (testEnvironment()) {{
        Ok(_) {{ println("environment-only test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#,
        name = source_path(Path::new(&name)),
    );
    let output = run(&source, directory.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("environment API emitted without command support\n"), "{stdout}");
}

#[test]
fn environment_snapshot_overrides_and_validation_are_end_to_end() {
    let directory = TempDir::new();
    let global_name = format!("NACRE_PROCESS_GLOBAL_{}", std::process::id());
    let absent_name = format!("NACRE_PROCESS_ABSENT_{}", std::process::id());
    let invalid_utf8_name = format!("NACRE_PROCESS_UTF8_{}", std::process::id());
    let similar_name = format!("{global_name}_NEIGHBOR");
    let source = format!(r#"
Result<Unit, Text> testEnvironment() {{
    Text globalName = new Text({global_name});
    Text globalValue = new Text("parent snapshot");
    Env.set(globalName, globalValue)?;
    Text similarName = new Text({similar_name});
    Text similarValue = new Text("neighbor value");
    Env.set(similarName, similarValue)?;
    Text absentName = new Text({absent_name});
    Env.unset(absentName)?;
    match (Env.get(absentName)?) {{
        None {{ println("missing environment is None"); }}
        Some(value) {{ println("missing environment unexpectedly present"); }}
    }}

    Command inherited = Command.create(Path.fromText(new Text("/usr/bin/printenv"))?);
    inherited.addArg(new Text({global_name}))?;
    ProcessOutput inheritedOutput = inherited.output(128L, 5000L)?;
    borrow ByteBuffer inheritedBytes = inheritedOutput.stdout();
    if (inheritedBytes.length() == 16L
        && inheritedBytes.get(0L) == (byte)112) {{
        println("spawn inherited synchronized environment snapshot");
    }}

    inherited.setEnv(new Text({global_name}), new Text("child override"))?;
    ProcessOutput overrideOutput = inherited.output(128L, 5000L)?;
    borrow ByteBuffer overrideBytes = overrideOutput.stdout();
    if (overrideBytes.length() == 15L
        && overrideBytes.get(0L) == (byte)99) {{
        println("child environment override replaced exact key");
    }}
    Command exactKey = Command.create(Path.fromText(new Text("/usr/bin/printenv"))?);
    exactKey.setEnv(new Text({global_name}), new Text("child override"))?;
    exactKey.addArg(new Text({similar_name}))?;
    ProcessOutput similarOutput = exactKey.output(128L, 5000L)?;
    borrow ByteBuffer similarBytes = similarOutput.stdout();
    if (similarBytes.length() == 15L
        && similarBytes.get(0L) == (byte)110) {{
        println("prefix environment name remained inherited");
    }}
    match (Env.get(globalName)?) {{
        Some(value) {{ if (value.equalsString("parent snapshot")) {{ println("command override left global unchanged"); }} }}
        None {{ println("global environment unexpectedly missing"); }}
    }}

    Command cleared = Command.create(Path.fromText(new Text("/usr/bin/printenv"))?);
    cleared.clearEnv()?;
    cleared.addArg(new Text({global_name}))?;
    ProcessOutput absentInChild = cleared.output(16L, 5000L)?;
    borrow ByteBuffer absentBytes = absentInChild.stdout();
    if (absentBytes.length() == 0L) {{
        match (absentInChild.status()) {{
            Exited(code) {{ if (code == 1L) {{ println("clearEnv suppressed inheritance"); }} }}
            Signaled(signal) {{ println("printenv unexpectedly signaled"); }}
        }}
    }}

    Text emptyName = new Text("");
    Text equalName = new Text("BAD=NAME");
    Text nulName = new Text("BAD\0NAME");
    Text nulValue = new Text("BAD\0VALUE");
    match (Env.set(emptyName, globalValue)) {{ Err(error) {{ println("empty env name rejected"); }} Ok(_) {{ println("empty env name accepted"); }} }}
    match (Env.set(equalName, globalValue)) {{ Err(error) {{ println("equals in env name rejected"); }} Ok(_) {{ println("equals in env name accepted"); }} }}
    match (Env.set(nulName, globalValue)) {{ Err(error) {{ println("NUL in env name rejected"); }} Ok(_) {{ println("NUL in env name accepted"); }} }}
    match (Env.set(globalName, nulValue)) {{ Err(error) {{ println("NUL in env value rejected"); }} Ok(_) {{ println("NUL in env value accepted"); }} }}
    match (Env.unset(emptyName)) {{ Err(error) {{ println("invalid unset name rejected"); }} Ok(_) {{ println("invalid unset name accepted"); }} }}

    Command invalid = Command.create(Path.fromText(new Text("/usr/bin/true"))?);
    match (invalid.addArg(new Text("bad\0argument"))) {{ Err(error) {{ println("NUL argument rejected and consumed"); }} Ok(_) {{ println("NUL argument accepted"); }} }}
    match (invalid.setEnv(new Text("BAD\0NAME"), new Text("value"))) {{ Err(error) {{ println("NUL override name rejected and consumed"); }} Ok(_) {{ println("NUL override name accepted"); }} }}
    match (invalid.setEnv(new Text("GOOD_NAME"), new Text("bad\0value"))) {{ Err(error) {{ println("NUL override value rejected and consumed"); }} Ok(_) {{ println("NUL override value accepted"); }} }}

    Text invalidUtf8 = new Text({invalid_utf8_name});
    match (Env.get(invalidUtf8)) {{ Err(error) {{ println("invalid UTF-8 environment value rejected"); }} Ok(option) {{ println("invalid UTF-8 environment value accepted"); }} }}
    Env.unset(similarName)?;
    Env.unset(globalName)?;
    return new Result<Unit, Text>.Ok(());
}}

void main() {{
    match (testEnvironment()) {{
        Ok(_) {{ println("environment test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#,
        global_name = source_path(Path::new(&global_name)),
        similar_name = source_path(Path::new(&similar_name)),
        absent_name = source_path(Path::new(&absent_name)),
        invalid_utf8_name = source_path(Path::new(&invalid_utf8_name)),
    );
    let value = OsString::from_vec(vec![0xff, 0xfe]);
    let program = compile_program(&source, &["-DNACRE_MEMORY_STATS"]);
    let output = execute(&program, directory.path(), &[(&invalid_utf8_name, &value)]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for marker in [
        "missing environment is None",
        "spawn inherited synchronized environment snapshot",
        "child environment override replaced exact key",
        "command override left global unchanged",
        "prefix environment name remained inherited",
        "clearEnv suppressed inheritance",
        "empty env name rejected",
        "equals in env name rejected",
        "NUL in env name rejected",
        "NUL in env value rejected",
        "invalid unset name rejected",
        "NUL argument rejected and consumed",
        "NUL override name rejected and consumed",
        "NUL override value rejected and consumed",
        "invalid UTF-8 environment value rejected",
    ] {
        assert!(stdout.contains(marker), "missing {marker:?} in output:\n{stdout}");
    }
    assert!(!stdout.contains("accepted"), "invalid input was accepted:\n{stdout}");
}

#[test]
fn path_search_uses_child_path_and_empty_components_mean_current_directory() {
    let directory = TempDir::new();
    let helper = build_helper(directory.path());
    let helper_name = helper.file_name().unwrap().to_str().unwrap();
    let empty_component_name = format!("process-current-{}", std::process::id());
    let current_helper = directory.path().join(&empty_component_name);
    fs::copy(&helper, &current_helper).expect("copy current-directory executable");
    fs::set_permissions(&current_helper, fs::Permissions::from_mode(0o700)).unwrap();
    let source = format!(r#"
Result<Unit, Text> testSearch() {{
    Command pathCommand = Command.create(Path.fromText(new Text({helper_name}))?);
    pathCommand.clearEnv()?;
    pathCommand.setEnv(new Text("PATH"), new Text({path}))?;
    pathCommand.addArg(new Text("fixed"))?;
    ProcessOutput pathOutput = pathCommand.output(64L, 5000L)?;
    borrow ByteBuffer pathBytes = pathOutput.stdout();
    if (pathBytes.length() == 4L && pathBytes.get(0L) == (byte)65) {{
        println("executable resolved from child PATH snapshot");
    }}

    Command current = Command.create(Path.fromText(new Text({empty_component_name}))?);
    current.clearEnv()?;
    current.setEnv(new Text("PATH"), new Text(""))?;
    current.addArg(new Text("fixed"))?;
    ProcessOutput currentOutput = current.output(64L, 5000L)?;
    borrow ByteBuffer currentBytes = currentOutput.stdout();
    if (currentBytes.length() == 4L && currentBytes.get(0L) == (byte)65) {{
        println("empty PATH component searched current directory");
    }}
    return new Result<Unit, Text>.Ok(());
}}
void main() {{
    match (testSearch()) {{
        Ok(_) {{ println("PATH resolution test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#,
        helper_name = source_path(Path::new(helper_name)),
        path = source_path(directory.path()),
        empty_component_name = source_path(Path::new(&empty_component_name)),
    );
    let output = run(&source, directory.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("executable resolved from child PATH snapshot\n"), "{stdout}");
    assert!(stdout.contains("empty PATH component searched current directory\n"), "{stdout}");
}

#[test]
fn enoexec_is_an_error_without_shell_fallback() {
    let directory = TempDir::new();
    let script = directory.path().join("no-shebang");
    fs::write(&script, "printf '%s\\n' 'SHELL_FALLBACK_RAN'\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let source = format!(r#"
Result<Unit, Text> testEnoexec() {{
    Command command = Command.create(Path.fromText(new Text({script_name}))?);
    command.clearEnv()?;
    command.setEnv(new Text("PATH"), new Text({path}))?;
    match (command.output(128L, 5000L)) {{
        Err(error) {{ println("ENOEXEC rejected without shell fallback"); }}
        Ok(output) {{
            println("executable format error was hidden");
            borrow ByteBuffer stdout = output.stdout();
            if (stdout.length() > 0L) {{ println("shell fallback produced child output"); }}
        }}
    }}
    return new Result<Unit, Text>.Ok(());
}}
void main() {{
    match (testEnoexec()) {{
        Ok(_) {{ println("ENOEXEC test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#,
        script_name = source_path(Path::new("no-shebang")),
        path = source_path(directory.path()),
    );
    let output = run(&source, directory.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ENOEXEC rejected without shell fallback\n"), "{stdout}");
    assert!(!stdout.contains("shell fallback produced child output"), "{stdout}");
    assert!(!stdout.contains("SHELL_FALLBACK_RAN"), "a shell executed the no-shebang file:\n{stdout}");
}

#[test]
fn process_capture_drains_both_pipes_bounds_prefixes_and_times_out_descendants() {
    let directory = TempDir::new();
    let helper = build_helper(directory.path());
    let source = format!(r#"
Result<Unit, Text> testCapture() {{
    {large_command}
    ProcessOutput large = command.output(1024L, 10000L)?;
    borrow ByteBuffer largeStdout = large.stdout();
    borrow ByteBuffer largeStderr = large.stderr();
    if (largeStdout.length() == 1024L && largeStderr.length() == 1024L
        && largeStdout.get(0L) == (byte)79 && largeStdout.get(1023L) == (byte)79
        && largeStderr.get(0L) == (byte)69 && largeStderr.get(1023L) == (byte)69
        && large.stdoutTruncated() && large.stderrTruncated() && !large.timedOut()) {{
        println("simultaneous streams drained with independent bounded prefixes");
    }}
    match (large.status()) {{
        Exited(code) {{ if (code == 3L) {{ println("nonzero exit retained as status"); }} }}
        Signaled(signal) {{ println("large child unexpectedly signaled"); }}
    }}

    {fixed_command}
    ProcessOutput exact = fixedCommand.output(4L, 5000L)?;
    borrow ByteBuffer exactStdout = exact.stdout();
    borrow ByteBuffer exactStderr = exact.stderr();
    if (exactStdout.length() == 4L && exactStderr.length() == 3L
        && !exact.stdoutTruncated() && !exact.stderrTruncated()) {{
        println("exact capture limit is not truncated");
    }}
    {fixed_zero_command}
    ProcessOutput zero = zeroCommand.output(0L, 0L)?;
    borrow ByteBuffer zeroStdout = zero.stdout();
    borrow ByteBuffer zeroStderr = zero.stderr();
    if (zeroStdout.length() == 0L && zeroStderr.length() == 0L
        && zero.stdoutTruncated() && zero.stderrTruncated()) {{
        println("zero capture limit drains and marks both streams");
    }}

    {hang_command}
    ProcessOutput timed = hang.output(32L, 50L)?;
    borrow ByteBuffer timedStdout = timed.stdout();
    borrow ByteBuffer timedStderr = timed.stderr();
    if (timed.timedOut() && timedStdout.length() == 6L && timedStderr.length() == 5L
        && timedStdout.get(0L) == (byte)112 && timedStderr.get(0L) == (byte)101
        && !timed.stdoutTruncated() && !timed.stderrTruncated()) {{
        println("capture timeout preserved available stream prefixes");
    }}
    match (timed.status()) {{
        Signaled(signal) {{ if (signal == 9) {{ println("timeout status is SIGKILL"); }} }}
        Exited(code) {{ println("timed child unexpectedly exited"); println(code); }}
    }}

    {fork_command}
    ProcessOutput descendant = forkCommand.output(64L, 80L)?;
    if (descendant.timedOut()) {{ println("descendant-held pipes do not extend timeout"); }}
    match (descendant.status()) {{
        Exited(code) {{ if (code == 0L) {{ println("descendant parent exit retained"); }} }}
        Signaled(signal) {{ println("descendant parent unexpectedly signaled"); }}
    }}
    return new Result<Unit, Text>.Ok(());
}}
void main() {{
    match (testCapture()) {{
        Ok(_) {{ println("capture test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#,
        large_command = helper_command(&helper, "large", "command"),
        fixed_command = helper_command(&helper, "fixed", "fixedCommand"),
        fixed_zero_command = helper_command(&helper, "fixed", "zeroCommand"),
        hang_command = helper_command(&helper, "prefix-hang", "hang"),
        fork_command = helper_command(&helper, "fork-writer", "forkCommand"),
    );
    let output = run(&source, directory.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for marker in [
        "simultaneous streams drained with independent bounded prefixes",
        "nonzero exit retained as status",
        "exact capture limit is not truncated",
        "zero capture limit drains and marks both streams",
        "capture timeout preserved available stream prefixes",
        "timeout status is SIGKILL",
        "descendant-held pipes do not extend timeout",
        "descendant parent exit retained",
    ] {
        assert!(stdout.contains(marker), "missing {marker:?} in output:\n{stdout}");
    }
}

#[test]
fn wait_timeout_preserves_child_signals_are_reported_and_drop_reaps() {
    let directory = TempDir::new();
    let helper = build_helper(directory.path());
    let dropped_pid_path = directory.path().join("dropped-child.pid");
    let source = format!(r#"
Result<Unit, Text> testWaitAndDrop() {{
    {inherited_command}
    Process inheritedProcess = inherited.spawn()?;
    match (inheritedProcess.wait()?) {{
        Exited(code) {{ if (code == 0L) {{ println("inherited stdio process waited"); }} }}
        Signaled(signal) {{ println("inherited stdio child signaled"); }}
    }}
    {inherited_input_command}
    Process inheritedInput = inheritedInputCommand.spawn()?;
    match (inheritedInput.wait()?) {{
        Exited(code) {{ if (code == 0L) {{ println("inherited stdin process waited"); }} }}
        Signaled(signal) {{ println("inherited stdin child signaled"); }}
    }}


    Command invalidTimeout = Command.create(Path.fromText(new Text({helper_path}))?);
    match (invalidTimeout.output(-1L, 0L)) {{
        Err(error) {{ println("negative capture maximum rejected"); }}
        Ok(output) {{ println("negative capture maximum accepted"); }}
    }}
    match (invalidTimeout.output(1L, -1L)) {{
        Err(error) {{ println("negative capture timeout rejected"); }}
        Ok(output) {{ println("negative capture timeout accepted"); }}
    }}

    {delayed_command}
    Process delayedProcess = delayed.spawn()?;
    match (delayedProcess.waitFor(-1L)) {{
        Err(error) {{ println("negative wait timeout rejected"); }}
        Ok(option) {{ println("negative wait timeout accepted"); }}
    }}
    match (delayedProcess.waitFor(10L)?) {{
        None {{ println("waitFor timeout preserved process"); }}
        Some(status) {{ println("waitFor unexpectedly consumed process"); }}
    }}
    match (delayedProcess.wait()?) {{
        Exited(code) {{ if (code == 0L) {{ println("wait reaped after timeout"); }} }}
        Signaled(signal) {{ println("delayed process unexpectedly signaled"); }}
    }}
    match (delayedProcess.wait()) {{
        Err(error) {{ println("repeated wait rejected"); }}
        Ok(status) {{ println("repeated wait unexpectedly succeeded"); }}
    }}
    {zero_wait_command}
    Process zeroWait = zeroWaitCommand.spawn()?;
    match (zeroWait.waitFor(0L)?) {{
        None {{ println("zero wait timeout returned None"); }}
        Some(status) {{
            match (status) {{
                Exited(code) {{ if (code == 0L) {{ println("zero wait timeout is unbounded"); }} }}
                Signaled(signal) {{ println("zero wait timeout child signaled"); }}
            }}
        }}
    }}


    {signal_command}
    Process signaled = signalCommand.spawn()?;
    match (signaled.wait()?) {{
        Signaled(signal) {{ if (signal == 15) {{ println("signal exit status retained"); }} }}
        Exited(code) {{ println("self-signaled child exited"); println(code); }}
    }}

    {kill_command}
    Process killed = killCommand.spawn()?;
    killed.kill()?;
    match (killed.wait()?) {{
        Signaled(signal) {{ if (signal == 9) {{ println("kill signal sent only to child"); }} }}
        Exited(code) {{ println("killed child unexpectedly exited"); println(code); }}
    }}
    {terminate_command}
    Process terminated = terminateCommand.spawn()?;
    terminated.terminate()?;
    match (terminated.wait()?) {{
        Signaled(signal) {{ if (signal == 15) {{ println("terminate signal sent only to child"); }} }}
        Exited(code) {{ println("terminated child unexpectedly exited"); println(code); }}
    }}


    {drop_command}
    Path pidFilePath = Path.fromText(new Text({pid_file}))?;
    File pidFile = File.create(pidFilePath, 384)?;
    dropCommand.redirectStdout(pidFile)?;
    {{
        Process running = dropCommand.spawn()?;
        match (running.waitFor(200L)?) {{
            None {{ println("drop test child was running"); }}
            Some(status) {{ println("drop test child unexpectedly exited"); }}
        }}
    }}
    println("running process scope dropped");
    return new Result<Unit, Text>.Ok(());
}}
void main() {{
    match (testWaitAndDrop()) {{
        Ok(_) {{ println("wait and drop test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#,
        delayed_command = helper_command(&helper, "delayed", "delayed"),
        signal_command = helper_command(&helper, "self-term", "signalCommand"),
        kill_command = helper_command(&helper, "hang", "killCommand"),
        drop_command = helper_command(&helper, "pid-sleeper", "dropCommand"),
        pid_file = source_path(&dropped_pid_path),
        inherited_command = helper_command(&helper, "inherit", "inherited"),
        helper_path = source_path(&helper),
        zero_wait_command = helper_command(&helper, "delayed", "zeroWaitCommand"),
        terminate_command = helper_command(&helper, "hang", "terminateCommand"),
        inherited_input_command = helper_command(&helper, "inherit-input", "inheritedInputCommand"),
    );
    let program = compile_program(&source, &["-DNACRE_MEMORY_STATS"]);
    let output = execute_with_input(&program, directory.path(), b"nacre-inherited-input");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for marker in [
        "waitFor timeout preserved process",
        "wait reaped after timeout",
        "repeated wait rejected",
        "signal exit status retained",
        "kill signal sent only to child",
        "drop test child was running",
        "running process scope dropped",
        "inherited stdio process waited",
        "negative capture maximum rejected",
        "negative capture timeout rejected",
        "negative wait timeout rejected",
        "zero wait timeout is unbounded",
        "terminate signal sent only to child",
        "inherited stdin process waited",
    ] {
        assert!(stdout.contains(marker), "missing {marker:?} in output:\n{stdout}");
    }
    assert!(stdout.contains("INHERITED_STDOUT\n"), "inherited stdout was not preserved: {stdout}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("INHERITED_STDERR\n"),
        "inherited stderr was not preserved: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("INHERITED_INPUT:nacre-inherited-input"),
        "inherited stdin was not preserved: {stdout}");
    let child_pid: i32 = fs::read_to_string(&dropped_pid_path)
        .expect("dropped child wrote its pid before scope cleanup")
        .trim()
        .parse()
        .expect("parse dropped child pid");
    assert!(!Path::new(&format!("/proc/{child_pid}")).exists(),
        "dropping a running Process left child {child_pid} alive or zombie");
}

#[test]
fn file_redirections_retain_and_replace_owned_files_across_spawns() {
    let directory = TempDir::new();
    let helper = build_helper(directory.path());
    let input_path = directory.path().join("input.bin");
    let unused_output_path = directory.path().join("unused.bin");
    let output_path = directory.path().join("output.bin");
    let error_path = directory.path().join("error.bin");
    let closed_path = directory.path().join("closed.bin");
    fs::write(&input_path, b"I").unwrap();
    let source = format!(r#"
Result<int, Text> countDescriptors() {{
    Path descriptorPath = Path.fromText(new Text("/proc/self/fd"))?;
    Directory directory = Directory.open(descriptorPath)?;
    int count = 0;
    while (true) {{
        Option<Text> entry = directory.nextName()?;
        match (entry) {{ Some(name) {{ count = count + 1; }} None {{ break; }} }}
    }}
    directory.close()?;
    return new Result<int, Text>.Ok(count);
}}

Result<Unit, Text> testRedirection() {{
    Path input = Path.fromText(new Text({input}))?;
    File source = File.openRead(input)?;
    Path unusedOutputPath = Path.fromText(new Text({unused_output}))?;
    File firstOutput = File.create(unusedOutputPath, 384)?;
    Path outputFilePath = Path.fromText(new Text({output}))?;
    File output = File.create(outputFilePath, 384)?;
    Path errorFilePath = Path.fromText(new Text({error}))?;
    File error = File.create(errorFilePath, 384)?;
    {helper_command}
    command.redirectStdin(source)?;
    int beforeReplace = countDescriptors()?;
    command.redirectStdout(firstOutput)?;
    int afterFirstRedirect = countDescriptors()?;
    command.redirectStdout(output)?;
    int afterReplace = countDescriptors()?;
    if (afterFirstRedirect == beforeReplace && afterReplace + 1 == afterFirstRedirect) {{
        println("redirect replacement closed previous descriptor");
    }}
    command.redirectStderr(error)?;
    match (command.output(16L, 5000L)) {{
        Err(message) {{ println("output rejects configured redirects"); }}
        Ok(captured) {{ println("output unexpectedly replaced redirects"); }}
    }}
    Process first = command.spawn()?;
    match (first.wait()?) {{ Exited(code) {{ if (code == 7L) {{ println("redirected first exit code retained"); }} }} Signaled(signal) {{ println("first redirected child signaled"); }} }}
    Process second = command.spawn()?;
    match (second.wait()?) {{ Exited(code) {{ if (code == 7L) {{ println("redirected second exit code retained"); }} }} Signaled(signal) {{ println("second redirected child signaled"); }} }}

    Path closedFilePath = Path.fromText(new Text({closed}))?;
    File closed = File.create(closedFilePath, 384)?;
    closed.close()?;
    Command closedCommand = Command.create(Path.fromText(new Text("/usr/bin/true"))?);
    match (closedCommand.redirectStdout(closed)) {{
        Err(message) {{ println("closed redirect rejected and consumed"); }}
        Ok(_) {{ println("closed redirect accepted"); }}
    }}
    return new Result<Unit, Text>.Ok(());
}}
void main() {{
    match (testRedirection()) {{
        Ok(_) {{ println("redirection test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#,
        input = source_path(&input_path),
        unused_output = source_path(&unused_output_path),
        output = source_path(&output_path),
        error = source_path(&error_path),
        closed = source_path(&closed_path),
        helper_command = helper_command(&helper, "redirect", "command"),
    );
    let output = run(&source, directory.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for marker in [
        "output rejects configured redirects",
        "redirected first exit code retained",
        "redirected second exit code retained",
        "closed redirect rejected and consumed",
        "redirect replacement closed previous descriptor",
    ] {
        assert!(stdout.contains(marker), "missing {marker:?} in output:\n{stdout}");
    }
    assert_eq!(fs::read(&unused_output_path).unwrap(), b"");
    assert_eq!(fs::read(&output_path).unwrap(), b"OIO-");
    assert_eq!(fs::read(&error_path).unwrap(), b"EE");
}

#[test]
fn spawn_failures_close_real_descriptors_and_reject_closed_redirects() {
    let directory = TempDir::new();
    let output_path = directory.path().join("retained-output.bin");
    let missing = directory.path().join("missing-executable");
    let source = format!(r#"
Result<int, Text> countDescriptors() {{
    Path descriptorPath = Path.fromText(new Text("/proc/self/fd"))?;
    Directory directory = Directory.open(descriptorPath)?;
    int count = 0;
    while (true) {{
        Option<Text> entry = directory.nextName()?;
        match (entry) {{ Some(name) {{ count = count + 1; }} None {{ break; }} }}
    }}
    directory.close()?;
    return new Result<int, Text>.Ok(count);
}}
Result<Unit, Text> testFailures() {{
    int beforeCapture = countDescriptors()?;
    Command captureFailure = Command.create(Path.fromText(new Text({missing}))?);
    match (captureFailure.output(64L, 1000L)) {{
        Err(error) {{ println("capture spawn failure returned error"); }}
        Ok(captured) {{ println("capture spawn failure unexpectedly succeeded"); }}
    }}
    int afterCapture = countDescriptors()?;
    if (beforeCapture == afterCapture) {{ println("capture spawn failure closed every descriptor"); }}
    else {{ println("capture spawn failure leaked descriptors"); println(beforeCapture); println(afterCapture); }}
    Path sinkPath = Path.fromText(new Text({output}))?;
    File sink = File.create(sinkPath, 384)?;
    Command command = Command.create(Path.fromText(new Text({missing}))?);
    command.redirectStdout(sink)?;
    int before = countDescriptors()?;
    int attempt = 0;
    while (attempt < 64) {{
        match (command.spawn()) {{
            Err(error) {{}}
            Ok(process) {{
                process.kill()?;
                match (process.wait()?) {{
                    Exited(code) {{}}
                    Signaled(signal) {{}}
                }}
            }}
        }}
        attempt = attempt + 1;
    }}
    int after = countDescriptors()?;
    if (before == after) {{ println("spawn failures closed temporary descriptors"); }}
    else {{ println("spawn failure leaked descriptors"); println(before); println(after); }}
    Path closedPath = Path.fromText(new Text({output}))?;
    File closed = File.create(closedPath, 384)?;
    closed.close()?;
    Command invalidRedirect = Command.create(Path.fromText(new Text("/usr/bin/true"))?);
    match (invalidRedirect.redirectStdin(closed)) {{
        Err(error) {{ println("closed stdin redirect rejected"); }}
        Ok(_) {{ println("closed stdin redirect accepted"); }}
    }}
    return new Result<Unit, Text>.Ok(());
}}
void main() {{
    match (testFailures()) {{
        Ok(_) {{ println("failure cleanup test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#,
        output = source_path(&output_path),
        missing = source_path(&missing),
    );
    let output = run(&source, directory.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("spawn failures closed temporary descriptors\n"), "{stdout}");
    assert!(stdout.contains("closed stdin redirect rejected\n"), "{stdout}");
    assert!(!stdout.contains("leaked descriptors"), "{stdout}");
    assert!(stdout.contains("capture spawn failure returned error\n"), "{stdout}");
    assert!(stdout.contains("capture spawn failure closed every descriptor\n"), "{stdout}");
}

#[test]
fn process_native_consumer_runs_under_address_and_undefined_sanitizers() {
    let directory = TempDir::new();
    let helper = build_helper(directory.path());
    let source = format!(r#"
Result<Unit, Text> testSanitizedCapture() {{
    {helper_command}
    ProcessOutput output = command.output(4096L, 10000L)?;
    borrow ByteBuffer stdout = output.stdout();
    borrow ByteBuffer stderr = output.stderr();
    if (stdout.length() == 4096L && stderr.length() == 4096L
        && output.stdoutTruncated() && output.stderrTruncated()) {{
        println("sanitized concurrent process capture complete");
    }}
    match (output.status()) {{ Exited(code) {{ if (code == 3L) {{ println("sanitized child status preserved"); }} }} Signaled(signal) {{ println("sanitized capture unexpectedly signaled"); }} }}
    Command failed = Command.create(Path.fromText(new Text("/no/such/process-alpha-executable"))?);
    match (failed.spawn()) {{ Err(error) {{ println("sanitized spawn failure cleaned up"); }} Ok(process) {{ process.kill()?; match (process.wait()?) {{ Exited(code) {{}} Signaled(signal) {{}} }} }} }}
    return new Result<Unit, Text>.Ok(());
}}
void main() {{
    match (testSanitizedCapture()) {{
        Ok(_) {{ println("sanitizer test complete"); }}
        Err(error) {{ error.print(); }}
    }}
}}
"#,
        helper_command = helper_command(&helper, "large", "command"),
    );
    let c = compile_to_c(&source)
        .unwrap_or_else(|error| panic!("{}", error.render("process_alpha.ncr", &source)));
    let program = native::compile_c_with_flags(
        &c,
        &["-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"],
    ).unwrap_or_else(|error| panic!("sanitized native consumer compilation failed:\n{error}"));
    let output = execute(&program, directory.path(), &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("sanitized concurrent process capture complete\n"), "{stdout}");
    assert!(stdout.contains("sanitized child status preserved\n"), "{stdout}");
    assert!(stdout.contains("sanitized spawn failure cleaned up\n"), "{stdout}");
}

#[test]
fn standalone_process_types_compile_without_command_dependencies() {
    let directory = TempDir::new();
    for source in [
        "boolean expired(borrow ProcessOutput output) { return output.timedOut(); }\nvoid main() { println(42); }\n",
        "Result<Option<ExitStatus>, Text> pollChild(borrow mut Process process) { return process.waitFor(1L); }\nvoid main() { println(42); }\n",
    ] {
        let c = compile_to_c(source)
            .unwrap_or_else(|error| panic!("{}", error.render("standalone_process.ncr", source)));
        let program = native::compile_c_optimized(&c, "-O0", &["-DNACRE_MEMORY_STATS"])
            .unwrap_or_else(|error| panic!("standalone native type failed to compile:\n{error}"));
        assert_eq!(execute(&program, directory.path(), &[]).stdout, b"42\n");
    }
}
