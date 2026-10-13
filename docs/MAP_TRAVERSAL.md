# Built-in map traversal

Target release: Nacre 0.12.0, experimental pre-1.0 and not a stability promise.

```java
for (int key, int value : counts) { println(key); println(value); }
for (int key, borrow Counter value : counters) { println(value.value()); }
for (int key, borrow mut Counter value : counters) { value.increment(); }
```

- A map header has two typed bindings separated by a comma. Integer keys use
  `int key`; owned Text keys use shared `borrow Text key`. Values copy primitive
  representations (`byte`, `char`, `int`, `long`, `double`, `Unit`, `boolean`,
  `String`) or explicitly use `borrow V` / `borrow mut V` for affine values.
- Each live entry is visited once; empty maps visit none. Order is unspecified,
  including after growth, deletion or reinsertion. Integer keys are copies;
  altering a local integer key does not alter the map. Text keys stay immutable.
  See [TEXT_MAP_KEYS.md](TEXT_MAP_KEYS.md) for borrowed key/value pairs.
- Evaluate the collection expression once. Require a stable owner place or an
  existing borrowed return; reject temporary owned maps.
- Copied/shared traversal holds a shared whole-map/root loan. Exclusive value
  traversal holds an exclusive whole-map/root loan; it may mutate values through
  the current binding but not read/mutate the map through another alias.
- Insertion, replacement, removal (including absence), clear, movement, destruction
  and ancestor replacement remain rejected during traversal. There is no per-key
  alias domain. Nested shared traversal is allowed; conflicting traversal is not.
- Bindings last for one iteration. `continue`, `break`, `return` and nested loops
  use existing lexical cleanup. A value cannot escape a local owner or retain an
  iteration binding into another iteration. Existing receiver-derived borrowed
  returns remain valid when their root outlives the call; early return ends the
  traversal and retains the returned root loan at the caller. Mutation is
  permitted again after the loop and all surviving result loans end.
- Scan occupied storage directly without allocating an iterator, copying affine
  owners, rehashing keys for value access, or cloning the user-body AST.
- Built-in traversal is not a first-class iterator API. Custom keys, NLL,
  lifetime parameters, disjoint-entry borrowing, fallible allocation and exception
  unwinding remain excluded.

The lending desk retains ordered ID lists for insertion-ordered listings and
persistence. Order-independent summaries may use map traversal.

Acceptance covers signed keys, empty maps, growth and shifted collision chains,
exact entry coverage without depending on order, all value representations,
evaluated-once borrowed getters, generic headers, nested loops and early exits,
iteration/owner cleanup, key-copy independence, and rejected alias/escape/type
cases. Actual generated C programs must pass GCC/Clang ASan/UBSan. Syntax queries,
snippets, actual editor diagnostics/overlays and remote CI must also pass.
