# Map and Set key policies

Target compiler release: 0.12.0, experimental pre-1.0.

The public collection signatures and traversal rules are in
[STDLIB.md](STDLIB.md#sett-and-mapk-v). This page records key-specific ownership,
hashing, and borrowing behavior.

```java
Map<Text, int> counts = new Map<Text, int>();
counts.put(new Text("pear"), 2);
Text query = new Text("pear");
match (counts.tryGet(query)) { Some(count) { println(count); } None {} }
for (borrow Text key, int count : counts) { key.print(); println(count); }
```

## Default keys and custom policies

Default hashing covers `byte`, `char`, `int`, `long`, `double`, `boolean`, `Unit`,
static `String`, and owned `Text`. `Double` NaNs compare/hash equally, and signed
zeros compare/hash equally. `String` and `Text` compare exact-length UTF-8 bytes,
including embedded NUL; no normalization or C-string semantics apply.

Class, interface, and enum keys use an owned `Hasher<T>`:

```java
interface Hasher<T> {
    readonly long hash(borrow T value);
    readonly boolean equals(borrow T left, borrow T right);
}

Map<Key, int> values = Map<Key, int>.withHasher(policy);
Set<Key> keys = Set<Key>.withHasher(policy);
```

Hashers receive only readonly borrowed keys. Equal keys must have equal hashes,
and a stored key's hash/equality behavior must stay stable. Custom-key
specializations require `withHasher`; builtin-key specializations retain the
zero-argument constructor and do not expose the custom factory. A concrete
Hasher implementation cannot carry local borrowed fields. Cross-thread checks
recursively inspect concrete implementations behind the policy interface.

## Ownership and lookup

- `put`/`add` consume owned keys. On equal-key Map replacement, the stored key is
  retained, the incoming key is dropped, and the previous value is returned in
  `Some`. Set duplicates drop the incoming key.
- `containsKey`, `tryRemove`, and borrowed value lookups never consume a query.
  Scalar and static `String` specializations pass copied values; `Text`, class,
  interface, and enum queries use shared borrows.
- `tryRemove` drops a present key and transfers its value in `Some`. `clear` and
  collection destruction drop live keys, values, and any owned Hasher policy.
  Growth relocates owners and cached 64-bit hashes without cloning owners or
  calling the user Hasher again.
- Stored keys are readonly during traversal. Scalar and static `String` key
  bindings are copies; `Text`, class, interface, and enum bindings are shared
  borrows. Map traversal binds a key and value together under the existing
  whole-root loan; it does not provide per-entry alias domains.

The shared open-addressed table uses checked growth and backward-shift deletion;
order and hash-flood resistance are not guaranteed. Allocation failures retain
the existing fatal behavior. Custom hashers do not introduce a second Map/Set
storage implementation.
