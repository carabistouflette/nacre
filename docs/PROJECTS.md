# Source bundles, projects, and LSP

## CLI source bundles

Compilation commands accept one or more source paths. A path may be a file or a
directory:

```sh
nacre check src
nacre run src/Main.ncr src/Statistics.ncr
nacre run src -- argument1 "argument with spaces"
nacre build src -o out/app
nacre emit-c src -o out/app.c
nacre emit-llvm src -o out/app.ll
nacre lsp
```

Directories are traversed recursively in sorted order. Directory inputs collect
`.ncr` files and skip hidden entries, `out`, and `target`. An explicitly supplied
regular file is read as given. Symlink sources are refused, as are duplicate
canonical paths. Source text must be UTF-8. A bundle is limited to 64 files,
1 MiB of combined source, 4,096 directory entries across discovery, and
directory nesting depth 32.

Each file is lexed and parsed independently, then its declarations are resolved
as one whole program. A source-unit header assigns declarations to a namespace;
it does not derive a namespace from the file or directory name:

```java
// src/model/Counter.ncr
module model;
public class Counter {
    private int value = 0;
    public readonly int read() { return value; }
}
```

```java
// src/Main.ncr
module app;
import model.Counter;

void main() {
    Counter counter = new Counter();
    println(counter.read());
}
```

`import model.Counter;` imports one public type or function;
`import model.*;` imports the public declarations in that module. Functions may
also be imported by name. Private declarations, including traits, are available
within their own module but cannot be imported from another module. Built-in
standard-library names are reserved and available without user-authored imports.
If a source unit has no `module` header, its declarations belong to the unnamed
module.

Modules and imports resolve only among the source units selected for the
compilation bundle, including local package dependencies. This is whole-program
compilation, not separate module compilation, package linking, or incremental
compilation. A module name does not cause a matching directory to be loaded.

## Rust `SourceBundle` API

```rust
use nacre::project::SourceBundle;

let sources = SourceBundle::from_sources(vec![
    ("Counter.ncr".into(),
     "class Counter { readonly int read(){ return 42; } }".into()),
    ("Main.ncr".into(),
     "void main(){ Counter c=new Counter(); println(c.read()); }".into()),
]).expect("invalid source bundle");

let c = sources.compile_to_c().unwrap_or_else(|error| {
    panic!("{}", sources.render(&error));
});
let result = nacre::native::compile_c(&c).unwrap().output().unwrap();
assert!(result.status.success());
assert_eq!(result.stdout, b"42\n");
```

`SourceBundle::from_sources` takes `(diagnostic name, source text)` pairs;
`from_paths` loads paths from disk. `paths`, `source_count`, and
`combined_source` expose bundle metadata and text. `validate`, `compile_to_c`,
`emit_flow`, `emit_borrows`, and `emit_types` use the bundle's whole-program
analysis. `render` maps a diagnostic's primary span and notes back to their
original source files. `locate(span)` returns the original path, source text, and
file-local byte span. The single-source APIs remain available.

For multi-file C output, a compact source table maps runtime line numbers back to
the original file. `emit-flow` and `emit-borrows` currently report lines in the
combined text; they are not stable diagnostic formats.

## `Nacre.toml` packages and workspaces

Commands accept a package directory or a `Nacre.toml` path in place of raw
sources. `check`, `run`, and `build` with no input use the current directory's
manifest when it exists.

```toml
[package]
name = "app"
version = "0.1.0"
sources = ["src"]
entry = "src/Main.ncr"

[dependencies]
utilities = { path = "../utilities" }

[profile.dev]
opt-level = 0
lto = false

[profile.release]
opt-level = 3
lto = false
```

`package.name` and `package.version` are required metadata; names begin with an
ASCII letter or digit and otherwise allow ASCII letters, digits, `_`, and `-`.
`sources` defaults to `["src"]`. An optional `entry` adds an existing `.ncr`
file to those roots; it does not select among multiple `main` declarations.
Sources and entry paths must stay inside the package. Unknown keys, malformed
TOML, unsupported dependency forms, and manifests larger than 1 MiB are errors.
At least `[package]` or `[workspace]` is required.

Dependencies are local paths relative to the declaring manifest. Their table
keys must match the target package names. Resolution visits dependencies before
consumers, loads a shared dependency once, and rejects cycles or one package name
resolving to different roots. Registry, Git, and version-based dependency
resolution are not supported. Package names do not create language namespaces:
use `module` and `import` declarations. All selected files form one program with
one entry point, subject to the existing source limits. Graphs are bounded to
64 manifests and dependency/workspace nesting depth 32.

```toml
[workspace]
members = ["app", "utilities"]
```

A workspace may be virtual (no `[package]`) or also define its own package.
Members are explicit local relative paths, not globs. Compiling the workspace
combines its members; compile a member directory instead when members are
independent executables with separate entry points.

`nacre add ../utilities` adds the dependency to `./Nacre.toml`;
`--manifest-path path/to/Nacre.toml` selects another owning package. The supplied
dependency path is relative to the command's working directory, then stored
relative to that package. Updates preserve TOML comments, refuse replacement of
an existing dependency, and reject reachable dependency cycles.

`nacre build --release` and `nacre run --release` select the release profile.
Manifest builds otherwise use the dev profile. Optimization levels accept
integers `0`–`3` or strings `"0"`–`"3"` and `"s"`; explicit `-O` flags override
the profile. `--lto` enables LTO even when the profile disables it. Raw-source
commands retain `-O2` by default. Build output defaults to `out/<package-name>`
for a package, or `out/<source-name>` for raw inputs.


## Project creation and diagnostics

`nacre new new-directory` (also spelled `nacre init new-directory`) requires a
path that does not already exist. It creates `src/` example modules, a README,
`.gitignore`, and `Nacre.toml` package metadata. Run `nacre check`, `nacre run`,
or `nacre build --release` inside the new directory. No registry is contacted.
Generated modules use a package-specific `pkg_` namespace so untouched helper
modules from separate scaffolds can coexist as path dependencies. The namespace
encodes `_` as `__` and `-` as `_h`; names differing only by those characters
remain distinct. A dependency must expose library declarations rather than a
second application `main`.
If writing a new project fails, the new directory may remain partially populated;
existing paths are never removed automatically.

`nacre doctor` runs the frontend and a small generated native `List`/`Text`
program. It expects `test=42`, balanced allocation counts, and no live objects at
exit. A successful doctor run is a smoke check, not an exhaustive validation.

Outputs are checked against all loaded source files to prevent overwriting an
input. On Unix, the check also compares device and inode to account for hard
links. It is not a guarantee against another process changing the filesystem
during compilation. The C compiler is invoked with separate arguments, not
through a shell.

## LSP project scope

`nacre lsp` is a stdio JSON-RPC language server backed by the same source-bundle
validation. The nearest ancestor `Nacre.toml` or legacy `nacre.json` within the
workspace boundary defines a project scope. `Nacre.toml` takes precedence when
both exist and resolves the same source roots, local dependencies, and members
as the compiler. Legacy `nacre.json` still accepts a non-empty `sources` array:

```json
{ "sources": ["src"] }
```

Paths that are absolute or escape the project root are rejected. Symlinks are
not followed. Legacy JSON source roots also reject symlinks in existing parent
components, so a symlinked ancestor cannot redirect loading outside the project.
The compiler's file, source-size, and directory-depth limits also apply to a
project. Legacy JSON configuration must be a regular file and is limited to 64
KiB; TOML manifests use the 1 MiB limit above.
`examples/language/multi_file_project/nacre.json` is a working legacy configuration.

Without a project marker, each open source document is compiled by itself.
Nearby files are not merged automatically, so files that each declare `main`
remain separate unless a project explicitly selects them together.

The server supports full-text `didOpen`/`didChange` snapshots, saves, closes, and
workspace-folder changes. It advertises full-document synchronization; ranged
incremental edits are not applied. Open buffers override their disk files, and
an unsaved new `.ncr` file under a configured source root participates in the
bundle. Stale document versions do not replace newer buffers. Closing a document
removes its overlay, allowing its saved disk contents to be used again.

Validation publishes the first frontend diagnostic, mapped to the owning file
with UTF-16 positions and related notes. Diagnostics are cleared when resolved
or when a document leaves the scope. Invalid configuration or source loading is
reported with `window/logMessage`, not as successful validation. The server
revalidates on document and workspace changes; editor clients should use the
same `nacre lsp` executable for project-aware diagnostics.
