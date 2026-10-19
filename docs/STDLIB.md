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

Static-only prelude utility classes can be called by class name but cannot be
used as value types. Factory-returned native owners remain usable affine values,
including factory-only `TlsConfig` and values returned inside `Result` or `Option`.
Native values with instance methods, such as `RegexMatch` and `CryptoDigest`,
remain values as well.

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

Shift operators `<<` and `>>` require an integer left operand and a `byte`,
`char`, or `int` count. A `byte` or `char` left operand is promoted to `int`;
`int` shifts use 32 bits and `long` shifts use 64 bits. Counts must be
nonnegative and less than the operand width, or execution terminates with
`[E6104]`. Left shifts discard bits beyond that width; right shifts are
arithmetic and preserve the sign bit.

A character literal contains exactly one Unicode scalar, for example `'λ'` or
`'\u{1f9ed}'`. Character literals support `\n`, `\r`, `\t`, `\0`, `\"`, `\'`,
`\\`, and `\u{...}` escapes. String literals support `\n`, `\r`, `\t`, `\0`,
`\"`, `\'`, and `\\`, but not Unicode escapes. Failed `assert` messages preserve
embedded NUL bytes when printed. Numeric expressions are promoted
to a common numeric type; a wider numeric value can be used where a narrower type
is not required. Explicit numeric casts use `(type)value`. A checked cast that is
out of range, non-finite where disallowed, or not a Unicode scalar terminates
with a runtime diagnostic rather than returning `Result`.

`Text.appendByte` formats the unsigned byte as decimal text; it does not append a raw byte. Use `ByteBuffer` for binary bytes.

## Exact big numbers

`BigInt` and `BigDecimal` are affine native objects backed by GMP integers.
Operations never convert through `double`; operands passed as `borrow` remain
usable, and every returned object is a separately owned value. Use `clone()` only
when an explicit independent copy is needed.
Either numeric type can be used independently; referencing one does not require
constructing or referencing the other. Native emission selects its template
sections in one pass from the specialized classes.

```java
static BigInt BigInt.fromLong(long value)
static Result<BigInt, Text> BigInt.parse(borrow Text value, int radix)
readonly BigInt clone()
readonly Result<Text, Text> toText(int radix)
readonly int compareTo(borrow BigInt other)
readonly int signum()
readonly Result<int, Text> bitLength()
readonly BigInt add(borrow BigInt other)
readonly BigInt subtract(borrow BigInt other)
readonly BigInt multiply(borrow BigInt other)
readonly Result<BigInt, Text> divide(borrow BigInt other)
readonly Result<BigInt, Text> remainder(borrow BigInt other)
readonly BigInt gcd(borrow BigInt other)
readonly Result<BigInt, Text> mod(borrow BigInt positiveModulus)
readonly Result<BigInt, Text> pow(int nonnegativeExponent)
readonly Result<BigInt, Text> modPow(borrow BigInt nonnegativeExponent,
    borrow BigInt positiveModulus)
readonly Result<BigInt, Text> modInverse(borrow BigInt positiveModulus)
readonly BigInt negate()
readonly BigInt abs()
readonly Result<BigInt, Text> shiftLeft(int nonnegativeBits)
readonly Result<BigInt, Text> shiftRight(int nonnegativeBits)
readonly BigInt bitAnd(borrow BigInt other)
readonly BigInt bitOr(borrow BigInt other)
readonly BigInt bitXor(borrow BigInt other)
readonly BigInt bitNot()
readonly Result<boolean, Text> testBit(int nonnegativeBit)
readonly Result<long, Text> toLong()
```

`BigInt.parse` accepts an optional ASCII `+` or `-` sign and digits for radix 2 through 36;
leading zeroes do not consume the coefficient bit limit. It rejects whitespace, NUL,
prefixes, and digits outside the radix. `toText` formats the value in the requested
radix. Division truncates toward zero and remainder has the dividend's sign. `mod`
requires a positive modulus and returns the canonical nonnegative residue. `pow`
and `modPow` reject negative exponents;
modular operations require a positive modulus. `modInverse` returns `Err` when an
inverse does not exist. Shifts are arithmetic for negative values; bit operations
use GMP's infinite two's-complement convention. `bitLength` follows signed
two's-complement magnitude semantics (`-1` has length zero). `toLong` is exact and
returns `Err` outside the signed 64-bit range.

```java
enum RoundingMode { Down; Up; Floor; Ceiling; HalfUp; HalfDown; HalfEven; Unnecessary; }
static Result<BigDecimal, Text> BigDecimal.parse(borrow Text value)
static BigDecimal BigDecimal.fromLong(long value)
readonly BigDecimal clone()
readonly Result<Text, Text> toText()
readonly int scale()
readonly int compareTo(borrow BigDecimal other)
readonly int signum()
readonly Result<BigDecimal, Text> add(borrow BigDecimal other)
readonly Result<BigDecimal, Text> subtract(borrow BigDecimal other)
readonly Result<BigDecimal, Text> multiply(borrow BigDecimal other)
readonly Result<BigDecimal, Text> divide(borrow BigDecimal other, int resultScale,
    RoundingMode rounding)
readonly Result<BigDecimal, Text> rescale(int scale, RoundingMode rounding)
readonly BigDecimal negate()
readonly BigDecimal abs()
readonly Result<long, Text> toLong()
```

`BigDecimal` stores an exact signed coefficient and decimal scale: its value is
`coefficient × 10^-scale`. Parsing accepts a signed decimal significand and optional
decimal exponent, but no whitespace, NUL, `NaN`, or infinity. `toText` emits a
plain decimal string while preserving the stored scale where possible; it does
not use exponent notation. Addition and subtraction align to the greater operand
scale, and multiplication adds scales. Division and rescaling round the exact
quotient: Down truncates toward zero, Up away from zero, Floor toward negative
infinity, Ceiling toward positive infinity, HalfUp ties away from zero, HalfDown
ties toward zero, HalfEven ties to an even retained digit, and Unnecessary returns
`Err` for an inexact value. Division by zero, inexact `toLong`, and out-of-range
long conversion return `Err`.

The native numeric limit is 1,000,000 coefficient bits and 1,000,000 input/output
bytes; `BigDecimal` scales are limited to ±100,000. Parsing checks significant
coefficient digits before GMP conversion and checks boundary values exactly.
Decimal scale arithmetic and fallible numeric operations return `Err` when these
limits would be exceeded. Plain-returning `BigInt` growth operations report a fatal
runtime diagnostic rather than truncating a result that crosses the bit limit.
GMP's default allocator remains in use; native object headers and temporary byte
buffers use the runtime allocator and are cleaned up explicitly.

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

## Unicode properties and text operations

`Text` remains dependency-free UTF-8 storage and does not perform implicit
normalization or case conversion. ICU-backed whole-text operations are explicit
static calls on `Unicode`; programs using them require ICU.

```java
static Result<Text, Text> toUpperCase(borrow Text text, borrow Text locale)
static Result<Text, Text> toLowerCase(borrow Text text, borrow Text locale)
static Result<Text, Text> caseFold(borrow Text text)
static Result<Text, Text> normalize(borrow Text text, borrow Text form)
static Result<int, Text> graphemeCount(borrow Text text)
static Result<Text, Text> graphemeAt(borrow Text text, int index)
static Result<int, Text> compareLocale(borrow Text left, borrow Text right, borrow Text locale)
```

Text transformations preserve embedded NUL and return newly owned `Text` values.
`toUpperCase` and `toLowerCase` use full Unicode string mappings, including
expanding mappings such as German `ß` to `SS`. `caseFold` uses default full Unicode
case folding. `normalize` accepts exactly `NFC`, `NFD`, `NFKC`, or `NFKD`.
Grapheme counts and indexes use ICU character-break boundaries (extended
grapheme clusters), not UTF-8 bytes or scalar counts; indexes are zero-based and
out-of-range indexes return `Err`. `compareLocale` returns -1, 0, or 1 from ICU
collation.

Locale arguments accept canonical BCP-47 tags supported by ICU. The empty locale
selects ICU root explicitly. Malformed, noncanonical, or unavailable locales
return `Err`; they never use the process default or silently fall back. Unicode
extension keywords are preserved for ICU services: `u-co-...` selects collation
and `u-nu-...` selects the numbering system. Unsupported values return `Err`.

`Character` exposes ICU Unicode scalar properties and simple, non-expanding
single-character mappings:

```java
static boolean isLetter(char value)
static boolean isDigit(char value)
static boolean isWhitespace(char value)
static boolean isUpperCase(char value)
static boolean isLowerCase(char value)
static char toUpperCase(char value)
static char toLowerCase(char value)
static int category(char value)
static double numericValue(char value)
```

`isLetter` tests Unicode letter categories; `isDigit` tests decimal digits;
`isWhitespace` tests Unicode `White_Space`. The `toUpperCase` and `toLowerCase`
methods map one scalar only and do not expand text. `numericValue` returns the
Unicode numeric property (including fractions), or -1 when absent. `category`
returns ICU `UCharCategory` values:

| Value | ICU category | Unicode general category |
|---:|---|---|
| 0 | `U_UNASSIGNED` | Cn |
| 1 | `U_UPPERCASE_LETTER` | Lu |
| 2 | `U_LOWERCASE_LETTER` | Ll |
| 3 | `U_TITLECASE_LETTER` | Lt |
| 4 | `U_MODIFIER_LETTER` | Lm |
| 5 | `U_OTHER_LETTER` | Lo |
| 6 | `U_NON_SPACING_MARK` | Mn |
| 7 | `U_ENCLOSING_MARK` | Me |
| 8 | `U_COMBINING_SPACING_MARK` | Mc |
| 9 | `U_DECIMAL_DIGIT_NUMBER` | Nd |
| 10 | `U_LETTER_NUMBER` | Nl |
| 11 | `U_OTHER_NUMBER` | No |
| 12 | `U_SPACE_SEPARATOR` | Zs |
| 13 | `U_LINE_SEPARATOR` | Zl |
| 14 | `U_PARAGRAPH_SEPARATOR` | Zp |
| 15 | `U_CONTROL_CHAR` | Cc |
| 16 | `U_FORMAT_CHAR` | Cf |
| 17 | `U_PRIVATE_USE_CHAR` | Co |
| 18 | `U_SURROGATE` | Cs |
| 19 | `U_DASH_PUNCTUATION` | Pd |
| 20 | `U_START_PUNCTUATION` | Ps |
| 21 | `U_END_PUNCTUATION` | Pe |
| 22 | `U_CONNECTOR_PUNCTUATION` | Pc |
| 23 | `U_OTHER_PUNCTUATION` | Po |
| 24 | `U_MATH_SYMBOL` | Sm |
| 25 | `U_CURRENCY_SYMBOL` | Sc |
| 26 | `U_MODIFIER_SYMBOL` | Sk |
| 27 | `U_OTHER_SYMBOL` | So |
| 28 | `U_INITIAL_PUNCTUATION` | Pi |
| 29 | `U_FINAL_PUNCTUATION` | Pf |


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

## `Uuid`

`Uuid` is an affine owner of one 16-byte UUID value. Factory and conversion
methods return unique owners; no implicit clone, shared ownership, or collection
key support is provided.

```java
static Result<Uuid, Text> parse(borrow Text text)
static Result<Uuid, Text> fromBytes(borrow ByteBuffer bytes)
static Uuid fromLongs(long most, long least)
static Result<Uuid, Text> random()
static Result<Uuid, Text> nameMd5(borrow Uuid namespace, borrow ByteBuffer name)
static Result<Uuid, Text> nameSha1(borrow Uuid namespace, borrow ByteBuffer name)

readonly Uuid clone()
readonly Text toText()
readonly ByteBuffer toBytes()
readonly long mostSignificantBits()
readonly long leastSignificantBits()
readonly int version()
readonly int variant()
readonly boolean equals(borrow Uuid other)
readonly int compareTo(borrow Uuid other)
```

`parse` accepts exactly 36 ASCII bytes in canonical `8-4-4-4-12` hexadecimal
form, with upper- or lowercase input; every other shape, separator, or digit
returns `Err`, including embedded NUL. `toText` always emits lowercase canonical
text. `fromBytes` requires exactly 16 bytes and interprets them in network order.
`fromLongs` stores each signed `long`'s bit pattern, most-significant half first.
The two bit accessors reverse that mapping.

`random` uses OpenSSL's secure random generator and sets the RFC v4 version and
variant bits. `nameMd5` and `nameSha1` hash the 16 namespace bytes followed by
every name byte using OpenSSL EVP, then set the RFC v3 or v5 version and variant
bits. MD5 and SHA-1 here are only for standard UUID identity derivation; they are
not security, authentication, or integrity primitives. Hashing or random-source
failures return `Err`.

`version` returns the high nibble of byte 6. `variant` returns the RFC variant
number: `0` for the NCS-compatible `0` prefix, `2` for the RFC `10` prefix, `6`
for the Microsoft `110` prefix, and `7` for the future `111` prefix. `compareTo`
orders all 16 bytes lexicographically as unsigned network-order bytes and returns
a negative, zero, or positive integer. `clone` is the explicit way to create a
second independent UUID owner.


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
truncated tags, or authentication failure; unexposed unauthenticated plaintext is
wiped before it is freed. Successful plaintext is an ordinary caller-owned
`ByteBuffer`, not a secure-erasure container. Inputs that would narrow beyond
OpenSSL's `int` lengths are rejected.

Equal-length `constantTimeEquals` comparisons use OpenSSL's constant-time
comparison primitive. Different lengths return `false` immediately, so lengths
are not hidden. `Crypto` supplies cryptographic primitives; `CryptoKey`,
`CryptoDigest`, `CryptoMac`, and `Certificate` supply key and certificate operations.
Native programs require the OpenSSL 3 development and runtime libraries.

`privatePem`, `agree`, PBKDF2/HKDF, and successful AES-GCM decryption return
ordinary caller-owned `Text` or `ByteBuffer` values. Clearing or dropping these
owners has no secure-wiping guarantee. The implementation still cleanses
internal temporary material where its native paths explicitly require it, such
as the private-PEM BIO copy and unauthenticated plaintext that is not returned;
that cleanup does not wipe a successful returned value.

## Compression and checksums

```java
static Result<ByteBuffer, Text> Compression.deflate(borrow ByteBuffer input, int level)
static Result<ByteBuffer, Text> Compression.inflate(borrow ByteBuffer input, long maxOutputBytes)
static Result<ByteBuffer, Text> Compression.gzip(borrow ByteBuffer input, int level)
static Result<ByteBuffer, Text> Compression.gunzip(borrow ByteBuffer input, long maxOutputBytes)
static long Checksums.crc32(borrow ByteBuffer input)
static long Checksums.adler32(borrow ByteBuffer input)
```

`deflate` and `inflate` use the zlib-wrapped DEFLATE format; `gzip` and `gunzip`
use RFC 1952 gzip members. Compression levels are `0` through `9`, or `-1` for
the zlib default. Decompression requires a complete stream: truncation, corrupt
data, and trailing bytes after a zlib stream return `Err`. `gunzip` accepts
concatenated gzip members. `maxOutputBytes` must be nonnegative and bounds output
ByteBuffer capacity and growth; zero accepts only an empty decompressed payload. All
operations preserve arbitrary binary bytes, including NUL. Checksums return the
unsigned 32-bit zlib value as a nonnegative `long`. ZIP archive and entry
operations remain unsupported; gzip is not a ZIP substitute.

## `Locale`

`Locale.available()` returns the ICU-installed locales as canonical BCP-47
`Text` tags (including `und` for root). Locale arguments use the same rules as
`Unicode`: empty selects ICU root; malformed, noncanonical, or unavailable tags
return `Err`, with no process-default locale or fallback. `u-co-...` and
`u-nu-...` extensions configure collation and numbering respectively; unsupported
values return `Err`.

```java
static List<Text> available()
static Result<Text, Text> formatNumber(double value, borrow Text pattern, borrow Text locale)
static Result<Text, Text> formatDecimal(borrow Text exactDecimal, borrow Text pattern, borrow Text locale)
static Result<double, Text> parseNumber(borrow Text value, borrow Text pattern, borrow Text locale)
static Result<Text, Text> parseDecimal(borrow Text value, borrow Text pattern, borrow Text locale)
static Result<Text, Text> formatCurrency(double value, borrow Text isoCurrency, borrow Text locale)
static Result<Text, Text> formatCurrencyDecimal(borrow Text exactDecimal, borrow Text isoCurrency, borrow Text locale)
```

Number patterns follow ICU `DecimalFormat` pattern syntax and the locale's
numbering symbols. `formatNumber` and `parseNumber` use `double`;
`formatDecimal` and `formatCurrencyDecimal` pass nonempty ASCII decimal text to
ICU without a binary64 intermediate. `parseDecimal` uses ICU's exact decimal
parser and returns canonical ASCII decimal text, never converting the parsed
value through binary64. Its output can be passed to `BigDecimal.parse` when
arbitrary-precision arithmetic is needed and BigDecimal's configured text,
coefficient, and scale bounds permit it; only programs that use `BigDecimal`
link GMP. `Locale` alone remains ICU-backed and does not require GMP. The
returned text does not promise preservation of the input's lexical scale.

Both parse methods return `Err` unless ICU consumes the complete input. Invalid
patterns, malformed numbers, and trailing content return `Err`. Embedded NUL is
rejected in patterns, parse input, and currency codes; an empty locale is root.
`formatCurrency` and `formatCurrencyDecimal` use ICU's locale currency pattern
and rounding policy with an explicit three-letter ISO 4217 code. The code is
validated against ICU currency data; unrecognized codes return `Err`.

## `Charset`

```java
static List<Text> available()
static Result<ByteBuffer, Text> encode(borrow Text text, borrow Text charset)
static Result<Text, Text> decode(borrow ByteBuffer bytes, borrow Text charset)
```

`available()` enumerates ICU's canonical converter names; each returned name is
supported by `encode` and `decode`. Either method also accepts aliases recognized
by ICU. Empty names, the `default` pseudo-name, embedded NUL, non-ASCII names,
and unknown labels return `Err`; no platform-default encoding is selected.
Generic `UTF-16` follows ICU's BOM behavior, while explicit-endian `UTF-16LE` and
`UTF-16BE` are independent of platform byte order.

Conversions are strict: malformed input and text unrepresentable in the target
charset return `Err` without replacement characters, including for stateful
encodings. UTF-16LE/BE encoding does not add a BOM; decoding those explicit-
endian encodings preserves a BOM as U+FEFF text. UTF-8 conversion preserves
embedded NUL. Returned buffers and text are independent owned results.
Oversized ICU conversion inputs return `Err` before narrowing lengths to ICU's
32-bit API.

## OpenSSL cryptography

`CryptoKey`, `CryptoDigest`, `CryptoMac`, and `Certificate` are unique affine
owners of OpenSSL objects. They have no clone operation, implicit shared handle,
or finalizer beyond deterministic scope/ownership cleanup. The crypto APIs use
OpenSSL 3 and are emitted only when a program uses one of these classes.

### `CryptoKey`

```java
static Result<CryptoKey, Text> generate(borrow Text algorithm, int bits)
static Result<CryptoKey, Text> parsePrivatePem(borrow Text pem)
static Result<CryptoKey, Text> parsePublicPem(borrow Text pem)
readonly Result<Text, Text> publicPem()
readonly Result<Text, Text> privatePem()
readonly Result<ByteBuffer, Text> sign(borrow Text digest, borrow ByteBuffer data)
readonly Result<boolean, Text> verify(borrow Text digest, borrow ByteBuffer data, borrow ByteBuffer signature)
readonly Result<ByteBuffer, Text> agree(borrow CryptoKey peer)
```

### `CryptoDigest`

```java
static Result<CryptoDigest, Text> create(borrow Text algorithm)
Result<Unit, Text> update(borrow ByteBuffer bytes)
Result<ByteBuffer, Text> finish()
```

### `CryptoMac`

```java
static Result<CryptoMac, Text> create(borrow Text algorithm, borrow ByteBuffer key)
Result<Unit, Text> update(borrow ByteBuffer bytes)
Result<ByteBuffer, Text> finish()
```

### `Certificate`

```java
static Result<Certificate, Text> parsePem(borrow Text pem)
static Result<Certificate, Text> parsePemChain(borrow Text pem)
readonly Result<Text, Text> toPem()
readonly Result<Text, Text> toPemChain()
readonly Result<Text, Text> subject()
readonly Result<Text, Text> issuer()
readonly Result<Text, Text> serialText()
readonly Result<long, Text> notBeforeUnixSeconds()
readonly Result<long, Text> notAfterUnixSeconds()
readonly Result<boolean, Text> matchesHost(borrow Text host)
readonly Result<Unit, Text> verify(borrow Text host, borrow Path caFile, long unixSeconds)
```

`generate` accepts `RSA` with 2048–8192 bits in 256-bit steps; `Ed25519`,
`X25519`, `EC-P256`, and `EC-P384` require `bits == 0`. Names are exact and
case-sensitive. Private-key PEM must be unencrypted; key parsers accept one PEM
block with surrounding whitespace only. Public-key export is SubjectPublicKeyInfo
PEM. `privatePem` and signing require private material. Signing uses RSA-PSS with
the selected digest and an exactly digest-sized salt, standard DER ECDSA, or
Ed25519 with an empty digest name. Verification returns `Ok(false)` for an
invalid signature, including empty, truncated, noncanonical, or trailing-garbage
ECDSA DER. Unsupported keys/digests and genuine backend failures return `Err`.
Agreement is limited to X25519 and same-curve EC ECDH.

Digest and HMAC names are the exact case-sensitive allowlist `SHA256`, `SHA384`,
`SHA512`, `SHA3-256`, `SHA3-384`, and `SHA3-512`. Digest and MAC streams become
terminal after `finish`; later updates and finishes return `Err`. No secret is
included in backend error text.

Certificates reject malformed PEM and trailing non-whitespace. `parsePem` accepts
one certificate; `parsePemChain` owns the first certificate and every subsequent
certificate in input order. Subject and issuer use RFC 2253 text, and the serial
is decimal. Validity accessors convert ASN.1 times to checked signed Unix seconds,
including dates before the epoch; malformed dates or intervals with `notBefore`
after `notAfter` are rejected during parsing.
`toPem` exports only the leaf; `toPemChain` exports the leaf followed by every
stored chain member in preserved order. Both serialize the owned X.509 certificates
as canonical PEM rather than retaining input whitespace, preserving the certificate
content, public keys, signatures, subjects, serials, and validity values for
round-trip parsing and verification. `matchesHost` and `verify` match DNS names
against `dNSName` subject alternative names with wildcards limited to a complete
leftmost label; partial wildcards are rejected. IPv4/IPv6 literals match only
`iPAddress` subject alternative names; IPv6 zone identifiers do not affect
certificate identity. Both normalize DNS names through the same ICU-backed IDNA
processing as `Dns.idna`, so using these host checks requires ICU. Noncanonical
legacy numeric IPv4 spellings are rejected rather than treated as DNS names.
Neither method falls back to the common name. `verify` checks the supplied CA
file trust store, stored chain, server purpose, and explicit Unix time; it does
not fetch certificates or use the network.

## `Database` and `SqlStatement`

`Database` is an affine SQLite connection; each `SqlStatement` is an independent
affine prepared statement on that same connection. A connection may own multiple
statements and simultaneously active result cursors. Preparing, binding, stepping,
resetting, or closing one statement does not replace or reset another. `execute`
uses and finalizes a private transient statement, so it also leaves other cursors
alone. These are small native SQL APIs, not JDBC or a driver framework.

```java
static Result<Database, Text> open(borrow Text path)
Result<SqlStatement, Text> prepare(borrow Text sql)
readonly Result<long, Text> changes()
readonly Result<long, Text> lastInsertRowId()
Result<long, Text> execute(borrow Text sql)
Result<Unit, Text> begin()
Result<Unit, Text> commit()
Result<Unit, Text> rollback()
Result<Unit, Text> setBusyTimeoutMillis(long milliseconds)
Result<Unit, Text> close()

Result<Unit, Text> SqlStatement.bindNull(int index)
Result<Unit, Text> SqlStatement.bindLong(int index, long value)
Result<Unit, Text> SqlStatement.bindDouble(int index, double value)
Result<Unit, Text> SqlStatement.bindText(int index, borrow Text value)
Result<Unit, Text> SqlStatement.bindBlob(int index, borrow ByteBuffer value)
Result<boolean, Text> SqlStatement.step()
Result<Unit, Text> SqlStatement.reset()
Result<Unit, Text> SqlStatement.clearBindings()
readonly Result<int, Text> SqlStatement.columnCount()
readonly Result<Text, Text> SqlStatement.columnName(int index)
readonly Result<int, Text> SqlStatement.columnType(int index)
readonly Result<Option<long>, Text> SqlStatement.columnLong(int index)
readonly Result<Option<double>, Text> SqlStatement.columnDouble(int index)
readonly Result<Option<Text>, Text> SqlStatement.columnText(int index)
readonly Result<Option<ByteBuffer>, Text> SqlStatement.columnBlob(int index)
Result<Unit, Text> SqlStatement.close()
```

`open` accepts nonempty strict UTF-8 paths without embedded NUL; `:memory:` opens
an in-memory database. Connections use SQLite `SQLITE_OPEN_FULLMUTEX` and enable
foreign-key enforcement. SQL text is also strict UTF-8 and NUL-free. `prepare`
accepts exactly one executable statement, allowing leading empty commands and
SQLite's token-leading UTF-8 BOM whitespace. Only whitespace/comments may follow
the statement. A bounded linear lexical preflight rejects multi-statement SQL
before preparing any command on the live connection, while respecting quoted
semicolons and trigger bodies. Some SQLite PRAGMAs take effect during preparation,
so a successful `prepare` can have SQL-level side effects. Failed SQL/tail
validation leaves every existing statement and cursor untouched. SQLite failures
use generic Text diagnostics and do not echo SQL or expose SQLite's diagnostic
message.

Binding indices are one-based SQLite parameter indices; invalid indices return
`Err`. Text and ByteBuffer inputs are copied using SQLite's transient-binding
semantics, preserving embedded NUL in values. Bindings over `INT_MAX` bytes return
`Err`; SQLite may impose a lower runtime size limit. `step` returns `Ok(true)` for
a row, `Ok(false)` at completion, or `Err` on SQLite failure. Completion is
terminal:
repeated `step` calls return `Ok(false)` without rerunning the statement until
`reset`. After a failed step, another `step` returns `Err` until `reset`. `reset`
rewinds only its own statement and reports a prior evaluation error; call it
before rebinding for reuse. `clearBindings` clears only that statement's
parameters.

Column indices are zero-based. `columnType` returns SQLite's documented
`SQLITE_INTEGER` (1), `SQLITE_FLOAT` (2), `SQLITE_TEXT` (3), `SQLITE_BLOB` (4), or
`SQLITE_NULL` (5) code. `columnType` and typed value accessors require a current
row. Typed accessors accept only their exact corresponding SQLite storage type:
integer, float, text, or blob. `NULL` becomes `None`; a different type or an
invalid column index returns `Err` rather than silently coercing. SQLite text
results must be valid UTF-8. Returned Text and ByteBuffer values are owned copies,
not SQLite-cell borrows, and remain valid after another step, reset, or closing
or dropping either owner.

`execute` runs one unparameterized command without result columns and returns
SQLite's 64-bit change count. For values, use `prepare` and bind parameters rather
than concatenating untrusted data into SQL. `changes` and `lastInsertRowId` report
SQLite's corresponding connection-local 64-bit values. Busy timeout milliseconds
must be in `0..=INT_MAX`; an expired lock wait returns `Err`.

`Database.close` is idempotent and closes the connection with SQLite's
`sqlite3_close_v2`: the connection becomes unusable immediately, and SQLite
defers backend deallocation until all outstanding statements have been finalized.
After successful rollback, `Database.close` resets active statement cursors before
`sqlite3_close_v2`, releasing read locks while statement handles remain owned.
Explicit close returns `Err` and leaves the connection open if rollback fails;
drop attempts rollback and cursor reset without returning an error.
After the connection is closed or dropped, every operational
`SqlStatement` method returns `Err`. `SqlStatement.close` remains safe and
idempotent and finalizes only its handle; after closing a statement, its
operational methods return `Err`. Dropping a statement finalizes only its own
handle, in any order relative to Database close/drop. Programs that use these APIs
require SQLite's development headers and `libsqlite3`; unrelated programs do not
link SQLite.

## XML, streaming, and XSLT

The Linux XML APIs use libxml2; XSLT additionally uses libxslt. Native builds link
these libraries only when their respective templates are emitted; `emit-llvm`
needs the corresponding development headers. Install `libxml2-dev` and
`libxslt1-dev` (or distribution equivalents). No JVM, process-wide XML loader
setting, or user-provided extension function or callback is exposed.

`XmlDocument` is the in-memory DOM surface. `parse` rejects malformed input,
DOCTYPE declarations, external entities, and NUL; XPath methods are namespace-aware
and return independent owned values. `serialize`, mutation, escaping, and XSD
validation operate on that document; XInclude is not processed and schema
includes/imports are rejected.

`XmlDocument.setText` treats its value literally and accepts element, attribute,
and text-node selections. Selecting an element and a descendant applies the
element replacement once because it removes that descendant; an attribute owned
directly by the selected element remains a separate target and is updated.

`XmlReader` is a pull reader over an in-memory `Text` or `ByteBuffer`:

```java
static Result<XmlReader, Text> fromText(borrow Text xml)
static Result<XmlReader, Text> fromBuffer(borrow ByteBuffer bytes)
Result<Option<XmlEvent>, Text> read()
Result<Unit, Text> close()
```

`XmlDocument.parse` and `XmlReader.fromText` interpret `Text` inputs as UTF-8,
regardless of an XML declaration's encoding label. `XmlReader.fromBuffer` accepts
encoded XML bytes and retains libxml2's declaration/BOM-based encoding detection.

`XmlEvent` owns a snapshot returned by `XmlReader.read`:

```java
readonly int nodeType()
readonly int depth()
readonly boolean isEmptyElement()
readonly Text name()
readonly Text localName()
readonly Text namespaceUri()
readonly Text value()
readonly int attributeCount()
readonly Result<Text, Text> attributeName(int index)
readonly Result<Text, Text> attributeNamespaceUri(int index)
readonly Result<Text, Text> attributeValue(int index)
```

The reader copies its input to keep libxml2's memory reader valid, but does not
build a DOM: each `read` advances the pull parser once and returns `Some` with an
owned event snapshot, or `None` only at EOF. Malformed, namespace-invalid, and
truncated input is `Err`, never EOF; after an error, reads continue to report failure. `close` is
idempotent, reports libxml2 close errors, and makes later reads fail. Event fields
and attributes are copied before advancing, survive later reads and reader close,
and each text accessor returns a separate owned `Text`. `name` is the qualified
name; local name and namespace URI are separate. Attribute order follows the
reader. No DTD loading, entity substitution, XInclude, or external resource access
is enabled; DTD/entity-reference events are errors.

`nodeType` uses libxml2's `xmlTextReader` numeric node types (for example, Element
1, Text 3, CDATA 4, ProcessingInstruction 7, Comment 8, Document 9, whitespace
13, significant whitespace 14, EndElement 15, and XML declaration 17). Element
depth starts at zero. Fields without a value are returned as empty `Text`.

`XmlWriter` emits to an owned `ByteBuffer` through libxml2's streaming callback:

```java
static Result<XmlWriter, Text> create()
Result<Unit, Text> startDocument()
Result<Unit, Text> startElement(borrow Text name)
Result<Unit, Text> startElementNs(borrow Text prefix, borrow Text localName, borrow Text namespaceUri)
Result<Unit, Text> writeAttribute(borrow Text name, borrow Text value)
Result<Unit, Text> writeAttributeNs(borrow Text prefix, borrow Text localName, borrow Text namespaceUri, borrow Text value)
Result<Unit, Text> writeText(borrow Text value)
Result<Unit, Text> writeCData(borrow Text value)
Result<Unit, Text> writeComment(borrow Text value)
Result<Unit, Text> writeProcessingInstruction(borrow Text target, borrow Text value)
Result<Unit, Text> endElement()
Result<ByteBuffer, Text> finish()
```

Call `startDocument` once before document content. Names and values are validated
as XML 1.0 text and libxml2 escapes text and attribute values. Namespace-aware
methods take a prefix, local name, and URI; an empty prefix denotes the default
namespace for elements. Unqualified attributes remain outside that default
namespace. Namespace declarations are generated by element/attribute namespace
arguments; explicit `xmlns` attributes are rejected. A child unqualified element
resets an inherited default namespace. CDATA rejects `]]>`, comments reject `--`
and a trailing `-`, and processing instructions reject an `xml` target or `?>`.
Text/CDATA require an open element, attributes must precede content, `endElement`
requires an open element, and `finish` requires a balanced stack and exactly one
root. Comments and processing instructions may occur outside the root. Rejected
structure/arguments, including same-start-tag prefix conflicts, leave the writer
usable. A libxml2 write or flush failure returns `Err` and discards unfinished
output on drop. Successful `finish` flushes, closes the writer, and transfers the
complete byte buffer to the caller. The writer retains no document tree.
Start-tag attribute and prefix metadata is released when child content begins;
open frames retain only the inherited-default-namespace state and structure.

`XsltStylesheet` owns a compiled libxslt stylesheet. `compile` accepts an
in-memory `Text` containing an XML stylesheet; `transform` borrows an
`XmlDocument` and returns an independent owned result document without mutating
the source. Each transform makes a private deep source-tree copy because libxslt
may remove whitespace nodes; this costs time and storage linear in the source tree.
The document-returning `transform` requires one root and no non-whitespace text
outside it; text-only, multiple-root, and non-XML document results return `Err`.
`transformBytes` instead returns independent serialized bytes, honoring the
stylesheet's `xsl:output` method and encoding. It supports XML result fragments,
HTML, and text, including empty text output. Bytes are written directly into the
returned `ByteBuffer`, without an intermediate full-output copy. Unsupported
encodings and serialization failures return `Err`. Text-method conversion is strict:
unrepresentable characters return `Err`, never XML numeric references or replacement
bytes. UTF-8 aliases are accepted independently of the installed libxml alias table,
including case-insensitive `unicode-1-1-utf-8`, `unicode11utf8`, `unicode20utf8`, and
`x-unicode20utf8`; labels with extra suffixes are not aliases. Stateful encodings
flush their final shift sequence.
XML and HTML retain their numeric-reference encoding behavior. Unsupported output
methods return `Err` even for empty results. Use `Charset.decode` with a supported
declared encoding when owned text is needed. Both result forms outlive the source
document and stylesheet.

```java
static Result<XsltStylesheet, Text> compile(borrow Text stylesheet)
readonly Result<XmlDocument, Text> transform(borrow XmlDocument document)
readonly Result<ByteBuffer, Text> transformBytes(borrow XmlDocument document)
```

Only per-context built-in XPath/XSLT functions are available. Element availability
uses the restricted standard-instruction policy, never extension/plugin discovery.
Stylesheet `xsl:include`, `xsl:import`, extension elements, and extension functions
are rejected.

Each transformation gets its own libxslt security policy forbidding file/network reads,
file/network writes, and directory creation. A prohibited resource request (including
the XSLT `document()` function) is denied and makes either transform method return
`Err`, even if libxslt otherwise recovers from it. There are no process-global loaders or default
security-policy changes. DTDs/entities are also disabled by the XML parser used to
compile the stylesheet and parse the source document.

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
that conflict with a live loan are rejected while a reachable path can still use
it; last-use liveness releases local loans before scope exit when possible.

### `Pair<T, U>` and tuple syntax

`Pair<T, U>` is the prelude's pair type, with public `first` and `second` fields
and a constructor taking those two values. The parenthesized type `(T, U)` denotes
`Pair<T, U>`, and `(a, b)` constructs a pair value. Types, expressions, and
patterns are exactly binary and may nest; `Pair` is a reserved built-in name.

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
Iterator<T> drain()
static List<T> collect(Iterator<T> input)
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

Indexed traversal accepts `for (T value : values)` for copied values and
`for (borrow T value : values)` / `for (borrow mut T value : values)` for affine
values. Lists, deques, and arrays traverse in index order; each container is
evaluated once and stays loaned for the full loop, and each binding ends at the
iteration boundary. `drain()` instead explicitly transfers owned elements to an
iterator; `collect()` consumes an iterator and transfers each yielded value into a
new list.

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
Iterator<T> drain()
```

The deque is an owning double-ended collection. Pushes transfer affine elements;
strict pops from an empty deque and invalid indexed operations are fatal. Recoverable
pops return `None` when empty. Copied versus borrowed indexed access follows the
same specialization rules as `List<T>`. Negative reserve is fatal. `drain()` empties
the deque and transfers its elements to an iterator from front to back.

### `Array<T>` and `Arrays`

```java
Array<T>(List<T> values)                           // consumes the list
readonly int size()
readonly T get(int index)                         // copied specializations only
readonly Option<T> tryGet(int index)              // copied specializations only
readonly borrow T getRef(int index)                // affine specializations only
borrow mut T getMut(int index)                     // affine specializations only
void set(int index, T value)

Iterator<T> drain()
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

Default hashing is available for `byte`, `char`, `int`, `long`, `double`,
`boolean`, `Unit`, static `String`, and owned `Text` keys. Integer-like values,
booleans, and `Unit` use value equality. Double keys treat all NaNs as equal and
canonicalize signed zero for hashing. `String` and `Text` keys compare their exact
length-delimited UTF-8 bytes, including embedded NUL; there is no normalization or
case folding.

Class, interface, and enum keys require an explicit owned `Hasher<T>` policy:

```java
interface Hasher<T> {
    readonly long hash(borrow T value);
    readonly boolean equals(borrow T left, borrow T right);
}

Set<T>()
static Set<T> withHasher(Hasher<T> hasher)
void add(T key)
readonly boolean contains(borrow T key)
boolean remove(borrow T key)
readonly int size()
readonly boolean isEmpty()
void clear()
Iterator<T> drain()

Map<K, V>()
static Map<K, V> withHasher(Hasher<K> hasher)
readonly boolean containsKey(borrow K key)
readonly int size()
readonly boolean isEmpty()
Option<V> put(K key, V value)
Option<V> tryRemove(borrow K key)
readonly Option<V> tryGet(borrow K key)              // copied value specializations only
readonly borrow V getRef(borrow K key)               // affine value specializations only
borrow mut V getMut(borrow K key)                     // affine value specializations only
void clear()
Iterator<K> drainKeys()
Iterator<V> drainValues()
```

`hash` and `equals` are readonly borrowed queries. Equal keys must return the same
hash, and a stored key's hash/equality behavior must remain stable. Custom-key
specializations have no zero-argument constructor; construct them with
`Map<K, V>.withHasher` or `Set<T>.withHasher`. The factory consumes the policy
interface value; the collection owns and drops it once. Growth relocates cached
hashes and owned entries without calling user hashers again.

Concrete `Hasher<T>` implementations cannot carry local borrowed fields. A
cross-thread transfer recursively checks concrete implementations behind the
policy interface and rejects view-bearing or otherwise non-sendable policies.

Insertion transfers an owned key. Map replacement retains the stored equal key,
drops the incoming key, and returns the old value in `Some`; Set duplicates drop
the incoming key. `put` returns `None` for a new key; `tryRemove` drops the stored
key and transfers a removed value in `Some`, or returns `None` when absent.
Insertion or growth can allocate; allocation failure is fatal. Map and set
iteration order is unspecified.

`Set.drain()` transfers its keys in unspecified hash-table order. `Map.drainKeys()`
transfers keys and destroys every value on the calling thread before returning;
`drainValues()` transfers values and destroys every key before returning. Both empty
the map and yield entries in unspecified hash-table order. The resulting iterator
retains only the yielded owners, not the discarded entry halves.

Generic query declarations borrow keys; scalar and static `String` specializations
copy the value without an implicit clone, while object, interface, enum, and
`Text` queries use readonly borrows. `tryGet` copies only copied value types;
affine values require `getRef`/`getMut`. Absent borrowed lookup is a fatal boundary
error.

Map traversal uses two bindings, for example
`for (int key, borrow Value value : map)` or
`for (borrow Text key, int value : map)`. Set traversal accepts one readonly key
binding. OrderedMap uses the same two-binding form in comparator order; ordered
keys are readonly, while affine values may be bound shared or exclusively. OrderedSet
uses one readonly key binding in comparator order. PriorityQueue traversal accepts
one readonly element binding in heap-array order, not priority-pop order. These
loops hold a whole-root loan for their complete lexical extent; different entries
do not grant disjoint mutable access. See
[MAP_TRAVERSAL.md](MAP_TRAVERSAL.md) for the base map traversal contract.

### Fused collection pipelines

The compiler fuses terminal chains rooted at `List<T>.iter()`, `Array<T>.iter()`,
or `Map<K, V>.iter()`. Supported stages are `.map(callback)`,
`.filter(predicate)`, `.take(longCount)`, `.zip(otherPipeline)`, and
`.enumerate()`, in any written order; `.fold(initial, folder)` and `.sizeHint()`
are the terminals. A chain is recognized only as part of a terminal expression:
it is not an `Iterator<T>` or another first-class value and cannot be stored,
returned, or used as a `for` traversal source.

Stages run in written order. Thus `.take(2L).filter(...)` limits the input to the
first two items before filtering, while `.filter(...).take(2L)` takes the first
two accepted items. `take` takes one nonnegative `long`; a negative runtime count
is fatal, and `take(0L)` invokes no mapper, predicate, or folder method. `zip`
stops at the shorter input. `enumerate` pairs each item with a zero-based `long`
index.

Callbacks use the prelude interfaces `Mapper<T, U>` (`U apply(T value)`),
`RefMapper<T, U>` (`U apply(borrow T value)`), `Predicate<T>`
(`readonly boolean test(borrow T value)`), and `Folder<A, T>`
(`A apply(A accumulator, borrow T value)`). Copyable items are passed by value;
affine items are exposed as readonly borrows. `map` accepts `Mapper` for
copyable items and `RefMapper` for borrowed items (and may use `RefMapper` for
copyable items); its result must be a non-borrowed, non-view value. `filter`
uses `Predicate`; `fold` returns the accumulator type established by `initial`.

Collection items and the pair views produced by Map iteration, `zip`, and
`enumerate` are not copied into adapter objects. Those pair items are temporary
stack `Pair` views borrowed by callbacks. The fusion stages allocate no runtime
adapter objects, but callback construction/calls and mapped values may allocate.
`sizeHint()` returns an allocated ordinary `Pair<long, Option<long>>` value:
`first` is the lower bound and `second` is the upper-bound option. These finite
sources provide a `Some` upper bound. Without a filter the bounds are exact; any
filter makes the lower bound zero. `map` and `enumerate` preserve the bounds;
`take` caps the upper bound, and `zip` takes the minimum of the two upper bounds.
`sizeHint()` does not pull items or invoke mapper/predicate methods, although
stage-argument expressions are still evaluated normally.

## Ordered collections, priority queues, bit sets, and iterators

### `OrderedMap<K, V>` and `OrderedSet<T>`

```java
OrderedMap<K, V>(Comparator<K> comparator)
Option<V> put(K key, V value)
Option<V> tryRemove(borrow K key)
readonly boolean contains(borrow K key)
readonly Option<V> tryGet(borrow K key)              // copied value specializations only
readonly borrow V getRef(borrow K key)               // affine value specializations only
borrow mut V getMut(borrow K key)                    // affine value specializations only
readonly borrow K keyAt(int index)                  // affine keys only
readonly borrow V valueAt(int index)                // affine values only
readonly int size()
readonly boolean isEmpty()
void clear()
readonly Option<int> floorIndex(borrow K key)
readonly Option<int> ceilingIndex(borrow K key)
readonly Option<int> lowerIndex(borrow K key)
readonly Option<int> higherIndex(borrow K key)
borrow mut V valueAtMut(int index)                    // affine values only
Iterator<K> drainKeys()
Iterator<V> drainValues()

OrderedSet<T>(Comparator<T> comparator)
void add(T key)
readonly boolean contains(borrow T key)
boolean remove(borrow T key)
readonly int size()
readonly boolean isEmpty()
void clear()
readonly borrow T keyAt(int index)                    // affine elements only
Iterator<T> drain()
```

Ordered maps and sets keep keys in comparator order in contiguous storage. Every
lookup uses logarithmic comparisons; insertion and removal can move the remaining
entries. `keyAt` and `valueAt` use zero-based indices and fail on invalid indices.
The four index queries return the index of the greatest key at or below the query
(`floorIndex`), least key at or above it (`ceilingIndex`), greatest strictly lower
key (`lowerIndex`), or least strictly higher key (`higherIndex`); a missing result
is `None`. A comparator returns a negative, zero, or positive `int`. Comparator
equality identifies duplicate keys; `put` returns `None` for a new key, while
replacing an equal map key retains the original stored key and transfers the
replaced value in `Some`. Adding an equal set key drops the new key and retains the
existing one. `keyAt` and `valueAt` return shared borrows of affine entries;
`valueAtMut` returns an exclusive value borrow for explicitly mutable traversal.
`keyAt` is always readonly so comparator order cannot be invalidated through a key
reference. Key and value types must be copied values or owned nominal values, never
borrows or local views. Queries copy scalar keys and borrow affine keys; looking up
or removing a key does not consume an affine query object.

### `PriorityQueue<T>`

```java
PriorityQueue<T>(Comparator<T> comparator)
void add(T value)
T pop()
Option<T> tryPop()
readonly T peek()                                    // copied specializations only
readonly borrow T getRef()                           // affine specializations only
readonly Option<T> tryPeek()                         // copied specializations only
readonly int size()
readonly boolean isEmpty()
void clear()
readonly borrow T valueAt(int index)                  // affine elements only
Iterator<T> drain()
```

The queue is a binary min-heap: a value whose comparator result is negative
relative to another value has higher priority. Add and extraction preserve owned
values by transfer; `pop` and `peek` fail on an empty queue, while `tryPop` and
`tryPeek` return `None`. `clear` destroys every queued value but retains storage.
The queue owns its comparator. Readonly traversal and `drain()` visit heap-array
order, not sorted priority order; `drain()` transfers every value and leaves it empty.

### `BitSet`

```java
BitSet(long initialBits)
readonly boolean get(long bit)
void set(long bit)
void clear(long bit)
void flip(long bit)
void setRange(long fromInclusive, long toExclusive)
void clearRange(long fromInclusive, long toExclusive)
void flipRange(long fromInclusive, long toExclusive)
readonly Option<long> nextSetBit(long fromIndex)
readonly long cardinality()
readonly long length()
readonly boolean isEmpty()
readonly BitSet clone()
void and(borrow BitSet other)
void or(borrow BitSet other)
void xor(borrow BitSet other)
void andNot(borrow BitSet other)
```

`initialBits` reserves zeroed capacity; it does not set any bits. Ranges are
half-open, and negative indices or reversed ranges are fatal. `length` is one plus
the highest set-bit index (zero when empty); `nextSetBit` finds the first set bit
at or after its argument. Logical operations mutate the receiver. `clone` makes an
explicit independent copy.

### Owned iterators

`Iterator<T>.next()` returns `Option<T>` and transfers each yielded value to its
caller. `for (T value : iterator)` advances the original iterator mutably while
holding an exclusive root loan for the loop; affine values may be owned or
explicitly borrowed for one iteration. The iterator remains usable after the loop.
Break and return destroy the current owned item and release the loop loan; owned
temporary iterators are dropped on every exit.

`drain()` transfers collection elements without cloning. Lists and arrays yield in
index order, deques front to back, ordered maps and sets in comparator order, and
priority queues in heap-array order. Hash maps and sets yield in unspecified order.
`Map.drainKeys()` destroys unused values and `drainValues()` destroys unused keys
on the calling thread before returning; the ordered-map forms do the same in
comparator order. Each drain leaves its collection empty. Iterator sendability
therefore depends on its yielded owners, not discarded entry halves. Dropping an
iterator early destroys all remaining owned values.
`List.collect` consumes its input iterator and builds a new list.

`MapIterator<T, U>` owns an input iterator and a `Mapper<T, U>` callback;
`FilterIterator<T>` owns an input iterator and a `Predicate<T>` callback. They are
lazy: `map` invokes the mapper only as values are requested, and `filter` advances
until a value passes or the input ends. Rejected filter values are destroyed.
`Iterators<T>.take` and `skip` lazily bound and discard a nonnegative number of
values; a negative count is fatal. All adapters own their inputs and callbacks,
and dropping an adapter recursively drops those owners. These are explicit owned
`Iterator<T>` values, distinct from collection traversal and compiler-fused
terminal pipelines. A fused chain is only a compiler plan: it produces no
`Iterator<T>` value, and its source loan ends with the terminal.

Collection and iterator sendability includes hidden key/value/element storage
and owned comparator, mapper, predicate, and input objects. An empty container
does not make a `Guard<T>` payload or a callback capturing a guard sendable.

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
are `Err`. `Console.print`/`printText` write without a newline and flush stdout;
use `Console.print` for explicit cross-process readiness messages. Primitive
`println` retains normal stdio buffering. `Text.print` appends a newline.
`Args.count` excludes `argv[0]`; `Args.get` returns an owned UTF-8 copy and reports
an invalid index or non-UTF-8 argument as `Err`.

## Paths and filesystem

`Path`, `File`, `Directory`, `FileInfo`, `Watcher`, `WatchEvent`, and `ByteBuffer` are opaque native owners;
resource handles have no public constructors. Factories return fully initialized
owners or a `Result` error. Explicit `close()` reports errors and invalidates the
handle, except `Watcher.close()`, which is idempotent. Ordinary scope cleanup
best-effort closes an open descriptor. Calls on a closed resource return `Err`.

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
static Result<File, Text> File.openReadWrite(borrow Path path, int mode, boolean create)
Result<long, Text> File.readSome(borrow mut ByteBuffer destination, long maxBytes)
Result<long, Text> File.writeSome(borrow ByteBuffer source, long offset, long maxBytes)
Result<long, Text> File.read(borrow mut ByteBuffer destination, long maxBytes)
Result<long, Text> File.write(borrow ByteBuffer source, long offset, long maxBytes)
Result<Unit, Text> File.writeAll(borrow ByteBuffer source)
Result<long, Text> File.position()
Result<long, Text> File.seek(long offset, int origin)
Result<Unit, Text> File.truncate(long length)
Result<long, Text> File.readAt(borrow mut ByteBuffer destination, long fileOffset, long maxBytes)
Result<long, Text> File.writeAt(borrow ByteBuffer source, long bufferOffset, long fileOffset, long maxBytes)
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

`File.create`, `openAppend`, and `openReadWrite` require a permission mode in
`0..07777` (decimal `0..4095`), including the POSIX special bits. Invalid modes
return `Err` before opening, creating, or truncating a file, even when
`openReadWrite` does not create. Creation is subject to the process umask.
`openReadWrite` opens without truncation; `create` controls whether a missing
file is created.
`readSome` appends at most `maxBytes`, returns the count,
`0` at EOF, or `Err`; a zero maximum performs no read. `writeSome` writes at most
the selected `[offset, offset + maxBytes)` source range and returns one partial
count. Invalid ranges and negative limits are `Err`; zero maximum returns zero.
`writeAll` retries partial writes until complete or `Err`; an error can follow a
written prefix and does not undo it. `flush` uses `fsync`; it does not promise
hardware durability beyond the OS contract. File I/O is synchronous and blocking.
Drop closes but does not implicitly flush.

`read` and `write` implement `ByteSource` and `ByteSink` for streams. `position`
returns the current cursor. `seek` uses origin `0` for start, `1` for current
position, and `2` for end; invalid origins, negative resulting positions, and
offset overflow are errors. `truncate` changes file length but does not move the
cursor. `readAt` appends bytes to the destination buffer; `writeAt` uses the
specified source-buffer range. Both return one partial count and leave the cursor
unchanged; `readAt` is bounded by the platform's representable ByteBuffer length
and the remaining long offset range. Offset or buffer-range overflow is an error.
`writeAt` is rejected for append-mode files because Linux may force `pwrite` to
append on those descriptors.

`Directory.nextName` skips `.` and `..`, returns one owned UTF-8 entry component
per `Some`, `None` at end, and `Err` for I/O or invalid UTF-8. It does not return a
full path, sort entries, or recurse. Join the name to the original path when needed.

### `FileInfo` and `Files`

```java
readonly long FileInfo.size()
readonly boolean FileInfo.isFile()
readonly boolean FileInfo.isDirectory()
readonly boolean FileInfo.isSymlink()
readonly long FileInfo.modifiedMillis()
readonly long FileInfo.accessedMillis()
readonly long FileInfo.changedMillis()
readonly int FileInfo.mode()

static Result<Text, Text> Files.readText(borrow Text path)
static Result<Unit, Text> Files.writeText(borrow Text path, borrow Text content)
static Result<Unit, Text> Files.writeTextAtomic(borrow Text path, borrow Text content)
static Result<FileInfo, Text> Files.metadata(borrow Path path)
static Result<Unit, Text> Files.createDirectory(borrow Path path, int mode)
static Result<Unit, Text> Files.removeFile(borrow Path path)
static Result<Unit, Text> Files.removeDirectory(borrow Path path)
static Result<Unit, Text> Files.rename(borrow Path from, borrow Path to)
static Result<Unit, Text> Files.copy(borrow Path source, borrow Path target, boolean replace)
static Result<Unit, Text> Files.createDirectories(borrow Path path, int mode)
static Result<Path, Text> Files.readLink(borrow Path path)
static Result<Unit, Text> Files.symlink(borrow Path target, borrow Path link)
static Result<Unit, Text> Files.chmod(borrow Path path, int mode)
static Result<List<Text>, Text> Files.list(borrow Path path)
static Result<Path, Text> Files.canonical(borrow Path path)
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

`metadata` returns a `FileInfo` snapshot; `size()` is in bytes, the three time
getters are Unix-epoch milliseconds, and `mode()` returns the POSIX `st_mode`
bits. `Files.copy` copies one regular file with a fixed-size buffer; it rejects
symlinks at the source and target and does not recurse or promise `fsync`
durability. `replace` controls whether an existing target can be truncated;
errors after copying starts may leave a partial target. `createDirectories`
creates missing path components and can leave earlier components created on
error. `readLink` returns the symlink target as a UTF-8
`Path`; `symlink` stores the target path as supplied, while `chmod` changes mode
bits. `list` returns unsorted, nonrecursive UTF-8 entry names (excluding `.` and
`..`), with limits of 65,536 names and 16 MiB total name bytes. `canonical`
resolves an existing path to its canonical absolute path and returns `Err` with
`EILSEQ` if the resolved path bytes are not valid UTF-8.

Filesystem paths are passed through the operating-system boundary and reject
embedded NUL. All creation modes and `chmod` require permission bits in
`0..07777` (decimal `0..4095`); negative values and file-type bits return `Err`
before any filesystem operation. Creation is subject to umask; `chmod` is not.
Invalid modes cannot create even an initial component of `createDirectories`.
Filesystem commands report success as `Result<Unit, Text>`, not a boolean.

### `Watcher` and `WatchEvent`

```java
enum WatchKind {
    Created; Modified; AttributesChanged; ClosedForWrite; Deleted;
    MovedFrom; MovedTo; MovedSelf; DeletedSelf; Ignored; Overflow;
}
static Result<Watcher, Text> Watcher.open(borrow Path directory)
Result<Option<WatchEvent>, Text> Watcher.next(long timeoutMillis)
Result<Unit, Text> Watcher.close()
readonly borrow Text WatchEvent.name()
readonly WatchKind WatchEvent.kind()
readonly long WatchEvent.cookie()
readonly boolean WatchEvent.isDirectory()
readonly boolean WatchEvent.overflow()
```

`Watcher` uses Linux inotify and observes only immediate children of a real
directory; it does not recurse or follow a symlink at the watched path. `next`
waits against a monotonic deadline, returns `Ok(None)` on timeout, and treats
`timeoutMillis == 0` as one nonblocking poll. Negative timeouts are errors.
Events are separate records and need not represent one high-level filesystem
operation. Move-from/move-to events can be correlated by their nonzero cookie.
`name()` is the UTF-8 entry name. An entry with a non-UTF-8 name makes `next`
return `Err` after consuming that event, without invalidating the watch. The name
is empty for attribute changes to the watched directory itself, and for self,
ignored, and overflow events. Queue overflow is returned as
`WatchKind.Overflow` with `overflow() == true`, not silently discarded. The kernel
may have dropped events, so rescan the directory to reconstruct current state
before relying on later notifications; overflow does not invalidate the watch.
`DeletedSelf` or `Ignored` invalidates the watch; queued events remain readable,
then later `next` calls return `Err`.
`close()` is idempotent, including after invalidation.

## Byte streams and binary encoding

```java
interface ByteSource {
    Result<long, Text> read(borrow mut ByteBuffer destination, long maxBytes);
}
interface ByteSink {
    Result<long, Text> write(borrow ByteBuffer source, long offset, long maxBytes);
    Result<Unit, Text> flush();
}
class BinaryReader {
    BinaryReader(ByteBuffer bytes, boolean littleEndian)
    static Result<BinaryReader, Text> fromSource(ByteSource source, boolean littleEndian, long maxBytes)
    readonly long position()
    readonly long remaining()
    Result<Unit, Text> seek(long position)
    Result<byte, Text> readByte()
    Result<int, Text> readInt16()
    Result<int, Text> readInt32()
    Result<long, Text> readInt64()
    Result<double, Text> readFloat32()
    Result<double, Text> readDouble()
    Result<ByteBuffer, Text> readBytes(long length)
    Result<Text, Text> readUtf8(long length)
}
class BinaryWriter {
    BinaryWriter(long capacity, boolean littleEndian)
    static Result<BinaryWriter, Text> toSink(ByteSink sink, boolean littleEndian, long maxBytes)
    readonly long position()
    readonly long pendingOffset()
    Result<Unit, Text> writeByte(byte value)
    Result<Unit, Text> writeInt16(int value)
    Result<Unit, Text> writeInt32(int value)
    Result<Unit, Text> writeInt64(long value)
    Result<Unit, Text> writeFloat32(double value)
    Result<Unit, Text> writeDouble(double value)
    Result<Unit, Text> writeBytes(borrow ByteBuffer source)
    Result<Unit, Text> writeUtf8(borrow Text value)
    readonly borrow ByteBuffer bytes()
    Result<Unit, Text> flush()
    static ByteBuffer finish(BinaryWriter writer)
}
class StreamError {
    readonly borrow Text message()
    readonly long sourceBytesRead()
    readonly long sinkBytesWritten()
    readonly long confirmedOffset()
    readonly borrow Option<ByteBuffer> retainedBytes()
}
static Result<long, StreamError> Streams.copy(borrow mut ByteSource source, borrow mut ByteSink sink, long maxBytes, long bufferSize)
static Result<ByteBuffer, StreamError> Streams.readUpTo(borrow mut ByteSource source, long maxBytes, long bufferSize)
static Result<ByteBuffer, StreamError> Streams.readAll(borrow mut ByteSource source, long maxBytes, long bufferSize)
static Result<Unit, StreamError> Streams.writeAll(borrow mut ByteSink sink, borrow ByteBuffer source)
```

`ByteSource` and `ByteSink` are allocation-free interfaces implemented by
`File`, `TcpStream`, and `TlsStream`. A zero-byte read request returns zero
without changing the destination. A positive read request blocks until data or
EOF; `Ok(0)` means EOF, and `Ok(n)` appends exactly `n` bytes. A positive write
request must return a positive count no larger than the request; zero is valid
only for a zero-byte request. A failed source or sink operation can follow
physical transport progress, which the primitive result does not report.

`StreamError` owns the original `Text` message and exposes confirmed logical
`sourceBytesRead` and `sinkBytesWritten` counts. Its optional `retainedBytes`
is owned by the error and exposed through a shared borrow for inspection or
explicit replay while the error remains alive. Inspect the counts and
`confirmedOffset()` before replay. For `copy`, the buffer is the pending source packet and the
offset is its prefix accepted by successful sink calls. For `readUpTo` and
`readAll`, it is the accumulated output. For `writeAll`, no source copy is
retained: the caller keeps its borrowed source and the offset is the confirmed
written prefix. Other operations use offset zero. Counts include only logically
confirmed bytes, not unreported physical progress from a failed transport call;
retained pending bytes do not promise rollback or make retry automatically safe.
An allocation failure before stream effects reports zero progress.

`copy` consumes at most `maxBytes`, stops at EOF, and flushes the sink even when
the bound is zero. It retains staged source bytes if reading fails, and retains
the current packet plus confirmed write offset if a write fails. A flush error
reports all successful writes but does not undo them. `readUpTo` consumes at
most `maxBytes`, never probes past the limit, and returns the prefix at the
limit. Errors retain the accumulated prefix, including bounded valid bytes a
failing read already appended. `readAll` succeeds only at EOF. Once it reaches
`maxBytes`, it consumes at most one probe byte; an over-limit error retains the
output and that byte, including when `maxBytes` is zero. `writeAll` loops over
partial writes and flushes on completion; errors preserve the confirmed offset
while leaving the original source usable.

`BinaryReader(ByteBuffer, ...)` uniquely owns its input; `fromSource` owns the
`ByteSource` interface value and accepts a nonnegative `maxBytes` budget.
`BinaryWriter(long, ...)` owns its output buffer; `toSink` owns the `ByteSink`
interface value and likewise accepts a nonnegative budget. Both constructors
require an explicit byte order. In source mode, `remaining()` reports budget,
not known source availability; `seek` accepts only the current position.
Reads enforce the budget before calling the source. Each operation leaves the
logical cursor unchanged unless it succeeds. Bytes appended before a source
error remain staged for later smaller reads, and the source error is returned.
An append beyond the requested count is a contract violation; those bytes are
preserved, not clamped or treated as confirmed progress. An `Ok(0)` from a positive
source request marks permanent EOF. Zero-byte requests do not probe the source.
Invalid UTF-8 leaves the cursor unchanged. A source error does
not imply that the underlying transport rolled back; only staged bytes and
successful results establish recoverable progress.

`readInt16` returns a signed `int`. Binary doubles use IEEE-754 binary64 bits
copied with `memcpy`. `readFloat32` widens an IEEE-754 binary32 value to
`double`. `writeFloat32` encodes `double` values as binary32 and rejects finite
magnitudes above the largest finite binary32 value without advancing the writer.
`writeInt16` accepts only the signed 16-bit range and leaves the writer unchanged
when out of range.

In sink mode, a write stages one complete encoded operation. `position()` counts
only bytes confirmed by successful sink calls; `pendingOffset()` is the confirmed
prefix of the packet borrowed by `bytes()`. If a sink call fails, the packet
remains available and further writes are rejected until `flush()` resumes it.
After sending the pending packet, `flush()` calls the sink's `flush`; it does not
promise peer receipt. A failed transport call may have advanced the physical
transport without confirming it, so retrying may duplicate bytes. `finish`
consumes the writer without I/O or implicit flush and returns only the
still-unwritten suffix of a pending packet. In buffer mode, `flush()` is a
no-op and `finish` transfers the exact output buffer without copying.

The `File` adapters use the descriptor cursor and may return partial counts;
reads are blocking and return zero at EOF. `File.flush` calls `fsync`.
`TcpStream` interface reads and writes use timeout `0` (unbounded wait); its
flush is a no-op and makes no network-delivery promise. TLS interface reads and
writes also use timeout `0` (unbounded wait). A `TlsStream.write` error closes
the TLS connection because physical bytes may already have been sent. Its
`flush` only checks that the stream is open; it does not flush a TLS record or
acknowledge network delivery. `Streams.copy` and `writeAll` account only for
successful logical sink counts, not peer acknowledgement.

### Domain serialization

`Codec<T>` is an ordinary generic interface: each codec owns the schema for
its domain value and explicitly writes every dependent field. `ListCodec<T>`,
`OptionCodec<T>`, and `ResultCodec<T, E>` compose caller-supplied codecs for
their nested values; they do not use reflection or implicit cloning. The caller
must pass an explicit nonnegative `maxElements` to `ListCodec`, so decoding has
no hidden collection-size default. A negative limit is rejected before reading
input or writing output, including source-backed transport I/O. The mutable
reader or writer is borrowed only for each call. `Serialization<T>` provides the framed entry points:

```java
interface Codec<T> {
    Result<Unit, Text> encode(borrow T value, borrow mut BinaryWriter writer)
    Result<T, Text> decode(borrow mut BinaryReader reader)
}
static Result<ByteBuffer, Text> Serialization<T>.encode(
    borrow T value, borrow mut Codec<T> codec)
static Result<T, Text> Serialization<T>.decode(
    ByteBuffer input, borrow mut Codec<T> codec)
```

`ListCodec`, `OptionCodec`, and `ResultCodec` take ownership of their nested
codec interfaces. Each instance can therefore carry its own mutable child state;
the codec instance remains usable after a call, but state changes made by a
child before an error are not rolled back. Encoding borrows list elements and
enum payloads and does not drain or move the source value. Encoders and decoders
are ordinary independently owned codec instances when a caller wants independent
state.

The aggregate wire forms are explicit: a list is a signed 32-bit big-endian
element count followed by that many elements, an option uses byte tag `0` for
`None` and `1` for `Some`, and a result uses byte tag `0` for `Ok` and `1` for
`Err`. A list decoder rejects a negative count or a count above its configured
bound before allocating its result list or decoding elements; its encoder rejects
an oversized value before writing the count. If a nested list item fails to
decode, already decoded owned items are cleared before returning that error. All
nested codecs use the same borrowed `BinaryReader`/`BinaryWriter`, so compositions
work directly with those APIs as well as inside `Serialization<T>`; the outer
frame is unchanged.

`UuidCodec` reads and writes exactly 16 bytes using the existing UUID bit APIs:
the most- and least-significant 64-bit halves are emitted in canonical network
order, independent of the reader/writer byte-order setting, and reconstructed
with `Uuid.fromLongs()`. The codec does not copy native UUID layout or encode
through UUID text.

`encode` borrows the value, builds the header and codec payload directly in a
single `BinaryWriter`, then transfers its buffer. `decode` consumes the input
buffer into a reader and returns a fully owned `T`; neither the returned value
nor any decoded dependents borrow the input, reader, or codec. The format begins
with the four ASCII bytes `NCRS`, followed by one version byte (`1`), then the
codec-defined payload. The wrapper validates all five framing bytes before
calling the codec and rejects any remaining bytes after it returns. Truncated
headers, bad magic, unsupported versions, codec errors, and trailing input
return `Err`; partially decoded owners are dropped on error.
Resource cleanup does not roll back the borrowed codec's abstract state: mutations
made by a codec remain observable after its own error or trailing-input rejection.
Stateful codecs must define any transactional behavior themselves.

`ByteCodec`, `CharCodec`, `IntCodec`, `LongCodec`, `DoubleCodec`, `BooleanCodec`,
and `UnitCodec` cover the corresponding scalar values. Numeric fields use the
existing binary reader/writer's big-endian representation; booleans accept only
the canonical bytes `0` and `1`, and characters encode a Unicode scalar as a
four-byte signed integer. `TextCodec` and `BytesCodec` prefix UTF-8 text and
arbitrary bytes, respectively, with a four-byte signed big-endian length, then
emit the content. Decoders reject negative lengths and lengths larger than the
reader's remaining input before allocating or reading the content; text decoding
also requires valid UTF-8. Their lengths are limited to the nonnegative signed
32-bit range. Composed application codecs document their own field order and
schema evolution.


## ZIP archives

The pinned private dependency build in `scripts/build_libarchive.py` applies
ZIP-specific corrections to libarchive 3.8.10: complete filename validation before
conversion, local/central filename consistency, EOCD comment extent, and exact central
directory byte/entry-count checks; the reader also searches the full legal 65,557-byte
EOCD/comment range. Invalid EOCD signatures
inside a comment do not stop the search; Zip64 locators are read separately when
they precede that window. Self-extracting prefixes preserve relative entry offsets,
including Zip64 offset expansion. See README for compile/runtime library paths.
Both file and memory readers use the same strict seekable format. The file reader
uses explicit seek callbacks; outer compression filters are unsupported.

`ZipReader` and `ZipWriter` use libarchive's ZIP support, including Zip64. They
operate on archive entries only: no method extracts to the filesystem. UTF-8-flagged
names preserve their exact UTF-8 bytes, including decomposed Unicode and literal
backslashes; names are not normalized or rewritten as filesystem paths. When a
local header marks its name UTF-8, it must match the central-directory pathname
byte for byte or `ZipReader.next()` returns `Err`. Names
without the UTF-8 flag retain libarchive's charset decoding. Absolute names and
`.`/`..` components remain data, so callers that later extract entries must apply
their own destination-path policy. Writers permit duplicate names; entries remain
in the order written and readers expose them sequentially without merging or
replacing them.

```java
static Result<ZipReader, Text> ZipReader.open(borrow Path path)
static Result<ZipReader, Text> ZipReader.fromBytes(borrow ByteBuffer bytes)
Result<Option<ZipEntry>, Text> ZipReader.next()
Result<ByteBuffer, Text> ZipReader.readEntry(long maxBytes)
Result<Unit, Text> ZipReader.skipEntry()
Result<Unit, Text> ZipReader.close()

readonly borrow Text ZipEntry.name()
readonly long ZipEntry.size()
readonly boolean ZipEntry.isDirectory()

static Result<ZipWriter, Text> ZipWriter.create(borrow Path path, int level)
Result<Unit, Text> ZipWriter.add(borrow Text name, borrow ByteBuffer bytes)
Result<Unit, Text> ZipWriter.addDirectory(borrow Text name)
Result<Unit, Text> ZipWriter.close()
```

`fromBytes` keeps an internal copy of the archive, so the borrowed buffer may be
used or dropped after the call. File and memory readers require a seekable ZIP
archive with a valid central directory and end-of-central-directory record;
missing or truncated directory data returns `Err`, not `Ok(None)`. `next` returns
one owned metadata value at a time, `Ok(None)` only after the archive directory is
validated, and implicitly skips an unread prior entry. `readEntry` reads the
current file at most once, returns an owned binary buffer, and errors if the actual
decompressed bytes exceed `maxBytes`; the archive's declared size is metadata only
and is never used to preallocate the result. `ZipEntry.size` is the declared
uncompressed size, or -1 when the header omits it. A negative limit, a second read,
unsupported entry types, encrypted entries, invalid UTF-8 names or embedded NULs,
truncated data, and decompression or CRC failures return `Err`. Use `skipEntry` to
discard the current entry explicitly. Directories have no file data.
A header rejected by `next` does not become a readable current entry.
`readEntry` and `skipEntry` then return `Err`; a subsequent `next` can advance
past that header when the archive backend can continue.

`ZipWriter.create` accepts levels 0 through 9: level 0 stores entries, while levels
1 through 9 use DEFLATE. Entry names must be nonempty UTF-8 without NUL. `add`
rejects names ending in `/`; use `addDirectory` to create directory entries. Names,
absolute names and `..` components are not rejected or normalized. Archives are
never extracted automatically. Writes may leave a partial archive on error. Call
`close()` explicitly to report final ZIP directory/close errors; dropping an open
writer closes it best-effort and does not claim success. Reader and writer methods
return `Err` after explicit close.


## Regular expressions

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

`Regex` compiles PCRE2 8-bit patterns in UTF-8 and Unicode-property modes. It
supports PCRE2 syntax, including lookarounds, backreferences, Unicode properties,
and inline flags. Invalid patterns return `Err` with a PCRE2 diagnostic and byte
offset. `matches` anchors both the start and end of the subject, allowing the
engine to backtrack between alternatives until it finds a full match or proves
there is none. `search` returns PCRE2's leftmost-first match or `Ok(None)`;
alternatives at the same position are tried in pattern order rather than using
POSIX leftmost-longest selection.
The byte-wise `\C` escape is rejected at compilation because it can split a
Unicode scalar and produce a capture that violates the validated `Text` invariant.

Patterns and subjects are passed with explicit byte lengths. Embedded NUL is
ordinary text in patterns, subjects, and captures; regex APIs do not truncate at
NUL. `RegexMatch` owns independent copies of its captures, so they remain valid
after both the subject and `Regex` are dropped. Group 0 is the complete match,
and `groupCount` includes it. `start` and `end` are half-open UTF-8 byte offsets.
An empty capture participated and has equal start/end offsets; `hasGroup` is
false for a group that did not participate. `group`, `start`, and `end` on an
absent or out-of-range group are fatal runtime boundary errors.

Each PCRE2 execution is limited to 1,000,000 match-loop iterations per candidate
start position, depth 1,000, and 16 MiB of backtracking heap. Exceeding a limit
returns `Err`, never `false` or `Ok(None)`; PCRE2 allocation and execution errors
also return `Err`. Matching state is per call; each compiled pattern owns one
immutable limit context reused across calls. Programs using `Regex` require
PCRE2 10.30+ 8-bit development headers
when building and the `libpcre2-8` runtime library.

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
static double fma(double multiplicand, double multiplier, double addend)
static double nextAfter(double start, double direction)
static double pi()
static double e()
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
`fma` computes the product and addition with one final rounding, without an
intermediate overflow or loss of cancellation precision. `nextAfter` steps
toward its second operand, including signed-zero directions. `pi` and `e`
return the nearest binary64 representations of the mathematical constants.

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

### `DateTime`

```java
static Result<long, Text> fromParts(int year, int month, int day, int hour, int minute, int second, int millis, borrow Text zone, boolean preferLaterOffset)
static Result<DateFields, Text> parts(long epochMillis, borrow Text zone)
static Result<Text, Text> format(long epochMillis, borrow Text pattern, borrow Text zone, borrow Text locale)
static Result<long, Text> parse(borrow Text value, borrow Text pattern, borrow Text zone, borrow Text locale)
static boolean isLeapYear(int year)
static Result<int, Text> daysInMonth(int year, int month)
static Result<long, Text> addCalendar(long epochMillis, int years, int months, int days, borrow Text zone)
static Result<long, Text> durationMillis(long start, long end)
static List<Text> availableZones()

readonly int DateFields.year()
readonly int DateFields.month()
readonly int DateFields.day()
readonly int DateFields.hour()
readonly int DateFields.minute()
readonly int DateFields.second()
readonly int DateFields.millisecond()
readonly int DateFields.offsetSeconds()
readonly boolean DateFields.isDst()
```

`DateTime` uses ICU4C's Gregorian calendar with the Julian cutover moved before
its supported range. Years are proleptic Gregorian, including year zero; months
are 1 through 12. Invalid parts and daylight-saving gaps return `Err`.
`preferLaterOffset` selects the later instant in a repeated local time; `false`
selects the earlier instant. `addCalendar` applies years, then months (clamping
the day to each target month), then calendar days while retaining local wall
time. It returns `Err` if the result is invalid, falls in a daylight-saving gap
or overlap, or exceeds ICU's exact-millisecond range.

ICU represents time as a double millisecond value. Calendar epochs and their
offset-adjusted local milliseconds must both lie from -9007199254740991 through
9007199254740991. `fromParts` and `addCalendar` use checked proleptic-Gregorian
millisecond arithmetic with ICU's zone offsets, so field-to-instant conversion
covers that exact range without ICU's narrower calendar-field conversion limit.
`durationMillis` is a fixed signed `end - start` calculation, independent of
calendar days, and returns `Err` on signed overflow. `DateFields.offsetSeconds` is
the total zone offset from UTC; `isDst` reports ICU's daylight-time status.
`DateFields` is an owned affine result with primitive fields and readonly
accessors; only the native temporal factories construct it.

`zone` must be a known ICU system time-zone ID (for example `UTC` or
`America/New_York`); custom offsets, unknown IDs, and embedded NUL are rejected.
No host default time zone or locale is consulted. `locale` is a BCP 47 tag
identifying an installed ICU locale; the empty string explicitly selects ICU's
root locale. Invalid or unavailable locales return `Err` rather than silently
falling back. `availableZones` lists system time-zone IDs accepted by `parts`,
not the unrelated ICU locale inventory.

`format` and `parse` use ICU date-pattern syntax, not a Java-compatibility
contract. Embedded NUL and invalid patterns return `Err`. Parsing is strict and
must consume the complete input; trailing text, daylight-saving gaps, and
ambiguous local times return `Err`. Pattern fields not supplied by the input
default to `1970-01-01 00:00:00.000` in the requested zone. ICU's `yy` pattern
uses a fixed 1940 through 2039 year window anchored at local midnight in the
requested zone. Week-year and localized weekday fields retain the requested
locale's first-week rules. Unsupported unquoted pattern letters and unmatched
quotes return `Err`, and formatting errors never return a successful prefix.
Referencing `DateTime` or `Temporal` links ICU4C; the dependency-free `Time` clock
and sleep APIs remain unchanged.

### `Temporal`

```java
static Result<Temporal, Text> instant(long epochSecond, int nanosecond)
static Result<Temporal, Text> duration(long seconds, long nanoAdjustment)
static Result<Temporal, Text> period(int years, int months, int days)
static Result<Temporal, Text> localDate(int era, int year, int month, int day, borrow Text calendar)
static Result<Temporal, Text> localTime(int hour, int minute, int second, int nanosecond)
static Result<Temporal, Text> localDateTime(int era, int year, int month, int day, int hour, int minute, int second, int nanosecond, borrow Text calendar)
static Result<Temporal, Text> offsetDateTime(int era, int year, int month, int day, int hour, int minute, int second, int nanosecond, int offsetSeconds, borrow Text calendar)
static Result<Temporal, Text> zonedDateTime(int era, int year, int month, int day, int hour, int minute, int second, int nanosecond, borrow Text zone, borrow Text calendar, boolean preferLaterOffset)
static Result<Temporal, Text> parseIso(borrow Text value)
static List<Text> availableCalendars()

readonly Text kind()
readonly Text calendar()
readonly Text zoneId()
readonly int nanosecond()
readonly Result<long, Text> durationSeconds()
readonly Result<int, Text> periodYears()
readonly Result<int, Text> periodMonths()
readonly Result<int, Text> periodDays()
readonly Result<int, Text> hour()
readonly Result<int, Text> minute()
readonly Result<int, Text> second()
readonly Result<long, Text> epochSecond()
readonly Result<int, Text> offsetSeconds()
readonly Result<CalendarFields, Text> fields()
readonly Result<Text, Text> formatIso()
readonly Result<Text, Text> format(borrow Text pattern, borrow Text locale)
readonly Result<int, Text> compareTo(borrow Temporal other)
readonly Result<Temporal, Text> plusDuration(borrow Temporal duration)
readonly Result<Temporal, Text> plusPeriod(borrow Temporal period, boolean preferLaterOffset)
readonly Result<Temporal, Text> atZone(borrow Text zone, boolean preferLaterOffset)
readonly Result<Temporal, Text> withZone(borrow Text zone)
readonly Result<Temporal, Text> withOffset(int offsetSeconds)
readonly Result<Temporal, Text> withCalendar(borrow Text calendar)
readonly Result<Temporal, Text> withTime(borrow Temporal localTime)

readonly int CalendarFields.era()
readonly int CalendarFields.year()
readonly int CalendarFields.extendedYear()
readonly int CalendarFields.month()
readonly int CalendarFields.day()
readonly int CalendarFields.dayOfYear()
readonly int CalendarFields.hour()
readonly int CalendarFields.minute()
readonly int CalendarFields.second()
readonly int CalendarFields.nanosecond()
readonly int CalendarFields.offsetSeconds()
```

`Temporal` is one private-state affine value type; `kind()` identifies an
`instant`, `duration`, `period`, `localDate`, `localTime`, `localDateTime`,
`offsetDateTime`, or `zonedDateTime`. It is a purpose-built general temporal
model, not a claim of exact Java API parity. Each value owns its state; conversion
and arithmetic methods return independent values and do not mutate their input.
`calendar()` and `zoneId()` return empty text when the kind has no calendar or
zone. `durationSeconds()` and the `period*()` accessors apply only to their
matching kind; `hour()`, `minute()`, and `second()` apply to time-bearing kinds.
`epochSecond()` is available for instants and offset/zoned date-times.

`instant` stores an exact signed 64-bit epoch-second and a nanosecond in
0..999,999,999. `duration(seconds, nanoAdjustment)` normalizes the adjustment
using floor-based seconds plus a nonnegative nanosecond remainder; overflow is
`Err`. Arithmetic, comparison, and identity use the integer pair directly, never
a floating-point or millisecond representation. `period` instead stores
independent calendar years, months, and days. Months supplied to civil
constructors are 1 through 12; `era` and `year` are the selected ICU calendar's
era and year-of-era. `CalendarFields.extendedYear()` provides ICU's extended-year
field. For example, Japanese-era boundaries and year values are validated
strictly by ICU.

`availableCalendars()` returns the supported selectable IDs: `gregorian`,
`japanese`, `buddhist`, `roc` (Minguo), `islamic`, `islamic-civil`, and
`islamic-umalqura`. These are ICU calendar systems, not hand-maintained locale
approximations. `fields()` returns the selected calendar's era/year/month/day,
time, nanosecond, and offset; it returns `Err` for kinds without civil date
fields. `withCalendar()` converts an existing civil view, or re-expresses an
offset/zoned instant in the selected calendar without changing its instant.
`withTime()` combines a `LocalDate` with a `LocalTime` using the date's calendar.

`plusDuration()` adds elapsed seconds and nanoseconds to an `Instant`,
`OffsetDateTime`, `ZonedDateTime`, or `LocalDateTime`; it adds to a `Duration`
as well. For `LocalTime` it wraps modulo 24 hours, since no date is present.
`plusPeriod()` uses ICU calendar arithmetic in year, month, then day order and
accepts date-bearing local, offset, or zoned values.
Every nonzero component must leave an exactly representable ICU millisecond value;
an out-of-range intermediate is `Err` even if a later component would return
inside the range. Successful arithmetic preserves the submillisecond remainder.
A DST gap is `Err`; `preferLaterOffset` selects the later instant in an overlap, while
`false` selects the earlier one. `atZone()` interprets a `LocalDate` at midnight
or a `LocalDateTime` in the requested known ICU system zone, rejecting gaps and
using the same explicit overlap policy. `withZone()` and `withOffset()` convert
an instant to a zoned or fixed-offset representation without changing it.
`offsetDateTime()` accepts fixed offsets from -64,800 through +64,800 seconds.

`parseIso()` strictly consumes ISO date, time, local date-time, offset
date-time, or zoned date-time text. Offset date-times use an explicit `Z` or
numeric offset; a bracketed zone additionally requires an offset that agrees
with that zone at the parsed local time. Local date/time forms have no suffix.
Fractional seconds preserve one through nine input digits exactly. Expanded
years use a sign and six digits outside the four-digit range. `formatIso()`
formats the exact fraction with insignificant trailing zeros omitted. Date-bearing
forms use the proleptic Gregorian ISO chronology even when the stored civil
calendar is alternate. `compareTo()` returns -1, 0, or 1 only for compatible
domains: two timeline values, two durations, matching local-date kinds with the same calendar, or two local times.

`format(pattern, locale)` uses ICU date-pattern syntax and the receiver's
selected calendar. Instants are displayed in UTC, offset and zoned values use
their stored offset or system zone, and local values remain local; local values
cannot format zone fields. Date-only and time-only values reject patterns that
request unavailable fields. Durations and periods are not calendar dates and
return `Err` for date-pattern formatting. ICU locale digits are retained, and
fractional-second `S` fields support one through nine exact nanosecond digits.
The pattern and locale are borrowed and formatting does not mutate the receiver.

`parseIso()` also accepts ISO-style durations and periods. Forms without `T`
(including `P1D`) are calendar periods; forms containing `T` are elapsed
durations, with a day component before `T` elapsed as well. Thus an elapsed day
may be written as `PT24H` or `P1DT0S`, while `P1D` is a one-day calendar period.
Duration parsing preserves signed nanosecond carries and checks signed 64-bit
range; period components are independently signed and checked as signed 32-bit
values, matching `formatIso()`.
`formatIso()` writes negative durations with one leading minus; period components keep independent signs.

ICU represents calendar instants as a double millisecond value. ICU-backed
civil conversion is limited to exact integer milliseconds in the inclusive
range -9,007,199,254,740,991 through 9,007,199,254,740,991.
Both the UTC instant and its offset-adjusted local milliseconds must be inside
that range before ICU civil fields or ISO formatting are accepted.
Temporal values retain exact nanoseconds outside that range, but calendar lookup, zone conversion,
`fields()`, or `formatIso()` can return `Err` when ICU cannot represent the
requested civil instant exactly. Instant/duration arithmetic itself remains
checked integer arithmetic across the signed 64-bit seconds range.
Nanosecond carry is applied before the final seconds-range check, so a normalized
result at either signed boundary is accepted when representable.
`parseIso` uses Gregorian ISO fields and does not infer the host locale or time zone.
Unknown calendars/zones, invalid eras or civil fields, gaps, incompatible
comparisons, and arithmetic overflow return `Err`; there is no silent rounding,
fallback calendar, or default-zone lookup. `DateTime` retains its existing
millisecond contract and behavior.


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

## Atomics, semaphores, latches, barriers, and conditions

`AtomicInt`, `AtomicLong`, and `AtomicBoolean` store their values in C11 atomics.
Their load, store, exchange, compare-exchange, and integer fetch operations use
sequentially consistent ordering. `compareExchange(expected, desired)` returns
the value observed before the operation, whether or not it matched `expected`.
`cloneHandle()` explicitly shares the atomic control block.

`Semaphore.create` accepts a nonnegative permit count. `acquire(timeoutMs)` accepts
`-1` for an unbounded wait, `0` for a nonblocking attempt, or a positive monotonic
millisecond timeout; values below `-1` are errors. It returns `Ok(true)` after
consuming a permit and `Ok(false)` when a zero poll finds none or a positive timeout expires.
`release(count)` rejects negative counts and reports permit overflow. `CountDownLatch` accepts a
nonnegative count; `countDown()` saturates at zero, while `wait(timeoutMs)` uses
the same timeout and boolean result convention as `Semaphore.acquire`.

`cloneHandle()` on `Semaphore`, `CountDownLatch`, and `Barrier` shares the
control and its wait-condition node. A cloned `Condition<T>` shares its original
condition node and retains the associated mutex control. Condition nodes are
released when the final `Condition<T>` handle, including clones, is dropped, even
if the associated mutex remains alive. A `Guard<T>` independently retains the mutex
control, so dropping the last condition handle while holding that guard does not
reacquire the mutex. Repeated create/drop cycles therefore do not accumulate
condition nodes. Private semaphore, latch, barrier, future, and pool nodes remain
owned by their synchronization control.

`Barrier.create` requires at least one party. `wait(timeoutMs)` accepts `-1` for
an unbounded wait, `0` for an immediate poll that breaks the incomplete generation,
or a positive monotonic millisecond timeout; values below `-1` are errors. Each
generation returns a distinct arrival index. A timeout or zero-timeout arrival
breaks only that generation and wakes its other waiters with `Err`; a successfully
released generation remains successful for its waiters even if a later generation
breaks before they resume. `reset()` succeeds only after the broken generation's
waiters have left. A spurious condition wake is not a predicate result.

`Condition<T>` is created from a `Mutex<T>` and accepts only a `Guard<T>` belonging
to that same mutex. `wait(guard, timeoutMs)` accepts `-1` for an unbounded wait,
`0` for an immediate poll, or a positive monotonic millisecond timeout; values
below `-1` are errors. It atomically releases and reacquires the guard's mutex,
including for a zero-timeout poll; it returns `Ok(false)` on timeout and `Ok(true)`
on any notification. Spurious notifications are possible, so always recheck the
protected predicate while holding the guard. `signal()` and `broadcast()` do not
acquire the mutex: update the predicate under its guard and notify before releasing
that guard. Synchronizer timeout conventions intentionally differ from networking:
here `-1` is unbounded and `0` polls, while network APIs use `0` for unbounded waits.
Synchronization constructors and fallible operations report pthread/range errors
as `Text`.



## Cancellation, futures, and thread pools

Cancellation is cooperative: `CancellationToken.request()` sets a shared flag and
`isCancelled()` reads it. A task must check the borrowed token and return promptly
if it wishes to stop. No API kills a worker or promises bounded shutdown for a
noncooperative task.

```java
interface CancelableTask<T> {
    Result<T, Text> run(borrow CancellationToken token);
}
static Result<Future<T>, Text> Future.spawn(CancelableTask<T> task)
Result<Option<T>, Text> Future.await(long timeoutMs)
Result<Unit, Text> Future.cancel()
readonly boolean Future.isDone()

static Result<ThreadPool<T>, Text> ThreadPool.create(int workers, long queueCapacity)
Result<Future<T>, SubmitError<T>> ThreadPool.submit(CancelableTask<T> task)
Result<Future<T>, SubmitError<T>> ThreadPool.schedule(CancelableTask<T> task, long delayMs)
Result<Future<T>, SubmitError<T>> ThreadPool.scheduleAtFixedRate(CancelableTask<T> task, long initialDelayMs, long intervalMs)
Result<Future<T>, SubmitError<T>> ThreadPool.scheduleWithFixedDelay(CancelableTask<T> task, long initialDelayMs, long intervalMs)
Result<Unit, Text> ThreadPool.shutdown(boolean drain)
enum SubmitError<T> { Closed(CancelableTask<T>); Full(CancelableTask<T>); System(Text, CancelableTask<T>); }
```

`Future.await(timeoutMs)` accepts `-1` for an unbounded wait, `0` for a nonblocking
poll, or a positive monotonic millisecond timeout; values below `-1` are errors.
It returns `Ok(None)` when a zero-timeout poll or positive timeout expires, and
`Ok(Some(value))` when the task succeeds. A task's `Err(Text)` is returned as the
future's `Err(Text)`. The affine result is transferred exactly once; later completed
awaits return an error. Dropping a standalone future requests
cancellation and joins its worker, so drop may block until a noncooperative task
finishes. Dropping a pool future does not join a worker; the pool owns queued and
running work.

The prefix `await future` and postfix `future.await` expressions are shorthand
for `future.await(-1L)`. They accept an owned `Future<T>` place (a local
variable or writable field), reject other types and temporary receivers, and
block the calling operating-system thread until completion. They return
`Result<Option<T>, Text>` and do not suspend the Nacre task or create an
asynchronous frame. The caller's stack and borrows remain live during the wait.
The `async` function and method modifier is unsupported; use `Future.spawn` or a
`ThreadPool` to run `CancelableTask<T>` work concurrently.

`Future.spawn` consumes and drops its task if synchronization setup or worker
creation fails.

Thread-pool submissions reject immediately when the pool is closed or its bounded
capacity is full. Every `SubmitError` variant contains the original task for
recovery. `schedule` is one-shot with a nonnegative monotonic delay. Recurring
schedules require a nonnegative initial delay and a strictly positive interval;
invalid values and deadline overflow return `SubmitError.System` with the
original task. A deadline overflow while rearming ends the accepted future with
`Err(Text)`.

`scheduleAtFixedRate` anchors its first deadline at the initial delay and advances
each later deadline by one interval from the prior scheduled deadline. If a run
finishes late, missed runs are not skipped: that same task runs again immediately
and sequentially until it catches up. It never overlaps itself. A task that
consistently takes at least one interval can therefore run back-to-back and occupy
a worker continuously. `scheduleWithFixedDelay` instead anchors its next deadline
one interval after the callback returns successfully; it also never overlaps itself.
Successful-result destruction is sequential with invocations but does not move
that deadline. If disposal takes longer than the interval, the next invocation
becomes eligible immediately after disposal.

A recurring task owns one task instance until it terminates. Each successful
invocation's `T` value is dropped; the recurring `Future<T>` never completes with
`Ok(Some(T))`. A task error terminates the recurrence and is returned by
`Future.await`. `Future.cancel` requests cancellation through the task's token.
If a running task returns success after cancellation, that value is dropped and
the future ends with a cancellation error; a task error returned by that run is
preserved. Cancellation cannot stop a noncooperative invocation.

Each accepted recurring task reserves one queue-capacity slot for its lifetime,
including while its invocation is running, so its next run can always be queued.
Ordinary queued tasks use the remaining capacity. This may return `Full` while a
recurring invocation is active even if the waiting queue is otherwise empty.

The first `shutdown` call selects the shutdown mode. `drain=true` runs queued
one-shot jobs (including delayed jobs when due), cancels queued recurring jobs,
and lets each active recurring invocation finish once without rearming it.
`drain=false` cancels all queued jobs and likewise prevents active recurrences
from rearming. Neither mode waits for a recurrence's next deadline or permits it
to run forever. Both modes wait for current invocations and join every worker;
they do not kill or cancel an already-running callback. Pool drop uses the
cancel-queued mode and can block for noncooperative running tasks.
`Future<T>` and `ThreadPool<T>` require recursively sendable task and result
payloads. Shared synchronizer lifetime uses the existing explicit control handles;
ordinary objects are never implicitly cloned or garbage-collected.
An empty pool can be created and shut down without a local `CancelableTask<T>`
implementation. Abstract interfaces with no local implementers are opaque type
boundaries, not ownership cycles; concrete interface implementations still
participate in the finite-construction check.

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

## Networking

All socket operations are synchronous and blocking at the API level, with
nonblocking readiness handling underneath where needed. Socket handles are unique
affine owners; there is no descriptor clone, background I/O thread, or
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
unbounded wait (unlike synchronizers, where zero polls and `-1` is unbounded), a
positive monotonic millisecond timeout, and return `Err` for negative values. A timeout
closes an in-progress connect descriptor. TCP reads append at most `maxBytes`
and no more than 64 KiB per call. They return a positive count, `0` for orderly
EOF, or `Err` (including timeout); a zero maximum returns zero immediately.
`writeSome` returns one partial count. `writeAll` retries partial progress against
one end-to-end deadline; `Err` can follow bytes already sent. A peer close reports
an error rather than raising process-wide SIGPIPE.

`UdpSocket.sendTo` sends one datagram; it does not retry or split a message and a
short send is an error. `receiveFrom` consumes one datagram and appends at most
`maxBytes` (capped at 64 KiB per call). It reports the byte count, owned source
address, and truncation status. A zero-length datagram is a successful
`Datagram` with
`bytesRead() == 0`, not stream EOF. Oversized datagrams are consumed once and
reported as truncated. Both UDP methods use the same timeout convention as TCP;
timeout is `Err`. No automatic datagram retry or network service discovery is
provided.

Both TCP and UDP receive methods clamp the requested limit to 64 KiB before
checking append-length arithmetic, so a large positive `maxBytes` remains valid
when the destination already contains data.

## HTTP(S)

`HttpRequest` and `HttpResponse` are affine values backed by libcurl. A generated
program links libcurl only when it calls this API. Each request owns its own easy
handle; `send` may be called again on the same request and produces a fresh,
independently owned response. It does not retry requests or follow redirects.

```java
static Result<HttpRequest, Text> HttpRequest.create(borrow Text method, borrow Text url)
Result<Unit, Text> HttpRequest.setHeader(borrow Text name, borrow Text value)
void HttpRequest.setBody(borrow ByteBuffer body)
Result<Unit, Text> HttpRequest.setTimeoutMillis(long timeoutMillis)
Result<Unit, Text> HttpRequest.setMaxResponseBytes(long maxResponseBytes)
Result<Unit, Text> HttpRequest.setCaFile(borrow Path caFile)
Result<HttpResponse, Text> HttpRequest.send()

readonly int HttpResponse.status()
readonly borrow ByteBuffer HttpResponse.body()
readonly Option<Text> HttpResponse.header(borrow Text name)
readonly Text HttpResponse.finalUrl()
readonly Result<Text, Text> HttpResponse.bodyText()
```

`create` accepts an HTTP method token and an absolute `http://` or `https://` URL.
Embedded NUL and URL user information are rejected. Header names must be nonempty
HTTP tokens; values reject NUL, CR/LF, and other prohibited control bytes. Repeated
`setHeader` calls append request fields. `setBody` copies the source buffer's
current bytes into the request and retains no borrow or reference.

HTTP timeouts must be nonnegative; zero means unbounded, unlike synchronizer
timeouts where zero polls and `-1` is unbounded. The default response-body limit
is 16 MiB; `setMaxResponseBytes` accepts any nonnegative byte count, with zero
allowing only an empty body. The write callback checks the bound before growing
the response buffer, so a failed oversized response never escapes in a partial
Response headers have a separate 64 KiB cumulative limit on bytes delivered to
libcurl's header callback and a limit of 1024 distinct fields. Fields from interim
status blocks such as `100 Continue` are discarded. Malformed status lines,
including prohibited control bytes in reason phrases, are rejected.
Header lookup is ASCII case-insensitive,
returns an owned copy and uses the last value for duplicate field names; values
that are not valid UTF-8 make the response malformed. `bodyText` performs strict
UTF-8 validation and returns `Err` for binary bodies that are not valid UTF-8.
Embedded NUL is valid UTF-8 and remains part of the resulting `Text`.

HTTP status codes, including non-2xx responses, are successful transport results.
TLS peer and hostname verification are always enabled, using system trust by
default; `setCaFile` selects an explicit local CA file without disabling verification.
Redirects are disabled, so caller-supplied `Authorization` or `Cookie` fields are
never forwarded to another origin. The HTTP API does not add TLS to `TcpStream`
or `UdpSocket`; general URI operations are provided separately by `Uri`.


## URI and DNS

```java
static Result<Uri, Text> Uri.parse(borrow Text text)
static Result<Uri, Text> Uri.fromIri(borrow Text iri)
readonly Uri Uri.clone()
readonly Text Uri.toText()
readonly Option<Text> Uri.scheme(), Uri.userInfo(), Uri.host(), Uri.query(), Uri.fragment()
readonly Text Uri.path()
readonly Result<Option<int>, Text> Uri.port()
readonly boolean Uri.isAbsolute()
readonly Result<Uri, Text> Uri.normalize()
readonly Result<Uri, Text> Uri.resolve(borrow Uri reference)
readonly Result<Uri, Text> Uri.relativize(borrow Uri target)
readonly boolean Uri.equals(borrow Uri other)
static Text Uri.encodeComponent(borrow Text input)
static Result<Text, Text> Uri.decodeComponent(borrow Text input)
static Result<ByteBuffer, Text> Uri.percentDecode(borrow Text input)

static Result<Text, Text> Dns.idna(borrow Text host)
static Result<List<SocketAddress>, Text> Dns.resolve(borrow Text host, int port)
static Result<Text, Text> Dns.reverse(borrow SocketAddress address)
static Result<Text, Text> Dns.localHostName()
```

`Uri.parse` accepts a length-delimited, NUL-free ASCII RFC 3986 URI; it rejects
raw whitespace, non-ASCII bytes, malformed percent escapes, and invalid URI
syntax. `Uri.fromIri` percent-encodes non-ASCII UTF-8 bytes before applying the
same URI syntax checks; it is not a separate IRI parser. URI objects own their
serialized input. Getters return the encoded component spelling (the IPv6
brackets are excluded from `host`); an absent optional component is `None`,
while a present-but-empty query or fragment is `Some(Text(""))`. `port` returns
`None` for an absent or empty port, and `Err` for a nonnumeric or out-of-range
port. `equals` compares serialized URI bytes exactly. Normalization is explicit;
`resolve` requires an absolute base; `relativize` requires absolute base and
target URIs. Both return owned URIs or an error. Getters return owned Text values,
not borrows into URI storage; these values remain valid after dropping the URI.

Component encoding preserves only RFC 3986 unreserved ASCII bytes and emits
uppercase percent escapes for all other UTF-8 bytes. `decodeComponent` strictly
decodes percent escapes and requires the decoded bytes to be valid UTF-8;
`percentDecode` instead returns arbitrary decoded bytes in a `ByteBuffer`,
including NUL and invalid UTF-8. A literal `+` is never treated as a space.

`Dns.idna` percent-decodes host bytes, applies ICU UTS #46 nontransitional
ToASCII processing with STD3, BIDI, and ContextJ checks, preserves a trailing
root dot, and rejects encoded inputs longer than 3072 bytes or decoded inputs
longer than 1024 bytes. Canonical IPv4 and IPv6 literals are normalized directly;
`Dns.idna` rejects noncanonical numeric IPv4 spellings (including shortened,
octal, and hexadecimal forms) to avoid resolver reinterpretation. IPv6 zone
identifiers are retained after the address, so URI authority hosts must encode
their leading percent as `%25`. `Dns.resolve` normalizes through `Dns.idna` and
delegates to `SocketAddress.resolve` with passive lookup disabled. It returns all
resolved addresses and is synchronous, without a timeout; any connection deadline
begins only after resolution completes.
`Dns.reverse` requires a name lookup (it does not fall back to a numeric
address), and `localHostName` returns the system hostname.

URI operations require uriparser development headers and `liburiparser`; IDNA
requires ICU. Native emission follows the requested class and method metadata,
without a separate exhaustive URI-method list or another expression-tree walk.
DNS helper requests are supplied by the caller; DNS does not depend on higher-level
transport types.

## TLS

`TlsConfig`, `TlsStream`, `WebSocket`, and `WebSocketMessage` are compiler-owned
native prelude types; their names are reserved and cannot be redeclared by
application code.

```java
static Result<TlsConfig, Text> TlsConfig.client()
static Result<TlsConfig, Text> TlsConfig.clientWithCa(borrow Path caFile)
static Result<TlsConfig, Text> TlsConfig.clientWithIdentity(
    Certificate certificate, CryptoKey key, borrow Path caFile)
static Result<TlsConfig, Text> TlsConfig.server(Certificate certificate, CryptoKey key)
static Result<TlsConfig, Text> TlsConfig.serverWithClientAuth(
    Certificate certificate, CryptoKey key, borrow Path caFile)

static Result<TlsStream, Text> TlsStream.connect(
    borrow SocketAddress address, Mutex<TlsConfig> config, borrow Text host, long timeoutMs)
static Result<TlsStream, Text> TlsStream.accept(
    borrow mut TcpListener listener, Mutex<TlsConfig> config, long timeoutMs)
Result<long, Text> TlsStream.readSome(borrow mut ByteBuffer destination, long maxBytes, long timeoutMs)
Result<long, Text> TlsStream.writeSome(borrow ByteBuffer source, long offset, long maxBytes, long timeoutMs)
Result<Unit, Text> TlsStream.writeAll(borrow ByteBuffer source, long timeoutMs)
Result<long, Text> TlsStream.read(borrow mut ByteBuffer destination, long maxBytes)
Result<long, Text> TlsStream.write(borrow ByteBuffer source, long offset, long maxBytes)
Result<Unit, Text> TlsStream.flush()
readonly Result<Text, Text> TlsStream.cipher()
readonly Result<Text, Text> TlsStream.protocol()
readonly Result<Option<Certificate>, Text> TlsStream.peerCertificate()
Result<Unit, Text> TlsStream.close()
```

Client configurations use system trust or an explicit CA file and verify the peer.
`connect` uses `host` for SNI and certificate identity verification, independent of
the numeric `SocketAddress`. DNS hosts must match `dNSName` subject alternative
names; wildcards are restricted to a complete leftmost label, and Common Name
fallback is disabled. IP hosts must match `iPAddress` subject alternative names.
Configurations that take a certificate and private key own those identities.
`serverWithClientAuth` also requires and verifies client certificates. A stream
consumes one `Mutex<TlsConfig>` handle; callers who need to use a configuration
for more than one stream must explicitly clone the handle with the mutex API. The
TLS context and its owned identity remain alive until all such handles have been dropped.

TLS connect, accept, handshake, read, and write timeouts use the same monotonic
deadline convention as TCP: zero is unbounded, positive values are milliseconds,
and negative values return `Err`. A positive timeout is checked before each TLS
operation, including operations that otherwise succeed immediately. Reads append to
the destination. `writeSome` can report partial progress; `writeAll` uses one
end-to-end deadline, and an error may follow bytes already sent. `TlsStream.close`
uses a separate 250 ms deadline for TLS shutdown before releasing the transport.
`cipher`, `protocol`, and `peerCertificate` return owned results; the peer certificate
is `None` when the remote side supplied no certificate. A returned `Certificate`
owns independent copies of the leaf and every available peer-chain certificate in
their supplied order, omitting a duplicate leaf if OpenSSL includes it in that
chain. It retains no borrowed SSL pointers and its `toPemChain()` output remains
available after the originating stream and configuration are closed or dropped.

TLS operations clear their thread-local OpenSSL error queue before each SSL
attempt, so a failed unrelated configuration operation cannot poison a later
read, write, or handshake. Socket-generated SIGPIPE is handled with a scoped
thread signal mask; the library does not change the process-wide disposition,
and preserves the calling thread's prior mask and already-pending SIGPIPE.

## WebSocket

```java
static Result<WebSocket, Text> WebSocket.connect(
    borrow Uri uri, long timeoutMs, long maxMessageBytes)
static Result<WebSocket, Text> WebSocket.connectTls(
    borrow Uri uri, Mutex<TlsConfig> config, long timeoutMs, long maxMessageBytes)
Result<Unit, Text> WebSocket.sendText(borrow Text text, long timeoutMs)
Result<Unit, Text> WebSocket.sendBinary(borrow ByteBuffer bytes, long timeoutMs)
Result<Unit, Text> WebSocket.ping(borrow ByteBuffer payload, long timeoutMs)
Result<Option<WebSocketMessage>, Text> WebSocket.receive(long timeoutMs)
Result<Unit, Text> WebSocket.close(int code, borrow Text reason, long timeoutMs)
readonly boolean WebSocketMessage.isText()
readonly borrow ByteBuffer WebSocketMessage.body()
readonly Result<Text, Text> WebSocketMessage.asText()
```

`connect` accepts `ws` URIs; `connectTls` accepts `wss` URIs and consumes one
explicit TLS configuration handle. Both perform the HTTP upgrade and validate the
peer response before returning an affine connection. `maxMessageBytes` limits an
incoming message. `receive` returns `Ok(None)` after an orderly close; protocol,
UTF-8, I/O, and timeout failures are owned `Err(Text)` values. Fragmented data
frames are reassembled before a message is returned; incoming ping frames are
answered with Pong. `close` sends a close frame and completes the peer close
handshake, then releases the transport.
Each positive WebSocket timeout is one monotonic deadline through frame and control
processing, including WSS TLS shutdown; zero means unbounded waiting.

Network, TLS, and WebSocket operations share the same deadline-checking helper.
The emission coordinator requests lower-level resolver and readiness helpers;
those modules do not enumerate higher-level TLS or WebSocket methods.

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
