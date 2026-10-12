# Owned text map keys

Target compiler release: 0.12.0, experimental pre-1.0.

```java
Map<Text,int> counts = new Map<Text,int>();
counts.put(new Text("pear"), 2);
Text query = new Text("pear");
match (counts.tryGet(query)) { Some(count) { println(count); } None {} }
for (borrow Text key, int count : counts) { key.print(); println(count); }
```

## Ownership and API

- Supported keys are exactly `int` and owned `Text`; no static String/custom keys
  or public hash/equality extension interface is added.
- `put(K key, V value)` consumes a Text key and affine value (copies int keys and
  primitive values). A new key becomes map-owned. On equal-key replacement, retain
  the original stored key, destroy the redundant incoming Text key exactly once,
  and return the previous value in the existing inline Option<V>.
- `containsKey`, `tryGet`, `tryRemove`, `getRef`, and `getMut` take copied int keys
  or shared borrowed Text queries. A Text query is not consumed, cloned, allocated
  or retained by the call. Query construction, when necessary, is explicit.
- `tryRemove` destroys a present owned key once and transfers the value in Some;
  absence preserves storage and both owners. `clear`/map destruction drop every
  live owned key and value once. Growth/deletion relocation transfer entries
  without cloning/dropping owners; stale unoccupied bytes own nothing.
- Existing value representation/method restrictions and strict absent borrowed
  lookup remain unchanged. No new public methods or overloads are needed.

## Equality, storage and hashing

- Equality is exact byte length plus UTF-8 bytes, including empty keys and embedded
  NUL. No normalization, case folding or C-string semantics.
- Stored keys are immutable to source programs. Hash/equality read bytes directly,
  with no temporary String/Text conversion. Cache the hash for stored Text keys so
  growth/backward-shift deletion do not rescan strings; hash each query once per
  operation and reuse it for probing/insertion.
- Integer/text specializations share one probing/growth/deletion/cleanup algorithm.
  Key representation, hashing/equality and key destruction are narrowly specialized;
  do not duplicate whole map implementations or add a speculative strategy API.
- No hash-flooding resistance, traversal order or recoverable allocation guarantee
  is introduced. Capacity arithmetic and existing fatal failures stay checked.

## Borrowing and traversal

- Integer key headers still use `int key`. Text keys use exactly shared `borrow
  Text key`; owning or `borrow mut Text` key bindings are rejected.
- Values still copy primitives or explicitly borrow affine values, including
  `borrow mut V`. Text-key traversal must support both a read-only key and mutable
  value together, without granting mutable key access or weakening root rules.
- Treat the current key/value pair as one iteration capability, analogous to
  borrowed enum payload bindings. Bindings share that origin; key type remains
  readonly. Explicit further reborrows obey existing conservative root freezing.
  No distinct-entry/domain borrowing is added. Independent map access/mutation,
  replacement/removal (including absence), movement/destruction and ancestor
  replacement stay forbidden under conflicting whole-map/root loans.
- The collection is evaluated once, capacity captured once, and occupied slots
  scanned directly. No first-class iterator, key clone, per-entry hash lookup,
  owner copy, AST-body clone or new loop-cleanup implementation is introduced.
- Bindings end at iteration boundaries. Existing lexical cleanup and valid
  receiver-derived borrowed returns continue to apply.

## Acceptance

Behavior/native proof must cover empty/Unicode/NUL keys, exact equality with
independently allocated queries, replacement ownership, absent operations,
collision-chain shifting/growth, all value representations, borrowed query lifetime,
shared/exclusive paired traversal, immutable keys, root alias/escape rejection,
no extra query/iterator allocation and balanced key/value cleanup. Integer map
regressions remain unchanged except tests declaring all non-int keys invalid must
now use truly unsupported keys. Add a useful named-inventory example, update
syntax/snippets and actual desktop acceptance, and run sanitizers/toolchain/fuzz/CI.
