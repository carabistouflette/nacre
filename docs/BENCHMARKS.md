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

