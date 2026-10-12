# Nacre V9 / 0.10.0 — observed release verification

Executed evidence is not a safety certification. [V9.md](V9.md) defines the
bounded milestone and exclusions; [v9-validation.json](v9-validation.json)
records portable local results. The lending workflow and exact format are in
[LENDING_DESK.md](LENDING_DESK.md), with the major feature-to-function matrix in
[LENDING_DESK_FEATURES.md](LENDING_DESK_FEATURES.md).

## Compiler and native execution

| Check | Observed result |
|---|---|
| Feature-enabled Rust suite | 989 passed across 32 suites; two LLVM tests ignored by the ordinary suite |
| V9 behavioral regressions | 16 map/lending tests, including affine collision-chain deletion |
| Clippy | All targets, fuzzing feature, `-D warnings`: passed |
| Minimum Rust | 1.74.0 locked/offline all-target compilation: passed |
| Standard generated native validation | 414 successful GCC/Clang ASan/UBSan runs; 14 expected fatal runs; 420 invalid-source rejections |
| LLVM | 11 emission/round-trip cases; both ordinarily ignored tests explicitly executed |
| Dedicated V9 generated programs | 30 successful GCC/Clang ASan/UBSan runs, all reported `live=0` |
| Packaging audit | 62 `include_str!` paths checked; 991 declared Rust tests counted, not executed by this audit |

Map scenarios exercise full signed keys, absent lookup/removal, duplicate
replacement, growth, collision-chain head/middle/tail removal and reinsertion,
object/inline-enum/interface ownership, borrowed references and ancestor/root
invalidation. Affine backward-shift deletion was identified as a missing
high-risk case by read-only review; explicit representation-specific behavior
coverage was added and passed. Review found no concrete implementation defect,
but that source-level result is not a substitute for native execution.

Permanent generated-program allocation tests compare insertion baselines with
lookup/replacement/removal/clear/reuse and sweep replacement at sizes 1 through
1024. No absolute buffer capacity/default allocation count is pinned. These
operations add no allocation; insertion/growth and user-created payloads can.

Lending scenarios cover polymorphic books/tools, member relationships,
availability transitions, guarded removal, owned label replacement and exclusive
borrowed append, logical-day overflow, stable IDs/order, optional startup loading,
Unicode/path-with-spaces snapshots and candidate rollback. Malformed cases
include overflowing metadata, trailing data, UTF-8 field boundaries, missing
borrowers and duplicate IDs. EOF during resource creation and lending is checked
with a read-only post-run observer in a throwaway copied source bundle; ordinary
scenarios use the shipped bundle unchanged.

Positive memory reports balance allocations/frees. Rejected companions fail for
map aliasing (`E3001`), movement (`E3002`), local view values (`E5005`), local-view
escape (`E3007`) or borrowed-owner transfer, rather than a parser failure.

## Maintained commands

From the repository root, with locked dependencies already cached for offline
commands:

```sh
cargo test --offline --locked --features fuzzing --no-fail-fast
cargo clippy --offline --locked --all-targets --features fuzzing -- -D warnings
cargo +1.74.0 check --offline --locked --all-targets
python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm
python3 scripts/test_stdlib_support.py --cc gcc --cc clang
python3 scripts/audit_package.py
cargo run --offline --locked -- run examples/maps_v9.ncr --memory-stats
cargo run --offline --locked -- run examples/lending_desk --memory-stats
```

The standard native validator re-executes the suite for each selected C compiler.
The standalone fixture checks exact trusted C templates and 50,000 randomized
list operations per positive run; it is explicitly not generated-program proof.
Both compilers pass O2 and ASan/UBSan with balanced reports. Throwaway observer
sources are removed after execution.

## Fuzzing

Real bounded campaigns executed 377,150 frontend inputs and 2,941,757 LSP framing
inputs, each for 31 seconds, without observed crashes or timeouts. The frontend
starts from six intentional seeds including maps and traversal; framing uses
eight. Corpus mutations are isolated and removed. Peak RSS was 420 MiB/frontend
and 359 MiB/framing. Finite campaigns are not a general safety proof.

```sh
cargo install cargo-fuzz --version 0.13.2 --locked
python3 scripts/fuzz_smoke.py
```

## Editor packages and desktop evidence

Packages are VS Code/Zed 0.5.0 and grammar 0.4.0. All 24 snippets compile against
the real compiler. The grammar parses 45 examples, executes all four Zed queries,
and passes the locked WASI build. Headless checks do not certify actual syntax
colors or interactive client lifecycle behavior.

The actual V8 desktop baseline remains in
[v8-editor-verification.json](v8-editor-verification.json). Actual V9 acceptance
passes in official VS Code 1.141.0 and Zed 1.23.2+stable, with Nacre 0.5.0
extensions and compiler 0.10.0. [v9-editor-verification.json](v9-editor-verification.json)
records map colors, live loan rejection/clearing, unsaved cross-file declaration
edits/undo, restart recovery, save/reopen, the clean lending bundle and VS Code
mapref completion insertion. Parent inspected retained syntax/snippet and live
diagnostic screenshots. All owned temporary desktop resources were cleaned up.

## Remote CI

All six V9 jobs passed for source commit
`bdff6c15b707bf2239c482d873cee4a4e9585bd2`:
[run 38312906936](https://github.com/carabistouflette/nacre/actions/runs/38312906936).
The matrix covers Rust 1.74/stable × GCC/Clang, editors/locked WASI, and real
nightly fuzzing. An evidence-only follow-up records this exact source run without
changing executable sources. Hosted CI emitted non-failing Node 20 deprecation
and upcoming ubuntu-latest migration notices; toolchain checks passed.

The historical V8 baseline remains
[run 38303938675](https://github.com/carabistouflette/nacre/actions/runs/38303938675);
it is not relabeled as V9 evidence.

## Boundaries and publication

Loan lifetimes are lexical and roots conservative. V9 maps accept only int keys
and provide no map traversal or per-key alias domains. No custom hashing, NLL,
lifetime parameters, first-class borrowed iterators, arbitrary partial moves,
fallible allocation, exception unwinding, modules, threads, FFI, inheritance, or
direct LLVM backend are provided. Very deep owned trees can exhaust the C cleanup
stack. Runtime hash tables do not promise hash-flooding resistance. Untrusted
programs are not sandboxed; native Windows/macOS are not certified.
Atomic replacement promises visibility, not crash durability or old permissions.

Published summaries omit workstation paths, process IDs, session-local artifact
references and private screenshot locations. Curated UI proof stays outside Git;
owned desktop downloads/profiles/workspaces/caches are removed. The original MIT
attribution is retained as required, not relabeled.
