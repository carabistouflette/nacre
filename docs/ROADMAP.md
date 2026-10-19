# Nacre development roadmap

Nacre is an experimental pre-1.0 native language. Development is organized
around concrete language, runtime, and tooling problems, not numbered language
milestones. Ordinary `0.x.y` versions identify compiler releases; language and
standard-library compatibility are not promised before 1.0. No feature schedule
or release commitment is implied by this document.

Implementation and review should follow the
[Rust Design Patterns principles](https://rust-unofficial.github.io/patterns/additional_resources/design-principles.html):
composition, focused responsibilities, encapsulation, explicit contracts,
side-effect-free queries, DRY/KISS, and predictable behavior. Prefer existing
representations and shared cleanup paths over speculative abstractions.

## Current capabilities

- Affine ownership and definite initialization for classes, interfaces, and
  inline enums; control-flow-sensitive shared/exclusive borrowing, receiver-derived
  borrowed returns, owned-field replacement, borrowed enum matching, and list/map
  traversal.
- Generic nominal classes, interfaces, and enums, specialized before checking.
- Source-unit modules and named or wildcard imports resolve across the complete
  selected bundle, including local path dependencies and workspace members. This
  remains whole-program compilation, not separate compilation, package linking,
  or registry/Git/version-based dependency resolution.
- Built-in affine collections, including lists, deques, arrays, sets, and maps.
  Map/Set default keys cover scalar, `Unit`, static `String`, and owned `Text`;
  custom class/interface/enum keys use an owned `Hasher<T>` policy.
- UTF-8 `Text`, binary buffers, console and file APIs, paths and metadata,
  POSIX regular expressions, math/random/time utilities, processes, threads,
  mutexes, channels, and blocking TCP/UDP APIs.
- Native C11 and direct typed-IR LLVM backends, command-line project support, and
  an LSP server with project source roots and open-buffer overlays.
  Direct native builds still use separately compiled C runtime, standard-library,
  layout/destructor, and ABI support bitcode linked with user LLVM code.

The supported native target is Linux. Nacre has no JVM, garbage collector, GUI,
reflection, dynamic loading, or general-purpose FFI layer. Ordinary owners are
affine; shared control is explicit through the standard mutex/channel handles.
The native library and generated C runtime are part of the trusted base.

## Engineering priorities

- Keep ownership, borrowing, cleanup, and interface-dispatch contracts consistent
  across generic specialization and every concrete representation.
- Exercise generated programs with real values, early exits, nested ownership,
  collisions and growth, replacement, error paths, and exactly-once cleanup.
- Preserve lexical root-wide collection loans. Different map keys or list
  positions do not create independent alias domains.
- Design process, filesystem, network, and synchronization APIs around explicit
  ownership, partial effects, blocking, deadline, and failure semantics.
- Review compiler/runtime input boundaries independently; finite input limits and
  passing validation campaigns are not a security certification.
- Keep command-line, LSP, grammar, snippets, queries, and editor diagnostics
  aligned whenever syntax or project behavior changes.
- Validate persisted relationships before replacing live state. Atomic rename
  visibility is not a promise of power-loss durability.
- Publish curated source, changelog, and evidence only; do not include workstation
  configuration, credentials, caches, or private proof locations.

## Exploratory directions

Possible future work includes better diagnostics, profile-guided compiler/native
performance work, more complete project tooling, and additional standard-library
operations. Any addition must first have an explicit language and ownership
contract and an end-to-end implementation; these are areas of exploration, not
promised deliverables.

First-class escaping borrowed iterators, lifetime parameters, disjoint-entry
borrowing, FFI, class inheritance, and external registry/Git/version-based
dependency resolution remain outside the current language contract. Adding one
would require a separate design and compatibility decision; it must not be
inferred from built-in traversal or whole-program module imports.

## Safety boundaries

Local borrowed bindings release loans after their last use on each reachable
path, with scope exit as a fallback. Owners and transitively owned fields share
a conservative root; local views remain non-movable and non-exportable. Partial
moves are unsupported. Map traversal order is unspecified, and maps do not promise
hash-flooding resistance. Simple linear class/enum ownership chains with one
recursive child variant are destroyed iteratively; deep branched trees and
other recursive ownership shapes can still exhaust the C stack during
destruction. Programs are not sandboxed; limits on compiler and LSP input do not
make untrusted programs safe.

Validation reports retain the versions, environments, and scope in which their
evidence was collected. Historical labels and results are not statements of
current compatibility or a release promise.
