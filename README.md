# Nacre V8 — lexical traversal and recoverable collections

Nacre is an experimental Rust compiler with a Java-like syntax and the pipeline
**Nacre → C11 → GCC/Clang → native executable**. No JVM, garbage collector, or
reference counting is used. Cargo package/CLI: `nacre` **0.9.0**; source files:
`.ncr`; project marker: `nacre.json`. Rust **1.74** is the declared minimum.

V8 adds list `for` traversal, recoverable `tryGet`/`tryPop`/`tryRemove`, and
contact-book search, phone filtering, and confirmed batch removal. Receiver-bound
borrowed returns, owned-field replacement, and borrowed enum matching remain
available. Borrow lifetimes stay lexical and alias domains conservative.

The bounded milestone contract and explicit exclusions are in [V8.md](docs/V8.md).

## Start

Linux/WSL is the initial target. Install Rust/Cargo and GCC or Clang with C11
headers and a linker. Native Windows/macOS behavior is not certified.

```sh
cargo fetch --locked
cargo test --offline --locked --features fuzzing
cargo run --offline --locked -- doctor
cargo run --offline --locked -- run examples/contact_book.ncr --memory-stats
```

`doctor` executes a real generated native program using List/Text and checks its
output and allocation balance. It is a smoke check, not a general safety proof.

## Interactive applications

- [Contact book](docs/CONTACT_BOOK.md): list/show, add/edit/remove, name search,
  phone filtering, confirmed batch removal, validated load, atomic save, help,
  and quit. Traversal borrows contacts; edits replace their owned fields. Phone
  display borrows an `Option<Text>` payload. Unicode and paths with spaces work.
- [Sudoku](examples/sudoku.ncr): immutable clues, validated moves, solving,
  validated load, and atomic saves that preserve clues separately from moves.

```sh
nacre run examples/contact_book.ncr --memory-stats
nacre run examples/sudoku.ncr --memory-stats
```

Memory statistics appear on stderr as
`NACRE_MEMORY allocations=N frees=N live=0`; object allocations and owned buffers
are both counted.

## V8 list traversal and recovery

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

`tryGet` exists for copied primitives only. `tryPop` and `tryRemove` transfer
any supported element into an inline `Option<T>` without a wrapper allocation;
invalid boundaries return `None` without mutation. Existing strict operations
retain their fatal-boundary behavior. Run the complete
[collection example](examples/collections_v8.ncr) with:

```sh
nacre run examples/collections_v8.ncr --memory-stats
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

`List`, `Text`, `Option`, `Result`, `Console`, `Files`, and `Args` are reserved and
preinjected without imports. [STDLIB.md](docs/STDLIB.md) is the API reference.

| API | Contract |
|---|---|
| `List<T>` | add, size, capacity, isEmpty, set, pop, remove, clear, reserve. Primitive elements retain copied get; affine nominal elements expose checked getRef/getMut. |
| `Text` | Owned UTF-8 bytes, including NUL; append operations, byte access, clone, comparison, print, strict int32 parseInt, and boundary-checked owned sliceBytes. Distinct from static String. |
| `Console` | UTF-8 line input with explicit EOF; String/Text output. Errors are owned Result values. |
| `Files` | UTF-8 readText, non-atomic writeText, and writeTextAtomic. |
| `Args` | Native arguments excluding argv[0], with owned UTF-8 copies from get. |
| `Option<T>`, `Result<T,E>` | Inline affine enum values; exhaustive owning or borrowed matching. |

`writeTextAtomic` writes a sibling temporary file, flushes/closes it, and commits
with `rename`. Atomic visibility is not power-loss durability: no `fsync` promise.
Replacement uses `mkstemp` permissions, and replacing a symlink changes its entry,
not its referent. Ordinary `writeText` can leave a partial/truncated file on error.

Pop/remove return owners and fail fatally for invalid indices/empty lists. Native
allocation failure and checked arithmetic runtime failures are fatal; there is no
exception unwinding. Recoverable console/file/parse results use normal cleanup.

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

Commands accept multiple sources or recursively collected directories, sharing
one nominal namespace. `init` refuses an existing path. Existing outputs are
replaced except the input source. Native optimization defaults to `-O2`; default
build output is `out/<source-name>`. `--sanitize` enables ASan/UBSan.

`NACRE_CC` or `CC` selects one C compiler executable, without embedded flags.
`NACRE_CLANG` selects Clang for textual LLVM emission; this is the C frontend's
LLVM output, not a direct Rust LLVM backend. Flow/borrow/type inspection does not
require a C compiler. See [PROJECTS.md](docs/PROJECTS.md).

## VS Code and Zed

Packages **0.4.0**, grammar **0.3.0**, and 20 snippets cover copied/shared/exclusive
list traversal and recoverable removal alongside V7 borrowing constructs.
Both clients launch `nacre lsp` and use the real compiler for diagnostics.

```sh
npm --prefix editors/tree-sitter-nacre ci
npm --prefix editors/tree-sitter-nacre run generate
npm --prefix editors/tree-sitter-nacre test
npm --prefix editors/vscode ci
npm --prefix editors/vscode test
npm --prefix editors/vscode run package
```

Install `editors/vscode/nacre-0.4.0.vsix`; configure `nacre.serverPath` if the
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

[VALIDATION.md](docs/VALIDATION.md) records observed compiler/native/editor/CI
evidence and unverified platform boundaries. V8 compiler/native/fuzz checks and
both actual desktop editor acceptance runs pass; CI evidence is recorded there.
Obsolete identity artifacts and historical reports were removed at user request,
without relabeling their captured outputs.

## Boundaries and design notes

No modules/packages/imports or separate compilation, arrays `T[]`, maps, borrowed
iterators, NLL/lifetime parameters, network API, threads, FFI, inheritance,
overloading, or direct LLVM backend. Very deep owned trees may exhaust the C
stack during recursive destruction. Untrusted programs are not sandboxed.
Resource limits on compiler/LSP input are not a security certification.

Additional references: [architecture](docs/ARCHITECTURE.md),
[grammar](docs/GRAMMAR.md), [generics](docs/GENERICS.md),
[interfaces](docs/INTERFACES.md), [roadmap](docs/ROADMAP.md), and
[changelog](CHANGELOG.md). Original mandatory legal attribution is preserved.
License: [MIT](LICENSE).
