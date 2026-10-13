# Nacre

Nacre is an experimental Linux-only native-language alpha compiler implemented
in Rust, with Java-like source syntax and the pipeline **Nacre → C11 →
GCC/Clang → native executable**. It has no JVM, garbage collector, or implicit
reference counting/cloning. Ordinary owners are affine; only named mutex and
channel handles are shared explicitly. Cargo package/CLI: `nacre` **0.12.0**;
source files: `.ncr`; project marker: `nacre.json`. Rust **1.74** is the
declared minimum.

Nacre includes module/import name resolution within a whole source bundle,
generic classes/interfaces/enums, built-in collections, receiver-bound borrowing,
UTF-8 `Text`, binary buffers, and Linux filesystem, process, thread,
synchronization, regular-expression, time, and TCP/UDP APIs. It is pre-1.0:
language and standard-library compatibility are not guaranteed. Releases use
ordinary `0.x.y` package versions, not numbered language milestones. Historical
evidence retains its original labels. See the [development roadmap](docs/ROADMAP.md)
and [standard-library reference](docs/STDLIB.md).

## Start

Linux is the supported native target. Install Rust/Cargo and GCC or Clang with
C11 headers and a linker; Windows and macOS are not supported targets.

Optional native library APIs require their development headers and libraries:
OpenSSL 3 for `Crypto`, zlib for `Compression`/`Checksums`, GMP for arbitrary
precision, ICU for Unicode/calendar APIs, PCRE2 10.30+ 8-bit for Regex,
libcurl for HTTP, libarchive for ZIP, libxml2 for XML, libxslt for XSLT, SQLite
for SQL, and uriparser for URI operations. Generated programs link only the libraries
requested by their emitted native templates; ordinary programs retain their
existing C11/POSIX dependencies. The compiler accepts only a fixed dependency
allowlist, never linker flags supplied by Nacre source.

ZIP's strict seekable reader needs the upstream correction for zero-entry archives
that is not in released libarchive 3.8.9. The pinned dependency build verifies the
release SHA-256 and applies that one-line correction; it does not install globally.
With Python 3.12+, CMake, `patch`, and the native development packages installed:

```sh
python3 scripts/build_libarchive.py --prefix "$PWD/target/native-deps"
export CPATH="$PWD/target/native-deps/include${CPATH:+:$CPATH}"
export LIBRARY_PATH="$PWD/target/native-deps/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
export LD_LIBRARY_PATH="$PWD/target/native-deps/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

Keep these variables set for native compilation and execution. CI builds and uses
the same corrected dependency. ZIP does not fall back to lax streaming-directory
validation when a seekable archive is malformed.

```sh
cargo fetch --locked
cargo test --offline --locked --features fuzzing
cargo run --offline --locked -- doctor
cargo run --offline --locked -- run examples/io/contact_book.ncr --memory-stats
```

`doctor` executes a real generated native program using List/Text and checks its
output and allocation balance. It is a smoke check, not a general safety proof.

## Interactive applications

- [Lending desk](docs/LENDING_DESK.md): books/tools implementing one interface,
  generic resource/member catalogs, availability states, lending/return,
  maintenance, editing, safe removal, a logical day, and validated persistence.
  Its [feature matrix](docs/LENDING_DESK_FEATURES.md) maps major language features
  to real source functions rather than forcing every operator into one program.
- [Contact book](docs/CONTACT_BOOK.md): list/show, add/edit/remove, name search,
  phone filtering, confirmed batch removal, validated load, atomic save, help,
  and quit. Traversal borrows contacts; edits replace their owned fields. Phone
  display borrows an `Option<Text>` payload. Unicode and paths with spaces work.
- [Sudoku](examples/apps/sudoku.ncr): immutable clues, validated moves, solving,
  validated load, and atomic saves that preserve clues separately from moves.

```sh
nacre run examples/apps/lending_desk --memory-stats
nacre run examples/io/contact_book.ncr --memory-stats
nacre run examples/apps/sudoku.ncr --memory-stats
```

Memory statistics appear on stderr as
`NACRE_MEMORY allocations=N frees=N live=0`; object allocations and owned buffers
are both counted.

## List traversal and recovery

```java
void main() {
    List<int> numbers = new List<int>();
    numbers.add(1);
    numbers.add(2);
    for (int number : numbers) { println(number); }
    match (numbers.tryRemove(5)) {
        Some(number) { println(number); }
        None { println("No such element; the list is unchanged."); }
    }
}
```

Nominal elements use `for (borrow T item : list)` or
`for (borrow mut T item : list)`. The list stays loaned for the complete loop;
each element binding ends at its iteration boundary. Structural mutation through
another alias is rejected, even for copied primitive traversal. This is not a
first-class iterator or an NLL feature.

`tryGet` is available for copyable scalar types only. `tryPop` and `tryRemove`
transfer any supported element into an inline `Option<T>` without a wrapper
allocation;
invalid boundaries return `None` without mutation. Existing strict operations
recover their fatal-boundary behavior. See the
[collection recovery example](examples/collections/list_recovery.ncr):

```sh
nacre run examples/collections/list_recovery.ncr --memory-stats
```

## Map traversal

```java
for (int key, int count : counts) { println(count); }
for (borrow Text key, int count : textCounts) {
    key.print();
    println(count);
}
for (int key, borrow Contact contact : contacts) {
    borrow Text name = contact.getName();
    name.print();
}
for (int key, borrow mut Contact contact : contacts) {
    contact.setName(new Text("Updated"));
}
```

Integer keys copy; Text keys are readonly borrowed views of stored keys. Affine
values are borrowed for the whole loop. The collection is evaluated once and
structural mutation is rejected, including absent removal. Order is unspecified;
retain explicit ordered IDs when order matters. See [the contract](docs/MAP_TRAVERSAL.md)
and run the example:

```sh
cargo run --offline --locked -- run examples/collections/map_traversal.ncr --memory-stats
```

## Ownership and borrowed access

Owning class values are affine heap pointers. Enums are affine inline values;
interfaces are allocation-free data/vtable values. Passing or returning an owner
transfers its cleanup obligation. Scope exit, early return, break, and continue
clean up remaining owners; `drop` explicitly consumes a local owner.

```java
class Contact {
    private Text name;
    public Contact(Text name) { this.name = name; }
    public readonly borrow Text getName() { return this.name; }
    public void setName(Text name) { this.name = name; }
}

void main() {
    List<Contact> contacts = new List<Contact>();
    contacts.add(new Contact(new Text("Ada")));
    {
        borrow Contact reader = contacts.getRef(0);
        borrow Text name = reader.getName();
        name.print();
    }
    {
        borrow mut Contact editor = contacts.getMut(0);
        editor.setName(new Text("Grace"));
    }
}
```

Borrowed returns must derive from the method receiver or its transitively owned
fields. Locals/parameters cannot be returned as receiver-derived references.
Readonly receivers cannot return exclusive access. Shared/exclusive result loans
remain active until their lexical block ends, not their last use.

An owner and all its owned fields share one alias domain; list elements share the
whole list. Conflicting loans block mutation, relocation, movement, replacement,
and destruction. Different indices do not grant disjoint mutable domains.
See [BORROWING.md](docs/BORROWING.md) for exact rules and diagnostics.

### Owned field replacement

`this.field = newOwner;` evaluates and stabilizes the destination receiver before
the RHS, evaluates the RHS once, destroys the old owner once, and stores the new
owner. Object, interface, and enum fields are supported. Existing visibility,
mutability, definite initialization, invariance/upcasts, and loan rules apply.
Arbitrary field extraction/partial moves, borrowed-field reassignment, and local
view replacement are not supported.

### Generic enums and borrowed matching

```java
Option<Text> phone = new Option<Text>.Some(new Text("+123"));
match (borrow phone) {
    None { }
    Some(number) { number.print(); }
}
match (borrow mut phone) {
    None { }
    Some(number) { number.append("4"); }
}
```

`match (owner)` still consumes an enum and transfers payload ownership. Borrowed
matching preserves its tag/ownership: primitive payloads copy; nominal payloads
bind shared/exclusive references tied to the scrutinee. All modes are exhaustive
and enforce variant arity and uniqueness. `Option<T>` and `Result<T,E>` are
preinjected ordinary generic enums. Nested inline layouts must have finite size;
cycles through allocated objects require a real terminating case.

### Local views and interfaces

Shared borrowed fields form local views with constructor-origin tracking. Views
are lexical, non-movable, non-exportable objects; their cleanup never frees
borrowed targets. Receiver-bound returns do not turn externally borrowed view
fields into owned receiver origins.

Classes may implement interfaces; owned upcasts transfer the concrete cleanup
obligation. Borrowed upcasts retain origin/mutability. Interface method contracts
and concrete implementations agree on borrowed returns. Generics specialize
nominal classes/interfaces/enums before typing and remain invariant.

## Standard library

The standard library is injected into every program; built-in names are reserved
and available without user-authored imports. [STDLIB.md](docs/STDLIB.md) is the
complete API reference.

| API | Contract |
|---|---|
| `List<T>`, `Deque<T>`, `Array<T>`, `Set<T>` | Affine collections with specialized traversal, ownership, and borrowing operations. |
| `Map<int,V>`, `Map<Text,V>` | Signed integer or owned UTF-8 text keys; value access specializes for copyable scalars or affine nominal types. |
| `Text`, `ByteBuffer` | Owned UTF-8 text and binary bytes; `Text` is distinct from static `String`. |
| XML | Affine DOM/XPath/XSD, incremental pull events, balanced streaming output, and restricted local-only XSLT using libxml2/libxslt. |
| `Option<T>`, `Result<T,E>` | Inline affine enum values; exhaustive owning or borrowed matching. |
| Console, arguments, files | UTF-8 console input/output, arguments, text and byte files, directories, paths, and metadata. |
| Processes, threads, synchronization | Literal-argv processes, owned threads, and explicit shared mutex/channel handles. |
| Regex, Math, Random, Time, sockets | Unicode PCRE2-8 regular expressions, scalar utilities, monotonic/wall-clock time, TCP, and UDP. |

See the API reference for exact signatures, ownership, error, blocking, and
platform contracts. Fallible operations do not roll back side effects on `Err`;
ordinary allocation failure remains fatal.

`writeTextAtomic` writes a sibling temporary file, flushes/closes it, and commits
with `rename`. Atomic visibility is not power-loss durability: no `fsync` promise.
Replacement uses `mkstemp` permissions, and replacing a symlink changes its entry,
not its referent. Ordinary `writeText` can leave a partial/truncated file on error.

Strict list pop/remove and missing borrowed map access fail fatally. Recoverable
try operations return inline options. Allocation failure and checked arithmetic
runtime failures remain fatal; no exception unwinding is provided.

## CLI and projects

```text
nacre check source.ncr ... | directory
nacre run sources [--memory-stats] [--sanitize] [-O0|-O1|-O2|-O3|-Os] [-- arguments...]
nacre build sources [-o executable] [--memory-stats] [--sanitize] [-O0|-O1|-O2|-O3|-Os]
nacre emit-c sources [-o output.c] [--memory-stats]
nacre emit-flow sources [-o output.flow]
nacre emit-borrows sources [-o output.borrows]
nacre emit-types sources [-o output.types]
nacre emit-llvm sources [-o output.ll] [--memory-stats] [-O0|-O1|-O2|-O3|-Os]
nacre init new-directory
nacre doctor
nacre lsp
```

Commands accept multiple sources or recursively collected directories, resolving
their module namespaces together as one whole-program compilation. `init` refuses
an existing path. Existing outputs are replaced except the input source. Native
optimization defaults to `-O2`; default build output is `out/<source-name>`.
`--sanitize` enables ASan/UBSan.

`NACRE_CC` or `CC` selects one C compiler executable, without embedded flags.
`NACRE_CLANG` selects Clang for textual LLVM emission; this is the C frontend's
LLVM output, not a direct Rust LLVM backend. Flow/borrow/type inspection does not
require a C compiler. See [PROJECTS.md](docs/PROJECTS.md).

## VS Code and Zed

Packages **0.7.0**, grammar **0.6.0**, and 30 snippets cover integer/text maps,
copied/shared/exclusive traversal, and recoverable collection operations.
Both clients launch `nacre lsp` and use the real compiler for diagnostics.

```sh
npm --prefix editors/tree-sitter-nacre ci
npm --prefix editors/tree-sitter-nacre run generate
npm --prefix editors/tree-sitter-nacre test
npm --prefix editors/vscode ci
npm --prefix editors/vscode test
npm --prefix editors/vscode run package
```

Install `editors/vscode/nacre-0.7.0.vsix`; configure `nacre.serverPath` if the
compiler is not on PATH. The `Nacre: Restart Language Server` command supports
recovery after failed startup and serializes lifecycle operations.

For Zed, install Python `tree-sitter==0.25.2`, then run:

```sh
python3 editors/zed/check.py
python3 editors/zed/prepare.py
cargo build --manifest-path editors/zed/dist/nacre/Cargo.toml \
  --target wasm32-wasip2 --release --locked
```

Use Zed's Install Dev Extension action on `editors/zed/dist/nacre`. Keep that
prepared directory in place. Configure `lsp.nacre.binary.path` if needed.
The LSP supports UTF-16 diagnostics, unsaved cross-file overlays, current document
versions, project roots, and diagnostic clearing after fixes/close.

## Verification and fuzzing

```sh
cargo test --offline --locked --features fuzzing --no-fail-fast
cargo clippy --offline --locked --all-targets --features fuzzing -- -D warnings
cargo +1.74.0 check --offline --locked --all-targets
python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm
python3 scripts/fuzz_smoke.py
```

The fuzz command requires nightly and cargo-fuzz; it runs real frontend and LSP
framing targets with bounded time/input/memory and isolated copies of intentional
seed corpora. Preserve crash artifacts for reproduction. Finite successful runs
are evidence, not a general fuzz-safety proof.

See [VALIDATION.md](docs/VALIDATION.md) for the compiler, native backend, and
editor-facing evidence and its unverified platform boundaries. This README makes
no desktop-acceptance or remote-CI claim.

## Contribution workflow

- Keep `main` aligned with the accepted private remote; no direct feature pushes
  to `main`, force pushes, or history rewrites.
- Develop on focused feature branches. Parallel contributors use isolated
  worktrees; the integration owner reviews and integrates their commits into the
  feature branch. Contributor branches are not published.
- Stage explicit owned paths. Keep generated bundles, caches, workstation data,
  credentials, and private runtime evidence outside curated source.
- Open a pull request for the integration branch. Review the actual changes and
  exercise affected native consumers; require all CI jobs to pass before merge.
- Refresh the source manifest for the final source set. Preserve unrelated local
  files and existing contributor history; remove owned temporary worktrees only
  after their changes have been accepted.

The private repository's current GitHub plan does not permit protected branches;
these merge gates are procedural, not server-enforced.

## Boundaries and design notes

Modules and `import` resolve names only inside the source files selected for one
whole-program compilation; there is no separate compilation, package manager, or
external dependency resolver. There is no `T[]` syntax or bracket indexing; use
`Array<T>` or another collection. First-class borrowed iterators, NLL/lifetime
parameters, disjoint-entry borrowing, FFI, class inheritance, overloading, and a
direct LLVM backend are not provided. GUI, reflection, JVM execution, and dynamic
loading are outside the project scope. Very deep owned trees may exhaust the C
stack during recursive destruction. Programs are not sandboxed; compiler/LSP
input limits are not a security certification.

Additional references: [architecture](docs/ARCHITECTURE.md),
[grammar](docs/GRAMMAR.md), [generics](docs/GENERICS.md),
[interfaces](docs/INTERFACES.md), [roadmap](docs/ROADMAP.md), and
[changelog](CHANGELOG.md). Original mandatory legal attribution is preserved.
License: [MIT](LICENSE).
