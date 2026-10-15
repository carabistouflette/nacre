# Nacre and Rust workload measurements

These are host-specific measurements, not language-wide performance guarantees.
The sources, raw samples, checksums, commands, compiler versions, and measurement
limits are retained in [option1-results.json](../benchmarks/option1-results.json).
The six workloads use the same inputs and validated output checksums in Nacre
and Rust.

## Reproduce the workload comparison

```sh
cargo build --release --locked
flock -x /tmp/nacre-option1-measure.lock python benchmarks/run_suite.py \
  --repeats 5 --rss-repeats 3 --output benchmarks/option1-results.json
```

The runner uses the existing `target/release/nacre`; it does not rebuild the
compiler. It forces `NACRE_NO_CACHE=1` so build timings measure native compilation
and linking, not executable-cache lookup/copy. Workloads are
[suite_nacre.ncr](../benchmarks/suite_nacre.ncr) and
[suite_rust.rs](../benchmarks/suite_rust.rs). The Nacre path really executes
`nacre bench benchmarks/suite_nacre.ncr --native`. Rust is built with
`rustc --edition=2024 -O -C target-cpu=native`.

Recorded toolchains: Nacre 0.12.0, GCC 16.2.1, Rust 1.99.0, Linux x86_64,
glibc 2.44. The measurement run used the existing release compiler, before the
subsequent direct-LLVM/frontend feature integration; it is not a comparison of
the two Nacre backends.

Each runtime result is the median of five fresh process-level benchmark means.
Each process performs three warmups, calibrates to at least 20 ms, and averages
five timed batches. RSS is the median of three fresh `wait4`-supervised children
per workload; allocation counters come from separate, untimed one-call children.
Timing runs have no CPU affinity or fixed frequency policy. Raw samples show
scheduling/thermal variation; small differences are not evidence of parity or
an optimization's causal contribution.

## Runtime and throughput

Rates use each workload's stated unit, not a common operation definition.

| Workload | Nacre ns/call | Rust ns/call | Throughput unit | Nacre units/s | Rust units/s |
|---|---:|---:|---|---:|---:|
| JSON | 246 | 81 | Records parsed | 12,195,121 | 37,037,037 |
| String manipulation | 432 | 331 | Input bytes | 152,777,777 | 199,395,770 |
| Binary streaming | 19,060 | 14,757 | Payload bytes | 214,900,314 | 277,563,190 |
| Hash map | 38,840 | 84,137 | Map operations | 263,645,726 | 121,706,264 |
| Sudoku | 173,361 | 303,510 | Puzzles solved | 5,768 | 3,294 |
| Game of Life | 2,613,957 | 2,440,981 | Cell transitions | 382,561,763 | 409,671,357 |

- **JSON:** three fixed `id`/`name`/`active` objects. Both implementations parse
  array/object structure, ordered keys, integer/boolean tokens, JSON whitespace,
  and escaped strings. This is a schema-specific parser, not a general JSON
  library: keys are fixed unescaped literals, raw fixture strings are ASCII,
  and IDs are nonnegative 32-bit integers.
- **Strings:** replace `::`, split the result, and search fields for `nacre`.
- **Binary:** stream 4,096 deterministic bytes in 257-byte memory chunks, then
  lowercase-hex encode/decode. No filesystem or network I/O.
- **Map:** insert, query, replace, query, and remove 2,048 integer keys. Rust uses
  its normal randomized `HashMap`; Nacre uses `Map`. Hashing, growth, and table
  algorithms differ; equal checksums do not imply identical implementations.
- **Sudoku:** the fixed 9×9 example puzzle, solved by equivalent backtracking.
- **Life:** a deterministic 100×100 toroidal grid, 100 B3/S23 generations.

Nacre's built-in benchmark runner enables allocation counters in its generated
program; Rust's timing binary does not. That instrumentation asymmetry may
perturb Nacre timings and is retained rather than hidden.

## Memory

RSS includes process/runtime startup and is not live payload size. Allocation
counts below include setup, arguments, one kernel call, and result printing.
Nacre counts `ncr_allocate`; Rust counts global allocation/reallocation events.
The counter scopes differ, so these are observations, not allocator-efficiency
ratios.

| Workload | Nacre peak RSS KiB | Rust peak RSS KiB | Nacre allocations | Rust allocation events |
|---|---:|---:|---:|---:|
| JSON | 2,192 | 2,228 | 10 | 13 |
| String manipulation | 2,188 | 2,256 | 22 | 26 |
| Binary streaming | 2,204 | 2,344 | 21 | 21 |
| Hash map | 2,328 | 2,316 | 15 | 18 |
| Sudoku | 2,196 | 2,340 | 7 | 8 |
| Game of Life | 2,316 | 2,336 | 7 | 9 |

All six Nacre one-call reports had balanced allocations/frees and `live=0`.
Rust's reporting snapshot retained four runtime/argument blocks; it is taken
before complete process teardown and is not a leak verdict. The JSON artifact
retains the complete counters and individual RSS samples.

## Cold native build and link time

These are whole-workload-suite builds, not per-kernel or compiler-Cargo builds.
Each value is the median of five independent commands with Nacre's executable
cache disabled. Compiler startup and output I/O are included.

| Build variant | Nacre milliseconds | Rust milliseconds |
|---|---:|---:|
| Native, no allocation counters | 1,663.110 | 428.368 |
| Native, allocation counters enabled | 1,662.079 | 440.606 |

An earlier 15 ms Nacre result was an executable-cache hit and is deliberately
not presented as native compilation time.

## Controlled optimization deltas

[optimization-results.json](../benchmarks/optimization-results.json) retains the
exact experiment sources, flags/control edits, and raw paired counters. These
experiments used snapshot `f3073b3c25b90c452882535703d82769359c8843`, GCC 16.2.1,
and an Intel Core Ultra 7 258V. Counter runs were pinned to CPU 0, using
`cpu_core/cycles/`, `cpu_core/branches/`, and `cpu_core/branch-misses/` to avoid
unmeasured Atom events on the hybrid CPU. Paired measurements were interleaved.

| Controlled experiment | Baseline | Optimized | Supported conclusion |
|---|---:|---:|---|
| One million nonescaping `Cell` creations: heap allocations | 1,000,000 | 0 | Stack promotion removes these allocations; both checksums match and both report `live=0`. |
| 200,000 searches × 4,096 bytes: core cycles | 577,221,906 | 27,551,502 | Libc `memchr` uses 95.23% fewer cycles than the explicitly scalar control in this mechanism benchmark. |
| 200 million monomorphic calls: mean core cycles | 1,013,771,190 | 1,012,656,927 | Direct dispatch has a tiny 0.11% observed reduction, not a broad application speedup. |
| Same dispatch control: mean branch misses | 203,875 | 203,709 | The 0.081% observed change is too small to support a general prediction claim. |
| Stable-profile PGO control: mean branch misses | 2,905.6 | 2,875 | No reproducible gain: a separate seven-run batch reversed the direction. |

The stack baseline changes only `find_stack_promoted` in an isolated snapshot
to return an empty set. SIMD controls disable the compiler vectorizer and
builtin `memchr` substitution; GDB observed glibc `__memchr_avx2`, while scalar
disassembly remained byte-at-a-time. This tests the search primitive used by
Nacre, not an end-to-end Nacre workload speedup. Dispatch controls change only
the generated wrapper's direct thunk call to a vtable call and disable GCC
inlining/IPA transformations that would erase that distinction. Checksums match
for every control.

The PGO control used stable generated-C and executable paths, and `strace`
confirmed that GCC opened the training profile. Its paired mean cycle count was
slightly higher with PGO; no PGO performance gain is claimed. The CLI experiment
also exposed missing profile data caused by changing temporary paths. Commit
`ab01891` fixes source/auxiliary identity and bypasses caches for mutable profile
inputs. Its isolated CLI regression requires `-Werror=missing-profile`; an actual
training/use smoke printed `49995011` in both executables. Those checks establish
profile consumption, not a newly measured speedup from the fix.

## Direct LLVM build comparison

This separate integration measurement uses commit `0de4be4`, the debug Nacre
compiler, and Clang 22.1.8 for both native backends at `-O2`; direct linking uses
`llvm-link` 22.1.0. It is not comparable to the release-compiler whole-workload
suite timing above. [direct-build-results.json](../benchmarks/direct-build-results.json)
retains the exact consumer source, sample seeds, outputs, and build durations.

Each build changes the initial accumulator literal, preventing executable-cache
hits while retaining the same runtime layout. Cold values are medians of five
independent cache directories. Warm values are medians of four changed-source
builds after one excluded prewarm build; only direct LLVM reuses runtime
bitcode. Every resulting executable was run and its output checked.

| Backend | Cold build milliseconds | Runtime-cache-warm changed-source milliseconds |
| --- | ---: | ---: |
| Generated C / Clang | 373.692 | 349.772 |
| Direct LLVM / Clang | 1,057.216 | 665.622 |

The runtime cache removes repeated C frontend work, but this consumer does
**not** demonstrate a total-build speedup over generated C. These sequential,
unpinned batches measure one small pipeline program, not universal compiler
throughput or an isolated causal optimization delta.

