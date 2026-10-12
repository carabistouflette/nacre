# Nacre development roadmap

Nacre develops continuously as an experimental pre-1.0 language. Work is organized
by concrete language/tooling problems, not numbered language milestones. Ordinary
`0.x.y` releases identify shipped compiler versions; compatibility is not promised
before 1.0. Language/backend/ownership changes require explicit contracts and
behavioral evidence, not a schedule-driven collection of features.

Implementation and review must respect the
[Rust Design Patterns design principles](https://rust-unofficial.github.io/patterns/additional_resources/design-principles.html):
composition, focused responsibilities, encapsulation, explicit contracts,
side-effect-free queries, DRY/KISS and predictable behavior. Prefer existing
representations and shared cleanup paths over speculative abstractions.
When implementation or review encounters a violation, record concrete evidence
and a bounded remediation. Fix relevant issues with the current change; plan
unrelated debt separately. Resolve conflicting contracts explicitly rather than
merely label avoidable violations as tradeoffs.

## Encountered design remediation

- Map-key support was independently restricted in generic specialization,
  native type checking and C generation. Centralize key classification and pass
  checked metadata downstream; do not add another repeated int-or-Text whitelist.
- Existing probing/insertion recomputed a key hash, and growth/deletion derived
  bucket homes by rehashing stored keys. Hash each operation's query once; cache
  stored Text hashes so string bytes are not repeatedly scanned. Keep one map
  algorithm with narrow key operations rather than duplicate storage code.
- Stale rejection-fixture comments described all non-int keys as unsupported.
  Update the contract comments and reject genuinely unsupported keys while adding
  positive Text coverage; do not preserve obsolete restrictions through tests.
- Typed map-header grammar already supported borrowed nominal key syntax, but
  editor checks only exercised integer keys. Extend corpus/query/tokenizer and
  compiler contract checks for Text keys and mutable values; reuse the existing
  grammar production rather than introduce a parallel syntax.

## Recently shipped: built-in map traversal

[MAP_TRAVERSAL.md](MAP_TRAVERSAL.md) defines copied/shared/exclusive two-binding
loops over integer maps. Keep whole-root lexical loans, direct storage scanning,
unspecified order and existing cleanup. This does not introduce first-class
iterators or weaken invalidation rules.

## Existing capabilities

Affine ownership/definite initialization, generic nominal classes, interfaces and
inline enums, receiver-derived borrowed returns, owned-field replacement,
borrowed matching, lexical list traversal, recoverable extraction, UTF-8
console/files, native arguments, source bundles, LSP and editor support.

Integer maps transfer previous/removed owners and expose shared/exclusive value
references. The multi-file lending desk exercises polymorphic resources, generic
catalogs, availability states, member relationships, local parsing views,
recoverable commands and validated atomic persistence. Historical contracts and
verification reports remain labeled as originally captured.

## Candidates, not release commitments

- Text/string map keys after specifying owned-key replacement, equality, hashing
  and allocation-free borrowed lookup contracts.
- Profile-driven compiler/native performance, not speculative backend rewrites.
- Independent compiler/runtime input and trust-boundary review.
- Fallible allocation only after a separate recovery/cleanup design.
- First-class borrowed iterators only after explicit lifetime and movement/export
  contracts; built-in traversal does not imply them.

NLL, lifetime parameters, disjoint-entry borrowing, modules, threads, FFI,
inheritance and a direct LLVM backend require separate language contracts.

## Development and release gates

- Preserve receiver-origin contracts across generic/interface dispatch.
- Keep root-wide collection loans until lexical scope ends. Different keys do not
  imply independent alias domains; absent removal remains a modifying call.
- Verify object, inline-enum, interface and nested-collection representations,
  collision shifting, growth, extraction and exactly-once cleanup.
- Exercise actual generated programs under GCC/Clang sanitizers, meaningful
  consumer-visible regressions, minimum supported Rust and bounded fuzzing.
- Check grammar/snippets/queries and actual desktop syntax, diagnostics, unsaved
  overlays, recovery and reopening when the editing surface changes.
- Validate all persistence relationships before replacing live state; preserve
  order explicitly when it matters. Atomic visibility is not crash durability.
- Publish curated portable sources, changelog and evidence only; no workstation
  configuration, credentials, caches or private UI proof locations.
- Record the exact green source CI commit and run. Evidence-only follow-ups must
  distinguish their commit from the executable source that was tested.

## Unchanged safety boundaries

Loans end lexically, not at last use. Owners and transitively owned fields share a
root; local views remain non-movable/non-exportable. Partial moves are unsupported.
Maps promise neither traversal order nor hash-flooding resistance. Untrusted
programs are not sandboxed; finite passing campaigns are not a general proof.
