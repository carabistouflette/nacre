#!/usr/bin/env python3
"""Build, validate, and measure equivalent Nacre/Rust Option1 workloads."""
from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shutil
import statistics
import sys
import tempfile
import time
from collections import defaultdict
from pathlib import Path

import run as process_harness

ROOT = Path(__file__).resolve().parent.parent
BENCHMARK_DIR = ROOT / "benchmarks"
NACRE_SOURCE = BENCHMARK_DIR / "suite_nacre.ncr"
RUST_SOURCE = BENCHMARK_DIR / "suite_rust.rs"
WORKLOADS = ("json", "string", "binary", "hashmap", "sudoku", "game_of_life")
NACRE_LINE = re.compile(
    r"test\s+(bench_[A-Za-z0-9_]+)\s+\.\.\.\s+bench:\s+"
    r"([0-9,]+)\s+ns/iter\s+\(\+/-\s+([0-9,]+)\)\s+"
    r"\(([0-9,]+)\s+ops/sec\)"
)

WORKLOAD_DEFINITIONS: dict[str, dict[str, object]] = {
    "json": {
        "work_units_per_call": 3,
        "unit": "JSON records parsed",
        "input": "Three fixed ASCII objects with id, name, and active fields; varied spaces/tabs/newlines plus escaped quote, newline, and Unicode code point in names; parses array/object structure, ordered keys, integers, and booleans.",
    },
    "string": {
        "work_units_per_call": len(
            b"nacre::memory::benchmarks::string::search::stream::nacre::compiler"
        ),
        "unit": "input bytes transformed, split, and searched",
        "input": "Fixed ASCII string; replace '::' with '|', split, search each field for 'nacre'.",
    },
    "binary": {
        "work_units_per_call": 4096,
        "unit": "payload bytes streamed and round-tripped",
        "input": "4096 deterministic bytes; 257-byte in-memory stream chunks; lowercase hex encode/decode.",
    },
    "hashmap": {
        "work_units_per_call": 2048 * 5,
        "unit": "map operations",
        "input": "2048 i32 keys: insert, lookup, replace, lookup, remove (five operations per entry).",
    },
    "sudoku": {
        "work_units_per_call": 1,
        "unit": "Sudoku puzzles solved",
        "input": "Fixed 9x9 puzzle copied from examples/apps/sudoku.ncr; backtracking solver.",
    },
    "game_of_life": {
        "work_units_per_call": 100 * 100 * 100,
        "unit": "cell transitions",
        "input": "Deterministic 100x100 toroidal grid, 100 B3/S23 generations.",
    },
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare six matched native Nacre and Rust workloads; emit JSON."
    )
    parser.add_argument("--repeats", type=int, default=5, help="fresh benchmark command runs per implementation (1..20)")
    parser.add_argument("--rss-repeats", type=int, default=3, help="fresh per-workload child processes for RSS (1..20)")
    parser.add_argument("--deadline-seconds", type=float, default=120.0, help="deadline for each build/child process")
    parser.add_argument("--output", type=Path, help="write JSON here instead of stdout")
    args = parser.parse_args()
    if not 1 <= args.repeats <= 20:
        parser.error("--repeats must be between 1 and 20")
    if not 1 <= args.rss_repeats <= 20:
        parser.error("--rss-repeats must be between 1 and 20")
    if not 0 < args.deadline_seconds <= 3600:
        parser.error("--deadline-seconds must be greater than 0 and at most 3600")
    return args


def checked_run(
    supervisor: Path,
    executable: Path,
    arguments: list[str],
    deadline_seconds: float,
    *,
    implementation: str,
    expected_stdout: bytes,
) -> process_harness.ChildResult:
    result = process_harness.run_child(supervisor, executable, arguments, deadline_seconds)
    if result.returncode != 0:
        raise RuntimeError(
            f"{implementation} {arguments!r} exited {result.returncode}: "
            f"{result.stderr.decode('utf-8', errors='replace')}"
        )
    if result.stdout != expected_stdout:
        raise RuntimeError(
            f"{implementation} {arguments!r} stdout mismatch: "
            f"expected {expected_stdout!r}, got {result.stdout!r}"
        )
    return result


def parse_results(
    stdout: bytes, expected_names: set[str] | tuple[str, ...] = WORKLOADS
) -> dict[str, int]:
    results: dict[str, int] = {}
    for line in stdout.decode("utf-8", errors="replace").splitlines():
        fields = line.split()
        if len(fields) == 3 and fields[0] == "RESULT":
            try:
                results[fields[1]] = int(fields[2])
            except ValueError as error:
                raise RuntimeError(f"invalid checksum output line: {line!r}") from error
    if set(results) != set(expected_names):
        raise RuntimeError(
            f"suite output expected {sorted(expected_names)}, got {sorted(results)}"
        )
    return results


def expected_line(name: str, checksum: int, repetitions: int) -> bytes:
    return f"RESULT {name} {checksum * repetitions}\n".encode("ascii")


def parse_nacre_bench(stdout: bytes) -> dict[str, dict[str, int]]:
    measurements: dict[str, dict[str, int]] = {}
    text = stdout.decode("utf-8", errors="replace")
    for line in text.splitlines():
        match = NACRE_LINE.search(line)
        if match is None:
            continue
        name = match.group(1).removeprefix("bench_")
        measurements[name] = {
            "mean_ns_per_call": int(match.group(2).replace(",", "")),
            "max_deviation_ns": int(match.group(3).replace(",", "")),
            "reported_calls_per_second": int(match.group(4).replace(",", "")),
        }
    if set(measurements) != set(WORKLOADS):
        raise RuntimeError(
            "nacre bench did not report all six wrappers: "
            f"{sorted(measurements)}; output={text!r}"
        )
    return measurements


def parse_rust_bench(stdout: bytes) -> dict[str, dict[str, object]]:
    samples: dict[str, list[int]] = defaultdict(list)
    results: dict[str, dict[str, int]] = {}
    checksums: dict[str, int] = {}
    for line in stdout.decode("utf-8", errors="replace").splitlines():
        fields = line.split()
        if len(fields) == 3 and fields[0] == "BENCH_SAMPLE:":
            samples[fields[1]].append(int(fields[2]))
        elif len(fields) == 5 and fields[0] == "BENCH_RESULT:":
            results[fields[1]] = {
                "mean_ns_per_call": int(fields[2]),
                "max_deviation_ns": int(fields[3]),
                "reported_calls_per_second": int(fields[4]),
            }
        elif len(fields) == 3 and fields[0] == "BENCH_CHECKSUM:":
            checksums[fields[1]] = int(fields[2])
    if set(samples) != set(WORKLOADS) or set(results) != set(WORKLOADS) or set(checksums) != set(WORKLOADS):
        raise RuntimeError(
            "Rust benchmark output was incomplete: "
            f"samples={sorted(samples)}, results={sorted(results)}, checksums={sorted(checksums)}"
        )
    for name in WORKLOADS:
        if len(samples[name]) != 5:
            raise RuntimeError(f"Rust {name} expected five raw inner samples, got {samples[name]}")
    return {
        name: {
            **results[name],
            "raw_inner_samples_ns": samples[name],
            "checksum": checksums[name],
        }
        for name in WORKLOADS
    }


def timed_command(command: list[str], deadline_seconds: float) -> float:
    started = time.perf_counter()
    process_harness.checked_command(command, deadline_seconds)
    return time.perf_counter() - started


def build_nacre(
    nacre: Path,
    executable: Path,
    deadline_seconds: float,
    *,
    memory_stats: bool,
) -> float:
    command = [
        str(nacre),
        "build",
        NACRE_SOURCE.relative_to(ROOT).as_posix(),
        "-O3",
        "-o",
        str(executable),
        "--native",
    ]
    if memory_stats:
        command.append("--memory-stats")
    return timed_command(command, deadline_seconds)


def build_rust(
    rustc: str,
    executable: Path,
    deadline_seconds: float,
    *,
    allocation_stats: bool,
) -> float:
    command = [
        rustc,
        "--edition=2024",
        "-O",
        "-C",
        "target-cpu=native",
    ]
    if allocation_stats:
        command.extend(["--cfg", "allocation_stats"])
    command.extend([str(RUST_SOURCE), "-o", str(executable)])
    return timed_command(command, deadline_seconds)


def run_nacre_bench(
    nacre: Path, deadline_seconds: float
) -> tuple[dict[str, dict[str, int]], float]:
    command = [str(nacre), "bench", NACRE_SOURCE.relative_to(ROOT).as_posix(), "--native"]
    started = time.perf_counter()
    output = process_harness.checked_command(command, deadline_seconds)
    elapsed = time.perf_counter() - started
    return parse_nacre_bench(output), elapsed


def run_rust_bench(
    supervisor: Path, executable: Path, deadline_seconds: float
) -> tuple[dict[str, dict[str, object]], process_harness.ChildResult]:
    result = process_harness.run_child(supervisor, executable, ["bench"], deadline_seconds)
    if result.returncode != 0:
        raise RuntimeError(
            f"Rust benchmark executable failed: {result.stderr.decode('utf-8', errors='replace')}"
        )
    return parse_rust_bench(result.stdout), result


def main() -> int:
    args = parse_args()
    # Measure compilation, not Nacre's executable-cache lookup/copy path.
    os.environ["NACRE_NO_CACHE"] = "1"
    if os.name != "posix" or not (sys.platform.startswith("linux") or sys.platform == "darwin"):
        raise RuntimeError("the wait4 supervisor requires POSIX Linux or macOS")

    nacre = ROOT / "target" / "release" / "nacre"
    if not nacre.is_file():
        raise RuntimeError("target/release/nacre is required; this runner does not rebuild the compiler")
    rustc = shutil.which("rustc")
    if rustc is None:
        raise RuntimeError("rustc is not available on PATH")
    cc_name, cc_version, c_compiler = process_harness.find_c_compiler(
        os.environ.copy(), args.deadline_seconds
    )
    nacre_version = process_harness.checked_command(
        [str(nacre), "--version"], args.deadline_seconds
    ).decode("utf-8", errors="replace").strip()
    rustc_version = process_harness.version_line(rustc, args.deadline_seconds)
    rust_details = process_harness.checked_command([rustc, "-Vv"], args.deadline_seconds).decode(
        "utf-8", errors="replace"
    )
    rust_host = next(
        (line.partition(":")[2].strip() for line in rust_details.splitlines() if line.startswith("host:")),
        "unknown",
    )

    with tempfile.TemporaryDirectory(prefix="nacre-option1-") as temporary:
        temporary_dir = Path(temporary)
        supervisor = temporary_dir / "wait4-supervisor"
        process_harness.supervisor_build(c_compiler, supervisor, args.deadline_seconds)

        nacre_binary = temporary_dir / "nacre-suite"
        rust_binary = temporary_dir / "rust-suite"
        nacre_stats_binary = temporary_dir / "nacre-suite-stats"
        rust_stats_binary = temporary_dir / "rust-suite-stats"
        build_steps = [
            (
                "nacre_native",
                lambda: build_nacre(
                    nacre, nacre_binary, args.deadline_seconds, memory_stats=False
                ),
            ),
            (
                "rust_native",
                lambda: build_rust(
                    rustc, rust_binary, args.deadline_seconds, allocation_stats=False
                ),
            ),
            (
                "nacre_with_memory_stats",
                lambda: build_nacre(
                    nacre, nacre_stats_binary, args.deadline_seconds, memory_stats=True
                ),
            ),
            (
                "rust_with_allocation_stats",
                lambda: build_rust(
                    rustc, rust_stats_binary, args.deadline_seconds, allocation_stats=True
                ),
            ),
        ]
        compile_samples: dict[str, list[float]] = {
            name: [] for name, _build in build_steps
        }
        for trial in range(args.repeats):
            ordered_steps = build_steps[trial % len(build_steps):] + build_steps[:trial % len(build_steps)]
            for name, build in ordered_steps:
                compile_samples[name].append(build())
        compile_seconds = {
            name: round(statistics.median(samples), 9)
            for name, samples in compile_samples.items()
        }
        compile_seconds_samples = {
            name: [round(sample, 9) for sample in samples]
            for name, samples in compile_samples.items()
        }

        checksums: dict[str, dict[str, int]] = {"nacre": {}, "rust": {}}
        for name in WORKLOADS:
            outputs: dict[str, bytes] = {}
            for implementation, executable in (("nacre", nacre_binary), ("rust", rust_binary)):
                result = process_harness.run_child(
                    supervisor, executable, [name, "1"], args.deadline_seconds
                )
                if result.returncode != 0:
                    raise RuntimeError(
                        f"{implementation} preflight {name} exited {result.returncode}: "
                        f"{result.stderr.decode('utf-8', errors='replace')}"
                    )
                output_results = parse_results(result.stdout, {name})
                checksum = output_results[name]
                if checksum < 0:
                    raise RuntimeError(f"{implementation} preflight {name} returned a failure checksum")
                checksums[implementation][name] = checksum
                outputs[implementation] = result.stdout
            if outputs["nacre"] != outputs["rust"]:
                raise RuntimeError(
                    f"Nacre/Rust checksum mismatch for {name}: "
                    f"{outputs['nacre']!r} != {outputs['rust']!r}"
                )

        timing_runs: dict[str, dict[str, list[dict[str, object]]]] = {
            "nacre": {name: [] for name in WORKLOADS},
            "rust": {name: [] for name in WORKLOADS},
        }
        bench_processes: dict[str, list[dict[str, object]]] = {"nacre": [], "rust": []}
        for trial in range(args.repeats):
            order = ("nacre", "rust") if trial % 2 == 0 else ("rust", "nacre")
            for implementation in order:
                if implementation == "nacre":
                    measurements, elapsed = run_nacre_bench(nacre, args.deadline_seconds)
                    bench_processes["nacre"].append(
                        {"command_elapsed_seconds": round(elapsed, 9), "reported_all_six": True}
                    )
                    for name in WORKLOADS:
                        timing_runs["nacre"][name].append(
                            {
                                **measurements[name],
                                "command_elapsed_seconds": round(elapsed, 9),
                            }
                        )
                else:
                    measurements, result = run_rust_bench(
                        supervisor, rust_binary, args.deadline_seconds
                    )
                    bench_processes["rust"].append(
                        {
                            "process_elapsed_seconds": round(result.elapsed_seconds, 9),
                            "peak_rss_bytes": result.peak_rss_bytes,
                        }
                    )
                    for name in WORKLOADS:
                        if measurements[name]["checksum"] != checksums["nacre"][name]:
                            raise RuntimeError(
                                f"Rust timed workload {name} checksum differed from preflight"
                            )
                        timing_runs["rust"][name].append(
                            {
                                **measurements[name],
                                "process_elapsed_seconds": round(result.elapsed_seconds, 9),
                            }
                        )

        runtime_measurements: dict[str, dict[str, dict[str, object]]] = {
            "nacre": {},
            "rust": {},
        }
        rss_measurements: dict[str, dict[str, dict[str, object]]] = {
            "nacre": {},
            "rust": {},
        }
        allocation_measurements: dict[str, dict[str, dict[str, int]]] = {
            "nacre": {},
            "rust": {},
        }

        for name in WORKLOADS:
            work_units = int(WORKLOAD_DEFINITIONS[name]["work_units_per_call"])
            nacre_means = [
                int(row["mean_ns_per_call"]) for row in timing_runs["nacre"][name]
            ]
            rss_repetitions = max(
                1, min(20_000, (20_000_000 + int(statistics.median(nacre_means)) - 1)
                       // max(int(statistics.median(nacre_means)), 1))
            )

            process_samples: list[dict[str, object]] = []
            for trial in range(args.rss_repeats):
                for implementation, executable in (
                    ("nacre", nacre_binary),
                    ("rust", rust_binary),
                ):
                    result = checked_run(
                        supervisor,
                        executable,
                        [name, str(rss_repetitions)],
                        args.deadline_seconds,
                        implementation=implementation,
                        expected_stdout=expected_line(
                            name, checksums["nacre"][name], rss_repetitions
                        ),
                    )
                    process_samples.append(
                        {
                            "implementation": implementation,
                            "trial": trial + 1,
                            "kernel_calls": rss_repetitions,
                            "elapsed_seconds": round(result.elapsed_seconds, 9),
                            "peak_rss_bytes": result.peak_rss_bytes,
                        }
                    )

            for implementation in ("nacre", "rust"):
                samples = [
                    sample
                    for sample in process_samples
                    if sample["implementation"] == implementation
                ]
                rss_values = [int(sample["peak_rss_bytes"]) for sample in samples]
                elapsed_values = [float(sample["elapsed_seconds"]) for sample in samples]
                rss_measurements[implementation][name] = {
                    "kernel_calls_per_process": rss_repetitions,
                    "raw_peak_rss_bytes_samples": rss_values,
                    "median_peak_rss_bytes": int(statistics.median(rss_values)),
                    "raw_process_elapsed_seconds_samples": elapsed_values,
                    "median_process_elapsed_seconds": round(
                        statistics.median(elapsed_values), 9
                    ),
                    "process_scope": (
                        "one fresh direct workload child under wait4; includes startup, "
                        "arguments, kernel calls, output, and shutdown"
                    ),
                }

            for implementation, executable in (
                ("nacre", nacre_stats_binary),
                ("rust", rust_stats_binary),
            ):
                stats_result = process_harness.run_child(
                    supervisor, executable, [name, "1"], args.deadline_seconds
                )
                if stats_result.returncode != 0:
                    raise RuntimeError(
                        f"{implementation} allocation run {name} exited "
                        f"{stats_result.returncode}: "
                        f"{stats_result.stderr.decode('utf-8', errors='replace')}"
                    )
                expected = expected_line(name, checksums["nacre"][name], 1)
                if stats_result.stdout != expected:
                    raise RuntimeError(f"{implementation} allocation run {name} checksum mismatch")
                allocation_measurements[implementation][name] = (
                    process_harness.parse_memory_report(
                        implementation, stats_result.stderr
                    )
                )

            for implementation in ("nacre", "rust"):
                rows = timing_runs[implementation][name]
                per_run_means = [int(row["mean_ns_per_call"]) for row in rows]
                if any(sample <= 0 for sample in per_run_means):
                    raise RuntimeError(f"{implementation} {name} produced a zero timing sample")
                run_throughputs = [
                    (1_000_000_000 * work_units) / sample
                    for sample in per_run_means
                ]
                if implementation == "nacre":
                    raw_samples = per_run_means
                    raw_scope = (
                        "one built-in five-sample arithmetic mean per fresh nacre bench command"
                    )
                    inner_samples: list[list[int]] | None = None
                else:
                    inner_samples = [
                        list(row["raw_inner_samples_ns"]) for row in rows
                    ]
                    raw_samples = [
                        sample for group in inner_samples for sample in group
                    ]
                    raw_scope = (
                        "five Rust timed samples per process; runtime median is the "
                        "median of five process-level arithmetic means"
                    )
                median_ns = int(statistics.median(per_run_means))
                runtime_measurements[implementation][name] = {
                    "raw_run_mean_ns_per_call": per_run_means,
                    "median_ns_per_call": median_ns,
                    "median_kernel_calls_per_second": int(1_000_000_000 / median_ns),
                    "median_work_units_per_second": int(
                        statistics.median(run_throughputs)
                    ),
                    "work_units_per_call": work_units,
                    "reported_calls_per_second_each_run": [
                        int(row["reported_calls_per_second"]) for row in rows
                    ],
                    "reported_max_deviation_ns_each_run": [
                        int(row["max_deviation_ns"]) for row in rows
                    ],
                    "raw_sample_scope": raw_scope,
                    "raw_samples_ns_per_call": raw_samples,
                    "raw_inner_samples_ns_per_call": inner_samples,
                }

        workload_results: dict[str, dict[str, object]] = {}
        for name in WORKLOADS:
            workload_results[name] = {
                **WORKLOAD_DEFINITIONS[name],
                "validated_checksum": checksums["nacre"][name],
                "implementations": {
                    impl: {
                        "runtime": runtime_measurements[impl][name],
                        "per_process": rss_measurements[impl][name],
                        "allocation_stats_untimed_one_call": allocation_measurements[impl][name],
                    }
                    for impl in ("nacre", "rust")
                },
            }

        payload: dict[str, object] = {
            "schema_version": 1,
            "suite": "Option1 named Nacre/Rust workloads",
            "toolchains": {
                "nacre": nacre_version,
                "rustc": rustc_version,
                "rust_target": rust_host,
                "c_compiler_name": cc_name,
                "c_compiler_version": cc_version,
                "python": sys.version.split()[0],
                "platform": platform.platform(),
                "machine": platform.machine(),
            },
            "commands_and_flags": {
                "nacre_environment": {"NACRE_NO_CACHE": "1"},
                "nacre_benchmark": [str(nacre), "bench", "benchmarks/suite_nacre.ncr", "--native"],
                "nacre_benchmark_native_c": ["-O3", "-DNACRE_MEMORY_STATS", "-march=native"],
                "nacre_direct_build": [str(nacre), "build", "benchmarks/suite_nacre.ncr", "-O3", "-o", "<unique-temporary-file>", "--native"],
                "nacre_direct_build_memory_stats_extra": ["--memory-stats"],
                "rust_native_compile": ["rustc", "--edition=2024", "-O", "-C", "target-cpu=native", "benchmarks/suite_rust.rs", "-o", "<unique-temporary-file>"],
                "rust_allocation_stats_extra": ["--cfg", "allocation_stats"],
                "measurement_supervisor": [c_compiler, "-O2", "-std=c11", "-Wall", "-Wextra", "-pedantic", "benchmarks/wait4_supervisor.c", "-o", "<unique-temporary-file>"],
                "cpu_policy": "Nacre generated C and Rust both target the host CPU; no CPU affinity or frequency policy is set.",
            },
            "compile_elapsed_seconds": compile_seconds,
            "compile_elapsed_seconds_samples": compile_seconds_samples,
            "measurement": {
                "repeats": args.repeats,
                "compile_repeats": args.repeats,
                "compile_source": (
                    f"Median wall-clock time across {args.repeats} independent compile-and-link "
                    "commands per build variant with the native executable cache disabled; "
                    "includes compiler process startup and output I/O, not rebuilding Nacre itself."
                ),
                "rss_repeats": args.rss_repeats,
                "warmups": 3,
                "timed_inner_samples": 5,
                "nacre_calibration": "built-in nacre bench grows call count by 10 until one calibration batch reaches 20 ms, then reports the arithmetic mean of five per-call samples",
                "rust_calibration": "Rust runner mirrors three warmups, 20 ms calibration target, factor-10 count growth, and five per-call samples; raw five samples are retained per run",
                "nacre_bench_command_elapsed_seconds_samples": [
                    float(sample["command_elapsed_seconds"]) for sample in bench_processes["nacre"]
                ],
                "rust_bench_process_samples": bench_processes["rust"],
                "runtime_source": f"Nacre and Rust values are each the median of {args.repeats} fresh built-in bench/process means. These are per-kernel call times, not process launch times.",
                "peak_rss_source": "wait4 ru_maxrss of fresh direct one-workload child processes; platform value converted to bytes by the supervisor.",
                "allocation_source": "Separate untimed one-call direct children; counters include process setup, argument handling, workload, and result output.",
            },
            "limitations": [
                "Nacre has no standard-library JSON parser, so JSON uses the same schema-specific parser in Nacre and Rust: three fixed ASCII id/name/active objects, fixed field order, nonnegative 32-bit integer IDs and boolean values, JSON whitespace, and string escapes including \\uXXXX and surrogate pairs. It is not a general JSON implementation; keys are unescaped fixed literals, raw strings are ASCII in the fixture, and numeric syntax is limited to nonnegative integer IDs.",
                "Nacre Map and Rust's standard randomized HashMap differ in hashing, growth, bucket, and collision policies; these are the normal collection APIs, not identical hash-table implementations.",
                "Nacre's built-in bench path enables NACRE_MEMORY_STATS for its generated C program; those counters may perturb Nacre timings. Rust timing runs do not enable allocator counters.",
                "RSS is a per-process high-water mark and includes startup/runtime setup; direct RSS children repeat the kernel enough to target approximately 20 ms based on Nacre's measured per-call time, capped at 20,000 calls.",
                "Nacre allocation counters cover Nacre ncr_allocate/ncr_deallocate calls, not libc allocations. Rust counters cover Rust global-allocator calls; the reported live-block snapshot may include runtime/argument allocations and is not a leak verdict.",
                "No filesystem, network, or external data source is used. CPU frequency, scheduling, thermal conditions, and allocator state are not pinned or controlled.",
            ],
            "validated_checksums": checksums,
            "workloads": workload_results,
        }
        serialized = json.dumps(payload, indent=2, sort_keys=True) + "\n"
        if args.output is None:
            sys.stdout.write(serialized)
        else:
            output_path = args.output if args.output.is_absolute() else ROOT / args.output
            output_path.write_text(serialized, encoding="utf-8")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, AssertionError, KeyError) as error:
        print(f"benchmark suite: {error}", file=sys.stderr)
        raise SystemExit(1)
