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
truncated tags, or authentication failure; unauthenticated plaintext is wiped and
freed before returning, and no plaintext owner is exposed. Inputs that would
narrow beyond OpenSSL's `int` lengths are rejected.

Equal-length `constantTimeEquals` comparisons use OpenSSL's constant-time
comparison primitive. Different lengths return `false` immediately, so lengths
are not hidden. `Crypto` supplies cryptographic primitives; `CryptoKey`,
`CryptoDigest`, `CryptoMac`, and `Certificate` supply key and certificate operations.
Native programs require the OpenSSL 3 development and runtime libraries.

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
static Result<Text, Text> formatCurrency(double value, borrow Text isoCurrency, borrow Text locale)
```

Number patterns follow ICU `DecimalFormat` pattern syntax and the locale's
numbering symbols. `formatNumber` formats a `double`; `formatDecimal` formats
nonempty ASCII decimal text directly, without first rounding it through `double`.
`parseNumber` uses the same pattern and locale and returns `Err` unless ICU parses
the complete input. Invalid patterns, malformed numbers, and trailing content
return `Err`. Embedded NUL is rejected in patterns, parse input, and currency
codes; an empty locale is root.

`formatCurrency` uses ICU's locale currency pattern with the explicit three-letter
ISO 4217 code. The code is validated against ICU currency data; unrecognized codes
return `Err` rather than being used as an invented currency.

## `Charset`

```java
static Result<ByteBuffer, Text> encode(borrow Text text, borrow Text charset)
static Result<Text, Text> decode(borrow ByteBuffer bytes, borrow Text charset)
```

The supported case-insensitive charset names are `UTF-8`, `UTF-16LE`,
`UTF-16BE`, `UTF-32LE`, `UTF-32BE`, `US-ASCII`, and `ISO-8859-1`. Generic
`UTF-16` and platform-default encodings are not supported. Charset names with
embedded NUL and unknown names return `Err`.

Conversions are strict: malformed input and text unrepresentable in the target
charset return `Err` without replacement characters. UTF-16LE/BE encoding does
not add a BOM; when decoding those explicit-endian encodings, a BOM is preserved
as U+FEFF text. UTF-8 conversion preserves embedded NUL. Oversized ICU conversion
inputs return `Err` before narrowing lengths to ICU's 32-bit API.
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
one certificate; `parsePemChain` owns the first certificate and its PEM
intermediates. Subject and issuer use RFC 2253 text, and the serial is decimal.
Validity accessors convert ASN.1 times to checked signed Unix seconds, including
dates before the epoch; malformed dates or intervals with `notBefore` after
`notAfter` are rejected during parsing.
Hostname checks use subject alternative names without falling back to the common
name. `verify` checks the supplied CA file trust store, chain, server purpose,
explicit Unix time, and SAN hostname; it does not fetch certificates or use the
network.

## `Database`

`Database` is an affine SQLite connection with one internally owned prepared
statement. It is a small native SQL API, not JDBC or a driver framework. A
connection cannot own multiple independent statement handles or simultaneous
result cursors; open another `Database` for another connection/cursor.

```java
static Result<Database, Text> open(borrow Text path)
Result<Unit, Text> prepare(borrow Text sql)
Result<Unit, Text> bindNull(int index)
Result<Unit, Text> bindLong(int index, long value)
Result<Unit, Text> bindDouble(int index, double value)
Result<Unit, Text> bindText(int index, borrow Text value)
Result<Unit, Text> bindBlob(int index, borrow ByteBuffer value)
Result<boolean, Text> step()
Result<Unit, Text> reset()
Result<Unit, Text> clearBindings()
readonly Result<int, Text> columnCount()
readonly Result<Text, Text> columnName(int index)
readonly Result<int, Text> columnType(int index)
readonly Result<Option<long>, Text> columnLong(int index)
readonly Result<Option<double>, Text> columnDouble(int index)
readonly Result<Option<Text>, Text> columnText(int index)
readonly Result<Option<ByteBuffer>, Text> columnBlob(int index)
readonly Result<long, Text> changes()
readonly Result<long, Text> lastInsertRowId()
Result<long, Text> execute(borrow Text sql)
Result<Unit, Text> begin()
Result<Unit, Text> commit()
Result<Unit, Text> rollback()
Result<Unit, Text> setBusyTimeoutMillis(long milliseconds)
Result<Unit, Text> close()
```

`open` accepts nonempty strict UTF-8 paths without embedded NUL; `:memory:` opens
an in-memory database. Connections use SQLite `SQLITE_OPEN_FULLMUTEX` and enable
foreign-key enforcement. SQL text is also strict UTF-8 and NUL-free. `prepare`
accepts exactly one executable statement, allowing leading empty commands and
SQLite's token-leading UTF-8 BOM whitespace. Only whitespace/comments may follow
the statement. A bounded linear lexical preflight rejects multi-statement SQL
before preparing any command on the live connection, while respecting quoted
semicolons and trigger bodies. It replaces the previous prepared statement only
after the new SQL parses. Some SQLite PRAGMAs take effect during preparation;
`prepare` is a command, not a side-effect-free query.
Failed SQL/tail validation keeps the previous statement; a failure while
finalizing that previous statement is reported after SQLite has finalized it, so
no statement remains installed. SQLite failures use generic Text diagnostics and
do not echo SQL or expose SQLite's diagnostic message.

Binding indices are one-based SQLite parameter indices; invalid indices return
`Err`. Text and ByteBuffer inputs are copied using SQLite's transient-binding
semantics, preserving embedded NUL in values. `step` returns `Ok(true)` for a row,
`Ok(false)` at completion, or `Err` on SQLite failure. Completion is terminal:
repeated `step` calls return `Ok(false)` without rerunning the statement until
`reset` or a successful `prepare`. After a failed step, another `step` returns
`Err` until `reset`. `reset` rewinds the current statement and reports a prior
evaluation error; call it before rebinding for reuse. `clearBindings` clears all
parameters.

Column indices are zero-based. `columnType` returns SQLite's documented
`SQLITE_INTEGER` (1), `SQLITE_FLOAT` (2), `SQLITE_TEXT` (3), `SQLITE_BLOB` (4), or
`SQLITE_NULL` (5) code. `columnType` and typed value accessors require a current
row. Typed accessors accept only their exact corresponding SQLite storage type:
integer, float, text, or blob. `NULL` becomes `None`; a different type or an
invalid column index returns `Err` rather than silently coercing. SQLite text
results must be valid UTF-8. Returned Text and ByteBuffer values are owned copies,
not SQLite-cell borrows, and remain valid after another step, reset, or
closing/dropping the Database.

`execute` runs one unparameterized command without result columns and returns
SQLite's 64-bit change count. For values, use `prepare` and bind parameters rather
than concatenating untrusted data into SQL. `changes` and `lastInsertRowId` report
SQLite's corresponding connection-local 64-bit values. Busy timeout milliseconds
must be in `0..=INT_MAX`; an expired lock wait returns `Err`.

`close` finalizes the internal statement and is idempotent after success. Dropping
an open connection performs the same cleanup; SQLite automatically rolls back an
uncommitted transaction on close. Programs that use `Database` require SQLite's
development headers and `libsqlite3`; unrelated programs do not link SQLite.

If automatic destruction cannot close the SQLite connection after finalizing the
owned statement, the runtime aborts rather than releasing a wrapper around live
SQLite state.

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

List traversal uses `for (int value : values)` for copied values and
`for (borrow T value : values)` / `for (borrow mut T value : values)` for affine
values. The list is evaluated once and stays loaned for the full loop; each
binding ends at the iteration boundary. `drain()` is a separate explicit ownership
transfer: it empties the list and returns an owned iterator over its former storage.
`collect()` consumes an iterator and transfers each yielded value into a new list.

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

Generic query declarations borrow keys; scalar and static `String` specializations
copy the value without an implicit clone, while object, interface, enum, and
`Text` queries use readonly borrows. `tryGet` copies only copied value types;
affine values require `getRef`/`getMut`. Absent borrowed lookup is a fatal
boundary error. `Map`
traversal uses two bindings, for example
`for (int key, borrow Value value : map)` or
`for (borrow Text key, int value : map)`. Set traversal accepts one readonly key
binding. These loops hold a whole-root loan for their complete lexical extent;
different entries do not grant disjoint mutable access. See
[MAP_TRAVERSAL.md](MAP_TRAVERSAL.md) for the base map traversal contract.


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
readonly borrow K keyAt(int index)
readonly borrow V valueAt(int index)
readonly int size()
readonly boolean isEmpty()
void clear()
readonly Option<int> floorIndex(borrow K key)
readonly Option<int> ceilingIndex(borrow K key)
readonly Option<int> lowerIndex(borrow K key)
readonly Option<int> higherIndex(borrow K key)

OrderedSet<T>(Comparator<T> comparator)
void add(T key)
readonly boolean contains(borrow T key)
boolean remove(borrow T key)
readonly int size()
readonly boolean isEmpty()
void clear()
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
replaced value in `Some`. Adding an equal set key drops the new key and retains
the existing one. `keyAt` and `valueAt` copy copied specializations and borrow
affine ones. The comparator is owned by the collection. As with other collections,
key and value types must be copied values or owned nominal values, never borrows or
local views.
Queries copy scalar keys and borrow affine keys; looking up or removing a key
does not consume an affine query object.

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
```

The queue is a binary min-heap: a value whose comparator result is negative
relative to another value has higher priority. Add and extraction preserve owned
values by transfer; `pop` and `peek` fail on an empty queue, while `tryPop` and
`tryPeek` return `None`. `clear` destroys every queued value but retains storage.
The queue owns its comparator.

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
caller. `List.drain()` transfers the list's backing storage and leaves the list
empty. Dropping an iterator early destroys all remaining owned values. `List.collect`
consumes its input iterator and builds a new list.

`MapIterator<T, U>` owns an input iterator and a `Mapper<T, U>` callback;
`FilterIterator<T>` owns an input iterator and a `Predicate<T>` callback. They are
lazy: `map` invokes the mapper only as values are requested, and `filter` advances
until a value passes or the input ends. Rejected filter values are destroyed.
`Iterators<T>.take` and `skip` lazily bound and discard a nonnegative number of
values; a negative count is fatal. All adapters own their inputs and callbacks,
and dropping an adapter recursively drops those owners. These are explicit owned
pipelines, separate from `for` traversal's whole-root lexical loans; there are no
borrowed iterators or early-release loans.

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

`File.create`, `openAppend`, and `openReadWrite` (when creating) use POSIX
permission modes subject to the process umask. `openReadWrite` opens without
truncation; `create` controls whether a missing file is created.
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
embedded NUL. Creation `mode` arguments are POSIX permission bits, subject to
umask. Filesystem commands report success as `Result<Unit, Text>`, not a boolean.

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
`name()` is the UTF-8 entry name. It is empty for attribute changes to the
watched directory itself and for other self, ignored, and overflow events. Queue
overflow is returned as `WatchKind.Overflow` with `overflow() == true`, not
silently discarded. A `DeletedSelf` or `Ignored` event invalidates the watch;
queued events remain readable, then later `next` calls return `Err`.
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
    readonly long position()
    Result<Unit, Text> writeByte(byte value)
    Result<Unit, Text> writeInt16(int value)
    Result<Unit, Text> writeInt32(int value)
    Result<Unit, Text> writeInt64(long value)
    Result<Unit, Text> writeFloat32(double value)
    Result<Unit, Text> writeDouble(double value)
    Result<Unit, Text> writeBytes(borrow ByteBuffer source)
    Result<Unit, Text> writeUtf8(borrow Text value)
    readonly borrow ByteBuffer bytes()
    static ByteBuffer finish(BinaryWriter writer)
}
static Result<long, Text> Streams.copy(borrow mut ByteSource source, borrow mut ByteSink sink, long maxBytes, long bufferSize)
static Result<ByteBuffer, Text> Streams.readUpTo(borrow mut ByteSource source, long maxBytes, long bufferSize)
static Result<ByteBuffer, Text> Streams.readAll(borrow mut ByteSource source, long maxBytes, long bufferSize)
static Result<Unit, Text> Streams.writeAll(borrow mut ByteSink sink, borrow ByteBuffer source)
```

`ByteSource` and `ByteSink` are allocation-free interfaces implemented by
`File` and `TcpStream`; each operation may transfer a partial count. `Streams`
loops over partial writes, rejects invalid counts and zero-progress writes, and
uses positive `bufferSize` to bound each transfer chunk. `copy` consumes at most
`maxBytes`, stops at EOF, and flushes the sink (including when the bound is zero).
`readUpTo` consumes at most `maxBytes` and returns the prefix at the limit
without probing for another byte. `readAll` succeeds only when it reaches EOF.
After reading `maxBytes` bytes it probes one extra byte; if present, that byte is
consumed and `Err` is returned. This also applies when `maxBytes` is zero.
`writeAll` loops until the complete buffer is sent, then flushes. Errors do not
roll back source consumption or already-written output.

`BinaryReader` uniquely owns its input buffer; `BinaryWriter` uniquely owns the
buffer it builds. Both constructors require an explicit byte order. Reads of
fixed-width values and `readBytes` leave the cursor unchanged if the requested
data is insufficient; `readUtf8` is strict UTF-8 and likewise preserves the
cursor on insufficient input or invalid encoding. `readInt16` returns a signed
`int`. Binary doubles use IEEE-754 binary64 bits copied with `memcpy`.
`readFloat32` widens an IEEE-754 binary32 value to `double`. `writeFloat32`
encodes `double` values as binary32 and rejects finite magnitudes above the
largest finite binary32 value without advancing the writer.
`writeInt16` accepts only the signed 16-bit range and leaves the writer unchanged
when out of range. `bytes()` is a borrow tied to the writer; `finish` consumes the
writer and transfers its exact buffer without copying.

The `File` stream adapters use the descriptor cursor and may return partial
counts. `File.flush` calls `fsync`. `TcpStream.read` and `write` use timeout `0`
(unbounded wait); its `flush` is a no-op and gives no guarantee of network
delivery. `Streams.copy` and `writeAll` flush their sink according to that sink's
contract, not a peer-acknowledgement guarantee.

## ZIP archives

The pinned private dependency build in `scripts/build_libarchive.py` applies the
upstream seekable-reader correction for a zero-entry archive whose end record is
at offset zero. See README for compile/runtime library paths. Both file and memory
readers use the same strict seekable format configuration.

`ZipReader` and `ZipWriter` use libarchive's ZIP support, including Zip64. They
operate on archive entries only: no method extracts to the filesystem. Entry names
are preserved as UTF-8, including absolute names and `.`/`..` components, so callers
that later extract entries must apply their own destination-path policy. Writers
permit duplicate names; entries remain in the order written and readers expose
them sequentially without merging or replacing them.

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
unsupported entry types, encrypted entries, invalid UTF-8 names, truncated data,
and decompression or CRC failures return `Err`. Use `skipEntry` to discard the
current entry explicitly. Directories have no file data.
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

ICU represents time as a double millisecond value. Calendar conversions accept
only epoch milliseconds from -9007199254740991 through 9007199254740991 so every
millisecond remains exactly representable. `durationMillis` is instead a fixed
signed `end - start` calculation, independent of calendar days, and returns
`Err` on signed overflow. `DateFields.offsetSeconds` is the total zone offset
from UTC; `isDst` reports ICU's daylight-time status. `DateFields` is an owned
affine result with primitive fields and readonly accessors; only the native
temporal factories construct it.

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
Referencing `DateTime` links ICU; the dependency-free `Time` clock and sleep
APIs remain unchanged.


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

`Semaphore.create` accepts a nonnegative permit count. `acquire(timeoutMs)` returns
`Ok(true)` after consuming a permit and `Ok(false)` when a zero or positive
monotonic timeout expires; zero is a nonblocking attempt. `release(count)` rejects
negative counts and reports permit overflow. `CountDownLatch` accepts a
nonnegative count; `countDown()` saturates at zero, while `wait(timeoutMs)` has the
same boolean timeout convention as `Semaphore.acquire`.

`Barrier.create` requires at least one party. Each generation returns a distinct
arrival index. A timeout or zero-timeout arrival breaks that generation and wakes
its other waiters with `Err`; `reset()` succeeds only after the broken generation's
waiters have left. A spurious condition wake is not a predicate result.

`Condition<T>` is created from a `Mutex<T>` and accepts only a `Guard<T>` belonging
to that same mutex.
`wait(guard, timeoutMs)` atomically releases and reacquires the guard's mutex,
including for a zero-timeout poll; it returns `Ok(false)` on timeout and `Ok(true)`
on any notification. Spurious notifications are possible, so always recheck the
protected predicate while holding the guard. `signal()` and `broadcast()` do not
acquire the mutex: update the predicate under its guard and notify before releasing
that guard.
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
Result<Unit, Text> ThreadPool.shutdown(boolean drain)
enum SubmitError<T> { Closed(CancelableTask<T>); Full(CancelableTask<T>); System(Text, CancelableTask<T>); }
```

`Future.await` returns `Ok(None)` on a zero-timeout poll or positive monotonic
timeout, and `Ok(Some(value))` when the task succeeds. A task's `Err(Text)` is
returned as the future's `Err(Text)`. The affine result is transferred exactly
once; later completed awaits return an error. Dropping a standalone future requests
cancellation and joins its worker, so drop may block until a noncooperative task
finishes. Dropping a pool future does not join a worker; the pool owns queued and
running work.

Thread-pool submissions reject immediately when the pool is closed or its bounded
queue is full. Every `SubmitError` variant contains the original task for recovery.
Scheduling is one-shot with a nonnegative monotonic delay; recurring scheduling
is not provided. The first `shutdown` call selects the shutdown mode: `drain=true`
runs queued jobs (including delayed jobs when due), while `drain=false` cancels
queued jobs; both modes wait for running tasks and join every worker. Pool drop
uses the cancel-queued mode and can block for noncooperative running tasks.
`Future<T>` and `ThreadPool<T>` require recursively sendable task and result
payloads. Shared synchronizer lifetime uses the existing explicit control handles;
ordinary objects are never implicitly cloned or garbage-collected.

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
timeout is `Err`. No automatic datagram retry or network service discovery is
provided.

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

Timeouts must be nonnegative; zero means unbounded. The default response-body limit
is 16 MiB; `setMaxResponseBytes` accepts any nonnegative byte count, with zero
allowing only an empty body. The write callback checks the bound before growing
the response buffer, so a failed oversized response never escapes in a partial
`HttpResponse`. Response headers have a separate 64 KiB total wire-byte limit and
a limit of 1024 distinct fields. Fields from interim status blocks such as `100
Continue` are discarded. Header lookup is ASCII case-insensitive, returns an
owned copy, and uses the last value for duplicate field names; values that are not
valid UTF-8 make the response malformed. `bodyText` performs strict UTF-8
validation and returns `Err` for binary bodies that are not valid UTF-8. Embedded
NUL is valid UTF-8 and remains part of the resulting `Text`.

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
longer than 1024 bytes. Numeric IPv4 and IPv6 literals are canonicalized
directly; an IPv6 zone identifier is retained after the address, so URI
authority hosts must encode its leading percent as `%25`. `Dns.resolve`
normalizes through `Dns.idna` and delegates to `SocketAddress.resolve` with
passive lookup disabled. It returns all resolved addresses and is synchronous,
without a timeout; any connection deadline begins only after resolution
completes.
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
`connect` uses `host` for SNI and certificate hostname/IP verification; it is
independent of the numeric `SocketAddress`. Configurations that take a certificate
and private key own those identities. `serverWithClientAuth` also requires and
verifies client certificates. A stream consumes one `Mutex<TlsConfig>` handle; callers
need to use a configuration for more than one stream must explicitly clone the
handle with the mutex API. The TLS context and its owned identity remain alive
until all such handles have been dropped.

TLS connect, accept, handshake, read, and write timeouts use the same monotonic
deadline convention as TCP: zero is unbounded, positive values are milliseconds,
and negative values return `Err`. A positive timeout is checked before each TLS
operation, including operations that otherwise succeed immediately. Reads append to
the destination. `writeSome` can report partial progress; `writeAll` uses one
end-to-end deadline, and an error may follow bytes already sent. `TlsStream.close`
uses a separate 250 ms deadline for TLS shutdown before releasing the transport.
`cipher`, `protocol`, and `peerCertificate` return owned results; the peer certificate
is `None` when the remote side supplied no certificate.

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
UTF-8, I/O, and timeout failures are owned `Err(Text)` values. `close` sends a
close frame and completes the peer close handshake, then releases the transport.
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
