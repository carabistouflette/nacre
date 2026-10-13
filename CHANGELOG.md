# Nacre changelog

## 0.12.0
Experimental native alpha; Linux systems APIs, breaking source changes permitted.

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
- Ordered-key queries preserve affine borrows and specialize every copied-key
  query consistently. Native iterator interface adapters include their diagnostic
  arguments, and mapper extraction uses the element representation, not the
  input-interface representation. Hidden callback and element state remains
  subject to recursive sendability checks.
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
- Adds native zlib-wrapped DEFLATE, RFC 1952 gzip, bounded strict decompression,
  concatenated gzip-member handling, and unsigned CRC-32/Adler-32 checksums over
  borrowed binary ByteBuffers. ZIP archive and entry operations remain a gap;
  gzip is not presented as ZIP compatibility.
- Optional ICU-backed Unicode text transforms, grapheme boundaries, locale collation,
  Unicode scalar properties, strict charset conversion, and locale-aware number
  formatting use explicit root/BCP-47 locale rules, preserve supported Unicode
  collation/numbering extensions, and preserve affine ownership. Ordinary Text
  operations remain usable without ICU.
- Adds affine SQLite `Database` with one explicitly owned internal prepared
  statement, copied length-delimited bindings/rows, strict type accessors,
  transactions, foreign-key enforcement, and bounded busy timeouts. `prepare`
  and `execute` reject multi-statement SQL before preparing any part, while
  preserving trigger bodies, quoted semicolons, token-leading UTF-8 BOMs, and
  leading empty commands. `step` completion is terminal until reset/replacement,
  and failed steps require reset before retry. Empty
  blobs remain distinct from allocation failures. This native SQL API documents
  its single-cursor limit and does not claim JDBC parity.
- Affine ZipReader and ZipWriter use libarchive for sequential ZIP/Zip64 reads and
  store/DEFLATE writes with thread-scoped C.UTF-8 pathname conversion. File and
  memory readers require a complete seekable ZIP directory; bounded reads check
  decompressed output, CRC and truncation. Entries own their UTF-8 metadata, regular
  file additions reject trailing slashes, and writers require explicit close to
  report final directory errors without extracting or normalizing entry names.
  Header rejection leaves no readable current entry while allowing subsequent
  advancement. The native dependency build pins and verifies libarchive 3.8.9
  with its upstream zero-entry bidder correction; strict central-directory
  validation is preserved rather than falling back to streaming format detection.

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
  ByteSink are allocation-free interfaces implemented by File and TcpStream.
  Streams distinguishes prefix-returning `readUpTo` from EOF-enforcing `readAll`,
  which probes and consumes one excess byte before returning `Err`; bounded loops
  handle partial transfers, reject zero progress, and flush according to the sink.

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
  remain successful transport results. Loopback tests cover methods, headers,
  binary bodies, malformed inputs/headers, limits, timeouts, and local-CA TLS.

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
