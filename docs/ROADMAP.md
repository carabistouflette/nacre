# After V9

## Shipped capabilities

Nacre retains affine ownership/definite initialization, generic nominal classes,
interfaces and inline enums, receiver-derived borrowed returns, owned-field
replacement, borrowed matching, lexical list traversal, recoverable extraction,
UTF-8 console/files, native arguments, source bundles, LSP and editor support.

V9 adds Map<int,V> with full signed integer keys, transferred replacement/removal,
shared/exclusive value references and one-buffer open-addressed storage. The
multi-file lending desk gives the major feature families a coherent application:
polymorphic resources, generic catalogs, availability states, member relationships,
local parsing views, recoverable commands and validated atomic persistence.
[V9.md](V9.md) fixes the exact milestone and exclusions.

## Correctness gates

- Preserve receiver-origin contracts across generic/interface dispatch.
- Keep root-wide map loans through use of returned values; keys do not grant
  disjoint alias domains. Existing-key replacement and absent removal remain
  modifying calls under static borrowing rules.
- Test full signed keys, collision-chain deletion/reinsertion, growth and
  replacement/extraction with object, inline-enum and interface ownership.
- Raw relocation must not destroy values. Returned previous/removed owners carry
  one cleanup obligation; vacated slots carry none.
- Keep projected receiver freezing, lexical traversal cleanup and adversarial
  invalidation regressions, plus actual GCC/Clang sanitizer runs.
- Cross-validate persistence relationships and IDs before replacing live state.
- Keep bounded compiler/LSP fuzzing and actual editor diagnostics/overlay checks.
- Publish portable curated sources, not host configuration, caches or private UI
  evidence locations. Atomic visibility is not crash durability.

## Deliberately unchanged

Loans end lexically, not at last use. Owners and all transitively owned fields
share a root; local views stay non-movable/non-exportable. List traversal does
not imply transferable iterator objects. Partial moves remain unsupported.
Maps promise neither traversal/order nor adversarial hash-flooding resistance.
Programs remain outside a sandbox; finite passing campaigns are not a proof.

## Possible later milestones, not commitments

- Text/string/custom map keys after key ownership, equality and lookup contracts.
- Map traversal and first-class borrowed iterators after explicit invalidation,
  lifetime and movement/export designs.
- Independently specified fallible allocation and cleanup on recovery; no
  exception unwinding is currently promised.
- Profile-driven compiler/native performance, not speculative backend rewrites.
- Independent review of compiler/runtime input and trust boundaries.

NLL, lifetime parameters, disjoint-entry borrowing, modules, threads, FFI,
inheritance and a direct LLVM backend remain excluded. They require separate
language contracts rather than incremental weakening of V9 checks.
