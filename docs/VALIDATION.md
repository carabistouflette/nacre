# Nacre 0.12.0 verification

Nacre is experimental pre-1.0. Executed checks are evidence, not a safety certification.
[validation-0.12.0.json](validation-0.12.0.json) records portable local results.
Historical reports retain their original release labels; they are not current-release proof.

## User-thunk qualifier boundary and binary append hot path

The user-thunk change erases only slot-address qualifiers at the `void *` ABI
boundary. A permanent interface regression covers owned Text, a shared Text
borrow that remains live after dispatch, and owned `Result<Text, Text>` returns.
The ByteBuffer change skips reserve calls within existing capacity; it leaves
overflow checks and the growth/allocation policy intact.

| Executed check | Observed result |
| --- | --- |
| Strict interface regression before/after the slot cast | Failed before with Clang qualifier errors; passed afterward with `-Werror=incompatible-pointer-types-discards-qualifiers` |
| Release direct CLI interface consumer, `-O3 --native --memory-stats` under the same strict warning | Output `1`, `go!`, `!`; 3 allocations / 3 frees / live 0 |
| Release direct CLI full workload consumer under the strict warning | Built successfully; binary checksum `2457183`; 21 allocations / 21 frees / live 0 |
| Release C and direct LLVM buffer boundary consumers, `-O3` | Both passed zero/preallocated capacity, fill-to-capacity, one-byte growth, preserved payload, clear, and refill checks; 5 allocations / 5 frees / live 0 |
| Full C suite with `--offline --locked --features fuzzing -- --include-ignored --test-threads=4` | 1,349 passed across 68 suites |
| Final LLVM full suite, cache disabled, explicit Clang 22, no concurrent build jobs | 1,349 passed across 68 suites |
| Final six-workload C/direct comparison | All 60 timed samples and calibration/allocation runs matched Rust checksums; allocation counts unchanged and live 0 for both backends |
| All-target Clippy with `--features fuzzing -- -D warnings` | Passed without lint suppressions |

The first concurrent full LLVM run reported three ZIP failures. The isolated
six-test ZIP suite subsequently passed both as a direct test executable and
through Cargo; the empty-archive release CLI consumer also passed with both the
unchanged and final compilers at `-O2` and `-O3`, resolving libarchive 3.8.10.
The cause of that initial failure has not been established.
The final full LLVM run passed all 1,349 tests without concurrent build jobs,
with executable/runtime caches disabled and Clang selected explicitly for C
fixtures as well as direct compilation. This successful run does not explain
the earlier intermittent ZIP result.


CPU profiles, the isolated capacity-guard experiment, and executable footprint
are recorded in [the binary optimization report](BENCHMARKS.md#binary-append-hot-path-optimization).

## JSON native-adapter optimization

The JSON investigation reused `2e4e1ad`'s existing runtime report as the
unchanged baseline. The final comparison retained identical Nacre/Rust workload
source hashes and matching LLVM 22 tools. [Profile, runtime results, rejected
experiments, and footprint cost](BENCHMARKS.md#json-native-boundary-optimization)
are recorded separately from the earlier private-linkage measurements.

| Executed check | Observed result |
| --- | --- |
| Final LLVM full suite, `--offline --locked --features fuzzing -- --include-ignored --test-threads=4` | 1,348 passed across 68 suites |
| Final all-target Clippy with `--features fuzzing -- -D warnings` | Passed without lint suppressions |
| Final six-workload C/direct comparison | All 60 timed fresh-process samples, calibration samples, and separate allocation runs matched Rust checksums; tracked allocations/frees balanced with zero live allocations |
| Additional five-pair Sudoku/Life comparisons | Same-count before/after production executables did not reproduce apparent slowdowns from the sequential historical comparison |
| Actual `-O3 --native --memory-stats` UTF-8 file-boundary runs: before-direct, after-direct, current C | All rejected four malformed encodings, accepted Unicode plus embedded NUL as 7 bytes / 3 scalars with NUL at offset 0, and reported 35 allocations / 35 frees / live 0 |
| GDB 18.1 after the change, direct `-O0` with Text construction and byte-length calls | Original-source line-4 breakpoint verified; `input=4`, `changed=9`; exit 0 |

The change annotates only generated native ABI adapters, not user functions or
underlying standard-library functions. UTF-8 checks and runtime optimization
flags remain in place. The runtime cache already hashes generated C, so changed
adapter attributes invalidate old runtime bitcode without a separate cache shim.

### Generated-C qualifier warning debt: resolved

Manual replay of the earlier runtime C exposed 21 Clang
`-Wincompatible-pointer-types-discards-qualifiers` warnings. The follow-up in
`src/codegen.rs::direct_user_adapters` explicitly casts argument-slot addresses
to `void *`; owned/borrowed payload qualifiers remain unchanged. No warning
suppression or new optimization attribute was added. The strict interface
regression failed before the fix and passed afterward; an actual release build
of the full workload consumer also passed with this warning promoted to an
error. Current verification is recorded in the user-thunk/binary section above.


## Runtime backend comparison and private LLVM linkage

The runtime investigation compared an independently built `0bede0f` compiler
with whole-program-private LLVM linkage, using identical current workload
sources, Clang 22.1.8 at `-O3 --native`, `llvm-link` 22.1.0, and CPU 0 affinity.
[Runtime measurements and controlled linkage evidence](BENCHMARKS.md#cdirect-llvm-runtime-comparison)
record all six workloads, compiler/source hashes, RSS, allocation counts, and
measurement limits. Life improved substantially; the other five workloads
still favored C, and no allocation reduction is claimed.

| Executed check | Observed result |
| --- | --- |
| Final `NACRE_TEST_BACKEND=llvm cargo test --offline --locked --features fuzzing -- --include-ignored --test-threads=4` | 1,348 tests passed across 68 suites, including interface/vtable consumers |
| Final `cargo clippy --offline --locked --all-targets --features fuzzing -- -D warnings` | Passed |
| C/direct runtime comparison, baseline and final compiler | Five alternating fresh-process samples per workload/backend; all 60 timed samples per revision and all calibration samples matched Rust checksums |
| Separate allocation builds, both revisions/backends | All six workloads had balanced tracked allocations/frees and zero live allocations |
| Existing Nacre/Rust mode with `--repeats 1 --rss-repeats 1` | Actual build, preflight, timing, RSS, and allocation paths completed |
| GDB 18.1, final private-function build at `-O0` | Original-source line-3 breakpoint verified; argument `input=4` and initialized local `changed=6` inspected; executable exited 0 |


## Native backend parity, diagnostics, and source debugging

The 2026-10-15 run used Clang 22.1.8, `llvm-link` 22.1.0, and the corrected
libarchive 3.8.10 dependency. Ordinary native behavior tests now select C or
direct LLVM through `tests/support/mod.rs`, without a generated-C/source lookup.
Injected-C fault fixtures and standalone C helpers remain explicitly C-specific.

| Executed check | Observed result |
| --- | --- |
| `NACRE_TEST_BACKEND=c cargo test --offline --locked --features fuzzing -- --include-ignored --test-threads=4` | 1,348 passed across 68 suites |
| `NACRE_TEST_BACKEND=llvm cargo test --offline --locked --features fuzzing -- --include-ignored --test-threads=4` | 1,348 passed across 68 suites |
| `cargo clippy --offline --locked --all-targets --features fuzzing -- -D warnings` | Passed without lint suppressions |
| Actual release CLI `doctor`, `doctor --direct`, `doctor --zip`, and `doctor --direct --zip` | All exited 0; native output and allocation balance checked; ZIP modes verified a binary round-trip and empty archive ownership |
| Actual release CLI with missing Clang, missing `llvm-link`, and Clang 23 / `llvm-link` 22 | Each exited 1 with the selected path or incompatible majors and actionable environment-variable guidance; no premature compatibility-success banner |
| GDB 18.1, two-file direct build at `-O0` | Original-file breakpoints verified; step-in and next followed Nacre source; integer arguments, initialized locals, recursive frames, and match-arm bindings inspected |
| GDB 18.1, direct scalar-width build at `-O0` | All six arguments and their initialized local copies retained exact values, including a non-BMP Unicode scalar and a full-width signed `long`; executable exited 0 |
| Release build-stage profile, baseline and final compiler | 72 compiled-and-executed samples per revision passed output and allocation checks; [cold/warm timings and stage limits](BENCHMARKS.md#release-compiler-build-stage-profile) recorded for counter, text/map, and transitive-package consumers |

The debugger run observed `input=4`, then `changed=6`; a guarded success arm
exposed only its `value=6` and the enclosing `observed=0`, not inactive arms'
duplicate or uninitialized bindings. Recursive frames in a second `.ncr` file
retained distinct `n=0`, `n=1`, and `n=2` values; the middle frame retained
`adjusted=2`. After `?` propagated the base-case error, the caller's initialized
`error` binding was `9`. The executable printed `15` and exited 0.

The scalar run inspected byte `255`, character U+1F600 (`128512`), integer `-7`,
long `9223372036854770000`, double `1.25`, and boolean `true`, both as arguments
and as locals after all six initializations. Incoming ABI addresses now remain
available when argument registers are reused at `-O0`. The Unicode scalar uses
the standard UTF-32 `char32_t` debug representation: calling a 32-bit UTF type
`char` caused GDB's character printer to crash, while `char32_t` rendered the
same code point correctly.

These checks exposed and fixed missing implicit callsite locations, runtime
helpers diverting source-level stepping into deleted C files, function-wide
pattern-local visibility, and LLVM's 16-bit debug-column limit on a 250,000-digit
numeric input. Unrepresentable columns use LLVM's unknown-column value, not a
fabricated source column. Aggregate and borrowed debug types remain unavailable;
optimized source metadata does not guarantee variable availability.


## Earlier iterator, pattern, LLVM, and package integration

The integration run used matching LLVM 22 tools (Clang 22.1.8 and `llvm-link`
22.1.0), plus the corrected libarchive 3.8.10 built by
`scripts/build_libarchive.py`. The system Clang 23 / `llvm-link` 22 combination
failed to read runtime bitcode; matching LLVM major versions are required.

| Executed check | Observed result |
| --- | --- |
| `cargo test --offline --locked --features fuzzing -- --include-ignored --test-threads=4` | 1,342 passed across 68 suites; normally ignored LLVM cases executed |
| Isolated replay of 40 existing runtime suites | 523 tests passed; 503 exact-source native compilations used direct LLVM; 18 C-only helpers or injected-C fixtures remained C |
| `cargo clippy --offline --locked --all-targets --features fuzzing -- -D warnings` | Passed without lint suppressions |
| Tree-sitter 0.27.1 corpus | All seven cases passed |
| Actual recursive-pattern and fused-pipeline editor inputs | Parsed without errors; updated Zed highlights query executed |
| Package creation, local dependency addition, implicit manifest discovery, release build/run | Generated projects coexisted; C and direct LLVM printed `42` |
| Updated CI workflow | YAML parsed locally; matching LLVM tool selection configured; remote workflow not executed here |

Cache-disabled actual CLI runs produced identical stdout and allocation counters
for both backends. Every consumer finished with zero live allocations.

| Consumer | Stdout | Allocations | Frees |
| --- | --- | ---: | ---: |
| Nonescaping counter | `42` | 0 | 0 |
| Lazy filtered/taken pipeline | `1 2 2 4 3 6 10 9` in callback/output order | 9 | 9 |
| Recursive patterns | `patterns-ok` | 7 | 7 |
| Propagated owned error | `7` / `preserved` | 1 | 1 |

The full suite caught a checker stack overflow on a long boolean expression.
Boxing the large iterator-plan payloads restored the existing expression's
behavior without increasing thread stacks. Direct object replay also exposed
missing stack promotion; sharing the C eligibility analysis restored all 41
object tests' original outputs and allocation counts.

The isolated replay used a temporary adapter that mapped exact generated C back
to its original Nacre source and invoked the direct compiler. Modified C and
standalone helpers were counted separately rather than claimed as LLVM proof.
Address/undefined sanitizer flags and existing allocation expectations were
preserved. This is observed runtime coverage, not a claim that C was eliminated
from the standard-library implementation.

The final direct link reads textual user IR, imports only needed runtime
symbols, internalizes those imports, and optimizes the combined module once.
Cached runtime bitcode avoids repeated C parsing, but the measured direct build
still took longer than the C build for the recorded small consumer. See
[the build comparison](BENCHMARKS.md#direct-llvm-build-comparison).


## Canonical generic-name construction

Graphify located the generic-name formatter and its expander caller; live
cargo-modules output corroborated the local boundary. One argument writer now
builds canonical names directly in the final buffer, without temporary
per-argument strings or vectors.

A throwaway allocator probe compiled the production parser, resolver, prelude,
and generic expander directly. For `examples/language/generic_factories.ncr`,
100 expansions fell from **1,202,000 to 1,139,100 allocation/reallocation calls**.
Parsing, resolution, prelude injection, and input cloning were outside the
counted interval. Specialization names and discovery order were identical.
This is a focused allocator result, not a throughput claim.

Actual CLI type reports (2,688 bytes) and generated C (234,061 bytes) were
byte-identical before and after. GCC and Clang ASan/UBSan executions retained
the exact output, with three native allocations, three frees, and zero live
allocations.

The actual CLI also accepted a 4,096-byte specialization name and rejected
4,097 bytes with E4199; the pre-change production expander agreed at both
boundaries. Both GCC and Clang full locked/offline suites with `fuzzing` passed
**1,265 tests across 62 suites**, with two ordinarily ignored LLVM tests.
Strict all-targets Clippy with `fuzzing` and `-D warnings` passed.


## LSP source-admission boundary

Graphify scoped the configuration/discovery helpers inside the LSP server;
live cargo-modules output confirmed their extraction into the private
`lsp::source_files` module. That module depends on shared project predicates
and lexical path normalization, not server document state or protocol output.
The server retains overlays, versions, combined snapshots, and publication.

Before/after actual stdio sessions retained identical type-diagnostic payloads,
resolved cross-file unsaved buffers under a configured directory absent from
disk, preserved document versions, and restored the unresolved-type diagnostic
when the dependent buffer closed. A permanent end-to-end regression covers
that missing-root overlay lifecycle.

Live sessions also exercised hidden-directory exclusion, symlink rejection,
invalid UTF-8, and source paths escaping the project root. Configuration at
64 KiB, project sources at 1 MiB, 64 source files, 4,096 scanned entries, and
directory depth 32 remained enforced. Diagnostics recovered after each rejected
input was removed or corrected.

Both GCC and Clang full locked/offline suites with `fuzzing` passed **1,265 tests
across 62 suites**, with two ordinarily ignored LLVM tests. The dedicated live
LSP suite passed all three scenarios; strict all-targets Clippy with `fuzzing`
and `-D warnings` passed.


## Native-call helper selection

Graphify scoped the duplicated Crypto, HTTP, and Unicode typed-IR walks;
live cargo-modules dependency output confirmed their replacement by the
private `stdlib_codegen::native_usage` leaf. Family policy remains local,
while one exhaustive traversal owns expression and statement coverage.

Both GCC and Clang locked/offline suites with `fuzzing` passed **1,264 tests
across 62 suites**, with two ordinarily ignored LLVM tests. Strict all-targets
Clippy with `fuzzing` and `-D warnings` passed.

An actual combined CLI consumer exercised native calls in returned expressions,
match values, nested call/try arguments, and conditional/loop headers. Before
and after the refactor, generated C was byte-identical (294,827 bytes).
GCC and Clang ASan/UBSan executions retained exact output and balanced native
allocation counters: 12 allocations, 12 frees, zero live allocations.
Separate `-O0` consumers proved that an unused HTTP type declaration does not
link libcurl, while native calls in unexecuted function bodies still select
their required HTTP, Crypto, and ICU helpers and link successfully.


## Stable-toolchain modernization (2026-10-14)

Current compiler, fuzz, and Zed manifests use Rust 2024 and require Rust 1.99.
The repository pins stable Rust 1.99.0, Node 26.12.0, npm 12.2.0, and Python
3.15.0. Older Rust 1.74 records below describe historical executions, not the
current supported minimum.

| Exercised path | Observed result |
| --- | --- |
| Stable Rust, GCC, locked/offline suite | 1,259 passed across 62 suites; two ordinarily ignored LLVM tests |
| Stable Rust, Clang, locked/offline suite with `fuzzing` | 1,264 passed across 62 suites; two ordinarily ignored LLVM tests |
| Clippy, all targets, `fuzzing`, `-D warnings` | Passed without lint suppressions |
| Explicit ignored LLVM tests | Both passed |
| Corrected libarchive 3.8.10 | Release hash verified; complete private patch applied with zero fuzz; prefix-local build succeeded |
| Actual generic-factory CLI under ASan/UBSan | Expected output retained; three native allocations, three frees, zero live allocations |
| Actual C/LLVM validator, GCC and Clang | 414 successful native runs, 14 expected fatal exits, 418 invalid-source rejections, 11 LLVM round trips; sanitizers enabled for successful native runs |
| Real nightly fuzz campaigns | 31,602 frontend inputs and 2,673,327 LSP framing inputs; 31 seconds each, no observed crash or timeout |
| Rust 2024 benchmark reference | Four collection kernels passed paired checksum checks at smoke dimensions, including separate allocator-counter runs |
| Tree-sitter CLI 0.27.1 | All six corpus parses passed |
| VS Code package | Compiler-backed syntax/snippet validation and VSIX packaging passed on pinned Node/npm |
| Zed, Python Tree-sitter 0.26.0 | 63 examples parsed without errors; all four queries executed |
| Zed SDK 0.7.0 | Locked WASI build and prepared release build passed |
| Generated V2–V5 fixtures | All synchronization checks passed |

Parser allocation profiling used the production parser and the same generic-factory
input, excluding lexing: 1,000 parses fell from 116,000 to 96,000 allocator calls
after eliminating the redundant identifier clone. Before/after generated C was
byte-identical. This is a focused allocation result, not a general throughput claim.
Desktop editor hosts were unavailable; parser/query, compiler-backed syntax,
package, and WASI checks do not constitute GUI acceptance.


## Historical portable standard-library integration

The integrated standard-library feature branch was exercised on Linux with
GCC and Clang. Each full locked/offline suite with `fuzzing` passed **1,255 tests
across 62 suites**, with two ordinarily ignored LLVM tests. Rust 1.74.0 also
passed the locked/offline all-targets check with `fuzzing`.

Targeted actual CLI consumers passed GCC/Clang ASan/UBSan execution with balanced
native allocation counters: source/sink binary recovery, streamed domain codecs
in both byte orders, recoverable stream failures, independent SQLite statements,
certificate persistence, exact locale/temporal conversions, and synchronizer
waits and condition-node destruction. Independent Python `struct` output matched
the 39-byte streamed domain representation in both byte orders; OpenSSL DER
conversion matched the exported leaf and ordered three-certificate chain.

Final principle-review regressions exercise two file-backed SQLite connections:
closing or dropping a connection releases its uncommitted transaction's locks
while prepared statements remain owned, and only the second connection's writes
persist. Negative `ListCodec` limits preserve buffer/source reader positions and
budgets without invoking source I/O. Both regression suites passed with GCC and
Clang; the corresponding CLI reproductions passed with sanitizers.

The integrated CLI validator passed 207 successful executions, seven expected
fatal exits, and 209 invalid-source rejections per C compiler, including native
sanitizer runs. Generated V2–V5 sources are synchronized. Strict all-targets
Clippy with `fuzzing` and `-D warnings` passed without lint suppressions; template
consumers use their borrowed sections directly. An actual ordered-map traversal
and composed map/filter iterator CLI consumer also passed both compilers'
sanitizers with ten allocations, ten frees, and zero live allocations.

Native integration uses the corrected libarchive build from
`scripts/build_libarchive.py`; `CPATH`, `LIBRARY_PATH`, and `LD_LIBRARY_PATH` select
its isolated prefix. These are local execution results, not a remote-CI or
whole-Java API-parity claim.
CI exports those paths through `GITHUB_ENV` after the dependency build, rather
than referencing the unavailable `runner` context in a job-level environment.

The CI long-expression stack-overflow reproduction passed on an unoptimized
Rust 1.74.0 build with `RUST_MIN_STACK` unset after separating binary formatting
from recursive operand evaluation. An actual MSRV CLI consumer with 110-term
arithmetic, `&&`, and `||` chains passed GCC and Clang ASan/UBSan runs: all three
results were `true`, skipped division-by-zero operands stayed unevaluated, and
the allocation counters were zero allocations, zero frees, and zero live values.
The post-fix locked/offline all-targets suites with `fuzzing` each passed 1,255
tests across 61 executable suites (two ignored): Rust 1.74.0/Clang and stable/GCC,
both with `RUST_MIN_STACK` unset. Strict Clippy and the package/checksum audits passed.

The subsequent remote XSLT failure was reproduced with an isolated libxml 2.9.14 /
libxslt 1.1.39 build: the required `unicode-1-1-utf-8` label returned an unsupported
encoding error before the fix. After version-independent UTF-8 label classification,
the expanded actual CLI consumer passed GCC and Clang sanitizers against both that
legacy stack and libxml 2.15.4 / libxslt 1.1.45: seven `true` results and balanced
70-allocation/70-free counters, including mixed-case aliases, rejected malformed
suffixes, strict ASCII rejection, and the final ISO-2022-JP shift sequence.
The complete 13-case XML suite also passed Rust 1.74.0/GCC and stable/Clang against
the real isolated legacy libraries. No system libraries were replaced.

## Historical 0.12.0 baseline

The following measurements predate the portable standard-library integration.

| Check | Observed result |
|---|---|
| Nightly Rust suite with `fuzzing` | 1,111 passed across 45 suites; two LLVM tests ignored by the ordinary suite |
| Rust 1.74.0 locked/offline suite | 1,106 passed across 45 suites; two ordinarily ignored LLVM tests |
| GCC and Clang integration | Full compiler suites and generated-program validation passed for each compiler |
| Clippy | All targets, `fuzzing`, `-D warnings`: passed |
| Native validator | 414 successful runs, 14 expected fatal runs, 420 invalid-source rejections |
| LLVM | 11 emission/round-trip cases; both ordinarily ignored tests executed explicitly |
| Runnable catalog | 55/55 programs passed, including filesystem, process, concurrency, networking and applications |
| Native sanitizers | GCC/Clang ASan/UBSan generated-program paths passed |
| Blocked channel TSan | Real blocked-sender abort and last-sender-close receiver wakeup passed |
| Editor source checks | Six grammar corpus cases, 34 compiler-checked snippets, 62 parsed examples and four Zed queries passed |
| Zed extension build | Locked release `wasm32-wasip2` build passed |
| Packaging audit | 147 included source paths; 1,113 declared tests counted, not executed by this audit |

The native suites cover affine array storage transfer, wrapped Deque ownership,
Map/Set collision and key behavior, borrowed enum comparators, UTF-8/NUL Text,
POSIX regex captures, clocks/randomness, filesystem failures and SIGPIPE state,
thread/Guard representations and sendability, channel transfer/closure,
TCP/UDP/deadlines, and literal process execution/environment/redirection/capture.
Network tests use real loopback sockets; filesystem tests use real files and FIFOs.
Process tests exercise dual-stream draining, bounded prefixes, descendant-held
writers, PATH/no-shell behavior and synchronous reaping of SIGTERM-resistant children.
Standalone Process/ProcessOutput consumers compile at `-O0` without Command dependencies.

The blocked-channel observer retains the real `pthread_cond_wait` and mutex.
It reports wait entry while holding the channel mutex; the host then releases
closure through stdin. Closure cannot acquire that mutex before the atomic wait
releases it. Assertions check EOF or the original returned payload, not a startup race.
The two scenarios also passed generated-C ThreadSanitizer execution.

Fatal runtime errors intentionally terminate without stack unwinding. Expected
fatal cases are exit-code checks, not allocation-balance guarantees. Recoverable
and successful scenarios check native ownership cleanup where instrumented.

## Maintained commands

From the repository root, with dependencies cached for offline commands:

```sh
cargo test --offline --locked --features fuzzing --no-fail-fast
cargo clippy --offline --locked --all-targets --features fuzzing -- -D warnings
cargo check --offline --locked --all-targets --features fuzzing
python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm
NACRE_CC=clang cargo test --offline --locked --test features_v4_tools -- --ignored
python3 examples/run.py
python3 scripts/audit_package.py
npm --prefix editors/tree-sitter-nacre test
npm --prefix editors/vscode test
python3 editors/zed/check.py
python3 editors/zed/prepare.py
cargo build --manifest-path editors/zed/dist/nacre/Cargo.toml --target wasm32-wasip2 --release --locked
```

`editors/zed/check.py` requires `tree-sitter==0.26.0`. Source inventory counts
are not executed tests. Template checks alone are not generated-program proof.
Compiler input limits are resource bounds, not a sandbox guarantee.

## Bounded fuzzing

Actual libFuzzer campaigns ran 75,775 frontend inputs and 3,319,006 LSP framing
inputs, each for 31 seconds, without observed crashes or timeouts. Both began
with eight intentional seeds. Peak RSS was 297 MiB/frontend and 387 MiB/framing.
Campaigns use isolated corpus copies; finite runs are not a general safety proof.

```sh
cargo install cargo-fuzz --version 0.13.2 --locked
python3 scripts/fuzz_smoke.py
```

## Equivalent-work comparisons

```sh
python3 benchmarks/run.py --size 10000 --repeats 3 --warmups 1 --allocation-stats --output results.json
```

Both actual programs validate independently calculated checksums. Hash inputs and
mixers match, while table growth/probing policies differ. Timings include target
startup/setup/output/shutdown and exclude compilation and Python startup. A fresh
native supervisor measures direct-child elapsed time and peak RSS. Allocation
runs are separate and untimed; Nacre runtime counters and Rust GlobalAlloc hooks
have different scopes. Rust's three live blocks at its report snapshot include
runtime/global allocations and are not a leak verdict.

For the 10,000-item Text-map workload, Nacre observed 40,017 allocations and frees
with zero live owners, versus 40,021 Rust allocation events. Three process-level
sample medians were 8.766 ms/Nacre and 8.707 ms/Rust. These noisy samples support
no general speed claim. Inline Text storage reduced the earlier native workload's
80,020 events without changing checksums or ordinary affine-object semantics.

## Editing surface

VS Code/Zed packages are 0.7.0 and the grammar is 0.6.0. Source checks do not prove
visible desktop colors, completion, live diagnostics or unsaved overlays.
[editors-0.12.0.json](editors-0.12.0.json) records actual isolated native desktop
acceptance separately. Screenshots, private app profiles and workstation paths
stay outside Git. Any host-profile isolation incident is disclosed in that report.

## Design and boundaries

Implementation follows the required
[design principles](https://rust-unofficial.github.io/patterns/additional_resources/design-principles.html).
Concrete remediations include metadata-driven native Result helpers, opaque
resource factories, constant-time validated Text endpoints, shared Map/Set
storage, direct borrowed enum operands, focused Console/Args/filesystem emission,
correct signal-state preservation, Guard representation access, exhaustive native
sendability and independently emitted Process/ProcessOutput support.

Native systems support is Linux-only. Loans remain lexical and root-wide; keys
and sibling fields do not imply independent alias domains. Ordinary values have
no GC, implicit cloning or implicit reference counting. Explicit synchronization
controls can form cycles; locks and channels can deadlock. Thread drop can block
joining; unreaped Process drop SIGKILLs and synchronously reaps its child without
a process-tree guarantee. Deep owned trees can exhaust the C cleanup stack.
Hash tables do not promise hash-flooding resistance. Allocation failure and
strict boundary violations are fatal. Atomic file replacement promises visibility,
not crash durability or preservation of old permissions. No GUI, reflection,
JVM compatibility, dynamic loading, sandbox or Windows/macOS certification is implied.

## Private publication

CI uses the pinned stable Rust toolchain with GCC and Clang, pinned editor
runtimes and locked WASI builds, and real nightly fuzzing. Publish curated portable
sources only: no credentials, personal
paths, process IDs, app profiles, caches or session-local artifact links. Preserve
the original MIT attribution. Git history is not rewritten.
