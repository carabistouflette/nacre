# Nacre release verification

Nacre is pre-1.0. Executed checks are evidence, not a safety certification.
[validation-0.11.0.json](validation-0.11.0.json) records portable local results;
[MAP_TRAVERSAL.md](MAP_TRAVERSAL.md) specifies the current traversal contract.
Historical reports retain their original release labels and measurements.

## Compiler and actual generated programs

| Check | Observed result |
|---|---|
| Feature-enabled Rust suite | 998 passed across 33 suites; two LLVM tests ignored by the ordinary suite |
| New behavioral coverage | Eight map traversal regressions and one lending guard-scan regression |
| Clippy | All targets, fuzzing feature, `-D warnings`: passed |
| Minimum Rust | 1.74.0 locked/offline all-target check: passed |
| Standard native validator | 414 successful runs; 14 expected fatal runs; 420 invalid-source rejections |
| LLVM | 11 emission/round-trip cases; both ordinarily ignored tests executed explicitly |
| Dedicated traversal native execution | 16 successful final generated C11 runs under GCC/Clang ASan/UBSan; all `live=0` |
| Packaging audit | 82 `include_str!` paths; 1,000 declared Rust tests counted, not executed by this audit |

Traversal exercises exact key/value associations and entry coverage without
assuming order, signed key boundaries, empty maps, growth and shifted collision
chains, copied boolean/String values, object/inline-enum/interface/nested-collection
borrows, generic headers, evaluated-once borrowed getters and receiver-derived
borrowed returns. Nested shared loops and continue/break/return preserve owned
payload cleanup. Local key changes do not change stored keys. Twenty companions
reject invalid types/arity, temporary owners, ownership extraction, structural
mutation, conflicting nested/exclusive access, ancestor replacement and escape.

Allocation comparison proves traversal adds no iterator allocation. The backend
scans occupied storage directly and captures capacity once under a root loan;
it does not rehash keys or clone owners. Cost is linear in capacity, not merely
live count. C ABI conversion for borrowed map values is shared with ordinary
accessors, not independently reimplemented.

Lending guard queries traverse unordered map entries while listings and snapshots
retain ordered IDs. Native scenarios remove earlier resources, preserve an active
borrower's member record, return the resource, permit later member removal and
compare exact saved records. The existing malformed-load/overflow/startup suite
continues to pass. Command and persistence format contracts remain in
[LENDING_DESK.md](LENDING_DESK.md), with the implementation matrix in
[LENDING_DESK_FEATURES.md](LENDING_DESK_FEATURES.md).

## Maintained commands

From the repository root, with dependencies cached for offline commands:

```sh
cargo test --offline --locked --features fuzzing --no-fail-fast
cargo clippy --offline --locked --all-targets --features fuzzing -- -D warnings
cargo +1.74.0 check --offline --locked --all-targets
python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm
python3 scripts/audit_package.py
cargo run --offline --locked -- run examples/map_traversal.ncr --memory-stats
cargo run --offline --locked -- run examples/lending_desk --memory-stats
```

Standard validation executes the actual compiler and selected C compilers.
Standalone trusted-template fixtures are not a replacement for generated-program
proof. Temporary native sources/executables and observers are removed after
execution. Compiler input limits are resource bounds, not a sandbox guarantee.

## Fuzzing

Real bounded local development campaigns executed 374,503 frontend inputs and
2,931,176 LSP framing inputs, each for 31 seconds, without observed crashes or
timeouts. Frontend uses seven intentional seeds including map traversal; framing
uses eight. Peak RSS was 417 MiB/frontend and 358 MiB/framing. Mutations use isolated
corpus copies. The final source commit additionally runs remote CI fuzzing.
Finite successful campaigns are not a general safety proof.

```sh
cargo install cargo-fuzz --version 0.13.2 --locked
python3 scripts/fuzz_smoke.py
```

## Editing surface

Editor packages are VS Code/Zed 0.6.0 and grammar 0.5.0. All 27 snippets compile
against the real compiler. Four grammar corpus cases pass, 46 examples parse,
all four Zed queries execute, and the locked WASI build passes. Key/value binding
captures, borrow modes and scope isolation are checked behaviorally. Headless
checks alone do not prove desktop colors, unsaved overlays or lifecycle recovery.
[Actual desktop acceptance](editors-0.11.0.json) passes in official VS Code
1.141.0 and Zed 1.23.2+stable, with the new compiler/extensions. It covers actual
loop colors, live E3001 and clearing, unsaved cross-file declaration edits/undo,
server restart recovery, save/reopen, the clean lending bundle and VS Code
mapforeachref completion insertion. Parent inspected syntax/snippet and live
diagnostic screenshots. Owned temporary desktop resources were cleaned up.
An initial VS Code launch accessed existing host shared storage; it was preserved,
and the retained acceptance session used isolated profile/data paths.

## Design review

Implementation follows the required
[design principles](https://rust-unofficial.github.io/patterns/additional_resources/design-principles.html).
List and map binding/receiver rules share helpers; both lower through existing
block/while flow and cleanup. Only the native backend knows map C layout.
Queries have no user-visible side effects; commands state mutation through their
contracts. No callback framework, redundant persistence pass or speculative
iterator abstraction was introduced.

Two tradeoffs are explicit: mutating map commands return transferred owners;
`Catalog.entriesRef()` exposes the standard collection type as a read-only
capability because no opaque traversal-view type exists. It exposes neither
bucket layout nor mutation. Persistence continues to include referenced member
records and validates relationships before replacing live state.

### Native concurrency review

The concrete drift risk was separate sendability rules at thread, mutex and
channel boundaries. `Tables::is_sendable` now owns one recursive decision across
class fields, enum payloads, native-container contents and every concrete
implementation of an interface; all three APIs use it. This is a bounded
single-choice/DRY remediation rather than parallel validation policies.

The ownership risk was an ordinary value acquiring hidden shared lifetime when a
task, mutex or channel crossed threads. Atomic reference counts are confined to
named synchronization control blocks; task results and channel payloads move,
and the only source-visible increments are `cloneHandle` and `cloneSender`.
`Guard<T>` remains non-sendable, with borrows rooted in the Guard receiver.

The code-generation risk was growing a second native registry or a universal
handle abstraction. Concurrency keeps the existing central native router and
places its storage, lifecycle and method handlers in one cohesive backend unit
with shared low-level pthread error/refcount helpers. There is no executor,
callback framework or general-purpose reference-counted value layer.

## Remote source workflow

All six jobs passed for source commit
`3799b28f099582054d5c1320c2fee814c8b97c7c`:
[run 38320069934](https://github.com/carabistouflette/nacre/actions/runs/38320069934).
The matrix covers Rust 1.74/stable × GCC/Clang, editors/locked WASI, and real
nightly fuzzing. Evidence-only follow-ups change docs/checksums, not executable
sources. Non-failing hosted warnings concern Node 20 action deprecation and the
upcoming ubuntu-latest image migration.

The historical 0.10.0 workflow remains
[run 38312906936](https://github.com/carabistouflette/nacre/actions/runs/38312906936);
it is not relabeled as current-release evidence.

## Boundaries and publication

Loans remain lexical and root-wide; different keys are not independent alias
domains. Only int keys are supported. Traversal order is unspecified. The language
has no first-class iterators, NLL, lifetime parameters, partial moves, text/custom
keys, fallible allocation, exception unwinding, FFI, inheritance or a direct LLVM
backend. Native concurrency is POSIX/Linux-only; Windows/macOS are not certified.
Deep owned trees can exhaust the C cleanup stack. Hash tables do not promise
hash-flooding resistance. Untrusted programs are not sandboxed. Atomic replacement
promises visibility, not crash durability or preservation of old permissions.

Publication is curated and portable: no workstation paths, process IDs,
credentials, profiles, caches, session-local artifact references or private
screenshot locations. Curated desktop proof stays outside Git. Required original
MIT attribution is preserved. Ordinary 0.x.y releases replace milestone branding;
historical reports and Git history are not rewritten.
