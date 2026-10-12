# Nacre V7 / 0.8.0 — observed release verification

## Compiler and native behavior

[The current report](v7-validation.json) records **962 passing tests across 27
suites**, with two LLVM tests ignored by default. The native validation command
also ran those two explicitly and passed. Clippy passes with `-D warnings` for
all targets with fuzzing enabled. Rust **1.74.0** all-target locked offline
compilation passes.

`python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm` completed:

| Observed check | Count |
|---|---:|
| Successful generated native runs under sanitizers | 414 |
| Expected fatal runs, checked separately | 14 |
| Invalid-source rejections | 420 |
| C → LLVM → executable round trips | 11 |

All reported successful allocation balances end at `live=0`. The handwritten
stdlib-template fixture also passes GCC/Clang optimized and sanitized profiles;
its scope is the real C templates, not the Rust frontend or generated C.

Additional actual GCC/Clang sanitizer smokes cover nominal list references,
inline enum slot addresses, interface fat-pointer dispatch, receiver-derived
returns, and owned field replacement: output `8, 2, 3, 5`, with 12 allocations,
12 frees, and `live=0` per compiler.

## Contact book

The actual CLI exercises consultation, in-place editing, add/remove, atomic save,
reload, Unicode names, and paths containing spaces. GCC and Clang ASan/UBSan runs
each report **83 allocations, 83 frees, `live=0`**. Edited state is restored by
reload. Invalid UTF-8 field lengths, overflowing decimal metadata, and trailing
data preserve the existing book; the recovery smoke reports 70 allocations and
70 frees, `live=0`. Active-borrow deletion is rejected with `E3001` at the intended
remove statement.

A sanitizer reproduction exposed a use-after-free when an argument replaced an
ancestor's owned field after a child receiver had been stabilized. The compiler
now freezes that projected receiver's root against exclusive access during
argument/RHS evaluation. Permanent regressions cover argument calls, assignment
RHS, mutable argument aliases, and the safe alternative of evaluating mutation
before selecting the field receiver.

## Actual editors

[Observed desktop evidence](v7-editor-verification.json) covers final packages
0.3.0 in actual VS Code 1.141.0 and Zed
1.23.2+stable.369.c0199504b9cce79bc27524950022ed701bc3ebd5:

- V7 syntax colors and `.ncr` language association.
- Live `E3001` diagnostics for deletion during a list element borrow and clearing
  after shortening the lexical scope.
- Unsaved cross-file getter rename/restoration and diagnostic updates.
- Server process replacement on restart and valid source close/reopen.
- Packaged borrowed-match snippet insertion in VS Code.

Isolated Xvfb/software rendering produced desktop-service/glyph warnings in Zed;
source and diagnostics remained readable. These were host limitations, not
language-extension failures. Separately, all 16 snippets compile, 38 examples
parse, four Zed queries execute, and the locked WASI adapter build passes.

## Fuzzing

Real bounded campaigns executed **619,701 frontend inputs** and **3,222,702 LSP
framing inputs**, each for 31 seconds, without observed crashes or timeouts.
Peak RSS was 408 MiB and 395 MiB respectively. The frontend seed set includes
V7 borrowed collections and enum matching. Mutated corpora were isolated from
the intentional checked-in seeds. Finite runs are not a general safety proof.

## Remote CI

The complete final feature workflow passed on commit `3857e0e`:
[GitHub Actions run 38292945569](https://github.com/carabistouflette/nacre/actions/runs/38292945569).
All six jobs succeeded: Rust 1.74.0/stable × GCC/Clang, editor packaging/WASI, and
real nightly fuzzing. The Clang jobs explicitly execute the ignored LLVM tests.
The earlier stabilization workflow also passed before V7 implementation:
[run 38283995876](https://github.com/carabistouflette/nacre/actions/runs/38283995876).

## Boundaries

Loan lifetimes are lexical and roots conservative. No lifetime parameters, NLL,
arbitrary field extraction, modules, threads, FFI, inheritance, or direct LLVM
backend are provided. Very deep owned trees can exhaust the C cleanup stack.
Untrusted programs are not sandboxed. Linux is observed; native Windows/macOS
are not certified. Atomic file replacement promises visibility, not crash
durability or preservation of old permissions.

Obsolete reports and identity artifacts were removed at the user's explicit
request. Captured outputs were not relabeled. The original MIT copyright notice
is retained as required by its license terms.

Published summaries omit workstation paths, process IDs, session-local artifact
references, and private screenshot locations. Desktop proof remains local and
is not included in the source publication.
