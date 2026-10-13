#define _DEFAULT_SOURCE 1
#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#if !defined(__linux__) && !defined(__APPLE__)
#error "wait4_supervisor supports Linux and macOS"
#endif

static int write_all(int fd, const char *bytes, size_t length) {
    size_t written = 0;
    while (written < length) {
        ssize_t result = write(fd, bytes + written, length - written);
        if (result > 0) {
            written += (size_t)result;
        } else if (result < 0 && errno == EINTR) {
            continue;
        } else {
            return -1;
        }
    }
    return 0;
}

static int64_t elapsed_nanoseconds(const struct timespec *start, const struct timespec *end) {
    int64_t seconds = (int64_t)end->tv_sec - (int64_t)start->tv_sec;
    int64_t nanoseconds = (int64_t)end->tv_nsec - (int64_t)start->tv_nsec;
    return seconds * INT64_C(1000000000) + nanoseconds;
}

static uint64_t peak_rss_bytes(const struct rusage *usage) {
#if defined(__linux__)
    return (uint64_t)usage->ru_maxrss * UINT64_C(1024);
#else
    return (uint64_t)usage->ru_maxrss;
#endif
}

int main(int argc, char **argv) {
    if (argc < 3) {
        (void)fprintf(stderr, "wait4 supervisor: expected control fd and target executable\n");
        return 125;
    }

    char *end = NULL;
    errno = 0;
    long parsed_fd = strtol(argv[1], &end, 10);
    if (errno != 0 || end == argv[1] || *end != '\0' || parsed_fd < 0 || parsed_fd > INT32_MAX) {
        (void)fprintf(stderr, "wait4 supervisor: invalid control fd\n");
        return 125;
    }
    int control_fd = (int)parsed_fd;

    struct timespec start;
    if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) {
        (void)fprintf(stderr, "wait4 supervisor: clock_gettime failed: %s\n", strerror(errno));
        return 125;
    }

    pid_t child = fork();
    if (child < 0) {
        (void)fprintf(stderr, "wait4 supervisor: fork failed: %s\n", strerror(errno));
        return 125;
    }
    if (child == 0) {
        (void)close(control_fd);
        execvp(argv[2], &argv[2]);
        (void)dprintf(STDERR_FILENO, "wait4 supervisor: exec failed: %s\n", strerror(errno));
        _exit(127);
    }

    int status = 0;
    struct rusage usage;
    pid_t waited;
    do {
        waited = wait4(child, &status, 0, &usage);
    } while (waited < 0 && errno == EINTR);
    if (waited != child) {
        (void)fprintf(stderr, "wait4 supervisor: wait4 failed: %s\n", strerror(errno));
        return 125;
    }

    struct timespec finish;
    if (clock_gettime(CLOCK_MONOTONIC, &finish) != 0) {
        (void)fprintf(stderr, "wait4 supervisor: clock_gettime failed: %s\n", strerror(errno));
        return 125;
    }

    int target_exit_code;
    int target_signal = 0;
    if (WIFEXITED(status)) {
        target_exit_code = WEXITSTATUS(status);
    } else if (WIFSIGNALED(status)) {
        target_signal = WTERMSIG(status);
        target_exit_code = 128 + target_signal;
    } else {
        target_exit_code = 125;
    }

    char report[256];
    int report_length = snprintf(
        report,
        sizeof(report),
        "BENCH_CHILD elapsed_ns=%" PRId64 " peak_rss_bytes=%" PRIu64 " exit_code=%d signal=%d\n",
        elapsed_nanoseconds(&start, &finish),
        peak_rss_bytes(&usage),
        target_exit_code,
        target_signal
    );
    if (report_length < 0 || (size_t)report_length >= sizeof(report)
        || write_all(control_fd, report, (size_t)report_length) != 0) {
        (void)fprintf(stderr, "wait4 supervisor: failed to report child measurements\n");
        return 125;
    }
    (void)close(control_fd);
    return 0;
}
