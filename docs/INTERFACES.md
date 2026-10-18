# Interfaces

An interface is a nominal contract for instance methods. A class implements an
interface only when it names that interface and provides every required method
with an exactly matching signature.

## Declaration and conformance

```java
interface Reader {
    public readonly int read();
}
interface Writer {
    public void write(int value);
}

class Counter implements Reader, Writer {
    private int value = 0;
    public readonly int read() { return value; }
    public void write(int next) { value = next; }
}
```

An interface contains only public instance-method signatures terminated by `;`.
It cannot declare fields, constructors, static or private methods, method bodies,
or default methods. Method names must be distinct; method overloading is not
supported. Empty interfaces are permitted.

A class must explicitly list each implemented interface after `implements`.
The list cannot repeat an interface or name a concrete class. Declarations are
collected across the source bundle before conformance is checked, so a class and
its interface may appear in separate files or in either declaration order.

Every implementation method must be public, non-static, and match its interface
method exactly: method name, parameter count and types, parameter ownership or
borrowing mode, return type, and `readonly` status. There is no signature
variance, including covariant returns or strengthening a mutable method to
`readonly`. Generic interfaces are checked after the relevant types are
specialized.

Interfaces do not extend other interfaces. Classes do not inherit from other
classes. There is no structural conformance, interface-to-interface conversion,
cast, or `instanceof` operation.

## Owned interface values

At an owning boundary—such as a local declaration, assignment, owned argument,
return, or field—a concrete value can be implicitly upcast to an interface it
implements:

```java
void inspect(borrow Reader reader) {
    println(reader.read());
}

void main() {
    Counter counter = new Counter();
    inspect(counter);       // Borrows counter; it remains usable here.
    Reader reader = counter; // Moves counter into an owned interface value.
    println(reader.read());
}
```

After the upcast, only the interface's declared methods are available through
that static type. The dynamic object remains its original concrete class.
Moving an interface value transfers the one concrete cleanup obligation; it does
not clone the object or create a second owner. Replacing or leaving the scope of
an owned interface value invokes the concrete destructor exactly once, including
cleanup of its owned fields.

Owned interface fields can be replaced by typed assignment; the previous concrete
value is destroyed once. Arbitrary extraction and partial moves from fields
remain unsupported. Definite initialization, move, drop, and control-flow rules
are the same as for other affine values.

## Borrowed interface values

At a borrowed boundary, the conversion preserves the source place and its
mutability:

```java
void inspect(borrow Reader reader) {
    println(reader.read());
}

void update(borrow mut Writer writer) {
    writer.write(42);
}
```

Calling `inspect(counter)` borrows `counter`; it remains usable after the call.
A local `borrow Reader reader = counter` keeps the shared loan through its last
use on each control-flow path, then releases it before scope exit when possible.
`borrow mut Writer` is exclusive. An upcast cannot promote a shared borrow to
exclusive access or turn a borrow into an owner.

Concrete access and an interface borrow of the same object remain in the same
alias domain. The compiler tracks the original place, reborrows, and receiver
reservations across interface dispatch. Shared arguments may alias; conflicting
access through an exclusive borrow is rejected even when the static types differ.
Receiver-bound borrowed returns must obey the same origin and mutability rules as
their interface method contracts.

A local view—a class with borrowed fields—cannot implement an interface. It
cannot be hidden behind an owned polymorphic value or exported with its borrowed
fields.

## Interfaces in collections

Generic collections can own interface values:

```java
void main() {
    List<Reader> readers = new List<Reader>();
    readers.add(new Counter());
    borrow Reader reader = readers.getRef(0);
    println(reader.read());
}
```

`add` consumes the interface value. Transfer operations such as `pop` and
`remove` return ownership to the caller; `clear`, replacement, and collection
destruction dynamically destroy any values still stored. Copied `get` is
unavailable for affine interface elements. `getRef` and `getMut` provide
origin-tracked borrowed interface values without extracting or allocating an
owner. See [BORROWING.md](BORROWING.md) and [STDLIB.md](STDLIB.md) for collection
and loan contracts.

## Native representation

The current C backend represents an interface value with a data pointer and a
pointer to a static method table. This is an implementation detail, not a stable
public ABI. Owned and borrowed interface values use the same general
representation, while the frontend tracks ownership and access separately.

An upcast does not allocate an envelope or add a reference count. Moving an
interface value transfers its pointers and clears the source for internal
cleanup; that cleared state is not a source-language `null`. A method table
contains typed adapters for the concrete methods and destructor. Interface
dispatch calls through the table; no cast between incompatible C function
pointers is required.

## Diagnostics

| Code | Meaning |
|---|---|
| `E4001` | Invalid interface declaration or `implements` list. |
| `E4002` | Required interface method is missing. |
| `E4003` | Implementation method has an incompatible signature. |
| `E4004` | An interface is used with `new`. |
| `E4005` | A local view attempts to implement an interface. |

Move errors use `E2001`/`E2002`; borrow conflicts use `E3001`/`E3002`/`E3003`.
Other nominal type mismatches use the existing type diagnostics.
