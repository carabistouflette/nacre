# Nacre changelog

## Unreleased

- Postfix `?` unwraps trusted built-in `Result` values or returns the exact
  error payload after cleaning live locals and pending owned C temporaries.
  Typed IR preserves the carrier layouts; calls, stores, enum wrapping and
  returns explicitly transfer temporary ownership.

- Text map keys are owned, queried by shared borrow without allocation, compared
  by exact UTF-8 bytes and immutable during paired key/value traversal. Integer
  and text keys share storage algorithms; cached text hashes avoid repeated work.
- Reviewed fixes centralize key classification and the hash finalizer, skip
  hashing empty-map queries, and preserve readonly keys with mutable values
  without weakening root-wide loans. Named inventory exercises the public API.
- CI maintenance: update [checkout to 7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1),
  [setup-node to 7.1.0](https://github.com/actions/setup-node/releases/tag/v7.1.0),
  and [setup-python to 7.0.0](https://github.com/actions/setup-python/releases/tag/v7.0.0).
  These actions use Node 24 instead of deprecated Node 20; language semantics,
  test matrices and selected Node/Python toolchain versions are unchanged.

## 0.11.0

- Built-in integer-map traversal binds copied keys and copied primitive or
  shared/exclusive affine values. Each live entry is visited once in unspecified
  order; collections are evaluated once and loaned for the complete loop.
- Direct occupied-slot scanning avoids iterator allocations, affine-owner copies
  and key rehashing. Existing block/while ownership and cleanup handle nested
  loops, continue, break, return and receiver-derived borrowed results.
- List and map traversal share binding/receiver contract checks. Internal slot
  expressions keep C layout knowledge in the native collection backend.
- Editor packages 0.6.0, grammar 0.5.0 and 27 snippets cover both loop bindings.
- Development is feature-driven and versions use ordinary pre-1.0 0.x.y releases.
  Current help, diagnostics, dump headers and docs no longer use Vx branding;
  historical reports/contracts and Git history retain their original identity.
- Design/review follows the linked Rust design principles: focused composition,
  encapsulation, explicit contracts, side-effect-free queries, DRY/KISS and
  predictable behavior. Mutating commands may return transferred ownership.
- No first-class iterators, text/custom keys, NLL/lifetime refinements, disjoint
  entry borrowing, fallible allocation or new backend is introduced.

## 0.10.0

- Map<int,V> supports all signed int32 keys and the existing copied/owned value
  representations. Put transfers the previous value in an inline Option;
  tryRemove transfers a present value and preserves an absent map.
- Primitive map values expose tryGet; affine values expose receiver-bound
  shared/exclusive getRef/getMut. Whole-map/root loans retain lexical alias rules.
- One-buffer open addressing uses unsigned hashing, checked growth, and
  backward-shift deletion without tombstones. Relocation transfers ownership;
  replacement and removal do not allocate wrappers or destroy returned owners.
- The complete multi-file lending desk uses polymorphic books/tools, generic
  catalogs, stable IDs, availability states, member relationships, local parsing
  views, recoverable commands, logical-day overflow guards and atomic persistence.
- A feature-to-function matrix and rejected companions document the safety
  boundaries. Affine collision-chain deletion has explicit object/enum/interface
  regressions, in addition to primitive growth and replacement coverage.
- Editor packages 0.5.0, grammar 0.4.0 and 24 snippets cover map access and transfer.
  Generated C identifies the compiler package version rather than a pinned banner.
- Text/custom keys, map traversal, lifetime refinements, fallible allocation,
  exception unwinding and new backends remain excluded; see docs/V9.md.

## 0.9.0

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
- Editor packages 0.4.0, grammar 0.3.0, and 20 snippets include list traversal and
  recoverable removal. The historical release contract is in `docs/V8.md`.
- First-class iterators, maps, lifetime refinements, fallible allocation, exception
  unwinding, and new backends remain outside this release.

## 0.8.0

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
- Editor packages 0.3.0, grammar 0.2.0, and 16 snippets cover borrowed returns. Both actual
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
