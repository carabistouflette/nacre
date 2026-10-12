# Nacre V8 / 0.9.0 — observed release verification

This records executed checks, not a safety certification. The bounded feature
contract and explicit exclusions are in [V8.md](V8.md). Portable machine-readable
local evidence is in [v8-validation.json](v8-validation.json).

## Compiler and native execution

| Check | Observed result |
|---|---|
| Feature-enabled Rust suite | 973 passed across 30 suites; two LLVM tests ignored by the ordinary suite |
| New V8 regressions | 11 behavioral tests for traversal, recoverable extraction, search/filtering and batch persistence |
| Clippy | All targets, fuzzing feature, `-D warnings`: passed |
| Minimum Rust | 1.74.0 locked/offline all-target compilation: passed |
| Standard generated native validation | 414 successful GCC/Clang ASan/UBSan runs; 14 expected fatal runs; 420 invalid-source rejections |
| LLVM | 11 emission/round-trip cases; both ordinarily ignored tests explicitly executed |
| Dedicated V8 generated programs | 12 successful GCC/Clang ASan/UBSan runs, all reported `live=0` |
| Traversal allocation probe | Two additional sanitized generated runs: copied/shared/exclusive and empty traversal added zero language allocations |
| Trusted C-template fixtures | 50,000 randomized list operations per positive run, O2 and ASan/UBSan under both compilers; all reported `live=0` |
| Packaging audit | 59 `include_str!` paths checked; 975 declared Rust tests counted, not executed by this audit |

The generated-program scenarios exercise primitive traversal, interface dispatch,
inline enum element references, recoverable removal/pop, optional payload cleanup,
Unicode contact search, phone filtering, cancellation and confirmed reverse batch
removal, save/reload, and malformed-load rollback. Failure cases include a UTF-8
field boundary, decimal overflow, trailing data, and extreme invalid positions.

EOF at batch confirmation was exercised with a read-only post-loop observer in
a throwaway source copy, proving the in-memory book remained unchanged. The
ordinary contact scenarios execute the shipped source. Allocation probes observe
the actual generated traversal through an instrumented output checkpoint; they
are distinct from the C-template fixture's allocation-delta assertions.

Positive memory reports balance allocations/frees. Traversal structural mutation
rejects with `E3001`; rejected programs do not proceed to unsafe native execution.

## Reproduce the maintained checks

From the repository root:

```sh
cargo test --offline --locked --features fuzzing --no-fail-fast
cargo clippy --offline --locked --all-targets --features fuzzing -- -D warnings
cargo +1.74.0 check --offline --locked --all-targets
python3 scripts/test_stdlib_support.py --cc gcc --cc clang
python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm
python3 scripts/audit_package.py
cargo run --offline --locked -- run examples/collections_v8.ncr --memory-stats
cargo run --offline --locked -- run examples/contact_book.ncr --memory-stats
```

Offline commands assume their locked dependencies are already cached. The
standard native validator re-executes the suite for each selected C compiler.
The C-template harness is explicitly not a substitute for actual Rust-generated
programs. Throwaway probes were removed after execution.

## Fuzzing

Real bounded campaigns executed 461,326 frontend inputs and 3,035,925 LSP framing
inputs, each for 31 seconds, without observed crashes or timeouts. The frontend
starts from five intentional seeds including V8 traversal; framing uses eight.
Mutated corpus copies are isolated and removed. Peak RSS was 426 MiB/frontend and
316 MiB/framing. Finite campaigns are not a general fuzz-safety proof.

```sh
cargo install cargo-fuzz --version 0.13.2 --locked
python3 scripts/fuzz_smoke.py
```

## Editor packages and desktop evidence

Packages are VS Code/Zed 0.4.0 and grammar 0.3.0. All 20 snippets compile against
the real compiler. The grammar parses 39 examples, executes all four Zed queries,
and passes the locked WASI build. These headless checks do not certify actual
syntax colors or interactive client lifecycle behavior.

Both actual desktop applications passed V8 acceptance: VS Code 1.141.0 and Zed
1.23.3, with Nacre extension 0.4.0 and compiler 0.9.0. The checks cover new syntax
colors, live traversal `E3001` and clearing, unsaved cross-file rename/undo,
restart/recovery, save/close/reopen, and real VS Code `foreachref` completion.
Portable acceptance evidence is in
[v8-editor-verification.json](v8-editor-verification.json); parent inspected the
actual screenshots. Xvfb/software rendering and desktop-service setup warnings
are reported separately, not hidden or treated as compiler/editor failures.

## Remote CI

The complete V8 workflow passed on source commit `96d5030`:
[run 38303938675](https://github.com/carabistouflette/nacre/actions/runs/38303938675).
All six jobs succeeded: Rust 1.74.0/stable × GCC/Clang, editor packaging/WASI,
and real nightly fuzzing. The Clang jobs explicitly execute the ignored LLVM
tests. Non-fatal runner notices concern Node action-runtime deprecation and the
upcoming `ubuntu-latest` image migration, not test failures.

The earlier V7 cleanup baseline also passed:
[run 38295939217](https://github.com/carabistouflette/nacre/actions/runs/38295939217).

## Boundaries and publication

Loan lifetimes are lexical and roots conservative. V8 traversal is loop syntax,
not a first-class iterator API. No maps, NLL, lifetime parameters, disjoint-index
borrowing, arbitrary partial moves, fallible allocation, exception unwinding,
modules, threads, FFI, inheritance, or direct LLVM backend are provided.
Very deep owned trees can exhaust the C cleanup stack. Untrusted programs are
not sandboxed. Linux is observed; native Windows/macOS are not certified.
Atomic replacement promises visibility, not crash durability or old permissions.

Published summaries omit workstation paths, process IDs, session-local artifact
references, and private screenshot locations. Desktop proof remains outside the
source publication. Obsolete identity artifacts/reports were deleted rather than
relabeling captured output; the original MIT attribution is retained as required.
