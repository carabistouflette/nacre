# Nacre changelog

## V8 / 0.9.0

- List `for` traversal copies primitive elements or explicitly borrows affine
  elements shared/exclusively. The collection is evaluated once and loaned for
  the whole loop; each element binding ends at its iteration boundary.
- Existing block/while cleanup handles nested loops, `continue`, `break`, and
  `return`, without an iterator allocation or cloning the user-body AST.
- `tryGet` copies primitive elements into inline `Option<T>`; `tryPop` and
  `tryRemove` transfer every supported owned representation. Failure preserves
  the list; success clears the vacated slot and transfers cleanup exactly once.
- Contact-book substring search, phone filtering, and confirmed batch removal
  exercise traversal and recoverable extraction. Unicode persistence and
  candidate-load rollback retain the existing version-1 file format.
- Editor packages 0.4.0, grammar 0.3.0, and 20 snippets include V8 traversal and
  recoverable removal. The explicit milestone and exclusions are in `docs/V8.md`.
- First-class iterators, maps, lifetime refinements, fallible allocation, exception
  unwinding, and new backends remain outside this release.

## V7 / 0.8.0

- Instance methods and interface contracts return shared/exclusive borrows proven
  to derive from their receiver. Caller loans remain lexical; escaping locals or
  parameters and readonly-to-exclusive conversions are rejected.
- Affine `List<T>` elements expose checked, allocation-free `getRef`/`getMut`.
  Object, inline enum, and interface representations preserve their correct ABI;
  conflicting loans block whole-list mutation, relocation, movement, and cleanup.
- Typed assignment replaces owned object/interface/enum fields, evaluating the
  RHS once before destroying the old value. Arbitrary extraction/partial moves
  and borrowed-field reassignment remain unsupported.
- Explicit `borrow`/`borrow mut` expressions and borrowed exhaustive enum matches
  copy primitives and lend nominal payloads without consuming tags or owners.
- Projected receivers remain stable during argument/RHS evaluation; ancestor
  mutation cannot free a pending child receiver pointer.
- Complete interactive contact book: shared consultation, exclusive in-place
  editing, optional phone matching, add/remove, validated candidate loading, and
  atomic Unicode/path-with-spaces persistence.
- VS Code/Zed packages 0.3.0, grammar 0.2.0, and 16 snippets cover V7. Both actual
  desktop applications exercise compiler diagnostics, overlays, restart/reopen.
- Behavioral/native regressions replace obsolete restrictions and source-shape
  assertions. Rust 1.74, GCC/Clang sanitizer/LLVM paths, real fuzzing, and remote
  CI are exercised.
- Clean Nacre identity across crate, CLI, `.ncr` sources, `nacre.json`, editors,
  runtime selectors, project directory, and packages. Obsolete history, reports,
  binaries, generated caches, and graph artifacts are removed; original mandatory
  legal attribution is retained.
- Published verification summaries omit machine-specific paths, process IDs,
  session-local log references, and private desktop evidence locations.

## Stabilization / 0.7.1

- `Files.writeTextAtomic` writes a sibling temporary file and commits using
  `rename`; Sudoku saves use it. Atomic visibility does not imply crash durability.
- Real frontend and LSP framing libFuzzer targets run bounded campaigns over
  isolated copies of intentional seeds.
- VS Code lifecycle operations serialize activation, restart, and shutdown;
  failed-start recovery no longer stops an inactive client.
- Process-argument storage belongs to generated executables rather than generic
  runtime fixtures. Current Text Result dependencies and locked Zed builds are
  represented correctly in verification harnesses.
