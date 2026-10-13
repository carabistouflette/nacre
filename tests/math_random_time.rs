use nacre::{compile_to_c, native};
use std::process::Output;

const EXAMPLE: &str = include_str!("../examples/language/math_random_time.ncr");

fn execute(source: &str, flags: &[&str]) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("math_random_time.ncr", source)));
    native::compile_c_with_flags(&c, flags)
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_native_success(output: &Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
    assert!(!stderr.contains("runtime error:"), "{stderr}");
}

fn assert_live_zero(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")), "{stderr}");
}

#[test]
fn math_utilities_preserve_ieee_values_and_report_signed_minimum_abs() {
    let source = r#"
void main() {
    match (Math.absInt(-17)) {
        Ok(value) { println(value); }
        Err(error) { println("unexpected"); }
    }
    match (Math.absLong(-9L)) {
        Ok(value) { println(value); }
        Err(error) { println("unexpected"); }
    }
    match (Math.absInt(-2147483648)) {
        Ok(value) { println("unexpected"); }
        Err(error) { println("INT_MIN_ERR"); }
    }
    match (Math.absLong(-9223372036854775808L)) {
        Ok(value) { println("unexpected"); }
        Err(error) { println("LONG_MIN_ERR"); }
    }
    println(Math.absDouble(-3.5) == 3.5);
    println(Math.absDouble(-0.0) == 0.0);
    println(1.0 / Math.absDouble(-0.0) > 0.0);
    println(Math.minInt(8, -4) == -4);
    println(Math.maxInt(8, -4) == 8);
    println(Math.minLong(8L, -4L) == -4L);
    println(Math.maxLong(8L, -4L) == 8L);

    double nan = Math.sqrt(-1.0);
    double infinity = Math.exp(10000.0);
    println(Math.isNaN(nan));
    println(Math.isNaN(Math.pow(-1.0, 0.5)));
    println(Math.isNaN(Math.sin(infinity)));
    println(Math.minDouble(nan, 2.0) == 2.0);
    println(Math.maxDouble(nan, 2.0) == 2.0);
    println(Math.isNaN(Math.minDouble(nan, nan)));
    println(Math.isFinite(nan));
    println(Math.isFinite(infinity));
    println(Math.log(0.0) < -1.0e300);
    println(Math.minDouble(-0.0, 0.0) == 0.0);
    println(1.0 / Math.minDouble(-0.0, 0.0) < 0.0);
    println(1.0 / Math.maxDouble(-0.0, 0.0) > 0.0);
    println(Math.sqrt(9.0) == 3.0);
    println(Math.pow(2.0, 10.0) == 1024.0);
    println(Math.log(1.0) == 0.0);
    println(Math.sin(0.0) == 0.0);
    println(Math.cos(0.0) == 1.0);
    println(Math.tan(0.0) == 0.0);
    println(Math.floor(-2.1) == -3.0);
    println(Math.ceil(-2.1) == -2.0);
    println(Math.round(-2.5) == -3.0);
    println(Math.hypot(3.0, 4.0) == 5.0);
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"]);
    assert_native_success(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), concat!(
        "17\n9\nINT_MIN_ERR\nLONG_MIN_ERR\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\nfalse\nfalse\ntrue\ntrue\ntrue\ntrue\n",
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n"
    ));
    assert_live_zero(&output);
}

#[test]
fn deterministic_random_reseeds_and_bounds_are_reproducible() {
    let source = r#"
void main() {
    Random generator = new Random(0L);
    println(generator.nextLong() == -2152535657050944081L);

    generator.reseed(0L);
    match (generator.nextInt(1)) {
        Ok(value) { println(value == 0); }
        Err(error) { println("unexpected"); }
    }
    generator.reseed(0L);
    match (generator.nextInt(6)) {
        Ok(value) { println(value == 3); }
        Err(error) { println("unexpected"); }
    }
    generator.reseed(0L);
    match (generator.nextInt(2147483647)) {
        Ok(value) { println(value >= 0 && value < 2147483647); }
        Err(error) { println("unexpected"); }
    }
    Random rejection = new Random(3L);
    match (rejection.nextInt(1500000000)) {
        Ok(value) { println(value == 7737738); }
        Err(error) { println("unexpected"); }
    }

    generator.reseed(0L);
    double sample = generator.nextDouble();
    println(sample >= 0.0 && sample < 1.0);
    println(sample == 0.8833108082136426);

    generator.reseed(0L);
    match (generator.nextInt(0)) {
        Ok(value) { println("unexpected"); }
        Err(error) { println("ZERO_BOUND_ERR"); }
    }
    println(generator.nextLong() == -2152535657050944081L);
    match (generator.nextInt(-7)) {
        Ok(value) { println("unexpected"); }
        Err(error) { println("NEGATIVE_BOUND_ERR"); }
    }
    println(generator.nextLong() == 7960286522194355700L);

    match (Random.fromSystem()) {
        Ok(systemRandom) {
            double entropySample = systemRandom.nextDouble();
            println(entropySample >= 0.0 && entropySample < 1.0);
        }
        Err(error) { println("ENTROPY_ERR"); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"]);
    assert_native_success(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout),
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\nZERO_BOUND_ERR\ntrue\nNEGATIVE_BOUND_ERR\ntrue\ntrue\n");
    assert_live_zero(&output);
}

#[test]
fn system_entropy_short_read_failure_closes_and_drops_partial_state() {
    let source = r#"
void main() {
    match (Random.fromSystem()) {
        Ok(random) { println("unexpected"); }
        Err(error) { println("ENTROPY_ERR"); }
    }
}
"#;
    let mut c = compile_to_c(source).unwrap();
    c.push_str(r#"
static int ncr_test_reads;
static int ncr_test_entropy_fd = -1;
static void ncr_test_entropy_closed(void) {
    errno = 0;
    if (fcntl(ncr_test_entropy_fd, F_GETFD) != -1 || errno != EBADF) { abort(); }
}
int ncr_test_open(const char *path, int flags, ...) {
    ncr_test_entropy_fd = openat(AT_FDCWD, path, flags);
    if (ncr_test_entropy_fd < 0 || atexit(ncr_test_entropy_closed) != 0) { abort(); }
    return ncr_test_entropy_fd;
}
ssize_t ncr_test_read(int descriptor, void *buffer, size_t count) {
    (void)descriptor;
    if (ncr_test_reads++ == 0) {
        size_t partial = count < 3 ? count : 3;
        memset(buffer, 0x5a, partial);
        return (ssize_t)partial;
    }
    errno = EIO;
    return -1;
}
"#);
    let output = native::compile_c_with_flags(&c, &[
        "-DNACRE_MEMORY_STATS",
        "-Dopen=ncr_test_open",
        "-Dread=ncr_test_read",
        "-fsanitize=address,undefined",
        "-fno-omit-frame-pointer",
    ]).unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output().unwrap_or_else(|error| panic!("native execution failed: {error}"));
    assert_native_success(&output);
    assert_eq!(output.stdout, b"ENTROPY_ERR\n");
    assert_live_zero(&output);
}

#[test]
fn time_uses_monotonic_nanoseconds_wall_milliseconds_and_checked_sleep() {
    let source = r#"
void main() {
    long before = 0L;
    match (Time.monotonicNanos()) {
        Ok(value) { before = value; }
        Err(error) { println("MONOTONIC_ERR"); }
    }
    long next = 0L;
    match (Time.monotonicNanos()) {
        Ok(value) { next = value; }
        Err(error) { println("MONOTONIC_ERR"); }
    }
    println(before >= 0L);
    println(next >= before);

    match (Time.deadlineAfterMillis(5000L)) {
        Ok(deadline) { println(deadline > next); }
        Err(error) { println("DEADLINE_ERR"); }
    }
    match (Time.deadlineAfterMillis(-1L)) {
        Ok(deadline) { println("unexpected"); }
        Err(error) { println("NEGATIVE_DEADLINE_ERR"); }
    }
    match (Time.deadlineAfterMillis(9223372036854775807L)) {
        Ok(deadline) { println("unexpected"); }
        Err(error) { println("OVERFLOW_DEADLINE_ERR"); }
    }
    match (Time.sleepMillis(0L)) {
        Ok(value) { println(value); }
        Err(error) { println("ZERO_SLEEP_ERR"); }
    }
    match (Time.sleepMillis(2L)) {
        Ok(value) { println(value); }
        Err(error) { println("SLEEP_ERR"); }
    }
    long after = 0L;
    match (Time.monotonicNanos()) {
        Ok(value) { after = value; }
        Err(error) { println("MONOTONIC_ERR"); }
    }
    println(after >= next);
    match (Time.sleepMillis(-1L)) {
        Ok(value) { println("unexpected"); }
        Err(error) { println("NEGATIVE_SLEEP_ERR"); }
    }
}
"#;
    let output = execute(source, &["-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"]);
    assert_native_success(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout),
        "true\ntrue\ntrue\nNEGATIVE_DEADLINE_ERR\nOVERFLOW_DEADLINE_ERR\n()\n()\ntrue\nNEGATIVE_SLEEP_ERR\n");
    assert_live_zero(&output);
}

#[test]
fn monotonic_clock_failure_returns_err_and_drops_text() {
    let source = r#"
void main() {
    match (Time.monotonicNanos()) {
        Ok(value) { println("unexpected"); }
        Err(error) { println("CLOCK_ERR"); }
    }
}
"#;
    let mut c = compile_to_c(source).unwrap();
    c.push_str(r#"
int ncr_test_clock_gettime(clockid_t clock_id, struct timespec *timestamp) {
    (void)clock_id; (void)timestamp;
    errno = EIO;
    return -1;
}
"#);
    let output = native::compile_c_with_flags(&c, &[
        "-DNACRE_MEMORY_STATS",
        "-Dclock_gettime=ncr_test_clock_gettime",
        "-fsanitize=address,undefined",
        "-fno-omit-frame-pointer",
    ]).unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output().unwrap_or_else(|error| panic!("native execution failed: {error}"));
    assert_native_success(&output);
    assert_eq!(output.stdout, b"CLOCK_ERR\n");
    assert_live_zero(&output);
}

#[test]
fn complete_language_example_runs_under_native_sanitizers() {
    let output = execute(EXAMPLE, &["-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"]);
    assert_native_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("17\nERR\nERR\n"), "{stdout}");
    assert!(stdout.contains("negative sleep rejected\n"), "{stdout}");
    assert_live_zero(&output);
}

#[test]
fn wall_and_monotonic_clock_units_are_converted_without_loss_or_overflow() {
    let source = r#"
void main() {
    match (Time.unixMillis()) {
        Ok(value) { println(value); }
        Err(error) { println("unexpected"); }
    }
    match (Time.monotonicNanos()) {
        Ok(value) { println(value); }
        Err(error) { println("unexpected"); }
    }
}
"#;
    let mut c = compile_to_c(source).unwrap();
    c.push_str(r#"
int ncr_test_clock_gettime(clockid_t clock_id, struct timespec *timestamp) {
    (void)clock_id;
    timestamp->tv_sec = (time_t)1500000000;
    timestamp->tv_nsec = 789000123L;
    return 0;
}
"#);
    let output = native::compile_c_with_flags(&c, &[
        "-DNACRE_MEMORY_STATS", "-Dclock_gettime=ncr_test_clock_gettime",
        "-fsanitize=address,undefined", "-fno-omit-frame-pointer",
    ]).unwrap().output().unwrap();
    assert_native_success(&output);
    assert_eq!(output.stdout, b"1500000000789\n1500000000789000123\n");
    assert_live_zero(&output);
}
