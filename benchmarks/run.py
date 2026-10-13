#!/usr/bin/env python3
"""Build, validate, and measure equivalent Nacre and Rust collection kernels."""

from __future__ import annotations

import argparse
import json
import os
import re
try:
    import resource
except ImportError:  # Windows has no wait4/resource support for per-child RSS.
    resource = None
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO, Sequence

ROOT = Path(__file__).resolve().parent.parent
BENCHMARK_DIR = ROOT / "benchmarks"
NACRE_SOURCE = BENCHMARK_DIR / "nacre_collections.ncr"
RUST_SOURCE = BENCHMARK_DIR / "rust_collections.rs"
MAX_SIZE = 40_000
MAX_EMPTY_QUERY_BYTES = 65_536
MAX_EMPTY_QUERY_REPEATS = 4_096
WAIT_POLL_INTERVAL_SECONDS = 0.0001


@dataclass(frozen=True)
class ChildResult:
    returncode: int
    stdout: bytes
    stderr: bytes
    elapsed_seconds: float
    peak_rss_bytes: int


def rss_bytes(usage: object) -> int:
    if resource is None:
        raise RuntimeError("per-child peak RSS requires Python's resource module")
    maximum = int(getattr(usage, "ru_maxrss"))
    if sys.platform.startswith("linux"):
        return maximum * 1024
    if sys.platform == "darwin":
        return maximum
    raise RuntimeError(
        "per-child ru_maxrss units are supported on Linux and macOS only"
    )


def _exit_code(status: int) -> int:
    if os.WIFEXITED(status):
        return os.WEXITSTATUS(status)
    if os.WIFSIGNALED(status):
        return -os.WTERMSIG(status)
    return 1


def _read_file(file: BinaryIO) -> bytes:
    file.seek(0)
    return file.read()


def run_child(
    executable: Path,
    arguments: Sequence[str],
    timeout_seconds: float,
) -> ChildResult:
    """Run one native child; wait4 supplies its individual peak RSS."""
    if not hasattr(os, "wait4"):
        raise RuntimeError("this runner needs POSIX os.wait4 for per-child peak RSS")

    with tempfile.TemporaryFile() as stdout_file, tempfile.TemporaryFile() as stderr_file:
        started = time.perf_counter()
        process = subprocess.Popen(
            [str(executable), *arguments],
            cwd=ROOT,
            stdout=stdout_file,
            stderr=stderr_file,
            start_new_session=True,
        )
        deadline = started + timeout_seconds
        timed_out = False
        while True:
            try:
                pid, status, usage = os.wait4(process.pid, os.WNOHANG)
            except InterruptedError:
                continue
            if pid == process.pid:
                elapsed = time.perf_counter() - started
                process.returncode = _exit_code(status)
                break
            if time.perf_counter() >= deadline:
                timed_out = True
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                while True:
                    try:
                        pid, status, usage = os.wait4(process.pid, 0)
                        if pid == process.pid:
                            break
                    except InterruptedError:
                        continue
                elapsed = time.perf_counter() - started
                process.returncode = _exit_code(status)
                break
            time.sleep(WAIT_POLL_INTERVAL_SECONDS)

        result = ChildResult(
            returncode=process.returncode,
            stdout=_read_file(stdout_file),
            stderr=_read_file(stderr_file),
            elapsed_seconds=elapsed,
            peak_rss_bytes=rss_bytes(usage),
        )
    if timed_out:
        raise RuntimeError(
            f"child exceeded its {timeout_seconds:g}s deadline: {executable.name}"
        )
    return result


def command_output(
    command: Sequence[str], timeout_seconds: float, *, cwd: Path = ROOT
) -> tuple[int, bytes, bytes]:
    """Capture a finite command and kill its process group if it times out."""
    process = subprocess.Popen(
        list(command),
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout_seconds)
    except subprocess.TimeoutExpired as error:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        stdout, stderr = process.communicate()
        raise RuntimeError(
            f"command exceeded its {timeout_seconds:g}s deadline: {Path(command[0]).name}"
        ) from error
    return process.returncode, stdout, stderr


def checked_command(command: Sequence[str], timeout_seconds: float) -> bytes:
    returncode, stdout, stderr = command_output(command, timeout_seconds)
    if returncode != 0:
        rendered = " ".join(command)
        raise RuntimeError(
            f"command failed ({returncode}): {rendered}\n"
            f"stdout:\n{stdout.decode('utf-8', errors='replace')}\n"
            f"stderr:\n{stderr.decode('utf-8', errors='replace')}"
        )
    return stdout


def version_line(executable: str, timeout_seconds: float) -> str:
    output = checked_command([executable, "--version"], timeout_seconds)
    lines = output.decode("utf-8", errors="replace").splitlines()
    line = lines[0].strip() if lines else "unknown"
    line = re.sub(r"(?<![A-Za-z0-9])/[^\s)]+", "<path>", line)
    return re.sub(
        r"(?i)\b(password|token|credential|secret)\s*[:=]\s*\S+",
        r"\1=<redacted>",
        line,
    )


def find_c_compiler(environment: dict[str, str], timeout_seconds: float) -> tuple[str, str]:
    selected = environment.get("NACRE_CC") or environment.get("CC")
    if selected is None:
        selected = next(
            (candidate for candidate in ("cc", "clang", "gcc") if shutil.which(candidate)),
            None,
        )
    if selected is None:
        raise RuntimeError("no C compiler found (set NACRE_CC or CC to cc, clang, or gcc)")
    try:
        version = version_line(selected, timeout_seconds)
    except RuntimeError as error:
        raise RuntimeError(f"selected C compiler is not runnable: {Path(selected).name}") from error
    return Path(selected).name, version


def native_build(
    nacre: Path,
    output: Path,
    timeout_seconds: float,
    *,
    memory_stats: bool,
) -> None:
    command = [
        str(nacre),
        "build",
        NACRE_SOURCE.relative_to(ROOT).as_posix(),
        "-O3",
        "-o",
        str(output),
    ]
    if memory_stats:
        command.append("--memory-stats")
    checked_command(command, timeout_seconds)


def rust_build(
    rustc: str,
    output: Path,
    timeout_seconds: float,
    *,
    allocation_stats: bool,
) -> None:
    command = [
        rustc,
        "--edition=2021",
        "-C",
        "opt-level=3",
        "-C",
        "overflow-checks=yes",
        "-C",
        "target-cpu=generic",
        "-C",
        "debuginfo=0",
    ]
    if allocation_stats:
        command.extend(["--cfg", "allocation_stats"])
    command.extend([str(RUST_SOURCE), "-o", str(output)])
    checked_command(command, timeout_seconds)


def expected_output(kernel: str, size: int) -> bytes:
    sum_zero_to_n_minus_one = size * (size - 1) // 2
    if kernel == "list_fill_traverse":
        lines = (sum_zero_to_n_minus_one, size)
    elif kernel == "map_int_insert_query_remove":
        value_sum = sum_zero_to_n_minus_one + 17 * size
        lines = (value_sum, value_sum, 0)
    elif kernel == "map_text_insert_query_replace":
        initial_sum = size * (size + 1) // 2
        final_sum = size * (size + 3) // 2
        lines = (initial_sum, initial_sum, final_sum, size)
    elif kernel == "empty_map_large_text_miss":
        lines = (0,)
    else:
        raise AssertionError(f"unknown benchmark kernel: {kernel}")
    return ("".join(f"{line}\n" for line in lines)).encode("ascii")


def kernel_arguments(
    kernel: str,
    size: int,
    empty_query_repeats: int,
    empty_query: str,
) -> list[str]:
    if kernel == "list_fill_traverse":
        return ["list", str(size), str(size)]
    if kernel == "map_int_insert_query_remove":
        return ["int-map", str(size), str(size)]
    if kernel == "map_text_insert_query_replace":
        return ["text-map", str(size), str(size)]
    if kernel == "empty_map_large_text_miss":
        return ["empty-text-miss", "0", str(empty_query_repeats), empty_query]
    raise AssertionError(f"unknown benchmark kernel: {kernel}")


def validate_run(
    result: ChildResult,
    implementation: str,
    kernel: str,
    expected: bytes,
) -> None:
    if result.returncode != 0:
        raise RuntimeError(
            f"{implementation} {kernel} exited {result.returncode}: "
            f"{result.stderr.decode('utf-8', errors='replace')}"
        )
    if result.stdout != expected:
        raise RuntimeError(
            f"{implementation} {kernel} produced unexpected output; "
            f"expected {expected!r}, got {result.stdout!r}; "
            f"stderr={result.stderr.decode('utf-8', errors='replace')}"
        )


def parse_memory_report(implementation: str, stderr: bytes) -> dict[str, int]:
    text = stderr.decode("utf-8", errors="replace")
    fields: dict[str, int] = {}
    for token in text.split():
        if "=" in token:
            name, value = token.split("=", 1)
            if value.isdecimal():
                fields[name] = int(value)
    if implementation == "nacre":
        required = {"allocations", "frees", "live"}
        marker = "NACRE_MEMORY"
    else:
        required = {
            "allocation_events",
            "alloc",
            "zeroed",
            "realloc",
            "dealloc",
            "live",
            "alloc_bytes",
            "realloc_bytes",
            "dealloc_bytes",
        }
        marker = "RUST_BENCH_MEMORY"
    if marker not in text or not required.issubset(fields):
        raise RuntimeError(
            f"missing {implementation} allocation report: {text.strip()!r}"
        )
    return {name: fields[name] for name in sorted(required)}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Build and run actual Nacre-generated C and equivalent Rust collection "
            "kernels; emit measurement JSON on stdout."
        )
    )
    parser.add_argument("--size", type=int, default=1_000, help="list items/map entries (1..40000)")
    parser.add_argument("--repeats", type=int, default=5, help="timed samples per implementation (1..100)")
    parser.add_argument("--warmups", type=int, default=2, help="untimed paired warmups (0..100)")
    parser.add_argument(
        "--empty-query-bytes",
        type=int,
        default=MAX_EMPTY_QUERY_BYTES,
        help="large ASCII Text key length for the empty-map miss case (1..65536)",
    )
    parser.add_argument(
        "--empty-query-repeats",
        type=int,
        help="queries in the empty-map case (default: min(size, 512); 1..4096)",
    )
    parser.add_argument(
        "--deadline-seconds",
        type=float,
        default=120.0,
        help="finite deadline for each build, version probe, and child process",
    )
    parser.add_argument(
        "--allocation-stats",
        action="store_true",
        help="run separate, untimed Nacre and Rust allocator-counter binaries",
    )
    parser.add_argument("--output", type=Path, help="write JSON here instead of stdout")
    args = parser.parse_args()

    if not 1 <= args.size <= MAX_SIZE:
        parser.error(f"--size must be between 1 and {MAX_SIZE}")
    if not 1 <= args.repeats <= 100:
        parser.error("--repeats must be between 1 and 100")
    if not 0 <= args.warmups <= 100:
        parser.error("--warmups must be between 0 and 100")
    if not 1 <= args.empty_query_bytes <= MAX_EMPTY_QUERY_BYTES:
        parser.error(f"--empty-query-bytes must be between 1 and {MAX_EMPTY_QUERY_BYTES}")
    if args.empty_query_repeats is None:
        args.empty_query_repeats = min(args.size, 512)
    if not 1 <= args.empty_query_repeats <= MAX_EMPTY_QUERY_REPEATS:
        parser.error(
            f"--empty-query-repeats must be between 1 and {MAX_EMPTY_QUERY_REPEATS}"
        )
    if not 0 < args.deadline_seconds <= 3_600:
        parser.error("--deadline-seconds must be greater than 0 and at most 3600")
    return args


def main() -> int:
    args = parse_args()
    if os.name != "posix" or resource is None or not hasattr(os, "wait4"):
        raise RuntimeError("per-child peak RSS requires Python resource and POSIX os.wait4")
    if not (sys.platform.startswith("linux") or sys.platform == "darwin"):
        raise RuntimeError("ru_maxrss normalization is supported on Linux and macOS only")

    environment = os.environ.copy()
    c_compiler_name, c_compiler_version = find_c_compiler(
        environment, args.deadline_seconds
    )
    rustc = shutil.which("rustc")
    cargo = shutil.which("cargo")
    if rustc is None or cargo is None:
        raise RuntimeError("both cargo and rustc must be available on PATH")

    checked_command(
        [cargo, "build", "--offline", "--locked", "--release"],
        args.deadline_seconds,
    )
    nacre = ROOT / "target" / "release" / ("nacre.exe" if os.name == "nt" else "nacre")
    if not nacre.is_file():
        raise RuntimeError("release Nacre compiler was not produced by cargo build")
    nacre_version = checked_command(
        [str(nacre), "--version"], args.deadline_seconds
    ).decode("utf-8", errors="replace").strip()
    rustc_version = version_line(rustc, args.deadline_seconds)
    cargo_version = version_line(cargo, args.deadline_seconds)
    rust_details = checked_command(
        [rustc, "-Vv"], args.deadline_seconds
    ).decode("utf-8", errors="replace")
    rust_host = next(
        (line.partition(":")[2].strip() for line in rust_details.splitlines() if line.startswith("host:")),
        "unknown",
    )

    kernels = (
        "list_fill_traverse",
        "map_int_insert_query_remove",
        "map_text_insert_query_replace",
        "empty_map_large_text_miss",
    )
    empty_query = "x" * args.empty_query_bytes

    with tempfile.TemporaryDirectory(prefix="nacre-bench-") as temporary:
        temporary_dir = Path(temporary)
        nacre_binary = temporary_dir / ("nacre-bench.exe" if os.name == "nt" else "nacre-bench")
        rust_binary = temporary_dir / ("rust-bench.exe" if os.name == "nt" else "rust-bench")
        native_build(nacre, nacre_binary, args.deadline_seconds, memory_stats=False)
        rust_build(rustc, rust_binary, args.deadline_seconds, allocation_stats=False)

        nacre_stats_binary: Path | None = None
        rust_stats_binary: Path | None = None
        if args.allocation_stats:
            nacre_stats_binary = temporary_dir / (
                "nacre-stats.exe" if os.name == "nt" else "nacre-stats"
            )
            rust_stats_binary = temporary_dir / (
                "rust-stats.exe" if os.name == "nt" else "rust-stats"
            )
            native_build(
                nacre,
                nacre_stats_binary,
                args.deadline_seconds,
                memory_stats=True,
            )
            rust_build(
                rustc,
                rust_stats_binary,
                args.deadline_seconds,
                allocation_stats=True,
            )

        expected_outputs = {
            kernel: expected_output(kernel, args.size) for kernel in kernels
        }
        arguments_by_kernel = {
            kernel: kernel_arguments(
                kernel,
                args.size,
                args.empty_query_repeats,
                empty_query,
            )
            for kernel in kernels
        }
        binaries = {"nacre": nacre_binary, "rust": rust_binary}

        # Validate both actual programs and their expected checksums before any timed sample.
        preflight_outputs: dict[str, bytes] = {}
        for kernel in kernels:
            for implementation in ("nacre", "rust"):
                result = run_child(
                    binaries[implementation],
                    arguments_by_kernel[kernel],
                    args.deadline_seconds,
                )
                validate_run(
                    result,
                    implementation,
                    kernel,
                    expected_outputs[kernel],
                )
                preflight_outputs[implementation] = result.stdout
            if preflight_outputs["nacre"] != preflight_outputs["rust"]:
                raise RuntimeError(f"Nacre/Rust output mismatch for {kernel}")

        measurements: dict[str, dict[str, object]] = {}
        for kernel in kernels:
            per_implementation: dict[str, dict[str, object]] = {
                implementation: {"samples": []}
                for implementation in ("nacre", "rust")
            }
            total_trials = args.warmups + args.repeats
            for trial in range(total_trials):
                order = ("nacre", "rust") if trial % 2 == 0 else ("rust", "nacre")
                trial_outputs: dict[str, bytes] = {}
                for implementation in order:
                    result = run_child(
                        binaries[implementation],
                        arguments_by_kernel[kernel],
                        args.deadline_seconds,
                    )
                    validate_run(
                        result,
                        implementation,
                        kernel,
                        expected_outputs[kernel],
                    )
                    trial_outputs[implementation] = result.stdout
                    if trial >= args.warmups:
                        per_implementation[implementation]["samples"].append(
                            {
                                "elapsed_seconds": round(result.elapsed_seconds, 9),
                                "peak_rss_bytes": result.peak_rss_bytes,
                            }
                        )
                if trial_outputs["nacre"] != trial_outputs["rust"]:
                    raise RuntimeError(f"Nacre/Rust output mismatch for {kernel}")

            for implementation in ("nacre", "rust"):
                samples = per_implementation[implementation]["samples"]
                elapsed = [sample["elapsed_seconds"] for sample in samples]
                per_implementation[implementation].update(
                    {
                        "sample_count": len(samples),
                        "median_elapsed_seconds": round(statistics.median(elapsed), 9),
                        "min_elapsed_seconds": min(elapsed),
                        "max_elapsed_seconds": max(elapsed),
                    }
                )
            measurements[kernel] = {
                "validated_stdout_lines": expected_outputs[kernel]
                .decode("ascii")
                .splitlines(),
                "dimensions": {
                    "collection_size": args.size,
                    "empty_query_bytes": (
                        args.empty_query_bytes
                        if kernel == "empty_map_large_text_miss"
                        else None
                    ),
                    "empty_query_repeats": (
                        args.empty_query_repeats
                        if kernel == "empty_map_large_text_miss"
                        else None
                    ),
                },
                "warmups_per_implementation": args.warmups,
                "implementations": per_implementation,
            }

        allocation_measurements: dict[str, dict[str, dict[str, int]]] | None = None
        if args.allocation_stats:
            assert nacre_stats_binary is not None and rust_stats_binary is not None
            stats_binaries = {"nacre": nacre_stats_binary, "rust": rust_stats_binary}
            allocation_measurements = {}
            for kernel in kernels:
                stats_for_kernel: dict[str, dict[str, int]] = {}
                outputs: dict[str, bytes] = {}
                for implementation in ("nacre", "rust"):
                    result = run_child(
                        stats_binaries[implementation],
                        arguments_by_kernel[kernel],
                        args.deadline_seconds,
                    )
                    validate_run(
                        result,
                        implementation,
                        kernel,
                        expected_outputs[kernel],
                    )
                    outputs[implementation] = result.stdout
                    stats_for_kernel[implementation] = parse_memory_report(
                        implementation, result.stderr
                    )
                if outputs["nacre"] != outputs["rust"]:
                    raise RuntimeError(f"instrumented output mismatch for {kernel}")
                allocation_measurements[kernel] = stats_for_kernel

    compiler_flags = {
        "nacre_compiler_build": [
            "cargo",
            "build",
            "--offline",
            "--locked",
            "--release",
        ],
        "nacre_native_c": [
            "-O3",
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-pedantic",
            "-Wno-unused-function",
            "-Wno-unused-variable",
            "-Wno-unused-parameter",
            "-Wno-unused-but-set-variable",
        ],
        "nacre_allocation_run_extra": ["-DNACRE_MEMORY_STATS"],
        "rust_reference": [
            "--edition=2021",
            "-C opt-level=3",
            "-C overflow-checks=yes",
            "-C target-cpu=generic",
            "-C debuginfo=0",
        ],
        "rust_allocation_run_extra": ["--cfg allocation_stats"],
        "cpu_policy": (
            "No machine-specific CPU flags: generated C uses the selected C compiler's "
            "default target CPU (no -march=native/-mcpu override); Rust uses "
            "-C target-cpu=generic. The compiler/target defaults may differ by toolchain."
        ),
    }
    payload: dict[str, object] = {
        "schema_version": 1,
        "toolchains": {
            "nacre": nacre_version,
            "c_compiler_name": c_compiler_name,
            "c_compiler_version": c_compiler_version,
            "cargo": cargo_version,
            "rustc": rustc_version,
            "rust_target": rust_host,
            "python": sys.version.split()[0],
        },
        "flags": compiler_flags,
        "arithmetic": {
            "nacre_i32": (
                "Defined wrapping 32-bit arithmetic; generated C uses widened helpers "
                "and explicit 32-bit reduction rather than signed-overflow UB."
            ),
            "rust_i32": (
                "-C overflow-checks=yes guards accidental plain arithmetic overflow; "
                "wrapping operations are used where Nacre defines wrapping behavior. "
                "Accepted dimensions keep workload checksums in i32 range."
            ),
        },
        "hashing": {
            "nacre_int": (
                "i32 bit pattern followed by Nacre's 32-bit xor/multiply finalizer "
                "(0x7feb352d, 0x846ca68b)."
            ),
            "nacre_text": (
                "FNV-1a over UTF-8 bytes, XOR byte length, then the same Nacre "
                "32-bit finalizer."
            ),
            "rust_reference": (
                "Custom NacreHasher mirrors those exact 32-bit key hashes and widens "
                "the hash into both halves of Rust's u64 Hasher result; not SipHash."
            ),
            "standard_rust_hashmap_hasher_baseline_included": False,
            "empty_map_large_text_case": (
                "Nacre Map.containsKey reaches ncr_map_query, which returns when map "
                "length is zero before calling ncr_map_hash; each child receives the "
                "configured large ASCII Text query."
            ),
        },
        "measurement": {
            "elapsed_seconds": (
                "perf_counter wall time from immediately before Popen through wait4 reap; "
                "includes process startup, argument parsing, kernel, checksum output, "
                "and shutdown; excludes compilation."
            ),
            "peak_rss_bytes": (
                "Per-child wait4 RUSAGE_CHILDREN ru_maxrss, normalized to bytes "
                "(Linux KiB x 1024; macOS bytes); includes setup and output."
            ),
            "timed_printing": "No printing occurs inside collection/query loops; checksums print after each kernel.",
            "wait4_poll_interval_seconds": WAIT_POLL_INTERVAL_SECONDS,
            "checksum_validation": (
                "Both actual programs' stdout is checked against independently calculated "
                "expected lines before warmups and timed samples."
            ),
            "allocation_stats": (
                "Optional separate, untimed runs. Nacre counts ncr_allocate/ncr_deallocate "
                "calls; Rust counts successful GlobalAlloc alloc/zeroed/realloc/dealloc "
                "calls and requested bytes. Counters cover the whole child including "
                "argument handling/output; Nacre counters exclude libc allocations, while "
                "Rust's hook sees Rust global-allocator calls. "
                "Rust live is the hook's live-block count at its report snapshot and "
                "can include process-global runtime allocations; it is not a leak verdict."
            ),
            "deadline_seconds_per_child_or_build": args.deadline_seconds,
        },
        "comparison_limits": [
            "Elapsed samples are process-level; startup and argument parsing can dominate the smallest kernels.",
            "Hash inputs/mixers match, but Nacre Map and Rust HashMap retain distinct bucket, growth, and probing policies.",
            "Peak RSS includes process startup, arguments, setup, and output; it is not kernel-only memory.",
            "Allocation scopes differ: Nacre counters cover ncr_allocate/ncr_deallocate, Rust covers Rust GlobalAlloc calls, and both include process setup.",
        ],
        "configuration": {
            "size": args.size,
            "warmups": args.warmups,
            "repeats": args.repeats,
            "deadline_seconds": args.deadline_seconds,
            "allocation_stats_enabled": args.allocation_stats,
        },
        "dimensions": {
            "list_items": args.size,
            "integer_map_entries_queries_removals": args.size,
            "text_map_entries_queries_replacements": args.size,
            "large_text_query_bytes": args.empty_query_bytes,
            "empty_map_miss_repeats": args.empty_query_repeats,
        },
        "kernels": measurements,
        "allocation_runs_untimed": allocation_measurements,
    }
    serialized = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    if args.output is None:
        sys.stdout.write(serialized)
    else:
        args.output.write_text(serialized, encoding="utf-8")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"benchmark runner: {error}", file=sys.stderr)
        raise SystemExit(1)
