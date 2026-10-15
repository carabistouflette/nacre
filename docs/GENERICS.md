# Generic specialization

Nacre explicitly specializes generic classes, interfaces, and enums before type
checking. This is static nominal specialization, not Java-style runtime erasure or
a bounded trait system.

```java
class Duo<A, B> {
    A first;
    B second;
    Duo(A a, B b) { first = a; second = b; }
}
void main() {
    Duo<int, String> pair = new Duo<int, String>(42, "ok");
    println(pair.first);
    println(pair.second);
}
```

`Duo<int,String>` and `Duo<String,int>` are distinct nominal types. Applications
are invariant; whitespace around `<`, `>`, and `,` does not affect their identity.
Nested applications are supported. In type syntax `>>` closes two nested type
arguments; in expressions it is the signed right-shift operator.

`Pair<T, U>` is the prelude's reserved built-in pair type used by parenthesized
pair types `(T, U)`, tuple expressions and patterns, and iterator pair items.
Pair types and tuples are exactly binary and may nest; user declarations cannot
reuse the built-in name.

## Type parameters and applications

Declarations may have up to eight distinct type parameters. A type parameter
cannot hide a declared type name. Type arguments may be `byte`, `char`, `int`,
`long`, `double`, `Unit`, `boolean`, `String`, or a nominal class, interface, or
enum type with its own valid arguments. `void` and borrowed types cannot be type
arguments. An immutable `borrow T` in a template specializes to a readonly borrow
for nominal types and a copied value for scalar types. This adaptation applies
to substituted declarations, not direct primitive borrow syntax. `borrow mut T`
still requires a borrowable nominal type.

Every application supplies all arguments: no diamond, inferred arguments, raw
generic type, bounds, wildcard, or variance is supported. Generic functions and
methods are not supported. A static method may be called through an applied type,
such as `Box<int>.create()`; a type parameter cannot be used for static dispatch
such as `T.create()`.

## When templates are checked

All source files are parsed and resolved before specialization. Duplicate
declarations and invalid type-parameter declarations are rejected even when the
template is unused. A template body that is never instantiated is not fully
checked against concrete types.

A specialization is requested when an applied type appears in a concrete
signature, field, local declaration, construction, or another requested
specialization. The compiler checks every method of each requested
specialization, even if a method is never called. Thus specialization is on
nominal types, not on reachable methods.

For example, this template is not fully type-checked until an application is
requested:

```java
class Example<T> {
    T value() { return 42; }
}
void main() {}
```

`Example<int>` is compatible with the return type; `Example<boolean>` is not.
Operations and interface contracts are checked after substituting the concrete
type, not against bounds declared on `T`.

## Ownership after substitution

Specialization does not change ownership rules. A type parameter substituted by a
copyable scalar (`byte`, `char`, `int`, `long`, `double`, `Unit`, `boolean`, or
`String`) is copied. Substituted by a class, interface, or enum, it is an affine
owner; passing it by value transfers ownership. Borrowed types remain references
and are not cloned.

```java
class Box<T> {
    T value;
    Box(T value) { this.value = value; }
    readonly T get() { return this.value; }
}
```

`Box<int>` and `Box<String>` can return the field by value. `Box<Counter>` cannot:
the method would move an owned field out of `this`, which is unsupported. The
specialized body is rejected even if `get()` is never called. To expose an
operation on a nominal value, define a method that operates through the field
without moving it, or declare a readonly `borrow T` getter. A nominal result
keeps a lexical loan of the template instance; a scalar result is copied.

A generic immutable borrowed getter supports both kinds of specialization:

```java
class Box<T> {
    T value;
    Box(T value) { this.value = value; }
    readonly borrow T getRef() { return this.value; }
}
```

`Box<int>.getRef()` returns a copied `int`; `Box<Counter>.getRef()` returns a
readonly borrow. Mutable primitive borrows, nominal borrowed fields, local views,
and other unsupported escapes remain rejected after substitution.

## Generic interfaces

```java
interface Source<T> {
    readonly borrow T read();
}
class Constant<T> implements Source<T> {
    T value;
    Constant(T value) { this.value = value; }
    readonly borrow T read() { return this.value; }
}
```

`Source<Counter>` and `Source<OtherCounter>` are different nominal interfaces;
there is no conversion between them just because their layouts look alike.
`Constant<T>` must satisfy the interface contract after substitution. This
immutable borrowed-return example adapts scalar `T` to a copied result.
Generic interfaces use the same exact method-signature and borrowing rules as
non-generic interfaces. See [INTERFACES.md](INTERFACES.md).

## Native collection templates

Built-in generic collections are reserved nominal templates specialized on
demand. Unused native classes are not emitted. Native requests use the same
specialization bounds as user templates.

`List<T>`, `Deque<T>`, and `Array<T>` expose copied `get`/`tryGet` only when `T`
is copyable: `byte`, `char`, `int`, `long`, `double`, `Unit`, `boolean`, or
`String`. For affine nominal values, copied accessors are pruned and
`getRef`/`getMut` provide borrowed access instead. The recoverable extraction
methods on lists and deques return owners in specialized inline `Option<T>`.
`Array<T>` is fixed-length and its constructor consumes a `List<T>`.

`Map<K,V>` accepts exactly two type arguments. Default hashing covers `byte`,
`char`, `int`, `long`, `double`, `Unit`, `boolean`, static `String`, and owned
`Text`; custom class, interface, or enum keys require `Map<K,V>.withHasher`.
`Set<T>` supports the same key policies through `Set<T>.withHasher`. Custom-key
specializations prune the default constructor and retain only the explicit policy
factory; builtin specializations prune that factory. Query declarations borrow
generic keys, then specialize copyable keys to value parameters and leave
object-like keys borrowed. Copied map-value `tryGet` is available for copyable
`V`; affine values use `getRef`/`getMut`. `put` and `tryRemove` return specialized
inline `Option<V>`.

Collection values remain invariant: `Map<int,Dog>` is not
`Map<int,Animal>`. An individual owner or borrow can still be converted to an
implemented interface under the ordinary interface rules.

## Algorithm and bounds

`generics.rs` works on the AST rather than replacing source text. It registers
templates and discovers normalized specializations in a deterministic queue.
Substitution visits types, signatures, fields, constructions, and method bodies;
it does not rewrite local variable names as type names. Concrete instances then
pass through the ordinary checker, ownership analysis, and borrow analysis.
C symbols use numeric identifiers rather than source names containing `<` or
`>`.

Expansions such as `Chain<T>` containing `Chain<Chain<T>>` can produce an
unbounded sequence of specializations. Limits are 64 user-template specializations,
128 concrete user types, syntactic type depth 32, and specialized-name length 4096.
Trusted built-in types and their compiler-owned prerequisites do not consume the
user specialization/type budgets; adding standard-library helpers cannot reduce
an application's available generic budget. Depth and name-length checks still apply
to all type requests. Reaching a limit produces `E4199`. These limits are not a
general security guarantee.

## Diagnostics

| Code | Meaning |
|---|---|
| `E4101` | Invalid type parameters or unsupported diamond syntax. |
| `E4102` | Unknown template, missing arguments, or incorrect arity. |
| `E4103` | Invalid or unknown type argument, including a borrowed type argument. |
| `E4199` | A specialization or type-expansion limit was reached. |

Other errors are reported against the resulting concrete type. Source spans are
preserved; later-pass diagnostics do not yet include every possible
specialization note. See [STDLIB.md](STDLIB.md) for built-in template signatures.
