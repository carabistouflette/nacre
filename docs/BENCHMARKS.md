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

## Release compiler build-stage profile

The 2026-10-15 profile compares release compiler baseline `a81f2fc` with
backend-hardening commit `0bede0f`, on Linux x86-64, pinned to CPU 0. Both backends use
Clang 22.1.8 at `-O2`; direct LLVM uses `llvm-link` 22.1.0. Compiler SHA-256
identities, tool output, individual samples, stdout, and allocation counters are
retained in [the baseline report](../benchmarks/build-profile-baseline.json)
and [the optimized report](../benchmarks/build-profile-optimized.json).

`benchmarks/profile_builds.py` exercises `examples/language/counter.ncr`,
`examples/collections/text_map_inventory.ncr`, and a generated three-package
transitive dependency chain. Each build changes a literal, avoiding executable
cache hits. Cold samples use independent caches; warm samples reuse runtime
bitcode with changed source. Each revision has 72 compiled-and-executed samples:
three repetitions per consumer/backend/cache state, separately with and without
tool instrumentation. All outputs and zero-live-allocation counters passed.

### Uninstrumented build wall time

Values are three-sample medians in milliseconds. These exclude the Python tool
wrappers used for the separate stage profile.

| Consumer | Backend | Baseline cold | Optimized cold | Baseline warm | Optimized warm |
| --- | --- | ---: | ---: | ---: | ---: |
| Counter | C | 166.620 | 166.736 | 165.609 | 164.650 |
| Counter | Direct LLVM | 790.916 | 594.596 | 413.918 | 270.303 |
| Text/map inventory | C | 322.250 | 321.156 | 321.525 | 320.376 |
| Text/map inventory | Direct LLVM | 1,006.956 | 782.376 | 550.971 | 389.397 |
| Three packages | C | 164.399 | 165.102 | 164.438 | 164.457 |
| Three packages | Direct LLVM | 788.468 | 593.726 | 412.991 | 271.750 |

### Direct LLVM stage observations

Values are median milliseconds. The emit CLI column separately measures actual
`emit-llvm --direct` execution, including frontend and user-IR generation. Other
columns measure child-tool durations during cold instrumented native builds.
Version/target probes are grouped; runtime compilation is grouped per build.

| Consumer | Revision | Emit CLI | Tool probes | Runtime bitcode | llvm-link | Clang native driver | Native linker, nested |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Counter | Baseline | 9.953 | 36.346 | 377.674 | 28.991 | 389.803 | 18.166 |
| Counter | Optimized | 10.128 | 36.293 | 325.631 | 15.638 | 262.092 | 17.987 |
| Text/map inventory | Baseline | 10.641 | 36.288 | 457.119 | 35.353 | 522.010 | 18.262 |
| Text/map inventory | Optimized | 10.803 | 36.343 | 393.497 | 19.511 | 375.720 | 18.078 |
| Three packages | Baseline | 9.924 | 36.142 | 375.087 | 28.537 | 390.298 | 18.242 |
| Three packages | Optimized | 10.085 | 36.512 | 323.836 | 15.785 | 262.547 | 18.021 |

Runtime bitcode compilation and the final Clang invocation dominate cold direct
builds, not Nacre frontend work. The emitted user functions previously stayed
externally live, including unused prelude specializations. `linkonce_odr` makes
unused functions discardable; concrete interface implementations retain strong
symbols because imported C vtable thunks reference them. Direct-runtime C now
uses `-g0`, also avoiding debug information for temporary, deleted C sources.

The Clang-driver column includes optimization, code generation, native linking,
and startup of its nested linker wrapper; it is **not** isolated LLVM optimizer
time. The linker column is already nested in that duration. Wrapper startup also
adds substantial traced wall time, so stage columns must not be summed into the
uninstrumented wall measurements. This is a local sequential before/after
comparison, not an isolated causal delta for either individual code change.
Direct LLVM improved against its baseline but remains slower than C for all
three consumers; no universal throughput or total-build speedup over C is claimed.

To reproduce with matching LLVM tools and native dependency paths configured:

```sh
cargo build --offline --locked --release --bin nacre
python3 benchmarks/profile_builds.py --compiler target/release/nacre \
  --label local-release --repeats 3 --output /tmp/nacre-build-profile.json
```


## C/direct LLVM runtime comparison

The 2026-10-15 runtime comparison at `2e4e1ad` uses the same six existing Nacre
workloads before and after making whole-program-only LLVM functions private.
The baseline compiler is an independently built `0bede0f`; compiler hashes, source hashes,
tool versions, calibration runs, individual samples, allocation counts, and
per-child RSS are retained in
[backend-runtime-baseline.json](../benchmarks/backend-runtime-baseline.json) and
[backend-runtime-optimized.json](../benchmarks/backend-runtime-optimized.json).

Both backends use Clang 22.1.8, `-O3`, and `--native`; direct LLVM also uses
`llvm-link` 22.1.0. The runner pins CPU 0 and alternates backend order over five
fresh child processes per workload/backend. Calibration chooses one shared
repetition count per workload within each revision, targeting 0.2 seconds for
the faster backend. Both workload drivers now allow up to two million calls:
the previous 20,000-call ceiling produced excessively short fast-workload
samples. Kernel inputs and algorithms are unchanged.

Rust supplies the independent one-call checksum oracle. Every calibration,
timing, and allocation run checks its expected aggregate checksum. Timing and
RSS use uninstrumented executables; separate one-call processes collect tracked
Nacre allocations. All six workloads matched Rust for both revisions/backends,
with balanced allocation/free counts and zero live tracked allocations.

### Runtime medians

Microseconds per kernel call, including amortized process startup, argument
handling, workload dispatch, result formatting, and shutdown. These are not the
built-in `nacre bench` timer's isolated function measurements.

| Workload | C baseline | C after | LLVM baseline | LLVM after |
| --- | ---: | ---: | ---: | ---: |
| JSON | 0.189 | 0.183 | 0.344 | 0.324 |
| String | 0.489 | 0.494 | 0.537 | 0.515 |
| Binary | 33.906 | 33.740 | 47.601 | 45.939 |
| Hash map | 63.038 | 66.084 | 83.188 | 81.903 |
| Sudoku | 424.429 | 422.285 | 502.352 | 455.227 |
| Game of Life | 5,996.534 | 5,965.678 | 12,042.393 | 4,057.852 |

### Memory observations after the change

RSS is median fresh-process peak RSS from `wait4`, including loader, stack,
runtime, arguments, and workload—not a heap-only or per-call measurement.
Tracked allocation counts include one-call process setup and output. Counts
were identical across both backends and both revisions; no allocation reduction
or meaningful RSS improvement is claimed.

| Workload | C peak RSS KiB | LLVM peak RSS KiB | Allocations / frees, both backends |
| --- | ---: | ---: | --- |
| JSON | 2,244 | 2,236 | 10 / 10 |
| String | 2,196 | 2,256 | 22 / 22 |
| Binary | 2,320 | 2,384 | 21 / 21 |
| Hash map | 2,372 | 2,296 | 15 / 15 |
| Sudoku | 2,324 | 2,328 | 7 / 7 |
| Game of Life | 2,312 | 2,396 | 7 / 7 |

### Localized cause and controlled experiment

`perf` 7.2.9 at 499 Hz collected 2,992 samples from the original direct Life
executable. The C build inlined this work into `lifeKernel`; Clang's inline
remarks rejected both direct calls as too costly.

| Function | Sampled cycles % | Inline cost | Inline threshold |
| --- | ---: | ---: | ---: |
| `LifeSim.neighbors` | 71.76 | 655 | 525 |
| `LifeSim.step` | 27.97 | 425 | 250 |

Adding `inlinehint` alone did not materially improve three paired runs and was
not retained. Changing only the linked user functions' `linkonce_odr` linkage
to `internal` improved three alternating, CPU-pinned 100-call Life runs, all
with the same checksum:

| Trial | Original seconds | Internal-linkage seconds |
| --- | ---: | ---: |
| 1 | 1.211878 | 0.407242 |
| 2 | 1.187465 | 0.407028 |
| 3 | 1.189199 | 0.406820 |

Production now gives whole-program-only functions private linkage so LLVM can
optimize their calling conventions and inline them.
Concrete interface implementations remain external for imported C vtable thunks.

Life is now faster than the matched C build on this machine; the other five
workloads still favor C. Smaller before/after movements are not isolated causal
claims, and the C controls also varied. No universal backend speedup is claimed.

With matching LLVM tools and native dependency paths configured:

```sh
cargo build --offline --locked --release --bin nacre
python3 benchmarks/run_suite.py --compare-backends --repeats 5 \
  --output /tmp/nacre-backend-runtime.json
```

Pass `--compiler /path/to/another/revision/target/release/nacre` to measure a
separately built revision against the same current workload sources. Without
`--compare-backends`, the existing Nacre/Rust benchmark mode is unchanged.


## JSON native-boundary optimization

The JSON-focused follow-up reuses the preceding `2e4e1ad` results as its
baseline rather than repeating an unchanged compiler run. The new
[json-runtime-optimized.json](../benchmarks/json-runtime-optimized.json) uses
the same Nacre/Rust workload source hashes, Clang 22.1.8 at `-O3 --native`,
`llvm-link` 22.1.0, CPU 0 affinity, five alternating fresh-process samples per
backend/workload, and separate allocation builds. The compiler identities are
recorded in the reports; workload algorithms and inputs did not change.

### Profile and retained change

Ten fresh processes each executed two million JSON calls under `perf` at
999 Hz. All returned the same checksum. The original direct executable
collected 6,559 CPU samples, including the following hotspots:

| Symbol | Role | Sampled cycles % |
| --- | --- | ---: |
| `ncr_llvm_fn_7` | Workload dispatcher with inlined parser work | 40.08 |
| `ncr_llvm_native_140` | Native Text-from-String ABI adapter | 28.01 |
| `ncr_llvm_fn_19` | `JsonParser.key` | 14.94 |

The Text adapter contained the UTF-8 validator but hid caller constants behind
erased-pointer argument slots. Inlining that boundary allowed LLVM to optimize
the native operation in its caller's context. Production now marks generated
native ABI adapters `always_inline`; user functions and underlying standard
library functions receive no new attributes. Dynamic UTF-8 validation is not
removed or special-cased, and runtime optimization flags remain unchanged.

Native-adapter `inlinehint` alone did not materially improve JSON. Deferring
runtime LLVM passes also did not help by itself; combining deferred passes
with early adapter removal slowed Life in the controlled experiment, so that
pipeline change was rejected. Three rotating same-count comparisons across
all six workloads supported retaining the adapter-only change.

### Runtime medians after native-adapter transparency

Microseconds per call, with the same process-scope timing limitations as the
preceding comparison. The baseline column comes from the existing report;
the two current-backend columns were measured together.

| Workload | LLVM baseline | LLVM after | C current control |
| --- | ---: | ---: | ---: |
| JSON | 0.324 | 0.234 | 0.186 |
| String | 0.515 | 0.463 | 0.469 |
| Binary | 45.939 | 45.623 | 34.577 |
| Hash map | 81.903 | 53.894 | 62.823 |
| Sudoku | 455.227 | 464.815 | 411.022 |
| Game of Life | 4,057.852 | 4,217.976 | 6,224.098 |

JSON and map improvements were reproduced in the isolated adapter experiment.
Small cross-run movements must not be treated as causal: additional five-pair,
same-count comparisons of the actual before/after Sudoku and Life executables
did not reproduce the apparent slowdowns in this sequential report comparison.
Map and Life favor LLVM in the current run, String is near parity, and JSON,
binary, and Sudoku still favor C. No universal backend speedup is claimed.

### Footprint and validation limits

The optimization increases code footprint in this consumer. GNU `size` reports:

| Build | Text-category bytes | Data bytes | BSS bytes |
| --- | ---: | ---: | ---: |
| Before | 39,170 | 712 | 16 |
| After | 40,946 | 712 | 16 |

The text category includes read-only sections, not just the `.text` section.
Inlining adapters can duplicate already-expanded native implementation code;
this measured footprint cost is not a claim that inlining is free.

Tracked allocation/free counts remained identical to the baseline for every
workload, with zero live allocations. Per-process RSS samples remain in the new
report; their small differences do not establish a memory improvement. Separate
`-O3` before/after checks retained malformed UTF-8 rejection and valid
Unicode/NUL behavior, and `-O0` source breakpoints and scalar locals still worked.

Reproduce the current run with matching LLVM tools and dependency paths:

```sh
python3 benchmarks/run_suite.py --compare-backends --repeats 5 \
  --output /tmp/nacre-json-runtime.json
```

## Binary append hot-path optimization

This follow-up uses `e4fcb7b`'s
[json-runtime-optimized.json](../benchmarks/json-runtime-optimized.json) as the
unchanged six-workload baseline. The binary kernel still generates 4,096 bytes,
copies through `MemorySource`/`MemorySink` in 257-byte chunks, hex-encodes and
decodes, then checksums the result. No workload algorithm or input changed.

### Profile and controlled change

Separate CPU-0 `perf` recordings at 999 Hz each ran 100,000 binary calls and
returned checksum `245718300000`. The C recording collected 3,417 samples;
direct LLVM collected 4,564. Self-time sampled cycles were:

| Role | C % | Direct LLVM % |
| --- | ---: | ---: |
| Dispatcher with inlined kernel work | 59.24 | 53.41 |
| `ncr_byte_buffer_reserve_26` | 27.26 | 33.00 |
| Sink interface write implementation/thunk | 12.70 | 12.97 |

Single-byte append unconditionally called the reserve helper, including when
capacity was sufficient. The retained change guards that call with
`required_size > p->capacity`. Overflow checks still precede the addition;
the existing reserve, validation, allocation, and geometric-growth paths remain
unchanged. ByteBuffer's private representation and factories maintain
`length <= capacity` and capacity within the supported integer limits.
Bulk append and allocator policy are unchanged.

For causal isolation, only this guard was added to the captured C and runtime
sources; the direct experiment relinked the exact original user LLVM module.
Three rotating 10,000-call samples per executable all returned checksum
`24571830000`. Median microseconds per call changed from 34.195 to 22.065 for C
and from 45.347 to 28.293 for direct LLVM. This improves both backends rather
than attributing a shared runtime bottleneck solely to LLVM. LLVM remains about
28% slower than C in this controlled binary experiment; parity is not claimed.
No new inlining attributes or runtime optimization flags were introduced.

A follow-up direct recording of 100,000 calls returned the same checksum:
reserve fell to 0.14% of sampled cycles, while the inlined kernel dispatcher
accounted for 77.63% and `MemorySink.write` for 21.18%. This localizes remaining
work; it does not establish a safe further compiler change.

### Matched six-workload recheck

[binary-runtime-optimized.json](../benchmarks/binary-runtime-optimized.json)
records five alternating fresh-process samples per backend/workload, with the
same source hashes, Clang 22.1.8, `llvm-link` 22.1.0, CPU 0 affinity, and
`-O3 --native` methodology as the baseline. All 60 timed samples, calibration
runs, and separate allocation runs matched the Rust checksum preflight.
Microseconds per kernel call, including amortized process overhead:

| Workload | C before | LLVM before | C after | LLVM after |
| --- | ---: | ---: | ---: | ---: |
| JSON | 0.186 | 0.234 | 0.189 | 0.236 |
| String | 0.469 | 0.463 | 0.483 | 0.484 |
| Binary | 34.577 | 45.623 | 22.003 | 28.284 |
| Hash map | 62.823 | 53.894 | 63.600 | 53.882 |
| Sudoku | 411.022 | 464.815 | 409.434 | 494.336 |
| Game of Life | 6,224.098 | 4,217.976 | 5,975.303 | 4,015.705 |

Binary time decreased by 36.4% for C and 38.0% for LLVM against the retained
baseline; the isolated experiment supports attribution to the capacity guard.
LLVM remains 28.5% slower than C for binary. Movements in other workloads,
including the slower LLVM Sudoku result, are cross-run observations rather than
demonstrated effects of this change. String is near parity; map and Life favor
LLVM, while JSON, binary, and Sudoku favor C in the final run.

Tracked allocation/free counts were identical to the baseline for every
workload and both backends, with zero live allocations. RSS raw samples are
retained in the report; their small differences are not evidence of a memory
improvement. Reproduce with matching LLVM tools and dependency paths:

```sh
python3 benchmarks/run_suite.py --compare-backends --repeats 5 \
  --output /tmp/nacre-binary-runtime.json
```

### Executable footprint

GNU `size` on the actual uninstrumented `-O3 --native` suite executables:

| Backend/build | Text-category bytes | Data bytes | BSS bytes |
| --- | ---: | ---: | ---: |
| C before | 38,917 | 720 | 16 |
| C after | 38,805 | 720 | 16 |
| Direct LLVM before | 40,946 | 712 | 16 |
| Direct LLVM after | 40,962 | 712 | 16 |

The text category includes read-only sections, not just `.text`. This consumer's
small footprint changes do not establish a general code-size improvement.

## Typed direct-LLVM argument transport

The 2026-10-16 cutover changes user-function inputs, not returns. The baseline is
`d02be8b`; compiler identities, input hashes, commands, raw samples, hardware
counters, and checks are recorded in
[typed-arguments-controlled.json](../benchmarks/typed-arguments-controlled.json).
The original captured IR/runtime reproduces the baseline executable byte-for-byte.

### ABI contract

`c_repr::direct_input_abi` is the authoritative policy for LLVM definitions,
direct calls, constructors, C bridge declarations, and C bridge calls:

| Input | Internal transport |
| --- | --- |
| Boolean, byte, char, int, long, double | Scalar value |
| Thin owned object references and thin borrows, including borrowed sums | Pointer value |
| Unit, String, inline sum values, owned/borrowed interface values | Address of value storage |

Transport does not duplicate ownership. Callees place incoming values in local,
addressable homes; moved objects remain single-owner and borrows never become
owners. Existing cleanup and argument-consumption order is retained. These homes
also provide scalar debug locations without the old incoming-pointer debug spill.
Result slots, including Boolean results, are unchanged. Native/virtual erased
entry paths and C interface method contracts retain their existing signatures;
their internal user-function bridges use the shared input policy. C aggregate
argument/return classification remains Clang's responsibility.

### Argument versus return ablation

Only `MemorySink.write` and the necessary runtime declaration/adapter were changed
in these scratch controls. Both implementations remain out of line. Twenty
balanced fresh-process rounds used the same binary workload, repetition count,
Clang 22.1.8, `llvm-link` 22.1.0, CPU 0, and `-O3 --native` flags.
Negative paired elapsed change means faster:

| Lowering | Paired elapsed change | Faster pairs / 20 | Median cycles/call | Median instructions/call |
| --- | ---: | ---: | ---: | ---: |
| Original erased inputs and result slot | — | — | 80,274 | 490,442 |
| Typed inputs only | -4.59% | 16 | 77,295 | 478,001 |
| Typed return only | -1.17% | 11 | 80,736 | 490,332 |
| Typed inputs and return | -5.18% | 17 | 75,835 | 477,920 |

Inputs account for most of the instruction reduction and measured elapsed gain.
Return-only lowering did not improve median cycle counts. The permanent change
therefore preserves result slots instead of introducing target-specific aggregate
return rules. Separate invalid-offset controls returned the failure checksum and
balanced allocations, exercising the owned Text error payload.

### Production six-workload recheck

[typed-arguments-runtime-optimized.json](../benchmarks/typed-arguments-runtime-optimized.json)
records the existing runner's ten C/direct samples per workload, Rust checksum
preflight, calibration, RSS, and separate allocation runs. An additional twelve
balanced rounds compared the actual baseline, production LLVM, and C executables
using a shared count for each workload. Production LLVM versus baseline:

| Workload | Median paired elapsed change | Faster pairs / 12 |
| --- | ---: | ---: |
| JSON | +2.01% | 2 |
| String | -1.45% | 7 |
| Binary | -6.43% | 12 |
| Hash map | -8.52% | 8 |
| Sudoku | +0.74% | 5 |
| Game of Life | -0.21% | 6 |

Binary's separate hardware counters decreased from 80,634 to 76,594 cycles/call
and from 490,442 to 478,001 retired instructions/call. LLVM still trails C for
binary; this is not a claim that argument transport explains the whole gap.

JSON's small cost is recorded, not dismissed as noise: its median cycles increased
from 654.2 to 668.6 despite one fewer retired instruction per call. Other small
elapsed changes are not universal gains or regressions; an earlier identical-binary
control moved by about 1.3%, and hash-map samples have substantial process-to-process
variation. No causal map speedup is claimed. Life's retired instruction count
decreased, while elapsed time and cycle counts remained near baseline. No alignment
workaround, function-name exception, new inlining attribute, or runtime flag was added.

Every workload preserved its checksum and tracked allocation count for both
backends, with balanced frees and `live=0`. A mixed scalar/owned/borrowed/aggregate
interface consumer passed `-O0` sanitizers and release CLI checks at `-O0`/`-O3`.
GDB inspected its constructor argument, all six scalar parameters, and computed
full-width digest at original-source breakpoints.

### Footprint and limits

GNU `size` on the actual uninstrumented suite executables:

| Build | Text-category bytes | Data bytes | BSS bytes |
| --- | ---: | ---: | ---: |
| C baseline and final | 38,805 | 720 | 16 |
| LLVM baseline | 40,962 | 712 | 16 |
| LLVM typed inputs | 40,830 | 712 | 16 |

The C control's `.text` and `.rodata` are byte-identical. The text category includes
read-only sections; tracked allocation counts are not allocator-byte measurements.
These results cover one Linux x86-64 host, not a cross-target performance guarantee.
Reproduce the current C/direct report with matching SDK paths and LLVM tools:

```sh
python3 benchmarks/run_suite.py --compare-backends --repeats 10 \
  --output /tmp/nacre-typed-arguments-runtime.json
```

## Hex decode destination invariant

The 2026-10-16 follow-up starts from `26431d7`. Full identities, flags, raw samples,
staging controls, hardware counters, and verification are recorded in
[hex-decode-controlled.json](../benchmarks/hex-decode-controlled.json).
Preserved baseline executables and captured IR were reused, not rebuilt.

### Causal control and permanent change

The captured LLVM runtime's hex conversion was scalar and unrolled; it did not
already carry a vectorized-loop marker. Late optimization reported potentially
faulting loads and unidentified array bounds. Suppressing early loop/SLP
vectorization or compiling the runtime at `-O1` did not restore vectorized
conversion. Disabling SLP alone reproduced the baseline executable byte-for-byte;
disabling both vectorizers reproduced the loop-vectorizer-only control.

The narrower control cached `decoded->bytes` immediately after capacity allocation
and used that pointer throughout conversion. The output is fresh and private,
and conversion neither resizes nor exposes it. This makes its stable storage
explicit without new alias promises. Validation still precedes allocation; error
results, allocation order, ownership, and the final length update are unchanged.
The shared `stdlib/encoding.c.in` implementation serves both backends. There is no
compiler function-name exception or new optimization, inlining, or alignment flag.

Clang's runtime remarks now report a conversion width of 32 with interleave 1.
The optimized decoder loads the output pointer before its loop, reads 64 input
bytes, separates even/odd digits, and stores 32 output bytes. An eight-byte vector
epilogue and scalar tail handle shorter remainders. The early-exit validation loop
remains scalar; a missed-vectorization remark at the call site alone must not be
mistaken for failure to vectorize conversion.

The production LLVM executable is byte-identical to the cached-pointer scratch
control, and its user IR is byte-identical to the captured baseline user IR.
C's final `.text` and `.rodata` are byte-identical to its preserved baseline.

### Matched production measurements

Sixteen balanced cyclic/reversed rounds on CPU 0 compared preserved/final C and
LLVM executables at the same per-workload call counts as the staging experiment.
Each fresh child was measured by the `wait4` supervisor and checksum-checked.
Timing includes startup, dispatch, output, and shutdown. Negative paired change
means faster:

| Workload | C paired elapsed change | LLVM paired elapsed change | LLVM faster pairs / 16 |
| --- | ---: | ---: | ---: |
| JSON | +0.27% | +0.11% | 7 |
| String | -1.49% | +1.94% | 6 |
| Binary | +1.08% | -11.73% | 16 |
| Hash map | +2.19% | +7.01% | 5 |
| Sudoku | -0.09% | -2.53% | 10 |
| Game of Life | -0.48% | -1.33% | 10 |

Separate six-round alternating pairs collected user-mode counters without
multiplexing:

| Workload | LLVM baseline cycles/call | LLVM final cycles/call | Baseline instructions/call | Final instructions/call |
| --- | ---: | ---: | ---: | ---: |
| Binary | 77,457 | 68,072 | 478,002 | 423,507 |
| JSON | 669.2 | 672.1 | 4,697 | 4,697 |
| Hash map | 186,629 | 205,633 | 655,451 | 655,451 |
| Game of Life | 11,531,495 | 11,474,679 | 76,927,264 | 76,927,264 |

Binary's consistent elapsed and instruction reduction supports the conversion
change. The first hash-map timing/counter cost is retained above, not discarded.
A follow-up used two labels for each exact executable in 32 balanced rounds:
the two old/new paired comparisons moved -0.40% and +1.20%, while identical-byte
label comparisons moved +1.03% and +1.21%. The map effect did not reproduce;
neither a map gain nor a resolved universal regression is established.
The other small elapsed movements are not attributed to hex conversion.
Identical C code also moved between runs, demonstrating measurement variation.

### Footprint and correctness

| Backend | Baseline text-category bytes | Final text-category bytes | Unchanged data / BSS bytes |
| --- | ---: | ---: | ---: |
| C | 38,805 | 38,805 | 720 / 16 |
| Direct LLVM | 40,830 | 42,110 | 712 / 16 |

Additional vector code costs space. GNU `size` includes read-only sections in
its text category; these measurements are not allocator-byte measurements.
This is one Linux x86-64 host with Clang 22.1.8 and `llvm-link` 22.1.0, not a
cross-target performance guarantee.

Both backends matched the Rust oracle for all six checksums and reported
76 allocations, 76 frees, and zero live allocations. Release-CLI ASan/UBSan
consumers passed at `-O0` and `-O3` on both backends: empty and vector/tail
boundaries, all byte values, uppercase hex, odd lengths, invalid digits, NUL,
Unicode, and late invalid input. Each reported balanced allocation counters.
The permanent optimized boundary test checks every decoded byte and length.
Both complete fuzzing-enabled suites, including ignored tests, passed 1,351
tests each; strict all-target/all-feature Clippy and the release build passed.

## Whole-file Text read tail reuse

[read-text-tail-controlled.json](../benchmarks/read-text-tail-controlled.json)
records a shared native `Files.readText` change. The read-all caller caps its
next request by existing writable tail space, when present. The public
`File.readSome` helper, geometric growth policy, UTF-8 validation, Text-size
limit, error precedence and ownership transfer are unchanged. No compiler,
alias, inlining, ISA or layout policy was changed.

### Allocation cause and result

The contact-book import/export input is 463,347 bytes. After seven full
64 KiB reads and one 4,595-byte read, the input buffer has capacity 524,288
and 60,941 writable bytes left. Previously the next request insisted on
65,536 bytes, allocated a 1 MiB replacement and copied the input before
discovering EOF. The final request now uses the existing 60,941-byte tail.
Both backends retain nine read calls per import and identical file contents.
Exact-full-buffer EOF and empty-file initial allocation are not removed by
this bounded change.

Separate instrumented runs of the unchanged contact-book application perform
120 load/save pairs over 10,000 contacts. C and direct LLVM agree:

| Diagnostic | Baseline | Tail reuse |
| --- | ---: | ---: |
| Nacre allocation events | 4,405,683 | 4,405,563 |
| Nacre frees | 4,405,683 | 4,405,563 |
| Nacre live allocations | 0 | 0 |
| Intercepted requested bytes | 620,175,824 | 494,346,704 |

The reduction is 120 MiB, or 20.29% of cumulative intercepted request bytes,
not a 20.29% peak-RAM reduction. Realloc requests include their complete
replacement sizes; libc-internal allocations can bypass the interposer.
Search and lending-desk allocation counts and requested bytes are unchanged.

### Matched performance and limits

Saved baseline executables were reused. Both versions use Clang 22.1.8,
`llvm-link` 22.1.0, `-O3 --native`, CPU 0 and fresh processes. Timings and RSS
use a C `wait4` supervisor without allocation instrumentation or profiling.
Pairs alternate old/new order under the benchmark lock.

A separate repeated `Files.readText` API consumer reads cached files and
checks its accumulated byte lengths. Each size/backend has eight pairs with
one shared repetition count. Negative median paired elapsed change is faster:

| Input bytes | C elapsed change | Direct LLVM elapsed change |
| --- | ---: | ---: |
| 0 | -1.51% | +0.41% |
| 1 | -71.59% | -71.43% |
| 65,535 | -14.89% | -17.72% |
| 65,536 | -0.33% | +1.16% |
| 65,537 | -49.71% | -49.58% |
| 131,071 | -41.64% | -41.40% |
| 131,072 | -1.02% | +0.13% |
| 131,073 | +0.46% | -0.25% |
| 463,347 | -28.54% | -27.90% |

These are API-loop results, not whole-application speedups. Sixteen pairs of
the unchanged example applications produced:

| Scenario | C elapsed change | Direct LLVM elapsed change |
| --- | ---: | ---: |
| Contact import/export | -0.65% | -0.40% |
| Contact search | -0.08% | +2.50% |
| Lending-desk session | -0.54% | -0.42% |

The initial LLVM search cost is retained, not discarded. Its 106 native
instructions are unchanged after relocation normalization, but the function
entry moves by 16 bytes. A follow-up used two labels for each exact binary
in 16 balanced rounds: independent old/new comparisons moved +1.36% and
-0.35%; identical-old and identical-new comparisons moved +0.51% and -1.08%.
The initial cost did not consistently reproduce; neither a search gain nor
a universal absence of regressions is established. Application peak RSS was
mostly unchanged; raw RSS ranges and API-loop values are in the JSON.

Files are warm and stdout is redirected. Affinity does not control host
frequency, thermal state or competing work. No cold-storage, production
query-distribution or cross-target performance claim is made.

### Correctness

Both backends passed 37 affected tests each: filesystem, atomic files,
contact book and lending desk. The new permanent test checks exact bytes,
Unicode scalar counts, NUL and invalid UTF-8 around buffer boundaries.
Actual release-CLI ASan/UBSan read-all consumers passed nine sizes per backend.
Additional generated-runtime smokes exercised short reads/EINTR, the exact
INT32 Text-size boundary and overflow, read-error precedence over close
errors, close-only failure and unchanged `readSome`. With existing writable
storage and new allocations denied, the old EOF helper failed allocation;
the new helper completed without growing. All affected application outputs
and independent persistence-state oracles matched.

## Heap-backed Text reallocation

[text-realloc-controlled.json](../benchmarks/text-realloc-controlled.json)
records the shared native `Text` growth change against the retained
`df7f44de` compiler, including the preceding whole-file read tail fix.
Already heap-backed storage now uses `ncr_reallocate`; inline spill still
allocates and copies. Capacity rounding, size checks, UTF-8 validation,
clear/refill semantics, alias rejection and ownership are unchanged.
No allocator, inline capacity, public API or compiler policy was changed.

### Allocation diagnostics

Reallocation can retain storage or move it and copy internally. Requested
bytes are not copied, physical, live or peak bytes. Separate instrumented
contact-book runs perform 120 load/save pairs over 10,000 contacts:

| Diagnostic, identical on C and LLVM | Baseline | Retained |
|---|---:|---:|
| Nacre allocations and frees, each | 4,405,563 | 4,405,563 |
| Nacre live | 0 | 0 |
| Intercepted cumulative requested bytes | 494,346,704 | 494,346,704 |
| Intercepted malloc calls | 1,202,402 | 1,200,722 |
| Intercepted realloc calls | 1,440 | 3,120 |

The 1,680 eligible heap growths convert 180,868,320 requested bytes from
malloc to realloc, not remove those requests. Nacre's existing reallocation
accounting records a logical allocation/free pair. Inline spills and
unrelated allocation sites retain their paths.

### Matched elapsed time and peak RSS

Measurements reuse archived baseline consumer binaries, the same sources,
Clang 22.1.8/llvm-link 22.1.0, `-O3 --native`, CPU 0 and the existing
wait4 supervisor. Fresh processes run in balanced order under the benchmark
lock; no sanitizer, allocation interposer or profiler is used in timed runs.
Positive paired elapsed change means slower:

| Application, 16 pairs/backend | C | LLVM |
|---|---:|---:|
| Contact roundtrip | -1.478% | +0.224% |
| Contact search | -0.662% | -0.139% |
| Lending session | -0.787% | -0.253% |

Application elapsed time is mostly neutral. Median peak RSS falls 13.36%
and 13.39% for C/LLVM search, and 4.32% and 4.71% for lending.
Roundtrip RSS does not improve: C rises 64 KiB and LLVM 6 KiB, with
overlapping baseline/candidate ranges. Full byte values and ranges are
retained in the JSON.

API loops accumulate byte lengths and sampled bytes on every iteration and
check the final total against an independent oracle. The UTF-8/NUL token
is seven bytes; full-content correctness is checked separately.
These are API results, not application speedups:

| Fresh Text construction, 12 pairs/backend | C | LLVM |
|---|---:|---:|
| Eight 57,344-byte appends | -96.697% | -97.480% |
| 65,536 seven-byte appends | -66.144% | -67.068% |

Initial inline, 1,024-append and clear/refill runs lasted only about 1–11 ms.
Their complete raw data, including apparent refill costs of 6.09%/15.62%,
are retained but are not used to decide retention. Adaptive calibration
then required a fastest pilot of at least 20 ms and targeted 200 ms.
Sixteen counterbalanced four-label rounds use two labels for each exact
binary, without changing flags or layout:

| Longer API controls | C oldA→newA | C oldB→newB | LLVM oldA→newA | LLVM oldB→newB |
|---|---:|---:|---:|---:|
| Inline seven-byte Text | +1.889% | -0.638% | -0.011% | -0.195% |
| 1,024 seven-byte appends | -6.300% | -5.071% | -5.057% | -4.204% |
| Clear/refill retained capacity | +1.842% | +0.255% | +0.528% | +0.800% |

For C refill, identical candidate labels differ by -2.271%; identical
baseline labels differ by +0.648%. The initial large refill cost does not
reproduce at meaningful durations. The measured growth benefit and absence
of a confirmed material regression justify retention, not a universal
zero-regression claim. Warm caches, glibc, one host and uncontrolled
frequency/thermal state limit extrapolation.

### Behavior and ownership

Both backends pass 63 affected native tests each, including a new
full-byte/scalar regression across repeated growth, UTF-8/NUL,
clear/refill, independent clones and exact-size owned replacement storage.
All 132 borrowing frontend tests pass. Actual `-O3 --native` CLI probes
pass ASan/UBSan with live=0 before and after; direct, retained-borrow and
projected self-appends retain E3001 rejection.

A private probe includes the actual generated C runtime and forces moving
and in-place allocation behavior. Bytes and ownership survive both;
inline spill remains a raw allocation. Allocation failure and INT32
overflow retain exit 70 and identical source-located diagnostics.
Application stdout and independent persistence-state oracles match on
both backends. Strict Clippy for the changed test target and the normal
release build pass; the installed compiler matches the measured candidate.

## Signed 32-bit Text append formatting

[text-int-format-controlled.json](../benchmarks/text-int-format-controlled.json)
records a bounded change to `Text.appendInt` against the retained `55756bef`
compiler. The shared C/LLVM native method now backfills decimal digits into
an 11-byte stack buffer, adds the optional minus sign, and appends the span
through the existing Text helper. Unsigned magnitude avoids signed negation
overflow for INT32_MIN; a do/while emits zero correctly.

Every signed 32-bit value fits ten digits plus a sign, so general snprintf
format parsing and its unreachable formatting-failure branch are removed.
Text-size/OOM diagnostics, growth policy, inline capacity, ownership and
UTF-8/NUL behavior are preserved. Other numeric append methods, public APIs,
allocator and compiler/alias/inlining/ISA/layout policies are unchanged.

### Frozen gate and matched application results

The approved gate requires at least 3% median paired contact-roundtrip
elapsed improvement on both backends in both independent 16-pair batches,
and rejects reproducible greater-than-2% costs in search, lending or mixed
sign/width integer controls. Initial observations remain recorded; noisy
signals are checked with identical-binary labels, not layout tuning.

Archived baseline application binaries are reused without rebuilding.
Both versions use the same Clang 22.1.8/llvm-link 22.1.0 toolchain and
`-O3 --native`. The existing wait4 supervisor records elapsed time and peak
RSS in fresh processes on CPU 0 under the benchmark lock. No sanitizer,
interposer, memory counters or profiler is enabled in timing runs.
Batch B reverses scenario/backend order and pair orientation. Files are
warm; frequency, thermal state and other host work are not controlled.
Positive paired elapsed change means slower:

| Application | C batch A | LLVM batch A | C batch B | LLVM batch B |
|---|---:|---:|---:|---:|
| Contact roundtrip | -17.870% | -16.598% | -17.094% | -17.205% |
| Contact search | +2.385% | -1.549% | +1.977% | -0.855% |
| Lending session | -0.958% | -2.011% | +1.167% | -1.502% |

All 64 roundtrip pairs favor the candidate. The application improvement
passes the frozen gate; it is not inferred from the earlier sampling share
or from an isolated formatter benchmark.

C search is borderline in the initial batches. Two additional independent
16-round counterbalanced four-label cohorts run oldA/oldB on the exact same
baseline binary and newA/newB on the exact same candidate binary:

| C-search confirmation | oldA→newA | oldB→newB | oldA→oldB | newA→newB |
|---|---:|---:|---:|---:|
| Cohort I1 | +0.526% | +1.903% | +1.351% | +3.268% |
| Cohort I2 | -4.060% | -0.742% | -1.061% | +1.306% |

The initial cost does not reproduce in either fixed confirmation cohort;
identical candidate labels also differ materially. These results support
retention without a confirmed greater-than-2% regression, not a universal
zero-cost claim. No binary layout, name, flag or inlining adjustment was made.

### Integer controls, footprint and allocations

Mixed controls cycle through 61 signed values spanning zero, extrema and
every decimal-width transition. Fresh and reused inline storage, and an
already heap-backed destination, are checked against an independent
byte-length/sampled-byte oracle. Adaptive calibration requires a fastest
pilot of at least 20 ms and targets 200 ms using one common count per case
across both versions/backends. Counts are 14,090,647 fresh, 23,216,155 reused
and 6,259,714 heap-backed iterations:

| Mixed integer control | C batch A | LLVM batch A | C batch B | LLVM batch B |
|---|---:|---:|---:|---:|
| Fresh inline destination | -59.853% | -57.637% | -59.232% | -57.559% |
| Clear/refill retained storage | -69.975% | -68.189% | -70.129% | -68.119% |
| Heap-backed destination | -32.838% | -32.921% | -32.367% | -32.515% |

These API gains are separate from the measured application gain above.
Peak RSS is mostly neutral with overlapping ranges; every median/range is
preserved in the JSON. This change is not promoted as a general RAM saving.

Separate allocator diagnostics on both backends preserve Nacre allocations
and frees exactly: roundtrip 4,405,563 each, search 53,266, lending 416,811;
all live counts are zero. Intercepted cumulative requested bytes also stay
identical: 494,346,704, 5,707,695 and 23,705,716 respectively. Formatter
conversion adds no heap allocation; Text growth still uses its existing path.

### Correctness

Actual old/new release CLI probes verify an independent 8,263-byte oracle
over 61 values with empty, UTF-8/NUL, inline-spilling and heap prefixes,
growing accumulation, clone independence, scalar counts and clear/refill.
C/LLVM runs pass ASan/UBSan with live=0. All application stdout and independent
persistence-state oracles match in both batches and confirmation cohorts.
A private probe includes the actual generated C formatter: exact bytes,
zero growth allocations for inline formatting, and unchanged source-located
exit-70 Text-size/inline-OOM/heap-OOM diagnostics pass before and after.
The overflow check is a guard probe, not a new 2 GiB payload test.

Both backends pass 108 affected native tests each: Text/regex, Text map keys,
multi-source native consumers, Unicode, filesystem, contact book and lending.
The borrowing (132) and multi-source frontend (73) suites pass once: 421
targeted tests total, not a full-suite rerun. The new permanent regression
uses an independent Rust decimal oracle for the complete bytes and scalars
across the boundary/storage cases above. Strict Clippy for `text_regex` and
the normal release build pass. The installed `fab71cd5` compiler is identical
to the measured candidate; its actual C/LLVM ASan/UBSan CLI oracle passes.

## Trusted owned byte slices

`Text.sliceBytes` already checks bounds and UTF-8 scalar-aligned endpoints.
Its successful result now uses the same allocation, initialization and trusted
byte-copy sequence as `Text.clone`, instead of routing through the generic
validated constructor. The result still owns independent storage. Generic
`Text` construction/append, the UTF-8 decoder/validator, scalar `slice`, `trim`
and `split` are unchanged. Inline capacity, growth and allocation policy are
unchanged; there is no borrowed view or global validation bypass.

Raw pairs, RSS, source/compiler provenance, fixture configuration, control
iteration counts, allocation observations and lending confirmation cohorts are
in [`text-slice-owned-controlled.json`](../benchmarks/text-slice-owned-controlled.json).

### Matched application gate

The existing `fab71cd5` C/LLVM application binaries were reused without
rebuilding. Candidate binaries use the same Clang 22.1.8 / LLVM link 22.1.0
configuration. Measurements use fresh uninstrumented processes, CPU 0 affinity,
wait4 supervision, a shared lock, and balanced within-pair order. Batch B
reverses scenario/backend order and pair orientation. Values below are median
individual paired elapsed-time changes, not ratios of separate medians;
negative means faster. Each cell contains 16 pairs.

| Application | C batch A | LLVM batch A | C batch B | LLVM batch B |
|---|---:|---:|---:|---:|
| Contact roundtrip | -9.716% | -7.126% | -9.982% | -9.925% |
| Contact absent search | -0.370% | +0.241% | -1.641% | -0.151% |
| Lending session | +3.266% | +0.780% | -0.135% | -0.668% |

The frozen gate requires at least 3% roundtrip improvement on both backends in
each independent batch, with no reproducible control regression above 2%.
Roundtrip improves in 61 of 64 pairs. The C lending increase in batch A does
not reproduce in batch B. Two additional untuned 16-cycle four-label cohorts
compare two labels for each unchanged binary:

| C lending confirmation | old1→new1 | old2→new2 | old1→old2 | new1→new2 |
|---|---:|---:|---:|---:|
| I1 | -0.714% | +0.636% | -0.535% | +0.713% |
| I2 | -0.653% | -2.192% | +1.045% | +0.137% |

No greater-than-2% regression repeats in these confirmations. No binary layout,
name, flags or inlining adjustment was made to address the initial result.
The elapsed-time gate passes; this is not a universal zero-cost claim.

### Slice controls and memory

Eleven in-memory controls vary ranges at runtime and check exact independent
Python oracles for accepted/rejected counts, byte-length/sampled-byte checksums
and source preservation. Sources include NUL, dense Unicode, append-built text,
empty slices, both sides of the 23-byte inline threshold, tails and rejected
scalar-splitting ranges. Common iteration counts are calibrated separately
per case, with a fastest pilot of at least 20 ms and a target of 200 ms.

| Slice control | C batch A | LLVM batch A | C batch B | LLVM batch B |
|---|---:|---:|---:|---:|
| Tiny/NUL | -17.679% | -14.521% | -16.453% | -11.269% |
| Short/NUL | -19.758% | -22.186% | -18.517% | -18.380% |
| Empty | -9.108% | -6.066% | -9.703% | -8.825% |
| Inline boundary, 23 bytes | -24.386% | -22.207% | -24.302% | -21.255% |
| Heap boundary, 24 bytes | -24.294% | -26.019% | -24.475% | -24.028% |
| Longer ASCII, 256-byte source | -66.781% | -66.993% | -64.800% | -64.410% |
| Unicode-heavy | -20.808% | -21.725% | -17.873% | -21.921% |
| Mixed ASCII/Unicode | -14.660% | -16.630% | -16.458% | -15.842% |
| Tail/alignment | -27.558% | -18.772% | -27.795% | -18.789% |
| Invalid-range mix | -12.265% | -3.278% | -12.383% | -5.873% |
| Append-built source | -19.727% | -14.609% | -18.118% | -19.606% |

Application logical allocation/free counts and process allocator call/requested
byte totals remain identical on both backends; live counts and allocation
failures are zero. RSS ranges overlap; no general RAM or physical-copy saving
is claimed. The supervisor records elapsed time and peak RSS, not CPU time.
The full application improvement is not attributed exclusively to the earlier
sampled validation subgroup; compiler optimization effects are not isolated.

### Correctness

Actual baseline/candidate C and LLVM CLI probes pass a 4,670-byte independent
oracle under ASan/UBSan, with 1,119 allocations/frees and zero live allocations.
They cover 441 range pairs including signed extrema, 36 storage-boundary
slices, and source clear/growth/drop independence at 18 lengths, as well as
nested Unicode, empty/NUL data, cloned errors, construction/append and invalid
filesystem UTF-8/NUL paths. All application stdout and independent persistence
oracles pass. Both backends pass 70 affected native tests each across
Text/regex, filesystem, Unicode, Text map keys, binary streams/codecs and
encoding, including the new inline/heap slice-lifetime regression. These are
targeted checks, not a full repository suite.
Strict Clippy for `text_regex` and the normal release build pass. The installed
`d7bdfcb2` compiler is byte-identical to the measured candidate; its actual C
and direct-LLVM builds pass the same ASan/UBSan API oracle with zero live
allocations.


## Text substring cleanup, append fast path, and lending index

### Substring cleanup and append fast path

Following the verification of `Text.sliceBytes`, the trusted copy pattern
was extended across the remaining standard-library `Text` substring methods:
scalar `Text.slice`, `Text.trim`, and `Text.split`. In each case, character
boundaries are already verified before construction, so redundant UTF-8
revalidation is eliminated while preserving independent heap/inline storage,
capacity rounding, errors, and ownership.

In addition, `Text.append` introduces a short UTF-8 validation fast path for
0-, 1-, and 2-byte strings (single ASCII bytes and 2-byte ASCII combinations
are guaranteed RFC 3629 compliant without invoking the multi-branch scalar decoder),
and direct 1-byte byte store into `inline_storage` or heap buffer avoiding `memcpy`.

### Lending desk active loan indexing

The post-slice profile identified that 43% of lending session cycles were
spent in `DeskState.hasActiveLoanFor` performing full $O(N)$ catalog scans
across 11,600 live resources for 400 non-borrower member removals (~4.64 million
redundant availability checks).

`DeskState` now indexes active borrower counts in an internal `Map<int,int>`.
Loan creation, returns, and catalog loading/removal update the index, reducing
`hasActiveLoanFor` to an $O(1)$ map lookup. In the lending-session workload,
execution time drops from 330.75 ms to 182.12 ms (~45% faster) with exact
stdout and persistence oracles verified byte-for-byte on both C and direct-LLVM backends.

