# After V7

## Shipped foundation

Nacre V7 retains ownership/definite initialization, generic nominal classes,
interfaces and inline enums, recoverable UTF-8 console/file APIs, native arguments,
multifile source bundles, diagnostic stdio LSP, Sudoku, and VS Code/Zed support.

V7 adds receiver-derived borrowed returns, checked shared/exclusive references
into affine lists, typed owned-field replacement, borrowed enum matching, and a
complete contact-book application using atomic persistence.

V6 stabilization was completed first: actual VS Code/Zed acceptance, private
remote CI, real bounded frontend/framing fuzzing, atomic saves, and the clean
Nacre cutover. Obsolete identity artifacts and reports are removed.

## Maintain correctness before expanding scope

- Keep adversarial ownership/loan tests and actual GCC/Clang sanitizer execution.
- Preserve receiver-origin contracts across generic/interface dispatch.
- Keep projected receivers stable while evaluating arguments/RHS expressions;
  ancestor mutation must not free a pending child pointer.
- Exercise malformed LSP framing, unsaved overlays, editor restarts, and input
  bounds. Finite campaigns and passing suites are not general safety proofs.
- Preserve clear atomic visibility vs. crash-durability semantics and actual
  error recovery, without unchecked pointers or message-string classification.

## Deliberately conservative current model

Loans end at lexical boundaries, not their last use. A root and all its owned
fields share one alias domain; list indices are not analyzed as disjoint.
Returned references require a receiver-derived origin. Local views remain
non-movable/non-exportable. Arbitrary field extraction and partial moves are not
provided; replacement always preserves a fully typed owned value.

Any later lifetime/projection refinement must establish explicit function and
interface contracts, joins/loops/cleanup behavior, and adversarial invalidation
rejections before relaxing these rules. No such refinement is promised by V7.

## Possible later work, not V7 commitments

- Borrowed iterators/maps and additional everyday APIs, only after specifying
  ownership and invalidation behavior.
- Separately designed fallible allocation and recoverable versions of currently
  fatal bounds/pop operations; no exception unwinding is currently promised.
- Measured compiler/native performance work, using reproducible profiles rather
  than speculative backends or abstraction layers.
- Independent review of language/runtime trust boundaries and untrusted input
  resistance. Programs remain outside any sandbox.

Modules, threads, FFI, inheritance, non-lexical lifetimes, lifetime parameters,
and a direct LLVM backend were excluded from V7. Future discussion of them must
not be mistaken for an unfinished implementation in this release.
