# Ownership and lexical borrowing

Nacre separates owning values from shared/exclusive access. Ordinary objects
own their heap allocation; enums are affine inline values; interfaces use an
allocation-free data/vtable representation. Moving an owner transfers its
cleanup obligation. Borrowing never transfers ownership or adds reference counts.

## Borrow syntax

```java
borrow Contact reader = contacts.getRef(0);
borrow mut Contact editor = contacts.getMut(0);
```

A borrowed binding requires an initializer, cannot be reassigned, and lasts
until the end of its lexical block. Borrowed arguments normally last through
the call. There is no early-release operation, lifetime parameter, or non-lexical
lifetime analysis: using a reference for the last time does not end its loan.

A named owner, parameter, `this`, or owned-field projection can supply a borrow.
Explicit expressions use `borrow place` or `borrow mut place`. A temporary owner
must first be assigned to a local; it cannot supply an escaping reference.
Copyable scalar values (`byte`, `char`, `int`, `long`, `double`, `Unit`,
`boolean`, and `String`) cannot be borrowed. Nominal objects, interfaces, and
enums can be borrowed; local view objects cannot.

## Alias domains and reborrows

An owner and all transitively owned fields form one conservative alias domain.
Borrowing one child can therefore block incompatible access to its siblings or
parent. List elements share the complete list's domain, regardless of indices.

Multiple shared loans may coexist. An exclusive loan blocks unrelated reads,
writes, moves, replacement, and destruction. Reborrows retain a parent
capability: derived access may use its ancestors, while conflicting sibling or
parent access is rejected. A child loan ends at its own lexical boundary and
then restores use of the parent capability.

Shared access is transitive through owned fields. `readonly` methods cannot
modify their receiver or call modifying methods through it; readonly is not
purity, because distinct mutable parameters may still be modified.

The receiver is reserved before evaluating call arguments and activated after
arguments are evaluated. This preserves calls such as `list.add(list.get(0))`
for primitive elements while preventing destruction/replacement of the receiver
during argument evaluation. Borrowed arguments remain active through activation,
so incompatible same-root argument/receiver aliases are rejected.

Selecting a field receiver stabilizes an owned child pointer. Its whole root is
therefore frozen against exclusive access during argument/RHS evaluation, so an
ancestor setter cannot destroy that child before the pending call/write. Evaluate
the modifying expression first, then select the receiver. Whole named-owner
receivers still permit the existing two-phase primitive-update patterns.

## Receiver-bound borrowed returns

```java
class Contact {
    private Text name;
    public Contact(Text name) { this.name = name; }
    public readonly borrow Text getName() { return this.name; }
    public borrow mut Text editName() { return this.name; }
    public void setName(Text name) { this.name = name; }
}
```

Only instance methods, including interface contracts, can return borrows. Free
functions, static functions, and constructors cannot. A readonly receiver cannot
return exclusive access.

Every concrete return path must derive from `this` or a transitively owned
receiver field. Named reborrows, another receiver-bound method's result, and
borrowed match bindings retain their proven origin. Locals, owning parameters,
borrowed parameters, and a view's externally borrowed fields are not receiver
origins and cannot escape through such a return.

At the caller, the result retains a loan of the receiver's root for its lexical
lifetime. Argument loans end when the call finishes; the result's loan does not.
A returned reference may initialize a borrowed local, be passed as a borrowed
argument, or be returned by another receiver-bound method. Discarded borrowed
results end at the statement; argument results end at the outer call unless an
existing supported local view captures them.

```java
Contact contact = new Contact(new Text("Ada"));
{
    borrow Text name = contact.getName();
    name.print();
    // contact.setName(new Text("Grace")); // rejected: active shared loan
}
contact.setName(new Text("Grace"));
```

Borrowed nominal upcasts preserve origins and mutability. An exclusive result
may be downgraded to shared; shared access cannot be upgraded. Interface dispatch
must preserve the declared borrowed-return contract in each concrete body.
Generic nominal types remain invariant; no unchecked pointer conversion is used.
Method receivers still require named places: assign a returned borrow to a local
before using it as the receiver of another call.

## List element references

Affine nominal `List<T>` elements provide:

- `readonly borrow T getRef(int index)` — shared access.
- `borrow mut T getMut(int index)` — exclusive access.

Both bounds-check, allocate nothing, and neither remove nor copy an owner.
Ordinary object elements return stored pointers; inline enum elements return
addresses of their inline slots; interface elements retain the existing borrowed
fat-pointer ABI. Primitive lists instead retain copied `get(int index)` and do
not expose reference accessors.

While a conflicting element loan exists, `add`, `set`, `pop`, `remove`, `clear`,
`reserve`, replacement, movement, and destruction are rejected. A mutable element
loan also blocks independent list reads. Even a no-op reserve is a modifying
operation; different indices do not grant independent mutable domains.

## Lexical list traversal

`for (borrow T item : list)` holds a shared loan on the complete collection root.
`for (borrow mut T item : list)` holds an exclusive loan and permits mutation
through `item`, not independent access through `list` or its ancestor. Primitive
headers copy values but still freeze the list structurally for the loop.
The collection expression is evaluated once; temporary owned lists are rejected.
Each iteration ends its element binding before advancing. `continue`, `break`,
and `return` perform the same scoped cleanup as ordinary blocks and loops.
Traversal introduces no heap-allocated iterator, early release, or index-disjoint
alias domain.

`tryPop` and `tryRemove` are modifying methods even on an empty list or invalid
index, so active loans still reject them. Recoverable runtime failure does not
relax static ownership requirements. `tryGet` copies copyable scalar values only.

## Map and Set key/value references

`Map<K,V>` provides shared `getRef(key)` and exclusive `getMut(key)` access for
affine values. The result derives from the complete map root, with the same
object/inline-enum/interface representation rules as List access. Missing borrowed
lookup is fatal; `containsKey` is a presence observation, not a retained loan.

Map and Set key query declarations borrow their generic key for the call. Scalar
and static `String` specializations are passed by value; `Text`, class, interface,
and enum queries remain shared borrows and are neither consumed nor retained.
`put` and `add` consume owned keys. For equal map keys, replacement retains
the stored key, drops the incoming key, and returns the old value.
`tryRemove` drops the stored key and transfers any removed value. A custom
`Hasher<K>` receives only readonly borrowed keys; the collection owns and drops
the policy interface value.

References block conflicting put, tryRemove, clear, movement, destruction, and
ancestor-field replacement. Different keys do not grant independent alias
domains. Even an absent removal or existing-key replacement is a modifying
method. Copyable scalar values use copied `tryGet` instead of references.

Maps stored beside another field share their containing owner's root. Shared
traversal of an ID list may coexist with shared map lookup, but not mutation of
that sibling map. End those scopes before mutation; there is no NLL or early
release operation. A Catalog in the lending example follows this rule explicitly.

Two-binding map traversal holds the same root loan across the whole loop. Scalar
and static `String` keys are copied; owned `Text`, class, interface, and enum keys
are exposed as readonly shared borrows. Copyable scalar values copy, while affine
value bindings borrow occupied slots:
`for (int key, borrow V value : intMap)` and
`for (borrow Text key, borrow V value : textMap)`. `borrow mut V` permits mutation
through the current value binding, not independent map access. Bindings end per
iteration; existing loop cleanup handles all exits. Receiver-derived borrowed
returns retain the caller's root loan; local-owned values cannot escape.
[STDLIB.md](STDLIB.md) documents collection policies; [MAP_TRAVERSAL.md](MAP_TRAVERSAL.md)
defines the base traversal stability and ordering contract.

## Replacing owned fields

Ordinary assignment replaces a typed owned field. It evaluates and stabilizes
the destination receiver before the RHS, evaluates the new owner once, destroys
the old field once, and stores the new owner. Object, interface, and enum fields
follow the same ownership/type rules. Replacing an initialized constructor field
is allowed; every required field must still be initialized on constructor exit.

Visibility, transitive mutability, existing definite initialization, nominal
invariance/upcasts, and active-loan checks remain in force. The containing owner
cannot be moved, replaced, or destroyed during RHS evaluation. Borrowed fields
remain immutable, local views cannot be replaced, and arbitrary moves out of
owned fields/partial initialization are not introduced.

## Borrowed enum matching

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
// phone is still owned and retains its active variant.
```

`match (value)` still consumes an owning enum. Matching an explicitly borrowed
place or an already borrowed enum is non-consuming and follows its access mode.
All modes require exhaustive variants, correct payload arity, and no duplicate
arms. Primitive bindings copy; nominal owning payloads bind shared/exclusive
references tied to the scrutinee. Nested enum payloads use their inline address.

Borrowed matching never resets a tag or destroys borrowed payloads. Bindings
cannot outlive an arm. Payload bindings share the match's root capability;
subsequent reborrows use the same conservative conflict checks. Branches,
`return`, `break`, and `continue` retain lexical cleanup and definite-state rules.

## Local views

A class containing shared borrowed fields is a local view. Its constructor must
initialize each such field directly from a compatible shared borrowed parameter.
The typed IR records which constructor parameter supplies each field. Call sites
retain those actual argument loans for the view's lexical lifetime.

Views may read borrowed targets and own other fields, but cannot be moved, passed
as parameters, returned, reassigned, borrowed as a view, explicitly destroyed, or
contained in another owner. Their destructor cleans their owned fields, never
borrowed targets. A method returning a reference to an externally borrowed view
field cannot claim that the reference is tied to the view's owned receiver.

## Diagnostics and inspection

| Code | Meaning |
|---|---|
| `E3001` | Incompatible read/write access during a live loan. |
| `E3002` | Move, replacement, or destruction conflicts with a loan/reserved receiver. |
| `E3003` | Mutability violation, including exclusive returns from readonly receivers. |
| `E3004` | Borrowed return lacks an instance receiver or valid receiver-derived origin; temporary owner cannot supply a borrowed result. |
| `E3005` | Borrowed binding lacks initialization or is reassigned. |
| `E3007` | Unsupported escape/operation on a local view. |
| `E3099` | Borrow-checker resource limit. |

`nacre emit-borrows source.ncr` shows actual loan origins, parent capabilities,
receiver reservations/activations, and endings. `emit-flow` separately exposes
definite initialization and affine ownership flow. Neither requires a C compiler.
See [CONTACT_BOOK.md](CONTACT_BOOK.md) for the interactive end-to-end example and
`examples/invalid/contact_delete_during_borrow.ncr` for rejected invalidation.
