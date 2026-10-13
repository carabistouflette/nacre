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
  → codegen.rs       C11, interface tables, and deterministic cleanup
  → stdlib_codegen.rs / stdlib/*.c.in   native C method bodies
  → native.rs        GCC/Clang to executable, or Clang to textual LLVM IR
```

The Rust frontend uses the standard library; `serde_json` encodes and decodes LSP
messages. `#![forbid(unsafe_code)]` forbids Rust `unsafe` in the compiler, but
does not prove compiler correctness or generated-C safety. The native target
uses the supplied C runtime and system C library. It has no garbage collector or
implicit reference counting for ordinary language values; sharing synchronization
controls requires explicit mutex/channel handles.

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


Virtual dispatch transfers owned parameters to the concrete implementation,
without a second cleanup in its adapter. Return, replacement, and loop-exit
cleanup follow the same lexical ownership model.

The backend sequences expressions left to right. `&&` and `||` retain
conditional short-circuit evaluation rather than evaluating both operands
early. Fatal runtime exits do not unwind owners; memory counters therefore do
not guarantee cleanup after a fatal failure.

## Native code and LLVM

`run` and `build` emit C and invoke GCC or Clang at `-O0`, `-O1`, `-O2`
(the default), `-O3`, or `-Os`. Higher optimization is not a guarantee of faster
code. Measured comparisons and their limits belong in [VALIDATION.md](VALIDATION.md).

`emit-llvm` invokes Clang with `-S -emit-llvm` on the same generated C. Nacre has no
direct LLVM backend written in Rust. `NACRE_CLANG` selects that Clang executable;
`NACRE_CC`/`CC` separately select the compiler used by `run` and `build`.

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
borrowing. Map and Set traversal does not add a first-class iterator or a
per-entry lifetime model. Native collection code remains part of the trusted
base.

The backend injects source line/column arguments into native calls and substitutes
numeric identifiers into C templates. Buffers use the same accounting as object
allocations. Ordinary values do not use garbage collection or implicit reference
counts; mutexes and channels explicitly share their synchronization control
blocks. See [STDLIB.md](STDLIB.md).

The existing check for cycles of required fields still applies. A native list
does not require an element field, so its empty state supplies a base case. A
`Node` that owns a `List<Node>` is representable; multiple owners for the same
object are not.

## Source units and project tools

`project::SourceBundle` retains each source name, its text, and source offsets.
Files are lexed and parsed independently so a comment or declaration cannot
cross a file boundary. Their AST units are then resolved together: each
`module` header supplies a namespace, and each file's imports bind declarations
from the selected source bundle. This is whole-program compilation, not
separate compilation, package linking, or external dependency resolution.

Primary diagnostics and their notes map back to the original file. Multi-file C
output includes a compact table of file starting lines for runtime errors.
`emit-flow` and `emit-borrows` currently retain lines from the combined text.

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

The nearest ancestor `nacre.json` declares project source roots. Within a project
scope, open buffers override disk files, including unsaved new source files under
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

