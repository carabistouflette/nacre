# After V8

## Shipped foundation

Nacre retains ownership/definite initialization, generic nominal classes,
interfaces and inline enums, recoverable UTF-8 console/file APIs, native arguments,
multifile source bundles, diagnostic stdio LSP, Sudoku, and VS Code/Zed support.
V7 added receiver-derived borrowed returns, affine list element references,
owned-field replacement, borrowed enum matching, and atomic contact persistence.

V8 adds copied/shared/exclusive lexical list traversal, recoverable copied lookup
and owned extraction, plus contact-book search, phone filtering, and confirmed
batch removal. [V8.md](V8.md) defines the exact milestone and exclusions.

## Maintain correctness before expanding scope

- Keep adversarial ownership/loan tests and actual GCC/Clang sanitizer execution.
- Preserve receiver-origin contracts across generic/interface dispatch.
- Keep projected receivers stable while evaluating arguments/RHS expressions;
  ancestor mutation must not free a pending child pointer.
- Preserve whole-loop root loans and per-iteration cleanup through nested loops,
  `continue`, `break`, and `return`.
- Failed recoverable extraction must preserve contents, order, size, and capacity;
  success transfers exactly one cleanup obligation without an option allocation.
- Exercise malformed LSP framing, unsaved overlays, editor restarts, and input
  bounds. Finite campaigns and passing suites are not general safety proofs.
- Keep atomic visibility distinct from crash durability and publish only portable
  curated sources, never workstation configuration or private evidence locations.

## Deliberately conservative model

Loans end at lexical boundaries, not their last use. A root and all its owned
fields share one alias domain; list indices are not analyzed as disjoint.
Returned references require a receiver-derived origin. Local views remain
non-movable/non-exportable. Traversal is loop syntax, not a transferable iterator
object. Arbitrary field extraction and partial moves remain unsupported.

Any lifetime/projection refinement must establish explicit function and interface
contracts, joins/loops/cleanup behavior, and adversarial invalidation rejections
before relaxing these rules. V8 does not promise such a refinement.

## Possible later work, not V8 commitments

- Maps with explicit key equality/hashing, ownership, and invalidation contracts.
- First-class borrowed iterators, only after a design for their lifetime and
  movement/export behavior; current lexical traversal does not imply that design.
- Separately designed fallible allocation; no exception unwinding is promised.
- Measured compiler/native performance using reproducible profiles.
- Independent review of compiler/runtime trust boundaries and untrusted-input
  resistance. Programs remain outside any sandbox.

Modules, threads, FFI, inheritance, non-lexical lifetimes, lifetime parameters,
and a direct LLVM backend remain excluded. Future discussion is not an unfinished
implementation or an implicit commitment in this release.
