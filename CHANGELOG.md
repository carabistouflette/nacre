# Nacre changelog

## 0.12.0
Experimental native alpha; Linux systems APIs, breaking source changes permitted.

- Use `ncr_reallocate` for ByteBuffer, console input, and readLink growth,
  replacing manual allocation-copy-free cycles.

- Reallocate command argument/environment tables and HTTP response headers,
  avoiding explicit replacement-buffer copies.

- Leave BitSet spare capacity uninitialized; zero words only when they become logical.

- Remove duplicate clearing after calloc-backed XML and Temporal allocations.

- Avoid zero-initializing fully overwritten byte copies in BinaryReader, SQLite blobs, Encoding, crypto buffers, and HTTP.

- Use raw allocation for Process argument/environment tables and fully written copies.

- Use raw allocation for fully written BigInteger and BigDecimal text buffers.

- Use raw allocation for fully written file and database path buffers.

- Use raw allocation for fully copied HTTP request and response strings.

- Use raw allocation for Text.parseDouble's fully copied input.

- Reject non-ASCII LSP header bytes before parsing Content-Length.

- Correct ignored LLVM test commands to use the actual Cargo target name.

- Sanitize control characters in compiler diagnostic snippets and omit carriage
  returns from displayed CRLF source lines.

- Align Tree-sitter, Zed, and VS Code syntax support with trait/impl declarations,
  bounded type parameters, and async/await; capture Zed's named shift-right token.

- Reject legacy JSON LSP source roots with an intermediate symlink, preventing
  configured source loading from traversing outside the project tree.
- Canonicalize resolved implementation class names before generic-bound checks,
  allowing named-module types to satisfy valid trait bounds.
- Resolve implementation trait names and generic trait bounds to
  module-qualified identities, preventing same-named traits in other modules
  from satisfying a bound.
- Enforce module privacy when resolving trait implementations and bounds;
  private traits cannot be used by other modules.
- Validate trait implementations against exact method signatures, resolving
  types in the trait and impl source units and checking borrowing modes, receiver
  mutability, visibility, and synchronous instance behavior.
- Retain imports from the impl source unit when resolving implementation method
  signatures and bodies, including when the class is declared in another file.
- Bound CLI source-directory discovery to the same 4,096-entry limit as the LSP,
  preventing unrelated files from bypassing the traversal resource cap.
- Cache Unit-mutex dependency discovery as specializations are requested,
  avoiding a full request-set scan after each expansion.
- Count concrete user types incrementally during generic expansion instead of
  rescanning the accumulated class list after each specialization.
- Index interface implementers during contract checking, avoiding repeated
  whole-program scans in sendability and construction analysis.
- Propagate local-view status through reverse dependencies rather than rescanning
  all classes until the closure stabilizes.
- Stream enum variant initializer signatures and bodies directly into the
  generated output buffer, eliminating temporary parameter and initializer strings.
- Stream native and virtual pointer-slot runtime adapters directly into their
  output, avoiding per-parameter vectors and joined call strings.
- Stream direct LLVM C-ABI function prototypes without temporary parameter
  vectors or joined strings.
- Stream direct interface dispatchers into generated C without temporary
  parameter and argument vectors or joined strings.
- Stream direct user adapter signatures and call arguments into the destination
  buffer instead of allocating temporary vectors and joined strings.
- Emit LLVM runtime signatures directly into IR, avoiding temporary parameter
  vectors and joined strings.
- Stream interface thunk argument lists directly into generated C, avoiding
  temporary `Vec<String>` values and joined strings.
- Write class-support C directly to its destination, avoiding temporary
  formatted strings and joined interface parameter lists.
- Select builtin generic templates by borrowed minimum qualified name, avoiding
  per-definition string clones and candidate sorting.
- Avoid the discarded second token clone when parsing identifiers; advance the
  cursor directly after extracting the retained name.
- Avoid temporary ownership-state copies on identity control-flow nodes; only
  state-changing transfers clone an output buffer.
- Avoid copying complete local type maps at every nested block during iterator
  specialization discovery; track and restore only bindings introduced in each
  scope.
- Borrow names for direct concrete-class lookups during generic inference,
  avoiding a temporary owned string for named and non-generic borrowed types.
- Share LSP query source loading across document-symbol, hover, and definition
  requests; borrow the active document slice from its source bundle rather than
  cloning it for each request.
- Key cached C executables by compiler version/target and included header contents;
  isolate concurrent cache writes and clean failed staging files.
- Append generated C directly into the source-map buffer, avoiding a second full
  translation-unit allocation and copy for project compilation.
- Reuse the parsed AST and typed IR in test/benchmark runners, avoiding repeated
  parsing and type-checking before native compilation.
- Keep Clang-only strict qualifier warnings on the direct-LLVM side of parity
  tests, leaving the GCC/C fixture invocation with supported compiler options.
- Record matched JSON, string, binary-streaming, map, Sudoku, and Life benchmarks
  against Rust, including cache-disabled native build times, RSS/allocation
  samples, and controlled optimization counters with explicit measurement limits.
- Fix GCC PGO training/use identity across native temporary directories. Keep
  generated-C source locations and auxiliary profile names stable, and bypass
  executable/bitcode caches for builds with mutable profile inputs.
- Fuse terminal `List`, `Array`, and `Map` iterator chains into direct loops:
  `iter`, `map`, `filter`, `fold`, `zip`, `take`, `enumerate`, and finite size
  hints. Readonly source loans and stack-backed pair views avoid intermediate
  adapter allocations; pipelines cannot escape their terminal expression.
- Add recursive enum, class-field, and binary tuple patterns with guarded
  fallback arms, exhaustiveness checking, and unreachable-arm diagnostics.
  The prelude now reserves `Pair<T, U>`; existing user `Pair` types must be renamed.
  Tree-sitter and Zed queries recognize the same nested patterns, guards, and tuples.
- Lower user functions and entry points directly from typed IR to LLVM, including
  owned classes, sums, interfaces, native calls, and fused iterator pipelines.
  Link separately cached runtime bitcode with matching-major Clang/`llvm-link`;
  `--direct` selects this path for LLVM emission and native builds.
  Both backends share nonescaping-object stack-promotion eligibility and preserve
  fields-only destruction for promoted owners.
- Run ordinary native behavior suites through a shared C/direct-LLVM backend
  selector. CI covers GCC/C, Clang/C, and Clang/LLVM; injected-C fault fixtures
  and standalone C helpers remain explicitly backend-specific.
- Add machine-readable `nacre test --json` output with per-test status and
  failure details, filtered counts, and suite timing.
- Add `Text.join` for borrowed `List<Text>` parts and a borrowed delimiter.
  Measure UTF-8 output length first and reserve before appending.
- Extend local Graphify support for Nacre's `.ncr` and `.c.in` sources, linking
  `Text.join` through native IR and code generation without patching Graphify.
- Profile release-compiler builds on scalar, collection, and transitive-package
  consumers. Make unused direct-LLVM functions discardable instead of keeping
  them externally live, retaining strong symbols for concrete interface implementations
  referenced by C vtables. Record cold and changed-source warm measurements.
- Give whole-program-only LLVM functions private linkage so interprocedural
  optimization can change their pointer calling conventions and inline callers.
  Interface implementations referenced by C vtables remain externally visible.
  Add matched C/direct-LLVM runtime comparisons to the existing benchmark runner,
  with Rust checksum preflight, alternating runs, per-child RSS, and separate
  allocation checks. Raise both workload drivers' repetition limit to support
  stable batches for fast kernels.
- Make generated direct-LLVM native ABI adapters transparent to optimization
  with `always_inline`, exposing caller constants across erased-pointer slots.
  User functions and underlying standard-library functions keep their existing
  attributes; runtime optimization flags are unchanged. Preserve dynamic UTF-8
  validation, Unicode/NUL behavior, allocation balance, and source debugging.
- Explicitly erase argument-slot addresses at the direct user-thunk ABI boundary
  without weakening owned or borrowed payload qualifiers. Verify interface
  dispatch under strict Clang qualifier checks.
- Avoid calling the ByteBuffer reserve helper for single-byte appends within
  existing capacity. Preserve overflow checks and growth policy; record binary
  CPU profiles, matched six-workload measurements, and executable footprint.
- Pass direct-LLVM user-function scalar and thin-reference inputs by value using
  one shared C/LLVM transport policy. Preserve aggregate input addresses, result
  slots, native/interface contracts, ownership cleanup, and source debugging.
  Verify mixed arguments and early-error cleanup under sanitizers; record the
  argument/return ablation, all six workloads, JSON cost, and executable footprint.
- Keep the fresh hex-decoder destination byte pointer loop-invariant in the shared
  native implementation, enabling vectorized conversion without changing validation,
  allocation order, ownership, or compiler flags. Verify SIMD tails and all byte
  values; record matched six-workload timing, counters, and the code-size cost.
- Reuse writable tail capacity in whole-file `Files.readText` reads before
  growing the input buffer, avoiding needless growth when the next read is EOF.
  Preserve streaming `File.readSome`, UTF-8 and Text-size checks, partial reads,
  EINTR handling, close-error precedence, and ownership transfer.
- Grow heap-backed `Text` with the shared reallocation helper instead of
  allocate–copy–free. Preserve inline spill, capacity rounding, validation,
  alias rejection, ownership, and allocation-failure behavior.
- Format `Text.appendInt` with bounded stack-only decimal conversion instead of
  general `snprintf`. Preserve the full signed 32-bit range, UTF-8/NUL prefixes,
  Text growth, ownership and allocation-failure behavior; record matched
  consumer gains and independently checked search controls.
- Copy `Text.sliceBytes` results through the existing trusted owned-copy path
  after range and UTF-8 endpoint checks, avoiding redundant validation while
  preserving independent storage, inline/heap growth, errors and ownership.
- Copy `Text.slice` (scalar), `Text.trim`, and `Text.split` results through
  the trusted owned-copy path, removing redundant UTF-8 validation of verified
  character boundaries across the `Text` substring API.
- Use `ncr_reallocate` for contiguous `Deque` capacity growth when `head` is zero,
  avoiding allocate-copy-free cycles when ring buffer elements do not wrap around.
- Add explicit capacity pre-sizing for `Text`: `public void reserve(int capacity)`
  and `public readonly int capacity()`, matching `List` and `Deque` conventions
  to eliminate repeated geometric reallocations when building large text output.
- Fast-path short UTF-8 validation and single-byte stores in `Text.append`,
  reducing serialization and append overhead for common delimiter and newline tokens.
- Index active borrower counts in `DeskState` (`lending_desk`), replacing the
  $O(N)$ full-catalog scan in `hasActiveLoanFor` with an $O(1)$ check and accelerating
  non-borrower member removals by ~45% without changing catalog or persistence invariants.
- Add `--json` structured output support to `nacre bench`, emitting parseable
  throughput (ops/sec, ns/iter, max diff) and suite execution summaries.
- Prune unused `Crypto`, `Uuid`, and `Temporal` native implementation bodies and
  prototypes in C code generation, reducing generated C translation unit size and compilation time.
- Add optional `nacre doctor --direct` and `--zip` checks. Report selected LLVM
  tool versions and reject incompatible majors before compiling bitcode; verify
  native allocation balance and real binary/empty ZIP archive round-trips.
- Emit original-file direct-LLVM source locations, lexical scopes, and scalar
  argument/local debug types. Preserve unoptimized argument locations and UTF-32
  character values; avoid inactive match-arm locals and stepping into deleted
  runtime C sources. Aggregate and borrowed debug types remain unavailable.
- Keep large iterator plans out of ordinary IR expression storage so long
  expression trees retain bounded per-frame frontend stack usage.
- Add `Nacre.toml` packages with validated metadata, source roots, entry files,
  dev/release profiles, local path dependencies, and explicit workspace members.
  Resolve dependency diamonds once and reject cycles and conflicting identities.
  `nacre new`, `nacre add`, and `nacre build/run --release` use the manifest;
  the LSP recognizes it while retaining existing `nacre.json` projects.
  Generated helper modules use distinct, punctuation-preserving package namespaces.
- Add LSP document symbols (`textDocument/documentSymbol`), hover (`textDocument/hover`),
  and go-to-definition (`textDocument/definition`) providers for classes, interfaces,
  enums, variants, methods, fields, and functions. Report diagnostics on invalid `Nacre.toml`
  manifests instead of silently dropping diagnostic publication.
- Build canonical generic-specialization names directly in their final buffer.
  One argument writer owns separators and nested type spelling, eliminating
  temporary argument strings, vectors, and joins while preserving names,
  specialization order, expansion limits, type reports, and generated C.
- Separate LSP project-file admission into a private source-files module,
  independent of document state and diagnostic publication. Configuration,
  traversal, symlink, exclusion, and size-limit behavior are preserved.
  Source snapshots take ownership of collected paths instead of cloning them.
  Add a live stdio regression for cross-file unsaved overlays under a configured
  source directory that does not yet exist on disk.
- Add `scripts/publish.py` to regenerate hashes from staged Git blobs, audit an
  isolated export, commit selected changes, and push the feature branch.
  Unstaged sources and the working test inventory are preserved; Graphify
  artifacts and direct main/master publication are rejected. Real disposable
  Git/remote regression scenarios run in CI.
- Crypto, HTTP, and Unicode helper selection share a private, exhaustive
  native-call traversal instead of three copies of the typed-IR grammar.
  Short-circuit queries and full usage marking have separate interfaces;
  backend-specific method lists remain local. Traversal borrows IR without
  allocating a visitor or collecting nodes, and generated C is unchanged.
- Upgrade development to stable Rust 1.99.0 and edition 2024 across compiler,
  fuzz, and Zed crates. Pin Node 26.12.0, npm 12.2.0, and Python 3.15.0;
  refresh Rust/npm locks and current stable editor/parser dependencies.
- Use Rust let chains, typed slice chunks, and `is_multiple_of` where applicable.
  Process boundary tests inject invalid UTF-8 into the child environment rather
  than changing the parallel test runner's global environment.
- Parser lookahead snapshots the token index and split-generic closer together.
  Identifier parsing takes the consumed token's owned name instead of cloning
  it again; failed lookahead and generated output retain their behavior.
- Port the private ZIP-boundary correction to libarchive 3.8.10, reserving a
  filename-validation flag distinct from its new timestamp flags and retaining
  upstream's inclusive EOCD search. Release hashing and zero-fuzz application
  remain required.
- Native dependency selection is encapsulated in a private policy module,
  separate from compiler processes and temporary-file ownership. The driver
  consumes an opaque, prerequisite-closed set through header/linker queries.
  Exact marker matching, implicit requirements, flag ordering, and public
  compilation APIs retain their existing behavior.
- Resolved IR type names share one spelling implementation across checker
  diagnostics and nominal reports. Existing primitive and class names are
  borrowed; qualified borrow names remain owned. Type-report layout lives in an
  IR-only presentation module and streams fields without temporary vectors.
  Public APIs still perform full validation; report and diagnostic text are
  unchanged.
- CLI source discovery and LSP disk/overlay admission share exact extension and
  excluded-name predicates in the project module. Native names are inspected
  without lossy string allocation; hidden/output exclusions and explicit CLI
  file behavior remain unchanged.
- LSP diagnostic JSON and UTF-16 range conversion live in a private presentation
  module. URI resolution uses a borrowed path query instead of exposing server
  source-file records. Cross-file notes, malformed-offset clamping, and diagnostic
  publication behavior are preserved.
- LSP local file-URI conversion and lexical path normalization live in a private
  standard-library-only module, separate from workspace and document state.
  The 16 KiB input limit, local-authority restriction, malformed URI rejection,
  and percent-encoded filename behavior are preserved.
- CLI diagnostic rendering reuses the source bundle's authoritative file-local
  span mapping and borrows the original message instead of copying a diagnostic.
  LSP and CLI share UTF-8 boundary normalization while retaining their respective
  UTF-16 and Unicode-scalar column conventions. Cross-file notes are unchanged.
- Top-level C emission formats literal bytes, prototypes, and the entry point
  directly into its output buffer instead of allocating intermediate strings.
  Static UTF-8 literal contents and generated C remain unchanged.
- Native C template substitution skips absent method markers and placeholders,
  avoiding no-op body copies and unused function-name formatting. Replacement
  order, receiver selection, and emitted C remain unchanged.
- Primitive C type names are borrowed rather than allocated on every lookup.
  Nominal names remain owned; native template storage explicitly takes ownership
  where required. Generated C types and runtime behavior are unchanged.
- LSP byte framing lives in a private I/O/JSON-only module, separate from workspace
  and frontend validation. The server supplies message-size policy; header limits,
  duplicate/truncated frame rejection, output flushing, and the public fuzz entry
  point retain their existing behavior.
- Project loading preserves native filesystem paths through shared source-bundle
  assembly instead of cloning paths and round-tripping through lossy strings.
  Distinct non-UTF-8 Unix filenames no longer collide as duplicate sources.
  Canonical duplicate checks, UTF-8 source-content validation, and separator-aware
  source-size limits remain enforced for their respective input APIs.
- Collection emitters share receiver-scoped native-method lookup and required
  declaration contracts instead of duplicating per-family search helpers.
  Static factories retain their separate exact qualification rules; custom
  Map/Set factory checks no longer allocate strings for name comparison.
- C type and cleared-object representations live in an IR-only shared leaf,
  separating native emitters from the expression backend's orchestration.
  All internal callers use the leaf directly; emitted C representations remain
  unchanged.
- Iterator method/interface contract lookups use borrowed iteration instead of
  temporary vectors or allocated class-name prefixes, retaining uniqueness checks.
- Native generator value cleanup, cleared-value expressions, and specialized
  Option payload contracts live in shared support. Collection/iterator/ordering
  emitters no longer form a sibling dependency cycle; generated C and affine
  transfer/cleanup behavior remain unchanged.
- Native dependency markers, implicit requirements, headers, and ordered linker
  arguments derive from one fixed catalog. Standalone SSL and XSLT markers retain
  their crypto/XML prerequisites; malformed or unknown markers cannot add flags.
- XSLT text output recognizes the supported Unicode UTF-8 labels independently
  of the installed libxml alias table, without global alias registration, extra
  dependencies, or transcoding already-UTF-8 bytes. Alias matching is case-insensitive
  and exact; malformed suffixes remain unsupported encodings.
- Recursive expression emission keeps binary-operation formatting in a separate
  non-recursive helper. Long arithmetic and short-circuit chains compile on the
  default Rust test-thread stack, preserving operand order and conditional evaluation.
- CI exports isolated native dependency paths through the runner environment file
  after building the corrected ZIP library; workflow admission no longer uses the
  unavailable `runner` context in a job-level environment.
- Filesystem permission modes reject negative values and bits outside `07777`
  before opening, truncating, creating directories, or changing permissions.
  Zero and POSIX special permission bits remain valid.
- Native prelude identities and constructor availability derive from trusted AST
  declarations. Static utilities follow their actual methods, while factory-
  returned native owners remain values, including factory-only owners inside
  result/option payloads; declared concrete constructors retain their direct
  code-generation mappings.
- Shared `Mutex<Unit>` control dependencies are defined once in the native
  catalog and consumed by generic specialization and state-helper emission;
  control ownership and runtime allocation behavior remain unchanged.
- XSLT adds independent serialized XML, HTML, and text results through
  `transformBytes`, honoring output encodings with direct `ByteBuffer` writes
  and the same per-transform resource restrictions as document results.
  Text output rejects unrepresentable characters without numeric-reference
  substitution, preserves UTF-8 aliases, and flushes stateful encodings. Unsupported
  methods are rejected even for empty results; failed output-buffer construction
  releases its pending encoder.
- Collection drains destroy unreturned map entry owners on the calling thread
  before publishing their iterator, including custom-key and ordered-map drains.
  Sendable iterators cannot retain discarded thread-affine guards. Direct ordered
  traversal preserves readonly keys and mutable value loans; iterator break
  releases the loop loan without consuming the remaining input.
- Generic expansion counts user-template specializations separately from trusted
  built-in dependencies, matching the existing concrete-user-type budget. Native
  drain and iterator prerequisites no longer exhaust the budget in ordinary array
  programs; the 64 user-specialization, depth and name-length guards remain.
  Generic immutable `borrow T` declarations adapt copied arguments to values;
  nominal borrows and mutable-borrow restrictions remain unchanged. Obsolete
  rejection cases and nominal-only documentation were removed.
  Source-text assertions about vtable layout and allocator call sites were removed;
  native interface CLI checks retain output and ownership-balance verification.

- `BinaryReader` and `BinaryWriter` compose with owned `ByteSource`/`ByteSink`
  values as well as `ByteBuffer`. Bounded source reads preserve the logical
  cursor on failure; sink writers retain pending packets and resume from
  confirmed offsets. Consuming `finish` returns any unwritten suffix without
  implicit transport I/O or flush, and generic codecs work in either mode.
- Adds reusable affine `Codec<T>` and `Serialization<T>` APIs with a canonical
  big-endian `NCRS`/version-1 frame, primitive and aggregate codecs, explicit
  list element limits, canonical Option/Result tags, UUID network-byte encoding,
  nested domain schemas, and owned decode results that reject malformed data,
  truncated fields, and trailing input.
  Invalid list limits are rejected before binary input consumption or transport I/O.
- XML consumer regressions use named borrowed inputs and retain their process
  output for allocation checks. Native CLI checks validate XSD, preserve query
  values after the document is dropped, and reject external-entity declarations.
- Empty thread pools no longer require a concrete task implementation merely to
  instantiate their error types. Finite-construction analysis distinguishes an
  opaque abstract interface from a recursive concrete ownership cycle. Concurrency
  smoke checks cover owned Text result transfer, single consumption and cleanup
  of an abandoned result, without changing ordinary affine ownership.
- Synchronizer waits use `-1` for an unbounded wait, preserve `0` as a poll, and
  reject values below `-1`; networking retains its separate zero-is-unbounded
  timeout convention. Public Condition nodes are released with their final handle,
  including clones, rather than retained by a long-lived Mutex; guards retain the
  mutex independently. Blocking and lifetime regressions cover all five wait APIs.
- PCRE2 Regex rejects byte-wise `\C` patterns at compilation to preserve validated
  UTF-8 capture invariants. Each compiled Regex owns an immutable bounded match
  context reused across calls; execution data remains private to each call.
- Files.list uses the owned List representation without an extra pointer level.
  Binary and database templates share placeholder validation, and database
  emission uses native class presence rather than a second expression traversal.
- Math adds fused multiply-add, directed adjacent-value stepping, and binary64
  pi/e constants alongside checked arithmetic and IEEE floating utilities.
- Type checking dispatches recursive expression kinds to focused helpers instead
  of retaining one oversized match frame at every level. The existing 128-level
  expression limit is unchanged; deep valid expressions compile on normal test
  threads without a larger stack or additional heap storage.
- Native emission shares borrowed template sections and places ByteBuffer
  helpers before binary consumers. Compression and checksum emission is
  independent, so either API works without constructing the other class.
- Native placeholder checks share the actual `@NAME@` grammar rather than
  rejecting every literal `@` byte, preserving HTTP user-information validation.
- DateTime respects ICU timezone-buffer preconditions and enumerates actual system
  zone IDs. DateFields' private constructor now initializes every field.
  Class-presence emission replaces a duplicate expression traversal; advertised
  zone IDs are checked through actual temporal conversion.
- `Temporal` adds affine nanosecond-precision instant, duration, period, local,
  offset, and zoned values, with checked elapsed/calendar arithmetic, strict ISO
  parsing, explicit DST overlap selection, and ICU calendars for Gregorian,
  Japanese, Buddhist, ROC, and Islamic variants. Existing millisecond `DateTime`
  behavior is unchanged.
  The public nanosecond getter exposes exact fractions; normalized carry and
  millisecond conversion check the final integer result, and calendar periods
  reject precision loss after each component. Zone replacement copies scalar
  state without allocating a zone buffer that would immediately be discarded.
  Civil lookup and ISO formatting also check offset-adjusted milliseconds before
  accepting ICU fields. Short text results use inline storage; fixed-offset
  scratch and canonical zone conversion buffers are reused rather than copied.
  The same guard now protects existing `DateTime` parts, formatting and calendar
  arithmetic from local-field rounding near the exact range limits. Formatting
  reuses its already-open calendar for the check instead of allocating a clone.
- Temporal formatting uses the receiver's ICU calendar, locale digits, zone and
  civil fields; fractional patterns retain all nine nanosecond digits. ISO
  parsing round-trips durations and signed calendar periods with checked integer
  carries, and date-only versus elapsed `T` duration syntax is deterministic.
- Ordered-key queries preserve affine borrows and specialize every copied-key
  query consistently. Native iterator interface adapters include their diagnostic
  arguments, and mapper extraction uses the element representation, not the
  input-interface representation. Hidden callback and element state remains
  subject to recursive sendability checks.
- Collection traversal includes ordered maps/sets and priority queues with
  comparator order, readonly ordered keys, and explicitly mutable affine map
  values. Iterator traversal mutably advances the original iterator and preserves
  loop loans, item destruction, and early-exit cleanup.
- Arrays, deques, sets, priority queues, and maps now drain into owned iterators;
  map key/value drains destroy the unused entry half. Ordered drains preserve
  comparator order, deque and array drains preserve index order, and priority
  queue drains expose heap-array order. Existing map/filter/take/skip/collect
  adapters compose with every drain source.
- ECDSA verification classifies malformed DER as `Ok(false)` using bounded,
  allocation-free validation; unsupported algorithms and backend failures remain
  errors. Consumer regressions cover empty, truncated, noncanonical, and
  trailing-garbage signatures alongside the published P-256 verification vector.
- OpenSSL 3 adds affine key generation/import/export, RSA-PSS/ECDSA/Ed25519
  signatures, X25519/EC agreement, streaming digest/HMAC, and X.509 parsing with
  explicit-time CA-chain and SAN-host verification. RSA generation is bounded
  to 2048–8192 bits in 256-bit steps; Ed25519, X25519, EC-P256, and EC-P384 use
  a zero size argument. Private PEM import rejects encryption. Certificate
  parsing validates ASN.1 validity times and preserves checked signed Unix
  seconds, including dates before the epoch.
- Affine `Uuid` owners parse canonical UUID text and network-order bytes, preserve
  long bit patterns, format canonical lowercase text, and compare unsigned
  network-byte order. OpenSSL supplies RFC v4 randomness and EVP-backed RFC v3/v5
  namespace hashing; MD5/SHA-1 are documented as identity-only.

- Native `Encoding` converts affine byte buffers to lowercase hex, canonical
  padded RFC 4648 Base64, canonical unpadded Base64URL, and strict UTF-8 `Text`.
  Decoders reject malformed alphabets, padding, discarded bits, and invalid
  scalar encodings while preserving embedded NUL.

- Static `Crypto` adds OpenSSL 3 EVP/RAND digests, HMAC, PBKDF2, HKDF, AES-GCM,
  CSPRNG bytes, and constant-time equality with borrowed inputs and affine outputs.
  Authentication failures wipe unexposed plaintext; unsupported digests and invalid
  lengths return generic `Err` diagnostics.
- Certificates export canonical leaf-only or complete ordered-chain PEM, and
  round-trip regressions verify the preserved intermediate chain. TLS peer
  certificates own independent copies of the available leaf and peer chain, so
  their PEM remains exportable after the stream closes. Crypto, key signatures,
  streaming digest, and MAC share the same exact six-name digest allowlist.
- Secret-returning PEM, agreement, KDF, and decryption values remain ordinary
  caller-owned objects without a clear/drop wiping guarantee; documentation
  distinguishes those results from explicitly cleansed internal scratch.
- Adds native zlib-wrapped DEFLATE, RFC 1952 gzip, bounded strict decompression,
  concatenated gzip-member handling, and unsigned CRC-32/Adler-32 checksums over
  borrowed binary ByteBuffers. ZIP archive and entry operations remain a gap;
  gzip is not presented as ZIP compatibility.
- Optional ICU-backed Unicode transforms, grapheme boundaries, locale collation,
  exact decimal parsing/formatting, currency formatting and strict charset
  conversion preserve affine ownership. Locale parsing consumes complete input
  through ICU's exact decimal interface; currency decimals avoid binary64.
  Charset aliases and discoverable canonical names come from ICU's converter
  registry, with STOP callbacks and explicit-endian UTF-16 behavior. Locale
  arguments retain explicit root/BCP-47 rules and supported collation/numbering
  extensions. Ordinary Text operations remain usable without ICU.
- Adds affine SQLite `Database` connections and independently owned
  `SqlStatement` cursors on the same connection, with copied length-delimited
  bindings/rows, strict type accessors, transactions, foreign-key enforcement,
  and bounded busy timeouts. `prepare` and `execute` reject multi-statement SQL
  before preparing any part, while preserving trigger bodies, quoted
  semicolons, token-leading UTF-8 BOMs, and leading empty commands. `execute`
  uses a transient statement; statement completion is terminal until reset, and
  failed steps require reset before retry. Connection close/drop rolls back open
  transactions before `sqlite3_close_v2` defers backend deallocation, releasing
  transaction locks while independently owned statements remain alive. Explicit
  close preserves the connection when rollback fails. Empty blobs remain distinct
  from allocation failures.
- Affine ZipReader and ZipWriter use libarchive for sequential ZIP/Zip64 reads and
  store/DEFLATE writes with thread-scoped C.UTF-8 pathname conversion. File and
  memory readers require a complete seekable ZIP directory; bounded reads check
  decompressed output, CRC and truncation. UTF-8-flagged entry names preserve their
  exact bytes and separators without normalization; invalid complete UTF-8 names
  and embedded NUL are rejected before conversion. Header rejection leaves no
  readable current entry and permits advancement. The pinned private libarchive
  correction is generated against the exact release tarball and validates EOCD
  comment extent plus directory byte/entry counts. It searches the entire legal
  65,557-byte EOCD/comment range without stopping at invalid embedded signatures,
  reads preceding Zip64 locators separately, and corrects SFX-relative offsets
  after Zip64 expansion. Raw UTF-8 storage uses a fallible copy. Strict seekable
  ZIP/Zip64 handling is retained without streaming fallback.

- Affine XML uses libxml2 for namespace-aware DOM/XPath, mutation, serialization,
  XSD validation, incremental pull events with stable owned snapshots, and
  balanced streaming output written directly to ByteBuffer. Compiled libxslt
  stylesheets transform into independent XmlDocument owners. DTD/entity loading,
  XInclude, external schema composition, stylesheet include/import and extensions,
  and external transform reads/writes are denied without process-global loader or
  security settings. Native XML builds require libxml2/libxslt development packages.
  Namespace-invalid documents/fragments are rejected before mutation; setText
  preserves literal character data and appended fragments parse as UTF-8.
  Reader/event/writer clients emit their required helpers independently. Namespace
  errors remain sticky; rejected prefix conflicts preserve writer state. Snapshot
  local names share their owned qualified-name storage, and writer metadata transfers
  existing string owners rather than allocating duplicate attribute keys. XSLT
  preserves borrowed sources with private tree copies, rejects non-document results,
  frees failed compilations, and initializes global library state under one once
  callback. Availability checks never discover external plugins, verified against
  a module-enabled libxslt build and a harmless initializer sentinel.
- Floating min/max normalize signed-zero ties explicitly rather than relying on
  host libm's unspecified tie sign; both operand orders and same-sign zeros are
  covered by generated-program sanitizer regressions.
- Numeric generation handles BigInt and BigDecimal independently. Parsing accepts
  either ASCII sign and checks significant coefficient size before GMP conversion;
  formatted Text adopts its completed buffer and terminal GMP results transfer with
  `mpz_swap` instead of copying completed limbs.
- Numeric native emission derives method classification from its binding tables
  and selects template sections in one pass, without a second expression visitor
  or repeated whole-template copies. Numeric regressions use named borrowed
  operands and exercise coefficient boundaries within the source-size limit.
- Native affine concurrency adds sequentially consistent atomics, timed semaphores,
  countdown latches, reusable timeout-breaking barriers, mutex-associated conditions,
  cooperative cancellation, one-shot futures, and bounded/delayed thread pools.
  Recurring pool tasks support fixed-rate catch-up and fixed-delay cadence without
  overlapping themselves, preserve task ownership on rejection, and terminate cleanly
  on cancellation, task error, or pool shutdown.
  Fixed-delay deadlines are captured at callback completion rather than after
  synchronous result disposal; fixed-rate rearming avoids an unused clock read.
  Submission names, native methods and scheduling policy now share one metadata
  classification across validation and native dispatch.

- Condition-bearing synchronizer handles preserve their nodes across explicit clones;
  a later barrier timeout cannot change an earlier generation's result, partial pool
  startup publishes shutdown under the pool control mutex, and failed future setup drops
  its consumed task. Generic native dependencies are registered during specialization.

- Borrowed channel wakeup regressions synchronize closure after entry into the
  real condition wait, proving blocked receiver EOF and owned sender recovery.
- Existing composition and lending examples use `ReadJob` and `DeskCommand`
  rather than shadowing the new reserved Task and Command builtins.
- Public references describe the settled alpha in English; obsolete milestone
  documents are removed and examples retain their organized domain paths.
- Mandatory design-principles review found `src/stdlib_codegen/io.rs` mixing
  Console, Args and filesystem lowering (SRP), while `stdlib/files.c.in` kept a
  separate whole-file stdio path (DRY). Filesystem declarations/emission now have
  a dedicated prelude unit and backend leaf; `Files.readText`/`writeText` reuse
  direct ByteBuffer/descriptor helpers, and atomic replacement reuses path/write
  helpers with close-on-exec temporary descriptors. Filesystem APIs remain direct;
  the separate allocation-free ByteSource/ByteSink interfaces and bounded Streams
  loops provide reusable transfer operations.
- Linux filesystem support adds immutable Path, affine descriptor-backed File and
  Directory, `lstat` metadata, and nonrecursive Files operations. File supports
  cursor positioning, truncation, and cursor-preserving `pread`/`pwrite`; append
  descriptors reject positioned writes. FileInfo reports POSIX mode and Unix-
  millisecond timestamps. Files copy regular files with bounded memory, create
  directories, manage links and modes, list names, and resolve canonical paths;
  invalid UTF-8 canonical results return recoverable `EILSEQ`. Positioned reads
  respect the platform's `min(SIZE_MAX, INT64_MAX)` ByteBuffer bound. Linux inotify
  Watcher emits stable event kinds and explicit queue overflow, accepts nameless
  watched-directory attribute events without invalidating the watch, uses
  monotonic deadlines, invalidates on self-deletion, and closes idempotently.
  WatchEvent accessors are emitted independently of Watcher. Files writes return
  `Result<Unit,Text>`; standalone postfix `?` preserves ordinary call-statement
  success cleanup and propagates owned errors. FIFO write failures preserve the
  caller's signal mask and pending SIGPIPE state.
- BinaryReader and BinaryWriter are unique-owned, choose byte order explicitly,
  preserve reader cursors on incomplete/invalid UTF-8 input, encode binary32 and
  binary64 values, and transfer the writer's exact buffer on finish. ByteSource/
  ByteSink are allocation-free interfaces implemented by File, TcpStream, and
  TlsStream. Streams preserves partial progress in owned StreamError values,
  including pending copy packets, read prefixes and exact-limit probe bytes;
  writeAll retains the caller's source and reports only confirmed offsets.

- Linux networking adds immutable IPv4/IPv6 `SocketAddress`, owned TCP listeners/streams,
  UDP sockets and datagrams with poll-based deadlines and direct ByteBuffer appends.
  `stdlib/time_clock.c.in` now owns the monotonic clock/deadline helpers shared by
  Time and networking; no DNS-timeout or descriptor-sharing API is implied.

- OpenSSL-backed TLS adds affine client/server configurations, hostname-verified
  client streams, mutual-auth server support, bounded monotonic handshakes and
  I/O, and explicit mutex-handle ownership. URI-driven WebSocket clients add
  `ws`/`wss` upgrades, masked client frames, bounded message assembly, ping, and
  peer close handshakes. Both surfaces reuse shared socket, DNS, IDNA, and path
  helpers rather than duplicating platform conversion logic.
  The loopback test verifies automatic Pong replies between UTF-8 message
  fragments.
  Each SSL attempt isolates OpenSSL error-queue state and scopes SIGPIPE to its
  calling thread without changing process-wide signal handling. Shared deadline
  checks bound continuously successful frame processing, and WSS shutdown keeps
  the close operation's original deadline. Native dependency expansion retains
  WebSocket's hidden address-resolution types without requiring a consumer to
  mention `SocketAddress`.
  Compiler catalogs now register these native class/method identities for prelude
  injection and typed IR emission; VS Code, Zed, and Tree-sitter recognize their
  public builtin type names.
- Datagram accessors are emitted independently of UdpSocket specialization,
  including unoptimized native compilation of standalone borrowed helpers.
- `Uri` adds owned RFC 3986 parsing, explicit normalization/resolution,
  percent-component codecs and byte-exact equality; `Dns` adds ICU UTS #46
  hostname conversion, reverse lookup, local hostname, and synchronous forward
  resolution composed through `SocketAddress.resolve`.
  Native emission uses requested metadata rather than a duplicated URI-method
  list or recursive expression visitor, and template slices reuse the shared
  borrowed helper. URI/DNS parameter and Result substitutions follow one naming
  convention; native linking and CI install the URI and SQLite prerequisites.
- Design-principles review keeps TCP, UDP and address resolution in one cohesive
  networking unit, hides descriptor/sockaddr state, and keeps Datagram source
  ownership closed over its value. Results report timeouts; TCP EOF is Ok(0),
  while datagram truncation is successful metadata (CQS/DbC/POLA). Console and Args
  now have separate backend leaves; the IO storage helper has no emitter routing.

- Affine `HttpRequest` and `HttpResponse` use per-request libcurl easy handles
  with `pthread_once` global initialization, verified HTTPS, no automatic
  redirects/retries, copied request bodies, and bounded binary response capture.
  Response headers are parsed with an explicit byte/count bound, duplicate fields
  use last-value lookup, interim blocks are discarded, and non-2xx status codes
  remain successful transport results. The status parser rejects prohibited
  control bytes in reason phrases. Bounded body growth uses the runtime realloc
  helper instead of explicit allocate/copy/free. Loopback tests cover methods,
  headers, binary bodies, growth beyond initial capacity, malformed inputs and
  status lines, limits, timeouts, and local-CA TLS.

- Tree-sitter, Zed and VS Code syntax now cover module/import paths, qualified
  generic static calls, native scalar literals and casts, and postfix Result
  propagation. Editor snippets exercise owned task joins, bounded channels,
  TCP connections and process output capture.

- Per-file modules and imports resolve public/private type and function names
  before generic expansion, preserving file-local diagnostics and editor overlays.
- Byte, Unicode-scalar Char, Long, Double and Unit values work through native
  expressions and collections. Int/Long arithmetic wraps explicitly; casts and
  shift counts are checked. Text supports numeric append and parse operations.
- GMP-backed affine `BigInt` and `BigDecimal` provide exact radix parsing,
  integer arithmetic, signed division/modular operations, scaled decimal arithmetic,
  all eight decimal rounding modes, and checked long conversions. Numeric outputs
  that use these APIs link GMP; documented coefficient, text, and scale bounds
  reject oversized fallible operations without floating-point approximation.
- Generic static calls such as `Factory<List<int>>.identity(value)` specialize
  the owner before checking, without confusing generic closers and shift operators.
- Native library lowering and trusted prelude declarations use focused units and
  one builtin catalog. Runnable examples are organized by domain and cataloged.
- Equivalent Nacre/Rust collection benchmarks validate checksums, separate
  allocation instrumentation from timing, and isolate measured child RSS from
  the Python launcher through a native supervisor.
- Math exposes defined IEEE behavior and fallible signed-minimum absolute values.
  Unique-owned SplitMix64 Random supports deterministic reseeding, unbiased bounds
  and real OS entropy. Time clocks and sleeps return structured errors with checked units.
- Math adds C11 libm functions for inverse/hyperbolic trig, logarithmic and
  exponential refinements, angle conversion, adjacent-double stepping, scaling,
  exponent/ULP queries, sign handling and IEEE remainder. Checked int/long
  arithmetic and floor division/modulus return `Result` errors on overflow or
  zero divisors; consumer regressions cover both integer widths and IEEE edges.
- ICU4C-backed `DateTime` adds proleptic Gregorian conversion, strict parsing and
  locale formatting, explicit DST overlap choices, calendar arithmetic and zone
  listings. It is linked only when used; the existing `Time` API stays dependency-free.
- Fixed Arrays transfer List storage and preserve affine element ownership;
  ByteBuffer supports checked long-sized binary storage and shared native I/O reservation.
- Text supports scalar operations, slicing, search, replacement and splitting.
  PCRE2-8 Regex uses UTF-8 and Unicode properties, explicit lengths including
  embedded NUL, true full-subject matches, leftmost-first search and owned
  captures. Match, depth and heap exhaustion return `Err`; byte-slice endpoint
  checks remain constant-time under validated Text invariants.
- Set reuses Map table storage without a wrapper allocation; Deque moves affine
  ring elements through wrap and growth. List heapsort and lower-bound searches
  support total primitive ordering or borrowed Comparators, without enum operand copies.
- Comparator-ordered maps and sets use sorted contiguous storage; PriorityQueue
  uses a binary min-heap, and BitSet adds packed ranges and logical operations.
- List drain/collect and lazy map/filter/take/skip adapters form owned iterator
  pipelines that transfer elements and recursively drop unconsumed owners.
- Module resolution preserves E4101 classification for type parameters that shadow
  declared or imported types and reserved names.
- Native Text owners embed up to 16 bytes; growth and owned-buffer transfers
  preserve unique header identity and free only external byte storage.
  Cloning copies already-validated Text without another UTF-8 pass; comparative
  map query/replacement allocation regressions remain representation-independent.
- Native POSIX threads, explicitly shared mutex handles and bounded channels move
  owned tasks and values without cloning. One recursive sendability check covers
  task implementations, results, fields, enum payloads and native containers.
  Guard borrows are receiver-rooted; channel abort returns unsent values and drops
  queued owners outside its lock.
  Guard accessors borrow stored heap pointers or inline enum addresses as appropriate;
  hidden Deque payloads participate in exhaustive native sendability classification.

- Postfix `?` unwraps trusted built-in `Result` values or returns the exact
  error payload after cleaning live locals and pending owned C temporaries.
  Typed IR preserves the carrier layouts; calls, stores, enum wrapping and
  returns explicitly transfer temporary ownership.

- Map/Set default hashing covers scalar, `Unit`, static `String`, and owned `Text`
  keys. Double NaNs compare/hash equally and signed zeros share a hash; strings and
  Text compare exact-length UTF-8 bytes.
- Custom class/interface/enum keys use an explicitly owned `Hasher<T>` supplied
  through `withHasher`. Borrowed queries, retained equal Map keys, dropped Set
  duplicates, cached 64-bit hashes across growth, and recursive policy sendability
  preserve affine ownership without callbacks during relocation.
- POSIX Environment and affine Command/Process APIs synchronize environment
  snapshots, resolve PATH without shell fallback, and provide bounded concurrent
  capture, redirected files, monotonic waits, and deterministic child reaping.
  Unreaped Process owners are SIGKILLed and synchronously reaped on drop, including
  SIGTERM-resistant children. Process and ProcessOutput accessors specialize
  independently of Command; Random retains its seeded constructor.
- CI maintenance: update [checkout to 7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1),
  [setup-node to 7.1.0](https://github.com/actions/setup-node/releases/tag/v7.1.0),
  and [setup-python to 7.0.0](https://github.com/actions/setup-python/releases/tag/v7.0.0).
  These actions use Node 24 instead of deprecated Node 20; language semantics,
  test matrices and selected Node/Python toolchain versions are unchanged.

## 0.11.0

- Built-in integer-map traversal binds copied keys and copied primitive or
  shared/exclusive affine values. Each live entry is visited once in unspecified
  order; collections are evaluated once and loaned for the complete loop.
- Direct occupied-slot scanning avoids iterator allocations, affine-owner copies
  and key rehashing. Existing block/while ownership and cleanup handle nested
  loops, continue, break, return and receiver-derived borrowed results.
- List and map traversal share binding/receiver contract checks. Internal slot
  expressions keep C layout knowledge in the native collection backend.
- Editor packages 0.6.0, grammar 0.5.0 and 27 snippets cover both loop bindings.
- Development is feature-driven and versions use ordinary pre-1.0 0.x.y releases.
  Current help, diagnostics, dump headers and docs no longer use Vx branding;
  historical reports/contracts and Git history retain their original identity.
- Design/review follows the linked Rust design principles: focused composition,
  encapsulation, explicit contracts, side-effect-free queries, DRY/KISS and
  predictable behavior. Mutating commands may return transferred ownership.
- No first-class iterators, text/custom keys, NLL/lifetime refinements, disjoint
  entry borrowing, fallible allocation or new backend is introduced.

## 0.10.0

- Map<int,V> supports all signed int32 keys and the existing copied/owned value
  representations. Put transfers the previous value in an inline Option;
  tryRemove transfers a present value and preserves an absent map.
- Primitive map values expose tryGet; affine values expose receiver-bound
  shared/exclusive getRef/getMut. Whole-map/root loans retain lexical alias rules.
- One-buffer open addressing uses unsigned hashing, checked growth, and
  backward-shift deletion without tombstones. Relocation transfers ownership;
  replacement and removal do not allocate wrappers or destroy returned owners.
- The complete multi-file lending desk uses polymorphic books/tools, generic
  catalogs, stable IDs, availability states, member relationships, local parsing
  views, recoverable commands, logical-day overflow guards and atomic persistence.
- A feature-to-function matrix and rejected companions document the safety
  boundaries. Affine collision-chain deletion has explicit object/enum/interface
  regressions, in addition to primitive growth and replacement coverage.
- Editor packages 0.5.0, grammar 0.4.0 and 24 snippets cover map access and transfer.
  Generated C identifies the compiler package version rather than a pinned banner.
- Text/custom keys, map traversal, lifetime refinements, fallible allocation,
  exception unwinding and new backends remained excluded from 0.10.0.

## 0.9.0

- List `for` traversal copies primitive elements or explicitly borrows affine
  elements shared/exclusively. The collection is evaluated once and loaned for
  the whole loop; each element binding ends at its iteration boundary.
- Existing block/while cleanup handles nested loops, `continue`, `break`, and
  `return`, without an iterator allocation or cloning the user-body AST.
- `tryGet` copies primitive elements into inline `Option<T>`; `tryPop` and
  `tryRemove` transfer every supported owned representation. Failure preserves
  the list; success clears the vacated slot and transfers cleanup exactly once.
- Contact-book substring search, phone filtering, and confirmed batch removal
  exercise traversal and recoverable extraction. Unicode persistence and
  candidate-load rollback retain the existing version-1 file format.
- Editor packages 0.4.0, grammar 0.3.0, and 20 snippets include list traversal and
  recoverable removal.
- First-class iterators, maps, lifetime refinements, fallible allocation, exception
  unwinding, and new backends remain outside this release.

## 0.8.0

- Instance methods and interface contracts return shared/exclusive borrows proven
  to derive from their receiver. Caller loans remain lexical; escaping locals or
  parameters and readonly-to-exclusive conversions are rejected.
- Affine `List<T>` elements expose checked, allocation-free `getRef`/`getMut`.
  Object, inline enum, and interface representations preserve their correct ABI;
  conflicting loans block whole-list mutation, relocation, movement, and cleanup.
- Typed assignment replaces owned object/interface/enum fields, evaluating the
  RHS once before destroying the old value. Arbitrary extraction/partial moves
  and borrowed-field reassignment remain unsupported.
- Explicit `borrow`/`borrow mut` expressions and borrowed exhaustive enum matches
  copy primitives and lend nominal payloads without consuming tags or owners.
- Projected receivers remain stable during argument/RHS evaluation; ancestor
  mutation cannot free a pending child receiver pointer.
- Complete interactive contact book: shared consultation, exclusive in-place
  editing, optional phone matching, add/remove, validated candidate loading, and
  atomic Unicode/path-with-spaces persistence.
- Editor packages 0.3.0, grammar 0.2.0, and 16 snippets cover borrowed returns. Both actual
  desktop applications exercise compiler diagnostics, overlays, restart/reopen.
- Behavioral/native regressions replace obsolete restrictions and source-shape
  assertions. Rust 1.74, GCC/Clang sanitizer/LLVM paths, real fuzzing, and remote
  CI are exercised.
- Clean Nacre identity across crate, CLI, `.ncr` sources, `nacre.json`, editors,
  runtime selectors, project directory, and packages. Obsolete history, reports,
  binaries, generated caches, and graph artifacts are removed; original mandatory
  legal attribution is retained.
- Published verification summaries omit machine-specific paths, process IDs,
  session-local log references, and private desktop evidence locations.

## Stabilization / 0.7.1

- `Files.writeTextAtomic` writes a sibling temporary file and commits using
  `rename`; Sudoku saves use it. Atomic visibility does not imply crash durability.
- Real frontend and LSP framing libFuzzer targets run bounded campaigns over
  isolated copies of intentional seeds.
- VS Code lifecycle operations serialize activation, restart, and shutdown;
  failed-start recovery no longer stops an inactive client.
- Process-argument storage belongs to generated executables rather than generic
  runtime fixtures. Current Text Result dependencies and locked Zed builds are
  represented correctly in verification harnesses.
