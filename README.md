# Nacre

Nacre is an experimental Linux-only native-language alpha compiler implemented
in Rust, with Java-like source syntax. Its default native path is **Nacre → C11 →
GCC/Clang → native executable**; opt-in `--direct` lowers typed user functions
and `main` to LLVM while retaining separately compiled C runtime support. It has
no JVM, garbage collector, or implicit reference counting/cloning. Ordinary owners
are affine; only named mutex and channel handles are shared explicitly. Cargo
package/CLI: `nacre` **0.12.0**;
source files: `.ncr`; package manifest: `Nacre.toml` (legacy LSP marker:
`nacre.json`). Rust **1.99** is the
declared minimum; compiler and extension crates use edition **2024**.

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

`rust-toolchain.toml` pins stable Rust 1.99.0 with Clippy and rustfmt. Development
and CI runtimes are pinned in `.node-version` (26.12.0), `.npm-version` (12.2.0),
and `.python-version` (3.15.0). Nightly is used only for sanitizer-backed fuzzing.

The full Rust test suite also exercises direct LLVM and requires matching-major
Clang and `llvm-link`, even when `NACRE_CC=gcc`. Set `NACRE_CLANG` and
`NACRE_LLVM_LINK` to compatible tools. CI selects both from the directory
reported by `/usr/bin/llvm-config --bindir` and runs GCC/C, Clang/C, and
Clang/direct-LLVM jobs. `NACRE_TEST_BACKEND=c|llvm` selects the shared compiler
helper for ordinary native behavior suites without changing their Nacre source
or expectations. Injected-C fixtures and standalone C helpers remain C-specific;
explicit dual-backend tests continue to exercise both paths.

Optional native library APIs require their development headers and libraries:
OpenSSL 3 for `Crypto`, zlib for `Compression`/`Checksums`, GMP for arbitrary
precision, ICU for Unicode/calendar APIs, PCRE2 10.30+ 8-bit for Regex,
libcurl for HTTP, libarchive for ZIP, libxml2 for XML, libxslt for XSLT, SQLite
for SQL, and uriparser for URI operations. Generated programs link only the libraries
requested by their emitted native templates; ordinary programs retain their
existing C11/POSIX dependencies. The compiler accepts only a fixed dependency
allowlist, never linker flags supplied by Nacre source.

ZIP uses pinned libarchive 3.8.10 with a private ZIP-boundary correction. It
validates length-delimited filenames before conversion, preserves ZIP path names,
checks EOCD comment extent and exact directory byte/entry counts, and searches the
complete legal 65,557-byte EOCD range, including zero-entry archives at offset zero.
Zip64 locators remain readable with maximum-length comments; relative offsets in
self-extracting prefixes are corrected after Zip64 expansion. The dependency build
verifies the release SHA-256 and applies the exact-release patch with zero fuzz;
it installs only to the chosen private prefix.
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
`doctor --direct` reports the selected tool paths, LLVM versions, and target,
rejects incompatible LLVM majors before compilation, and runs the same checks
through direct LLVM. `doctor --zip` verifies a binary ZIP round-trip and empty
archive ownership with the corrected libarchive dependency above. The flags
compose: `nacre doctor --direct --zip` exercises both optional checks.

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
- [Game of Life](examples/apps/game_of_life.ncr): Conway's Game of Life on a
  toroidal grid with presets (glider, blinker, toad, beacon, r-pentomino),
  generation stepping, interactive cell editing, and scriptable arguments.
- [HTTP server](examples/apps/http_server.ncr): Lightweight HTTP/1.1 file and
  API server with MIME routing, JSON status endpoints, binary buffers, and TCP sockets.

```sh
nacre run examples/apps/lending_desk --memory-stats
nacre run examples/io/contact_book.ncr --memory-stats
nacre run examples/apps/sudoku.ncr --memory-stats
nacre run examples/apps/game_of_life.ncr --memory-stats
nacre run examples/apps/http_server.ncr --memory-stats -- 8089 1
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
another alias is rejected, even for copied primitive traversal. These loops do
not expose first-class escaping borrowed iterators or implement NLL.

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

## Fused iterator pipelines

Built-in chains such as `values.iter().map(...).filter(...).take(...).fold(...)`
are fused into one terminal traversal without intermediate adapter allocations.
Callbacks stay lazy and in chain order; `take(0)` runs none. These pipelines do
not provide first-class escaping borrowed iterators.

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
remain live while a reachable path can use them, then release after their last use
when possible rather than waiting for lexical block exit.

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

Nested patterns can destructure enum variants, named class fields, and `Pair`
values recursively, for example `Nested(Value(Leaf { value: value }))` or
`(first, (second, third))`. A failed pattern guard can fall through to a later
matching arm.

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
nacre check source.ncr ... | directory | Nacre.toml
nacre test  sources [--native] [--lto] [--jemalloc] [-- filter]
nacre bench sources [--native] [--lto] [--jemalloc] [-- filter]
nacre fmt   sources [--check]
nacre doc   sources [-o output-dir]
nacre run   [source.ncr ... | directory | Nacre.toml] [--release] [--memory-stats] [--direct] [--sanitize] [--native] [--lto] [--jemalloc] [-O0|-O1|-O2|-O3|-Os] [-- arguments...]
nacre build [source.ncr ... | directory | Nacre.toml] [-o executable] [--release] [--memory-stats] [--direct] [--sanitize] [--native] [--lto] [--jemalloc] [-O0|-O1|-O2|-O3|-Os]
nacre emit-c sources [-o output.c] [--memory-stats]
nacre emit-flow sources [-o output.flow]
nacre emit-borrows sources [-o output.borrows]
nacre emit-types sources [-o output.types]
nacre emit-llvm sources [-o output.ll] [--direct] [--memory-stats] [-O0|-O1|-O2|-O3|-Os]
nacre new new-project-directory
nacre init new-project-directory
nacre add <path> [--manifest-path <file>]
nacre doctor [--direct] [--zip]
nacre lsp
```

`nacre fmt` normalizes layout without splitting operators such as `<<` and `>>`.
It validates all changed files before writing; formatting or validation failures
leave the input files unchanged. `--check` reports changed files without writing.

`nacre doc` writes a searchable static site to `doc/index.html` by default.
API headings include module-qualified names, so same-named items from different
modules remain distinguishable.
Trait declarations and their method comments are included in the generated API site.

`nacre test` discards child stdout and captures at most 64 KiB of stderr. The
`nacre bench` runner captures at most 64 KiB per stream; it drains and discards
excess. Failed tests report truncated stderr; benchmarks exceeding the cap fail.
Native C/LLVM compiler diagnostics and toolchain probes are capped at 64 KiB per
captured stream. Truncated diagnostics are marked; incomplete cache probes skip
caching, while `emit-llvm` still returns the complete IR output.

Commands accept multiple sources or recursively collected directories, resolving
their module namespaces together as one whole-program compilation. `init` and
`new` create projects; `init` is an alias. Existing outputs are replaced except
the input source.

Raw source-file native builds and default C-front-end `emit-llvm` use `-O2`.
Manifest `run`/`build` use the dev profile by default; `--release` selects the
profile, and an explicit `-O` option overrides its optimization level. Default
build output is `out/<package-name>` for a manifest package, otherwise
`out/<source-name>`. `--sanitize` enables ASan/UBSan; `--native` enables host CPU
instruction extensions (`-march=native`); `--lto` enables link-time optimization
(`-flto`); `--jemalloc` links jemalloc (`-ljemalloc`). These options and the
selected optimization also apply to `--direct` native builds.

The default C `run`/`build` path caches completed executables. Its key includes
generated C, compiler command/version/target, flags, and resolved header paths
and contents. If dependency scanning fails, the build safely bypasses the cache.
The C executable key also hashes `LIBRARY_PATH` and `LD_RUN_PATH`, including
whether each is unset or set to an empty value, so changed link-time library
selection or embedded runtime search paths force a fresh link.
Named external libraries are content-hashed when they resolve in explicit `-L`
or `LIBRARY_PATH` paths. Libraries found only through compiler defaults, opaque
linker options, linker scripts, thin archives, and direct file inputs bypass
this executable cache.
The cache defaults to a nonempty `$XDG_CACHE_HOME/nacre`, falling back to a
nonempty `$HOME/.cache/nacre`; empty path values are ignored. A nonempty
`NACRE_CACHE_DIR` overrides the location, and `NACRE_NO_CACHE` disables
caching.
If no nonempty cache location exists, caching is disabled.
PGO builds bypass the cache. Direct native builds use a separate runtime-bitcode
cache only; their user LLVM module and final executable are not cached.

`emit-llvm` uses Clang's C frontend by default. With `--direct`, it emits the
typed-IR user functions and `main` as LLVM; `--memory-stats` is unavailable for
`emit-llvm --direct`. For `run`/`build --direct`, the generated C side is runtime,
standard-library, layout/destructor, and ABI support only. Clang compiles that
support C separately to reusable LLVM bitcode; `llvm-link` combines it directly
with the textual user LLVM module. Clang applies the selected optimization and
performs the native link. The runtime-bitcode cache excludes user bodies and
literal data.

`NACRE_CC` or `CC` selects the C compiler for the default C path. `NACRE_CLANG`
selects Clang for default C-frontend LLVM emission and direct native builds;
`NACRE_LLVM_LINK` selects `llvm-link` for direct native builds (default:
`llvm-link`). Direct native builds require Clang and `llvm-link` from matching
LLVM major versions: Clang 23 with `llvm-link` 22 failed, while a matching 22
pair succeeded. Flow/borrow/type inspection and `emit-llvm --direct` do not
require a C compiler. See [PROJECTS.md](docs/PROJECTS.md).

For source debugging, build with `nacre build sources --direct -O0 -o out/debug`
and open the executable in GDB. Direct LLVM preserves original bundle filenames,
line locations, scalar arguments and locals, and lexical match/block scopes.
Byte, char, int, long, double, and boolean values have scalar debug types
(`char` uses UTF-32 `char32_t`); aggregate and borrowed values are not yet
described as inspectable debug types.
Generated C support omits source-level debug information so stepping follows
Nacre source rather than deleted temporary runtime files. Optimized builds retain
source metadata, but optimization can remove or relocate variables and statements.

## VS Code and Zed

Packages **0.7.0**, grammar **0.6.0**, and 30 snippets cover integer/text maps,
copied/shared/exclusive traversal, and recoverable collection operations.
Both clients launch `nacre lsp` and use the real compiler for diagnostics.
The VS Code grammar highlights all standard-library types, including generic
collection, codec, and concurrency types.

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

For Zed, install Python `tree-sitter==0.26.0`, then run:

```sh
python3 editors/zed/check.py
python3 editors/zed/prepare.py
cargo build --manifest-path editors/zed/dist/nacre/Cargo.toml \
  --target wasm32-wasip2 --release --locked
```

Use Zed's Install Dev Extension action on `editors/zed/dist/nacre`. Keep that
prepared directory in place. Configure `lsp.nacre.binary.path` if needed.
The LSP supports UTF-16 diagnostics and incremental UTF-16 edits, unsaved
cross-file overlays, current document versions, project roots, and diagnostic
clearing after fixes/close.

## Verification and fuzzing

```sh
cargo test --offline --locked --features fuzzing --no-fail-fast
cargo clippy --offline --locked --all-targets --features fuzzing -- -D warnings
cargo check --offline --locked --all-targets
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

### Staged publication

After reviewing the change, exercising affected behavior, and staging explicit
owned paths, run:

```sh
python3 scripts/publish.py --message "Describe the verified change"
```

The command regenerates `MANIFEST.sha256` from exact staged Git blobs, including
added/deleted files; exports staged sources to a temporary directory; verifies
checksums and runs the package audit there; then stages only the manifest,
commits the selected changes, and pushes the current feature branch to
`origin`. Before pushing, the resulting commit's tree and parent are checked
against the audited snapshot. Commit hooks remain enabled; if one changes the
audited commit, the local commit is left for inspection and not pushed.
Unstaged edits and untracked files are not included. The audit's generated test
inventory never overwrites the working copy.

Use `--check` instead of `--message` for a non-mutating publication preview, or
add `--no-push` to commit locally. The command rejects main/master, detached HEAD,
Graphify outputs/caches, non-regular sources, ambiguous checksum filenames, and
unstaged manifest edits. Graphify analysis stays outside Git, even if someone
force-stages its output. A failed push leaves the local commit intact; retry
the push, not the commit. Package checks do not replace compiler/native tests.

`python3 scripts/test_publish.py` exercises the workflow with real disposable
Git repositories and a local bare remote; CI runs these regression scenarios.

## Boundaries and design notes

Modules and `import` resolve across the selected whole-program bundle, including
local path dependencies loaded from `Nacre.toml`; this is not separate
compilation. Manifests support local path dependencies and workspaces, but do
not resolve registry, Git, or version-based dependencies. There is no `T[]`
syntax or bracket indexing; use `Array<T>` or another collection.
First-class escaping borrowed iterators,
NLL/lifetime parameters, disjoint-entry borrowing, FFI, class inheritance, and
overloading are not provided. GUI, reflection, JVM execution, and dynamic
loading are outside the project scope. Very deep owned trees may exhaust the C
stack during recursive destruction. Programs are not sandboxed; compiler/LSP
input limits are not a security certification.

Additional references: [architecture](docs/ARCHITECTURE.md),
[grammar](docs/GRAMMAR.md), [generics](docs/GENERICS.md),
[interfaces](docs/INTERFACES.md), [roadmap](docs/ROADMAP.md), and
[changelog](CHANGELOG.md). Original mandatory legal attribution is preserved.
License: [MIT](LICENSE).
