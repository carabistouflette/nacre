# Nacre architecture

This note summarizes the compiler and runtime structure. Validation evidence is
recorded separately in [VALIDATION.md](VALIDATION.md).

## Pipeline

```text
one or more .ncr files or directories
  → project.rs       source collection, limits, source-unit boundaries
  → lexer.rs         tokens and file-local source positions
  → parser.rs        one AST unit per source file
  → resolver.rs      modules, imports, and qualified names
  → stdlib.rs        injection of reserved native declarations
  → generics.rs      nominal instantiation and type substitution
  → checker.rs       names, types, visibility, interfaces, and typed IR
  → ownership.rs     definite initialization and move-flow analysis
  → borrowing.rs     lexical loans, origins, and access conflicts
  ├─ default: codegen.rs C11 backend, interface tables, deterministic cleanup
  │           → stdlib_codegen.rs / stdlib/*.c.in native C method bodies
  │           → native.rs GCC/Clang executable or Clang C-frontend LLVM IR
  └─ --direct: llvm_codegen.rs emits typed-IR LLVM for user bodies and main
               + codegen.rs emits C runtime/support → LLVM bitcode
               → native.rs llvm-links textual user IR with runtime bitcode,
                 then Clang optimizes and links the native executable
```

The Rust frontend uses the standard library; `serde_json` encodes and decodes LSP
messages. `#![forbid(unsafe_code)]` forbids Rust `unsafe` in the compiler, but
does not prove compiler correctness or generated-C safety. The native target
uses the supplied C runtime and system C library. It has no garbage collector or
implicit reference counting for ordinary language values; sharing synchronization
controls requires explicit mutex/channel handles.

Compiler, fuzz, and Zed crates use Rust 2024 with a Rust 1.99 minimum;
`rust-toolchain.toml` selects the exact stable development toolchain. Conditional
pattern matching uses let chains where guards share one short-circuit path.
Process tests configure child environments through `Command::envs`, without
mutating the multithreaded Rust test runner's environment.

Disk and in-memory sources share source-bundle assembly, separator-aware byte
limits, and unit offset/line bookkeeping. Disk acquisition retains native
`PathBuf` identities and rejects duplicates by canonical path; it does not use
lossy display strings as identity keys. Source contents must still be UTF-8.
Named in-memory sources retain their separate exact string-name duplicate rule.
Diagnostic display may render non-UTF-8 filename bytes lossily without changing
the paths retained by the bundle.
The project module owns shared discovery predicates for the exact `.ncr`
extension and excluded names (leading dot, `out`, `target`). CLI directory scans,
LSP disk scans, and unsaved-source admission reuse these predicates on native
`OsStr` names, without allocating lossy display strings. Explicit CLI file inputs
retain their separate admission behavior; traversal budgets and symlink checks
remain with their callers.

The parser retains immutable token storage for speculative parsing. Its private
`TokenPosition` groups the physical index with a pending virtual `>` from a split
`>>`; declaration and generic-call lookahead restore that state together.
Identifier parsing validates the token kind before advancing and takes the
owned identifier from the consumed token, avoiding a redundant string clone.

## AST and generic specialization

The AST distinguishes `Named`, `Applied { name, args }`, and
`Borrow { name, args, mutable }`. Interfaces carry `is_interface = true`;
enums carry variants and payloads. `is_builtin` distinguishes trusted prelude
declarations from user types without granting privileges to a parameter named `T`.
Classes retain type parameters and `implements`; ordinary and variant
constructions retain explicit type arguments.

`generics.rs` indexes declarations and rewrites requested types. Each application
has a canonical key such as `Box<int>`. A deterministic queue discovers nested
instances; a cache prevents duplicate specialization. Substitution traverses
fields, signatures, locals, constructions, and method bodies. It does not rewrite
variable identifiers as type names.
Canonical names append into the specialization key's buffer. One argument writer
owns angle brackets and commas; recursive type spelling writes into that same
buffer instead of creating temporary strings and vectors. Type arguments are
resolved before key construction, and existing expansion bounds remain unchanged.

Instances become concrete nominal types before checking. Later analyses do not
resolve type variables. An unused template is not fully checked, but every
method of a requested instance is checked even when never called. See
[GENERICS.md](GENERICS.md).

Bounds on instance depth, count, and name length limit recursive expansion.
This is neither type-argument inference nor a bounded generic constraint system.

`ForEach` AST nodes preserve typed bindings, the collection, body, and source
spans: maps have two bindings; lists and sets have one. The checker evaluates a
collection expression once and lowers traversal to typed block/while IR. Lists
use indexed accessors; maps and sets use compiler-only capacity, occupancy, key,
and value expressions. Knowledge of their C layout stays in `stdlib_codegen`.
The cursor advances before the original body is checked by reference, not cloned.
Existing flow, borrowing, and cleanup passes govern joins and exits. Traversal
does not allocate iterator objects, copy affine owners, or rehash keys to read
stored values.

`iterator_fusion.rs` assembles finite `List`, `Array`, and `Map` iterator chains
as typed plans rather than runtime adapter objects. A terminal `fold` or
`sizeHint` consumes the full chain in one backend loop; `map`, `filter`, `take`,
`zip`, and `enumerate` retain their staged order. Borrowed sources stay loaned
through the terminal consumer, and a pipeline cannot escape as `Iterator<T>`.

Recoverable list intrinsics return inline `Option<T>` layouts obtained from
ordinary specialization. Native enum IDs/tags are looked up, not fixed in the
backend. Successful extraction reuses strict transfer/slot-clearing operations;
invalid boundaries return `None` before touching storage. List element validity
is checked before compiler-generated Option payloads, preserving primary
list-view diagnostics.

## Nominal IR and interfaces

The IR replaces names with nominal identifiers. `Object(id)` is an owner:
a class pointer, interface value, or inline affine sum.
`Borrow { class: id, mutable }` is non-owning access. Thus `Type::is_object()`
means **owned**, not necessarily a heap pointer.

`ir::Type::name` owns resolved language-type spelling for checker diagnostics
and nominal reports. Each phase supplies a borrowed class-name lookup, keeping
the IR independent of checker tables. Primitive and owning nominal names are
borrowed; borrow-qualified names own their formatted prefix. `type_report.rs`
renders checked IR directly into the report buffer; API entry points retain
frontend validation. AST generic-template spelling remains separate.

An IR class retains its name, interfaces, and concrete methods for each contract
slot. An interface retains declaration order. Virtual signatures have a
`virtual_slot`; `Upcast` records an owning class-to-interface conversion.
A borrowed conversion preserves its original concrete place and carries the
expected interface type.

Contracts are nominal: matching method names alone is insufficient; `implements`
must be declared. Checking covers visibility, `readonly` receiver mode,
parameters, borrowing modes, and return type. Signatures match exactly, without
covariance or implicit effect relaxation. A class with borrowed fields is a
local view and cannot implement an interface: type erasure must not hide its
lifetime restrictions.

## Ownership and borrowing

`ownership.rs` tracks each variable's possible uninitialized, available, or moved
states. Joins union states; a work queue computes a fixed point across loops.
A read requires availability on every incoming path. Owning interface
conversions transfer the source owner rather than creating another one.

`borrowing.rs` then checks typed blocks. Local loans remain lexical, their targets
are fixed, and argument loans last through the call unless captured by a view.
Origins preserve roots and parent capabilities for reborrows. Interface upcasts
do not change those origins or permit exclusive access beside a shared loan.

Virtual method signatures expose the same contracts as concrete methods; every
implementation body is checked separately. Borrow analysis need not select a
dynamic implementation to determine whether an argument owns or borrows.
The checker records borrowed returns in those signatures and proves their receiver origin;
caller-side result loans survive call cleanup. Local views remain non-exportable.

## C representation

`c_repr.rs` maps nominal IR values to C types and cleared object representations.
Both the expression backend and native emitters depend on this IR-only leaf;
native emitters do not import representation helpers from the backend orchestrator.
Fixed primitive C type names are borrowed static strings; nominal names own their
formatted class IDs. Emitters allocate an owned name only where template storage
requires it, preserving the same generated C types.
Top-level translation-unit assembly formats literal bytes, prototype declarations,
and the entry point directly into its output buffer. String literals retain
their exact UTF-8 bytes, trailing C sentinel, and static lifetime.

A concrete class is a pointer to its nominal structure. An owned or borrowed
interface is a two-pointer structure passed by value:

```c
struct interface_value {
    void *data;
    const struct interface_vtable *vtable;
};
```

The table contains a destructor and a typed function for each method. One
constant table is emitted per class/interface pair. Adapters receive `void *`,
convert the data pointer to its concrete type, and call the matching function.
They do not cast between incompatible function-pointer signatures.

An interface envelope adds no allocation. An owning transfer clears the source
to `{ NULL, NULL }`. Cleanup uses the table's concrete destructor, including
owned subobjects. A borrowed interface is never destroyed as an owner.
Physically copying two pointers for a borrow does not clone the object or add
a lifetime count.

### Inline sums

A C enum is a structure with an `int32_t` tag and a payload union. Nested sums
are defined in topological order; inline representation cycles are rejected
before emission. Class/interface indirection permits recursive structures with
a finite base case.

Construction adds no envelope allocation. A move sets the source tag to `-1`,
an internal state unavailable to source programs. Destruction visits only the
active variant and destroys owned payloads in reverse order. Consuming `match`
transfers payloads to branch bindings and invalidates its temporary; the CFG
and lexical cleanup cover returns and loop exits. It lowers to `if`/`else if`
so `break` still targets the source loop.

Enum references point to inline storage. Borrowed matching preserves the tag
and binds primitive copies or receiver/scrutinee-derived nominal references;
payloads containing borrowed fields/local views remain forbidden.

Typed match patterns recurse through enum variants, destructurable class fields,
and nested `Pair` values. Coverage and unreachable-arm checks follow the nested
shape; a guard that fails can continue to a later matching arm.


Virtual dispatch transfers owned parameters to the concrete implementation,
without a second cleanup in its adapter. Return, replacement, and loop-exit
cleanup follow the same lexical ownership model.

The backend sequences expressions left to right. `&&` and `||` retain
conditional short-circuit evaluation rather than evaluating both operands
early. Fatal runtime exits do not unwind owners; memory counters therefore do
not guarantee cleanup after a fatal failure.

### Native generator contracts

`stdlib_codegen/support.rs` owns shared stored-value destruction, cleared-value
expressions, and the specialized `Option<T>` layout contract. Collection,
iterator, and ordering emitters depend on this shared support rather than on a
concrete collection emitter for generic value operations. `None` must have no
payload; `Some` must contain exactly the expected element type. Variant tags
continue to come from the specialized IR, not hard-coded indices.
Iterator callback and adapter lookups inspect existing IR lists by reference,
without building temporary collections. Callback selection still requires exactly
one matching method, and an adapter still requires exactly one iterator interface.

Collection emitters share receiver-scoped native-method lookup in support.
Optional lookup preserves specialization pruning; required lookup enforces the
compiler-owned declaration contract. Static factories are not receiver methods:
custom Map/Set factories retain exact qualified-name matching, using borrowed
name slices rather than allocating comparison strings.

Concrete emitters retain their own storage layout and native method bodies.
They may compose another emitter's actual implementation, such as drain
callbacks, without giving it responsibility for unrelated value contracts.

## Native code and LLVM

Default `run` and `build` emit C and invoke GCC or Clang at `-O0`, `-O1`, `-O2`
(the default for source-file builds), `-O3`, or `-Os`. Manifest runs/builds use
the dev profile unless `--release` selects the release profile; an explicit
optimization option overrides the profile value. Higher optimization is not a
guarantee of faster code. Measured comparisons and their limits belong in
[VALIDATION.md](VALIDATION.md).

`emit-llvm` uses Clang's C frontend by default at `-O2`. `NACRE_CC`/`CC`
select the C compiler for the default native path. `NACRE_CLANG` selects Clang
for C-frontend LLVM emission and direct native builds. `--direct` is available
for `run`, `build`, and `emit-llvm`: `llvm_codegen.rs` emits every user-defined
function body and `main` from typed IR, without translating user code through C.
Direct `emit-llvm` writes that LLVM module; it does not include the linked runtime.

Direct native `run`/`build` still require C support. `codegen::generate_direct_runtime`
emits the trusted runtime and native standard-library bodies, plus type
layouts/destructors and ABI adapters/helpers. Clang compiles this support C to
reusable LLVM bitcode. `llvm-link` combines it directly with the textual user
LLVM module; there is no separate per-program Clang-to-bitcode step. Clang then
applies the selected optimization and performs the native link. User function
bodies and literal data remain in the per-program LLVM module, not the C support.

`NACRE_LLVM_LINK` selects `llvm-link` for direct native builds (default:
`llvm-link`). Clang and `llvm-link` must have matching LLVM major versions; a
Clang 23/`llvm-link` 22 pair failed, while matching version 22 tools succeeded.
The direct runtime-bitcode cache stores generated C support, not user bodies,
literal data, the per-program LLVM module, or the final executable. Its key
includes the compiler/linker identities and versions, target, runtime source,
compilation inputs, and relevant header/environment inputs. It defaults to
`$XDG_CACHE_HOME/nacre` (or `$HOME/.cache/nacre`), `NACRE_CACHE_DIR` overrides
that location, and `NACRE_NO_CACHE` disables it. PGO builds bypass runtime-bitcode
caching. Direct native builds retain the selected optimization and native/link
options, including `--native`, `--lto`, `--jemalloc`, and sanitizer flags.

Rust consumers can call `nacre::emit_llvm_direct`,
`nacre::compile_llvm_direct`, `nacre::compile_llvm_direct_with_flags`, and
`nacre::compile_llvm_direct_optimized_with_flags` for one source file.
`project::SourceBundle` exposes matching direct-emit and direct-compile methods
for a complete source bundle.

For GCC-backed C builds, `--pgo-generate=DIR` instruments the executable; run it
on representative inputs before rebuilding the same sources with
`--pgo-use=DIR`. Native compilation stabilizes GCC's auxiliary filenames and
generated-C source identity across temporary directories, including the source
locations used in profile checksums. Profile builds bypass executable and
runtime-bitcode caches because training data is mutable external input.


The private `native/dependencies.rs` module owns the fixed catalog for exact
standalone dependency markers, trusted header/linker arguments, dependency
closure, and stable linker order. The driver sees only an opaque dependency set
with read-only header/linker queries; it cannot construct raw membership bits or
mutate an unclosed selection. Compiler discovery, processes, and temporary-file
ownership remain in `native.rs`.
SSL requires crypto; XSLT requires XML and its include path. Repeated markers
coalesce without allocating a dependency collection. Unknown or incomplete
markers cannot supply compiler arguments. Native compilation and LLVM emission
consume the same header requirements.

## Inspection and tests

`emit-types` exposes specialized nominal types, `emit-flow` the ownership CFG,
`emit-borrows` checked loan events, and `emit-c` generated C. These commands run
frontend checks and cannot bypass a type or borrowing diagnostic.

Checked-in JSON case manifests generate maintained Rust suites. `validate.py`
uses the actual Rust compiler binary to exercise generated C and LLVM output.
Independent handwritten/template C fixtures are supplementary, not substitutes
for that proof.


## Native classes and collection memory

`ast::Class.native` is never produced by source parsing. Trusted prelude
injection marks built-in collection templates, scalar/text types, and system
APIs for I/O, filesystem, regex, math/random/time, processes, synchronization, and
networking. Generic specialization records native key handling for `Map<K,V>`
and `Set<K>`: built-in `int`/owned `Text` handling or an explicit owned
`Hasher<K>` policy for custom keys. The checker resolves native declarations and
typed methods without granting native privileges to a user-defined spelling.

Unused native classes are not materialized. Copying accessors are pruned for
affine elements; borrowed accessors are pruned for copyable elements. Their ABI
is derived from typed IR.
The native catalog records which resolved control families require
`Mutex<Unit>` once. Generic specialization requests that prerequisite and
native state-helper emission uses the same fact, rather than maintaining two
independent family lists.
Native functions have ordinary typed signatures, but their empty IR bodies are
exempt from analysis. `ownership.rs` and `borrowing.rs` do not prove native
method safety; user calls are checked against the same contracts as ordinary
functions.

`stdlib_codegen/native_usage.rs` owns typed-IR traversal for native helper
selection. Crypto and HTTP use its short-circuit query; Unicode uses its
complete call inspection to mark required ICU sections. Only resolved calls
count, not native declarations. All function bodies and branches are inspected
because C emission includes them regardless of runtime reachability. Traversal
borrows IR nodes, uses static callbacks, and allocates no collection; family
method lists and template policy remain in their respective emitters.

List storage contains a buffer pointer, size, and capacity. Only initialized
slots own values. The concrete element type selects the destructor, including
dynamic destruction for interface values. `pop` and `remove` clear the stored
slot before transferring its value to the caller. `Text` owns a byte buffer;
that buffer is not returned as a static `String` view.

Map and Set share one open-addressed table core. Each entry stores its resolved key
and value representations, a cached 64-bit hash, and an occupancy bit; custom-key
tables additionally own one `Hasher<K>` interface value by value. The
table record follows the interface-object layout needed by that policy, and entry
definitions follow complete ordinary/interface/enum layouts before drop bodies.
Default hashing covers scalar and `Unit` keys, static `String`, and owned `Text`;
custom class/interface/enum keys use borrowed readonly hash/equality callbacks.
Checked growth raw-relocates owners and cached hashes without re-calling callbacks.
Backward-shift deletion repairs probe chains without tombstones or allocation.
Replacement returns the previous value rather than dropping it. Borrowed value
lookup returns an object pointer, enum slot address, or interface value according
to the concrete value layout.

Each map remains one alias domain. Native argument sequencing and projected
receiver freezing remain in force; separate keys do not provide disjoint
borrowing. Map and Set pipelines do not provide first-class escaping borrowed
iterators or a per-entry lifetime model. Native collection code remains part of
the trusted base.

The backend injects source line/column arguments into native calls and substitutes
numeric identifiers into C templates. Buffers use the same accounting as object
allocations. Ordinary values do not use garbage collection or implicit reference
counts; mutexes and channels explicitly share their synchronization control
blocks. See [STDLIB.md](STDLIB.md).
Shared native template substitution skips markers absent from the current body
before formatting function names or copying strings. Present placeholders retain
ordered replacement, including expansion by later substitutions.

The existing check for cycles of required fields still applies. A native list
does not require an element field, so its empty state supplies a base case. A
`Node` that owns a `List<Node>` is representable; multiple owners for the same
object are not.

## Source units and project tools

`project::SourceBundle` retains each source name, its text, and source offsets.
Files are lexed and parsed independently so a comment or declaration cannot
cross a file boundary. Their AST units are then resolved together: each
`module` header supplies a namespace, and each file's imports bind declarations
from the selected source bundle, including local path dependencies loaded from
the manifest. This is whole-program compilation, not separate compilation,
package linking, or registry/Git/version-based dependency resolution.

Primary diagnostics and their notes map back to the original file. Multi-file C
output includes a compact table of file starting lines for runtime errors.
`emit-flow` and `emit-borrows` currently retain lines from the combined text.
`SourceBundle::locate` owns bundle-to-file span translation for both CLI errors
and LSP diagnostics. CLI rendering borrows the original diagnostic message and
passes its translated span to the primary renderer rather than constructing a
second diagnostic. UTF-8 byte-boundary normalization belongs to `diagnostic`;
CLI Unicode-scalar columns and LSP UTF-16 columns remain separate presentation
contracts.

`doctor` exercises the real compiler pipeline rather than a fixed fixture.
`init` creates a new local project skeleton. `--sanitize` passes memory-safety
instrumentation options to the C compiler; `doctor` is a smoke check, not an
exhaustive proof. See [VALIDATION.md](VALIDATION.md) for the recorded evidence.

## LSP and recoverable I/O

`nacre lsp` exchanges JSON-RPC messages with `Content-Length` framing. It reuses
`SourceBundle::validate` and `SourceBundle::locate`, without invoking the C
compiler, on bounded snapshots of disk sources and open buffers. Frontend byte
positions are converted to LSP UTF-16 positions; notes become related
information.

The private `lsp/diagnostics.rs` module owns diagnostic JSON, related information,
and UTF-16 ranges. It receives a borrowed path-to-URI query rather than server
source-file records; document versions, workspace state, and publication remain
in the server. Unresolvable primary locations suppress the diagnostic, while
unresolvable note locations are omitted from related information.

The private `lsp/framing.rs` module owns bounded header decoding, body acquisition,
and flushed byte-length-prefixed JSON output. It depends only on standard I/O
and JSON serialization, not workspace or compiler state. The server supplies its
message-size budget and retains JSON parsing, routing, and document policy.
The existing fuzz entry point exercises this same decoder through fragmented reads.

The private `lsp/file_uri.rs` module owns local file-URI conversion and lexical
path normalization. Its decoder accepts only local `file` URIs with valid UTF-8,
rejects NULs, query/fragment components, and inputs above 16 KiB. Percent-encoded
reserved path characters remain valid. It does not resolve symlinks or decide
workspace access; filesystem admission belongs to `lsp/source_files.rs`.

The private `lsp/source_files.rs` module owns project configuration parsing,
configured-root membership, directory traversal, symlink rejection, and bounded
UTF-8 reads. It reuses the project's filename predicates and file-count limit,
and the file-URI module's lexical path normalization. The server supplies the
remaining source-byte budget and retains document versions, overlay precedence,
scope association, combined snapshots, and diagnostic publication. The policy
module does not depend on `Server`, `Document`, or protocol output.

The nearest ancestor `Nacre.toml` (or legacy `nacre.json`) declares project source
roots. TOML manifests also resolve local path dependencies and workspace members.
Within a project scope, open buffers override disk files, including unsaved new source files under
the configured roots. Stale document versions do not replace newer snapshots.
Without a marker, each document is validated alone; nearby files are not merged.
VS Code and Zed launch the same server. See [PROJECTS.md](PROJECTS.md) for the
configuration and overlay contract.

Native I/O bodies construct the actual IR specializations of `Option`/`Result`.
They validate UTF-8, ranges, and conversions, returning owned error Text.
Validated owned byte buffers transfer into returned Text: larger storage is
adopted without a final byte copy; small content enters the header's 16-byte
inline storage and releases its former allocation. C entry receives `argc/argv`;
`Args` excludes `argv[0]`. Allocation failure and fatal boundaries terminate
without unwinding; recoverable `Err` returns follow ordinary cleanup.

