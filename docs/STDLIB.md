# Nacre standard library

This is the source-level API reference for the experimental Linux native alpha.
The compiler injects the built-in declarations before checking user source. Built-in
names are reserved and available unqualified; no standard-library import is needed.
There is no Java runtime, GUI, reflection, JVM, dynamic loading, or general-purpose
FFI layer. Native methods are part of the trusted compiler/runtime base.

## Values, errors, and ownership

The language's scalar types are `byte`, `char`, `int`, `long`, `double`, `boolean`,
`String`, and `Unit`. Scalars and static `String` values copy. `String` is an
immutable view of a source literal; `Text` is a separately owned, mutable object.
Ordinary object and handle values are affine: a by-value argument transfers its
owner, and scope cleanup destroys any owner that was not transferred. There is no
implicit clone or general reference counting. `clone()` and the named shared-handle
operations below are explicit exceptions.

Fallible library operations use `Result<T, Text>`. A successful command returns
`Ok(())`, where `()` is the value of the inhabited `Unit` type; an error returns an
owned diagnostic `Text`. `Option<T>` represents absence, not an error. A nonzero
process exit is an `ExitStatus`, not a spawn/wait error. Reads and writes report
counts or preserve partial effects where applicable; `Err` does not roll back OS
writes already performed.

Postfix `?` consumes an owned built-in `Result<T, E>` inside a function returning
`Result<U, E>` with the same error type. It moves out `Ok(value)` or returns the
owned `Err(error)` from the enclosing function after normal lexical cleanup. It
cannot be applied to a borrowed result, another sum type, or a function that does
not return a compatible built-in `Result`.

Ordinary allocation failure follows the runtime's fatal-allocation policy. A
`Result` reports the recoverable OS, range, conversion, and synchronization errors
specified below; it does not make allocation generally fallible. Fatal runtime
failures do not unwind scopes. No throughput or latency guarantees are made.

## Numeric and scalar types

| Type | Meaning |
|---|---|
| `byte` | Unsigned 8-bit integer, range 0 through 255. |
| `char` | One Unicode scalar value (not a UTF-8 byte, UTF-16 code unit, or grapheme). |
| `int` | Signed 32-bit integer. Addition, subtraction, multiplication, and negation wrap modulo 2^32; division or remainder by zero is fatal. |
| `long` | Signed 64-bit integer. Integer literals use an `L` suffix, such as `42L`. Arithmetic wraps modulo 2^64; division or remainder by zero is fatal. |
| `double` | C `double` / IEEE-754 binary floating-point value. Decimal and exponent literals include examples such as `1.25` and `2e3`. |
| `Unit` | An inhabited, trivially droppable value written `()`, used for successful commands and generic payloads. |
| `boolean` | `true` or `false`. |
| `String` | Immutable, statically stored UTF-8 literal view; it is not an owned dynamic string. |

A character literal contains exactly one Unicode scalar, for example `'λ'` or
`'\u{1f9ed}'`. Character literals support `\n`, `\r`, `\t`, `\0`, `\"`, `\'`,
`\\`, and `\u{...}` escapes. String literals support `\n`, `\r`, `\t`, `\0`,
`\"`, `\'`, and `\\`, but not Unicode escapes. Numeric expressions are promoted
to a common numeric type; a wider numeric value can be used where a narrower type
is not required. Explicit numeric casts use `(type)value`. A checked cast that is
out of range, non-finite where disallowed, or not a Unicode scalar terminates
with a runtime diagnostic rather than returning `Result`.

`Text.appendByte` formats the unsigned byte as decimal text; it does not append a raw byte. Use `ByteBuffer` for binary bytes.

## `Text` and UTF-8

`Text` owns validated, length-delimited UTF-8. Embedded NUL is valid content. Text methods treat data as UTF-8 text; this is distinct from the arbitrary binary data held by `ByteBuffer`.

```java
Text(String initial)
void append(String value)
void appendInt(int value)
void appendByte(byte value)
void appendChar(char value)
void appendLong(long value)
void appendDouble(double value)
void appendBoolean(boolean value)
void appendText(borrow Text other)
readonly int byteLength()
readonly int byteAt(int byteIndex)
readonly boolean isEmpty()
void clear()
readonly void print()
readonly Text clone()
readonly boolean equals(borrow Text other)
readonly boolean equalsString(String other)
readonly Result<int, Text> parseInt()
readonly Result<long, Text> parseLong()
readonly Result<double, Text> parseDouble()
readonly Result<Text, Text> sliceBytes(int start, int end)
readonly int charCount()
readonly char charAt(int scalarIndex)
readonly int compareTo(borrow Text other)
readonly int indexOf(borrow Text needle)
readonly boolean startsWith(borrow Text prefix)
readonly boolean endsWith(borrow Text suffix)
readonly Result<Text, Text> slice(int startScalar, int endScalar)
readonly Text trim()
readonly Text replace(borrow Text needle, borrow Text replacement)
readonly List<Text> split(borrow Text delimiter)
```

`byteLength`, `byteAt`, and `indexOf` use UTF-8 byte offsets. `charCount`, `charAt`,
and `slice` use Unicode scalar positions; neither unit counts grapheme clusters.
`sliceBytes` copies a half-open byte range and returns `Err` unless its bounds are
valid UTF-8 boundaries. `slice` copies a half-open scalar range and returns `Err`
for an invalid range. `indexOf` searches exact UTF-8 bytes and returns `-1` when
absent (an empty needle matches at offset zero). `compareTo`, equality, prefixes,
and suffixes compare exact bytes; there is no normalization or case folding.

`trim` removes Unicode White_Space scalars at both ends. `replace` uses exact byte
matches; an empty needle inserts the replacement at each scalar boundary,
including both ends. A nonempty `split` delimiter preserves empty fields; an empty
delimiter splits into one-scalar `Text` values, and empty text split by an empty
delimiter produces an empty list. Returned slices, replacements, and split fields
are owned values, not borrowed views.

`parseInt` and `parseLong` accept an optional ASCII sign followed by ASCII decimal
digits only, with no whitespace or suffix, and return `Err` on malformed or
out-of-range values. `parseDouble` accepts a full decimal significand with an
optional exponent, or a signed `NaN`/`Inf` spelling, and returns `Err` on invalid
syntax or range errors. `print()` writes the text followed by a newline. Static
`String` and owned `Text` are not implicitly convertible.

## `ByteBuffer`

`ByteBuffer` is an affine binary byte owner with a private resizable allocation. It
does not validate UTF-8 and exposes no pointer, slice, or implicit clone.

```java
static Result<ByteBuffer, Text> withCapacity(long initialCapacity)
readonly long length()
readonly long capacity()
readonly byte get(long index)
void set(long index, byte value)
void append(byte value)
Result<Unit, Text> appendBytes(borrow ByteBuffer source)
void clear()
Result<Unit, Text> truncate(long newLength)
```

Capacity and indices are byte counts. Negative capacity and an invalid truncate
range return `Err`; `get`/`set` out of bounds are fatal runtime diagnostics.
`appendBytes` copies the source's current contents and retains no reference to it.
`clear` sets length to zero but retains capacity. Growth checks size overflow and is
not specified as a latency or allocation-count guarantee; out-of-memory remains
fatal. Stream and file reads append to existing contents rather than clearing them.

## `Encoding`

`Encoding` provides strict native conversions between arbitrary bytes and owned
UTF-8 `Text`. Every returned `Text` or `ByteBuffer` is an affine owner; decoders
return an owned diagnostic `Text` in `Err` for invalid input.

```java
static Text hexEncode(borrow ByteBuffer bytes)
static Result<ByteBuffer, Text> hexDecode(borrow Text text)
static Text base64Encode(borrow ByteBuffer bytes)
static Result<ByteBuffer, Text> base64Decode(borrow Text text)
static Text base64UrlEncode(borrow ByteBuffer bytes)
static Result<ByteBuffer, Text> base64UrlDecode(borrow Text text)
static ByteBuffer utf8Encode(borrow Text text)
static Result<Text, Text> utf8Decode(borrow ByteBuffer bytes)
```

Hex encoding uses lowercase digits. Hex decoding accepts either digit case and
rejects odd-length or non-hex input. Basic Base64 encoding uses the RFC 4648
alphabet and canonical padding; its decoder requires padding where needed.
Base64URL uses `-` and `_` and emits canonical unpadded text; its decoder accepts
either canonical padded or unpadded input. Both Base64 decoders reject whitespace,
the other variant's alphabet, malformed padding, impossible lengths, and
nonzero discarded bits.

UTF-8 encoding copies every byte of valid `Text`, including embedded NUL. UTF-8
decoding preserves NUL and returns `Err` for truncated, overlong, surrogate, or
out-of-range encodings, or input too large for `Text`; invalid data is never replaced.

## `Crypto`

`Crypto` exposes focused primitives implemented with OpenSSL 3 EVP and RAND APIs.
Its byte-buffer inputs are borrowed; successful byte outputs are new affine owners.
Algorithm names are exact, case-sensitive spellings: `SHA256`, `SHA384`, `SHA512`,
`SHA3-256`, `SHA3-384`, and `SHA3-512`. Unsupported names and names containing
embedded NUL return `Err`; there is no algorithm fallback. Recoverable operation
errors use generic diagnostics and never include key, password, salt, or input bytes.

```java
static Result<ByteBuffer, Text> randomBytes(long length)
static Result<ByteBuffer, Text> digest(borrow Text algorithm, borrow ByteBuffer data)
static Result<ByteBuffer, Text> hmac(borrow Text algorithm, borrow ByteBuffer key, borrow ByteBuffer data)
static Result<ByteBuffer, Text> pbkdf2(borrow Text algorithm, borrow ByteBuffer password, borrow ByteBuffer salt, int iterations, long outputLength)
static Result<ByteBuffer, Text> hkdf(borrow Text algorithm, borrow ByteBuffer key, borrow ByteBuffer salt, borrow ByteBuffer info, long outputLength)
static Result<ByteBuffer, Text> aesGcmEncrypt(borrow ByteBuffer key, borrow ByteBuffer nonce, borrow ByteBuffer plaintext, borrow ByteBuffer aad)
static Result<ByteBuffer, Text> aesGcmDecrypt(borrow ByteBuffer key, borrow ByteBuffer nonce, borrow ByteBuffer ciphertextAndTag, borrow ByteBuffer aad)
static boolean constantTimeEquals(borrow ByteBuffer left, borrow ByteBuffer right)
```

`randomBytes` uses the system CSPRNG; zero length is valid, while negative lengths
and outputs above `INT_MAX` return `Err`. PBKDF2 requires positive iterations and
limits output to `INT_MAX`. HKDF follows RFC 5869's limit of 255 digest blocks.
Digest, HMAC, PBKDF2, and HKDF use the selected supported digest exactly.

AES-GCM accepts 16-, 24-, or 32-byte keys and a 12-byte nonce. Callers must never
reuse a nonce with the same key. Encryption returns ciphertext followed by the
16-byte authentication tag. Decryption returns `Err` for malformed keys/nonces,
truncated tags, or authentication failure; unauthenticated plaintext is wiped and
freed before returning, and no plaintext owner is exposed. Inputs that would
narrow beyond OpenSSL's `int` lengths are rejected.

Equal-length `constantTimeEquals` comparisons use OpenSSL's constant-time
comparison primitive. Different lengths return `false` immediately, so lengths
are not hidden. This API is not a complete cryptography library: it provides no
signature, certificate, key-management, or protocol-composition facilities.
Native programs require the OpenSSL 3 development and runtime libraries.

## `Option<T>` and `Result<T, E>`

```java
enum Option<T> { None; Some(T); }
enum Result<T, E> { Ok(T); Err(E); }
```

These are ordinary pre-injected affine enum values, stored inline without an
allocated wrapper. Construct variants explicitly, for example
`new Result<Unit, Text>.Ok(())`; `match` is exhaustive and consuming unless the
enum is explicitly borrowed. Payload ownership moves with its variant. There is no
implicit unwrapping or exception mechanism.

## Collections

Collections own their contents and are affine. `T` must be a copied scalar/static
`String` or an owned object, interface, or enum; borrows and local views cannot be
stored. A collection's element references loan the complete collection root, not
just one index or key. Structural mutation, movement, replacement, and destruction
that conflict with a live loan are rejected until its lexical scope ends.

### `List<T>`

```java
List<T>()
void add(T value)
readonly int size()
readonly int capacity()
readonly boolean isEmpty()
readonly T get(int index)                         // copied specializations only
readonly Option<T> tryGet(int index)              // copied specializations only
Option<T> tryPop()
Option<T> tryRemove(int index)
readonly borrow T getRef(int index)                // affine specializations only
borrow mut T getMut(int index)                     // affine specializations only
void set(int index, T value)
T pop()
T remove(int index)
void clear()
void reserve(int capacity)
void sort()                                        // supported default-order specializations only
void sortBy(Comparator<T> comparator)
readonly Option<int> binarySearch(T value)         // supported default-order specializations only
readonly Option<int> binarySearchBy(Comparator<T> comparator, borrow T value)
```

Copied access (`get`, `tryGet`) is specialized for `byte`, `char`, `int`, `long`,
`double`, `boolean`, `Unit`, and static `String`; affine nominal elements instead
expose `getRef`/`getMut`. `tryGet`, `tryPop`, and `tryRemove` return inline options;
invalid indices and empty extraction return `None` without mutation. Strict `get`,
`set`, `pop`, `remove`, and reference access diagnose invalid indices (or an empty
`pop`) fatally. `set` replaces and destroys the old owned value. `reserve` with a
negative capacity is fatal.

Default `sort`/`binarySearch` ordering is provided for `byte`, `char`, `int`,
`long`, `double`, `boolean`, and static `String`. Default string ordering is
lexicographic by UTF-8 bytes. `binarySearch` expects the list to be sorted using the
same ordering. `sortBy` and `binarySearchBy` use a user-supplied
`Comparator<T>`; the binary search also expects a matching pre-sorted list. A
comparator returns a negative, zero, or positive `int` for its pair of values.

List traversal uses `for (int value : values)` for copied values and
`for (borrow T value : values)` / `for (borrow mut T value : values)` for affine
values. The list is evaluated once and stays loaned for the full loop; each
binding ends at the iteration boundary. This is built-in traversal, not a
first-class iterator or an early-release/NLL feature.

### `Deque<T>`

```java
Deque<T>()
void pushFront(T value)
void pushBack(T value)
T popFront()
T popBack()
Option<T> tryPopFront()
Option<T> tryPopBack()
readonly int size()
readonly int capacity()
readonly boolean isEmpty()
readonly T get(int index)                         // copied specializations only
readonly Option<T> tryGet(int index)              // copied specializations only
readonly borrow T getRef(int index)                // affine specializations only
borrow mut T getMut(int index)                     // affine specializations only
void set(int index, T value)
void clear()
void reserve(int capacity)
```

The deque is an owning double-ended collection. Pushes transfer affine elements;
strict pops from an empty deque and invalid indexed operations are fatal. Recoverable
pops return `None` when empty. Copied versus borrowed indexed access follows the
same specialization rules as `List<T>`. Negative reserve is fatal.

### `Array<T>` and `Arrays`

```java
Array<T>(List<T> values)                           // consumes the list
readonly int size()
readonly T get(int index)                         // copied specializations only
readonly Option<T> tryGet(int index)              // copied specializations only
readonly borrow T getRef(int index)                // affine specializations only
borrow mut T getMut(int index)                     // affine specializations only
void set(int index, T value)

static Array<byte> zeroedBytes(int length)
static Array<char> zeroedChars(int length)
static Array<int> zeroedInts(int length)
static Array<long> zeroedLongs(int length)
static Array<double> zeroedDoubles(int length)
static Array<boolean> zeroedBooleans(int length)
static Array<Unit> zeroedUnits(int length)
```

`Array<T>` is a fixed-length affine owner, not `T[]` syntax. Constructing it from a
`List<T>` consumes the list and transfers its initialized elements; `set` replaces
one element but cannot resize the array. The `Arrays` factories initialize every
slot to the corresponding zero value. A negative factory length is fatal. Strict
indexed operations diagnose out-of-range indices; `tryGet` is available for copied
specializations and returns `None` when out of range. Affine arrays expose only
borrowed indexed access, not `Option<borrow T>`.

### `Set<T>` and `Map<K, V>`

The only supported set keys and map keys are `int` and owned `Text`. No static
`String`, `Path`, or user-defined hash/equality type is supported. `Text` key
comparison is exact UTF-8 byte equality, including embedded NUL; there is no
normalization or case folding.

```java
Set<T>()
void add(T key)
readonly boolean contains(T key)
boolean remove(T key)
readonly int size()
readonly boolean isEmpty()
void clear()

Map<K, V>()
readonly boolean containsKey(K key)
readonly int size()
readonly boolean isEmpty()
Option<V> put(K key, V value)
Option<V> tryRemove(K key)
readonly Option<V> tryGet(K key)                   // copied value specializations only
readonly borrow V getRef(K key)                    // affine value specializations only
borrow mut V getMut(K key)                          // affine value specializations only
void clear()
```

For `Map<int, V>`/`Set<int>`, integer queries copy. For `Map<Text, V>`/`Set<Text>`,
`put`/`add` take an owned `Text` key, while lookups and removals take a shared
`borrow Text` query; the query is not consumed or stored. Equal-key replacement
retains the existing map key and returns the replaced value in `Some`. `put`
returns `None` for a new key; `tryRemove` transfers a removed value in `Some`, or
returns `None` when absent. Insertion or growth can allocate; allocation failure
is fatal. Map and set iteration order is unspecified.

`tryGet` copies only copied value types; affine values require `getRef`/`getMut`.
Absent borrowed lookup is a fatal boundary error. `Map` traversal uses two bindings,
for example `for (int key, borrow Value value : map)` or
`for (borrow Text key, int value : map)`. Text map keys are readonly borrowed
bindings. Set traversal accepts one readonly key binding. All these loops hold a
whole-root loan for their complete lexical extent; different entries do not grant
disjoint mutable access. See [MAP_TRAVERSAL.md](MAP_TRAVERSAL.md) for the base
map traversal contract.

## Console and command-line arguments

```java
static Result<Option<Text>, Text> Console.readLine()
static void Console.print(String value)
static void Console.printText(borrow Text value)
static int Args.count()
static Result<Text, Text> Args.get(int index)
```

`readLine` returns `Ok(Some(line))`, strips LF and a preceding CR, and returns a
final unterminated line before EOF. An empty line is `Some(empty Text)`; EOF with no
remaining bytes is `None`. Input errors, excessive line length, and invalid UTF-8
are `Err`. `Console.print`/`printText` write without a newline; `Text.print` does
append one. `Args.count` excludes `argv[0]`; `Args.get` returns an owned UTF-8
copy and reports an invalid index or non-UTF-8 argument as `Err`.

## Paths and filesystem

`Path`, `File`, `Directory`, `FileInfo`, and `ByteBuffer` are opaque native owners;
resource handles have no public constructors. Factories return fully initialized
owners or a `Result` error. Explicit `close()` reports errors and invalidates the
handle; ordinary scope cleanup best-effort closes an open descriptor. Calls on a
closed resource return `Err`.

### `Path`, `File`, and `Directory`

```java
static Result<Path, Text> Path.fromText(Text path)
readonly Path Path.clone()
static Result<Path, Text> Path.join(borrow Path base, borrow Path child)
readonly boolean Path.isAbsolute()
readonly borrow Text Path.asText()

static Result<File, Text> File.openRead(borrow Path path)
static Result<File, Text> File.create(borrow Path path, int mode)
static Result<File, Text> File.openAppend(borrow Path path, int mode)
Result<long, Text> File.readSome(borrow mut ByteBuffer destination, long maxBytes)
Result<long, Text> File.writeSome(borrow ByteBuffer source, long offset, long maxBytes)
Result<Unit, Text> File.writeAll(borrow ByteBuffer source)
Result<Unit, Text> File.flush()
Result<Unit, Text> File.close()

static Result<Directory, Text> Directory.open(borrow Path path)
Result<Option<Text>, Text> Directory.nextName()
Result<Unit, Text> Directory.close()
```

`Path.fromText` consumes `Text`, rejects an empty path or embedded NUL, and takes
its storage. `Path.clone` is an explicit independent copy. `join` rejects an
absolute child and joins components without resolving symlinks, canonicalizing,
or normalizing `.`/`..`. `asText` is a shared borrow. Paths are UTF-8 boundary
strings; embedded NUL is rejected.

`File.create` and `openAppend` use POSIX permission modes subject to the process
umask. `readSome` appends at most `maxBytes`, returns the count, `0` at EOF, or
`Err`; a zero maximum performs no read. `writeSome` writes at most the selected
`[offset, offset + maxBytes)` source range and returns one partial count. Invalid
ranges and negative limits are `Err`; zero maximum returns zero. `writeAll` retries
partial writes until complete or `Err`; an error can follow a written prefix and
does not undo it. `flush` uses `fsync`; it does not promise hardware durability
beyond the OS contract. File I/O is synchronous and blocking. Drop closes but does
not implicitly flush.

`Directory.nextName` skips `.` and `..`, returns one owned UTF-8 entry component
per `Some`, `None` at end, and `Err` for I/O or invalid UTF-8. It does not return a
full path, sort entries, or recurse. Join the name to the original path when needed.

### `FileInfo` and `Files`

```java
readonly long FileInfo.size()
readonly boolean FileInfo.isFile()
readonly boolean FileInfo.isDirectory()
readonly boolean FileInfo.isSymlink()

static Result<Text, Text> Files.readText(borrow Text path)
static Result<Unit, Text> Files.writeText(borrow Text path, borrow Text content)
static Result<Unit, Text> Files.writeTextAtomic(borrow Text path, borrow Text content)
static Result<FileInfo, Text> Files.metadata(borrow Path path)
static Result<Unit, Text> Files.createDirectory(borrow Path path, int mode)
static Result<Unit, Text> Files.removeFile(borrow Path path)
static Result<Unit, Text> Files.removeDirectory(borrow Path path)
static Result<Unit, Text> Files.rename(borrow Path from, borrow Path to)
```

`readText` reads the complete file, preserves embedded NUL, and rejects invalid
UTF-8 or open/read/close/size errors. `writeText` writes exact UTF-8 bytes and
returns `Err` on failure; a failed write may leave a truncated or partial file.
Neither `Err` nor another operation rolls back an already written prefix.
`writeTextAtomic` writes a sibling temporary file and commits with `rename`;
success gives atomic visibility, not power-loss durability (`fsync` is not
promised). Before commit, failure preserves the destination and attempts to remove
the temporary file. The replacement uses temporary-file permissions, subject to
the umask, and replacing a symlink replaces its directory entry.

`metadata` returns a `FileInfo` snapshot; `size()` is in bytes. Filesystem paths
are passed through the operating-system boundary and reject embedded NUL. `mode`
arguments are POSIX permission bits, subject to umask. Filesystem commands report
success as `Result<Unit, Text>`, not a boolean.

## POSIX regular expressions

```java
static Result<Regex, Text> Regex.compile(borrow Text pattern)
readonly Result<boolean, Text> Regex.matches(borrow Text subject)
readonly Result<Option<RegexMatch>, Text> Regex.search(borrow Text subject)
readonly int RegexMatch.groupCount()
readonly boolean RegexMatch.hasGroup(int index)
readonly borrow Text RegexMatch.group(int index)
readonly int RegexMatch.start(int index)
readonly int RegexMatch.end(int index)
```

`Regex` uses Linux POSIX extended regular expressions (`REG_EXTENDED`), not Java
regex syntax. Compilation and matching use the `C.UTF-8` locale; compilation returns
`Err` if it is unavailable, with no fallback. `matches` requires a full-subject
match. `search` returns the first POSIX leftmost-longest match or `Ok(None)`;
`RegexMatch` owns copies of its captures, so they do not borrow the input subject.
Group 0 is the complete match, and `groupCount` includes it. `start` and `end` are
half-open UTF-8 byte offsets. `hasGroup` is false when a capture did not
participate; `group`, `start`, and `end` on an absent or out-of-range group are
fatal runtime boundary errors.

POSIX regex APIs consume NUL-terminated C strings. `Regex.compile`, `matches`, and
`search` reject a pattern or subject containing embedded NUL with `Err` rather
than truncating it. Regex is for text, not arbitrary binary `ByteBuffer` contents.

## Math, random numbers, and time

### `Math`

```java
static Result<int, Text> absInt(int value)
static Result<long, Text> absLong(long value)
static double absDouble(double value)
static int minInt(int left, int right)
static long minLong(long left, long right)
static double minDouble(double left, double right)
static int maxInt(int left, int right)
static long maxLong(long left, long right)
static double maxDouble(double left, double right)
static double sqrt(double value)
static double pow(double base, double exponent)
static double log(double value)
static double exp(double value)
static double sin(double value)
static double cos(double value)
static double tan(double value)
static double floor(double value)
static double ceil(double value)
static double round(double value)
static double hypot(double x, double y)
static boolean isNaN(double value)
static boolean isFinite(double value)
static double cbrt(double value)
static double log10(double value)
static double log1p(double value)
static double expm1(double value)
static double asin(double value)
static double acos(double value)
static double atan(double value)
static double atan2(double y, double x)
static double sinh(double value)
static double cosh(double value)
static double tanh(double value)
static double toRadians(double degrees)
static double toDegrees(double radians)
static double signum(double value)
static double copySign(double magnitude, double sign)
static double nextUp(double value)
static double nextDown(double value)
static double ulp(double value)
static int getExponent(double value)
static double scalb(double value, int scaleFactor)
static double ieeeRemainder(double dividend, double divisor)
static Result<int, Text> addExactInt(int left, int right)
static Result<long, Text> addExactLong(long left, long right)
static Result<int, Text> subtractExactInt(int left, int right)
static Result<long, Text> subtractExactLong(long left, long right)
static Result<int, Text> multiplyExactInt(int left, int right)
static Result<long, Text> multiplyExactLong(long left, long right)
static Result<int, Text> negateExactInt(int value)
static Result<long, Text> negateExactLong(long value)
static Result<int, Text> incrementExactInt(int value)
static Result<long, Text> incrementExactLong(long value)
static Result<int, Text> decrementExactInt(int value)
static Result<long, Text> decrementExactLong(long value)
static Result<int, Text> floorDivInt(int dividend, int divisor)
static Result<long, Text> floorDivLong(long dividend, long divisor)
static Result<int, Text> floorModInt(int dividend, int divisor)
static Result<long, Text> floorModLong(long dividend, long divisor)
```

`absInt` and `absLong` return `Err` for the respective signed minimum, whose
positive absolute value is not representable. The `*ExactInt` and `*ExactLong`
operations return `Err` when the mathematical result is outside that width;
successful scalar operations do not allocate. `floorDivInt`/`floorDivLong` use
mathematical floor rather than C's truncation toward zero.
`floorModInt`/`floorModLong` satisfy `a = floorDiv(a, b) * b + floorMod(a, b)`;
for nonzero remainders the result has the divisor's sign. Both floor operations
return `Err` for a zero divisor and for `MIN_VALUE / -1`, avoiding C undefined
behavior and reporting an unrepresentable quotient.

Floating operations use the C11 math library and retain its documented IEEE
domain/range results, including NaN and infinity rather than converting every
domain case into `Err`. `round` uses ties away from zero. `minDouble`/`maxDouble`
ignore one NaN and return NaN for two NaNs. For zero ties, min chooses negative
zero if either operand is negative zero; max chooses positive zero if either is
positive zero. `signum` preserves NaN and signed zero; `copySign` copies the
second operand's sign. `nextUp`/`nextDown` step to the adjacent representable
double and keep NaN or the infinity at the respective outward endpoint.
`ulp` returns a positive spacing (the smallest subnormal for zero/subnormal
inputs, positive infinity for either infinity, and NaN for NaN). `getExponent`
returns the normal binary exponent, `-1023` for zero or subnormals, and `1024`
for infinities or NaN. `scalb` scales by a power of two with libm overflow and
underflow behavior. `ieeeRemainder` uses the nearest-integer quotient with ties
to even. `toRadians` and `toDegrees` use fixed approximate conversion factors.

### `Random`

```java
Random(long seed)
static Result<Random, Text> fromSystem()
void reseed(long seed)
long nextLong()
Result<int, Text> nextInt(int bound)
double nextDouble()
```

A seeded generator is deterministic (SplitMix64): the same seed and call
sequence reproduce the same outputs. `nextInt` requires a positive exclusive
bound and returns `Err` otherwise. `nextDouble` is in `[0.0, 1.0)`. `fromSystem`
obtains an initial seed from `/dev/urandom`; failure is `Err`. The generator is
non-cryptographic, including when initialized from system entropy; do not use it
for keys, tokens, nonces, or other security-sensitive randomness.

### `Time`

```java
static Result<long, Text> monotonicNanos()
static Result<long, Text> unixMillis()
static Result<Unit, Text> sleepMillis(long milliseconds)
static Result<long, Text> deadlineAfterMillis(long timeoutMillis)
```

`monotonicNanos` returns nanoseconds from the monotonic clock's unspecified
origin; compare values from that same clock domain, not with wall time.
`unixMillis` returns milliseconds since the Unix epoch from the realtime clock and
can move when wall time is adjusted. `sleepMillis` blocks for a nonnegative
interval and resumes after interruption where possible; a negative duration is
`Err`. `deadlineAfterMillis` returns an absolute deadline in the monotonic
nanosecond domain and returns `Err` for a negative timeout, clock failure, or
unrepresentable deadline. The API does not promise exact scheduling precision.

## Threads, mutexes, guards, and channels

Native concurrency is synchronous POSIX threading and blocking synchronization,
not an executor, detached-thread API, or callback framework. `Thread<T>.spawn`
transfers a user-implemented `Task<T>` and its owned data to one worker thread.
The concrete task object and its result must be sendable: the recursive checker
rejects borrowed/view-bearing state and `Guard<T>`. Scalars, immutable static
`String`, and owned values whose fields/payloads recursively meet the rule can
cross threads; `Mutex<T>` and channel endpoints are the explicitly shareable
control handles when their payload type is sendable.

```java
interface Task<T> { T run(); }
static Result<Thread<T>, Text> Thread.spawn(Task<T> task)
Result<T, Text> Thread.join()

static Result<Mutex<T>, Text> Mutex.create(T initial)
Result<Mutex<T>, Text> Mutex.cloneHandle()
Result<Guard<T>, Text> Mutex.lock()
Result<Option<Guard<T>>, Text> Mutex.tryLock()

readonly borrow T Guard.getRef()                  // affine object specializations
borrow mut T Guard.getMut()                       // affine object specializations
readonly T Guard.read()                           // copied-value specializations
Result<Unit, Text> Guard.write(T value)            // copied-value specializations

static Result<Channel<T>, Text> Channel.bounded(long capacity)
Result<Sender<T>, Text> Channel.takeSender()
Result<Receiver<T>, Text> Channel.takeReceiver()
Result<Unit, SendError<T>> Sender.send(T value)
Result<Sender<T>, Text> Sender.cloneSender()
Result<Unit, Text> Sender.close()
Result<Option<T>, Text> Receiver.receive()
Result<Unit, Text> Receiver.close()

enum SendError<T> { Closed(T); System(Text, T); }
```

`join` waits for the worker and moves its single result back; a second join is an
error. Dropping an unjoined `Thread` synchronously joins it and drops its result;
that drop can block forever if the task does. Threads are never detached.

`Mutex<T>` owns one protected value. `cloneHandle()` explicitly shares only its
control block; it does not clone `T`. `lock` blocks until available; `tryLock`
returns `Ok(None)` only when currently busy. A `Guard<T>` owns the active lock and
unlocks on drop. For affine object values, `getRef`/`getMut` are the only ways to
access the protected object; their receiver-rooted borrows prevent moving or
dropping the guard while a borrow is live. For copied scalar values and immutable
`String`, the specialization exposes value `read`/`write` instead. `Guard` has no
public constructor or clone and cannot cross threads. Recursive same-thread
locking is reported as an error; `lock` can otherwise block indefinitely.

`Channel.bounded` requires capacity at least one; zero-capacity rendezvous is not
supported. It creates one sender and one receiver, each taken at most once. A
sender blocks while a full queue has no space. A closed receiver makes `send`
return the original value inside `SendError.Closed`; a synchronization failure
returns it inside `SendError.System`. `receive` blocks until an item arrives or
all senders close; it drains queued values and then returns `Ok(None)`. Closing a
receiver aborts the queue and drops queued values. More producers require an
explicit `cloneSender`; receivers cannot be cloned.

Only the named synchronization controls use explicit shared lifetime counts.
`cloneHandle` and `cloneSender` keep their control blocks alive; they do not clone
ordinary objects or channel payloads. Cycles made by storing cloned mutex handles
or channel sender handles inside shared payloads can retain those control blocks
indefinitely because Nacre has no cycle collector. `lock`, full-channel `send`,
empty-channel `receive`, thread `join`, and implicit drop of an unjoined thread can
block. Locking multiple mutexes in inconsistent orders can deadlock. Avoid cycles
and establish a consistent lock/wait order; drop guards before blocking on work
that may need the same mutex.

## Environment and child processes

### `Env`, `Command`, and `Process`

```java
static Result<Option<Text>, Text> Env.get(borrow Text name)
static Result<Unit, Text> Env.set(borrow Text name, borrow Text value)
static Result<Unit, Text> Env.unset(borrow Text name)

static Command Command.create(Path executable)
Result<Unit, Text> Command.addArg(Text argument)
Result<Unit, Text> Command.setEnv(Text name, Text value)
Result<Unit, Text> Command.redirectStdin(File file)
Result<Unit, Text> Command.redirectStdout(File file)
Result<Unit, Text> Command.redirectStderr(File file)
Result<Unit, Text> Command.clearEnv()
readonly Result<Process, Text> Command.spawn()
readonly Result<ProcessOutput, Text> Command.output(long maxBytesPerStream, long timeoutMs)

Result<ExitStatus, Text> Process.wait()
Result<Option<ExitStatus>, Text> Process.waitFor(long timeoutMs)
Result<Unit, Text> Process.terminate()
Result<Unit, Text> Process.kill()

enum ExitStatus { Exited(long code); Signaled(int signal); }
readonly borrow ByteBuffer ProcessOutput.stdout()
readonly borrow ByteBuffer ProcessOutput.stderr()
readonly borrow ExitStatus ProcessOutput.status()
readonly boolean ProcessOutput.stdoutTruncated()
readonly boolean ProcessOutput.stderrTruncated()
readonly boolean ProcessOutput.timedOut()
```

`Env` operates on the process environment. Names must be nonempty and contain
neither NUL nor `=`; values cannot contain NUL. `get` returns an owned UTF-8 value,
`None` only when absent, and `Err` for invalid UTF-8. Environment operations are
synchronized. `set`/`unset` affect later global reads and process snapshots; a
`Command` environment override is local to its child.

`Command.create` consumes its executable `Path`. Each `addArg` adds exactly one
literal argument; Nacre does not tokenize, concatenate, interpolate, or run a
shell command. Added arguments follow the executable path in `argv`. Before each
spawn, inherited environment and command overrides are snapshotted consistently.
`clearEnv` starts the child with an empty environment before explicit overrides.
An executable containing `/` is used as given; otherwise executable lookup uses
`PATH` from that child environment snapshot (the POSIX default search path is used
when `PATH` is absent, and empty components mean the current directory). No shell
fallback is used for an unrecognized executable format. Embedded NUL in
executable, arguments, environment names, or values is rejected.

Without redirects, `spawn` inherits the corresponding standard descriptors. A
redirect consumes an open `File`; replacing a redirect closes the previous owner,
and the `Command` retains its configured file for repeated spawns. Using a closed
redirect is `Err`. `output` rejects a command with configured redirects rather
than silently overriding them.

`wait` blocks and reaps once; `waitFor` with a negative timeout is `Err`, zero
waits without a deadline, and a positive timeout is one monotonic millisecond
deadline. `Ok(None)` means timed out without reaping; it leaves the process
available for another wait. `terminate` sends SIGTERM and `kill` sends SIGKILL to
the child only; call `wait`/`waitFor` to reap explicitly. A nonzero exit is
`Ok(ExitStatus.Exited(code))`; signal termination is
`Ok(ExitStatus.Signaled(signal))`. Dropping an unreaped `Process` sends SIGKILL
and synchronously reaps it. Drop can block. It does not kill a process tree and
there is no process-tree guarantee for descendants.

`Command.output(maxBytesPerStream, timeoutMs)` redirects child stdin from
`/dev/null`, captures stdout and stderr independently, and drains both concurrently
so one full pipe does not block the other. It stores at most the specified
nonnegative byte limit **per stream**, continues draining excess bytes, and marks
truncation. Capture is binary and preserves NUL bytes. A zero timeout is unbounded;
a positive timeout is one monotonic end-to-end deadline, and a negative timeout or
negative capture limit is `Err`. On timeout, it kills and reaps the child and
returns `Ok(ProcessOutput)` with `timedOut() == true`, the status, and any captured
prefix. The capture path closes its pipe readers after collecting any immediately
available prefix rather than waiting forever for descendant-held writers. A
capture timeout affects the direct child only; descendants are not guaranteed to
stop. Spawn/poll/read failures return `Err` after cleanup of a started child and
its pipes. A child that exits nonzero still produces a successful capture result.

## TCP and UDP

All socket operations are synchronous and blocking at the API level, with
nonblocking readiness handling underneath where needed. Socket handles are unique
affine owners; there is no descriptor clone, background I/O thread, TLS, or
cross-platform networking promise.

```java
static Result<List<SocketAddress>, Text> SocketAddress.resolve(
    borrow Text host, int port, boolean passive)
readonly Text SocketAddress.toText()

static Result<TcpListener, Text> TcpListener.bind(borrow SocketAddress address, int backlog)
Result<TcpStream, Text> TcpListener.accept(long timeoutMs)
Result<SocketAddress, Text> TcpListener.localAddress()
Result<Unit, Text> TcpListener.close()

static Result<TcpStream, Text> TcpStream.connect(borrow SocketAddress address, long timeoutMs)
Result<long, Text> TcpStream.readSome(borrow mut ByteBuffer destination, long maxBytes, long timeoutMs)
Result<long, Text> TcpStream.writeSome(borrow ByteBuffer source, long offset, long maxBytes, long timeoutMs)
Result<Unit, Text> TcpStream.writeAll(borrow ByteBuffer source, long timeoutMs)
Result<SocketAddress, Text> TcpStream.peerAddress()
Result<SocketAddress, Text> TcpStream.localAddress()
Result<Unit, Text> TcpStream.close()

readonly borrow SocketAddress Datagram.source()
readonly long Datagram.bytesRead()
readonly boolean Datagram.truncated()

static Result<UdpSocket, Text> UdpSocket.bind(borrow SocketAddress address)
Result<long, Text> UdpSocket.sendTo(borrow ByteBuffer source, long offset, long length,
    borrow SocketAddress destination, long timeoutMs)
Result<Datagram, Text> UdpSocket.receiveFrom(borrow mut ByteBuffer destination,
    long maxBytes, long timeoutMs)
Result<SocketAddress, Text> UdpSocket.localAddress()
Result<Unit, Text> UdpSocket.close()
```

`resolve` handles IPv4/IPv6 addresses and copies results into owned
`SocketAddress` values. The port must be in `[0, 65535]`; an empty host is valid
only for `passive == true` and denotes a wildcard address. `toText` returns a
numeric address without another DNS lookup. Name resolution is synchronous and
has no timeout; socket timeouts begin only after resolution completes.

Listener `bind` requires a positive backlog. `localAddress` reports an ephemeral
port selected by the OS. TCP `connect` and listener `accept` accept zero for an
unbounded wait, a positive monotonic millisecond timeout, and return `Err` for a
negative timeout. A timeout closes an in-progress connect descriptor. TCP reads
append to the existing destination and return a positive count, `0` for orderly
EOF, or `Err` (including timeout); a zero maximum returns zero immediately.
`writeSome` returns one partial count. `writeAll` retries partial progress against
one end-to-end deadline; `Err` can follow bytes already sent. A peer close reports
an error rather than raising process-wide SIGPIPE.

`UdpSocket.sendTo` sends one datagram; it does not retry or split a message and a
short send is an error. `receiveFrom` consumes one datagram, appends at most
`maxBytes`, and reports the number of bytes copied, its owned source address, and
whether it was truncated. A zero-length datagram is a successful `Datagram` with
`bytesRead() == 0`, not stream EOF. Oversized datagrams are consumed once and
reported as truncated. Both UDP methods use the same timeout convention as TCP;
timeout is `Err`. No TLS, automatic datagram retry, or network service discovery
is provided.

## Platform, allocation, and timing limits

This native systems surface targets Linux and POSIX facilities. It is not a
Windows or macOS support claim. OS-facing paths, environment, argv, host, and
other C boundary strings reject embedded NUL; `Text` itself can contain NUL.
File and socket I/O, process waits, mutex locks, channel send/receive, thread joins,
and sleeps are synchronous. A documented zero timeout means unbounded waiting;
positive relative timeouts use monotonic deadlines. DNS resolution has no timeout.
No API promises that a deadline interrupts arbitrary work outside the timed OS
operation, and no general latency, throughput, or allocation-count claim is made.

Native templates and runtime primitives are trusted code: source-level ownership
checking does not prove their C implementation. The compiler has no exception
unwinding or general sandbox. See [ARCHITECTURE.md](ARCHITECTURE.md) for the
compiler/native boundary and [VALIDATION.md](VALIDATION.md) for recorded
validation evidence.
