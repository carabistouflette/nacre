# Nacre 0.12.0 verification

Nacre is experimental pre-1.0. Executed checks are evidence, not a safety certification.
[validation-0.12.0.json](validation-0.12.0.json) records portable local results.
Historical reports retain their original release labels; they are not current-release proof.

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
