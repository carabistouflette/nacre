# Nacre grammar

This document describes the implemented source syntax. Identifiers are ASCII;
source text, string literals, and character literals are UTF-8. Line comments
begin with `//`; block comments use `/* ... */`.

## Source units and modules

Each source file is parsed independently. An optional module declaration comes
first, followed by zero or more imports and then declarations:

```ebnf
program       = [ moduleDeclaration ] { importDeclaration } { declaration } ;
moduleDeclaration = "module" qualifiedName ";" ;
importDeclaration = "import" qualifiedName [ "." "*" ] ";" ;
qualifiedName = identifier { "." identifier } ;
declaration   = [ visibility ] ( function | class | interface | enum ) ;
visibility    = "public" | "private" ;
```

`import model.Counter;` imports one public type or function;
`import model.*;` imports public declarations in one module. Names can also be
resolved within the same module. A source unit without a header belongs to the
unnamed module. Standard-library names are reserved and available without
imports. Modules are namespaces within the explicitly selected whole-program
source bundle; they do not load files or packages. See [PROJECTS.md](PROJECTS.md).

## Declarations and types

```ebnf
class         = "class" identifier [ typeParameters ]
                [ "implements" nominalType { "," nominalType } ]
                "{" { member } "}" ;
interface     = "interface" identifier [ typeParameters ]
                "{" { interfaceMethod } "}" ;
enum          = "enum" identifier [ typeParameters ]
                "{" variant { variant } "}" ;
variant       = identifier [ "(" [ type { "," type } ] ")" ] ";" ;
typeParameters = "<" identifier { "," identifier } ">" ;
typeArguments = "<" type { "," type } ">" ;
nominalType   = qualifiedName [ typeArguments ] ;
member        = modifiers ( constructor | method | field ) ;
modifiers     = { "public" | "private" | "static" | "readonly" } ;
function      = [ "static" ] returnType identifier parameters block ;
method        = returnType identifier parameters block ;
interfaceMethod = [ "public" ] [ "readonly" ] returnType identifier
                  parameters ";" ;
constructor   = identifier parameters block ;
field         = type identifier [ "=" expression ] ";" ;
parameters    = "(" [ parameter { "," parameter } ] ")" ;
parameter     = type identifier ;
returnType    = type | "void" ;
type          = "byte" | "char" | "int" | "long" | "double"
              | "boolean" | "String" | "Unit" | nominalType | borrowedType
              | pairType ;
borrowedType  = "borrow" [ "mut" ] nominalType ;
pairType      = "(" type "," type ")" ;
```

`public` and `private` may qualify top-level declarations and class members.
Class members can combine `static` and `readonly` where the rules permit; modifier
order is flexible, duplicate modifiers are rejected. Interfaces contain public
instance signatures only. A constructor's identifier must match its enclosing
class and is written without generic arguments, for example `Box(T value)`, not
`Box<T>(...)`. An enum must declare at least one variant.

Classes, interfaces, and enums may have up to eight distinct type parameters.
Every use of an applied type supplies all arguments, including after `new`:
`new Box<int>(42)`. Generic functions and methods, inferred or omitted type
arguments, bounds, wildcards, and variance are not supported. Types are
specialized before method bodies are checked; see [GENERICS.md](GENERICS.md).

## Statements

```ebnf
block         = "{" { statement } "}" ;
statement     = localDeclaration
              | place "=" expression ";"
              | expression ";"
              | "if" "(" expression ")" block
                  [ "else" ( block | "if" "(" expression ")" block ) ]
              | "while" "(" expression ")" block
              | "for" "(" type identifier [ "," type identifier ]
                  ":" expression ")" block
              | "match" "(" expression ")" "{"
                  { matchArm [ "," ] } "}"
              | "return" [ expression ] ";"
              | "break" ";" | "continue" ";" | block ;
localDeclaration = type identifier [ "=" expression ] ";" ;
matchArm      = pattern [ "if" ( expression | "(" expression ")" ) ] block ;
pattern       = "_" | qualifiedName [ variantPattern | recordPattern ]
              | "(" pattern "," pattern ")" ;
variantPattern = "(" [ pattern { "," pattern } [ "," ] ] ")" ;
recordPattern = "{" [ fieldPattern { "," fieldPattern } [ "," ] ] "}" ;
fieldPattern  = identifier ":" pattern ;
place         = ( identifier | "this" ) { "." identifier } ;
```

All control-flow bodies require braces. A `for` header has one typed binding for
list or set traversal, or two for map traversal. Scalar and static `String` keys
are copied; owned `Text`, class, interface, and enum keys use readonly borrows.
Map value bindings use a copyable value type or `borrow V`/`borrow mut V` for affine value types. The collection expression is
evaluated once and must be a stable place; traversal holds a lexical loan on the
complete collection root. Map and set order is unspecified; `OrderedMap` and
`OrderedSet` provide comparator order. `Iterator<T>` remains an explicit owned
library API, not a second loop syntax.
Separately, a `List`, `Array`, or `Map` `.iter()` call starts a compiler-fused
chain only when it ends in `.fold(...)` or `.sizeHint()`; that chain is not an
`Iterator<T>` value or a `for` input. See [BORROWING.md](BORROWING.md) and
[STDLIB.md](STDLIB.md).

`match` requires an owned or borrowed nominal subject; local views cannot be
subjects. Enum patterns may use a bare or qualified variant name, with positional
payload patterns in parentheses; nullary variants may omit the parentheses. A
bare identifier binds unless it names a nullary variant of the subject type, and
`_` ignores a value. `Class { field: pattern, ... }` recursively matches selected
accessible fields of an ordinary class; `(first, second)` matches a `Pair`.
Parenthesized pair types, tuple expressions, and tuple patterns have exactly two
elements and may nest.

Every match must cover the full nested pattern space. A boolean `if` guard (with
optional parentheses) does not contribute to exhaustiveness; an earlier unguarded
pattern that already covers a later arm makes that arm unreachable. Variant
payload arity and record field names are checked. Matching an owned enum consumes
it; matching an explicitly borrowed enum borrows its payloads. Pattern names are
scoped to their arm. Record patterns never partially move class fields: nominal
field bindings are readonly aliases and cannot escape their lexical scope.

## Expressions and operators

```ebnf
expression       = logicalOr ;
logicalOr        = logicalAnd { "||" logicalAnd } ;
logicalAnd       = bitwiseOr { "&&" bitwiseOr } ;
bitwiseOr        = bitwiseXor { "|" bitwiseXor } ;
bitwiseXor       = bitwiseAnd { "^" bitwiseAnd } ;
bitwiseAnd       = equality { "&" equality } ;
equality         = comparison { ( "==" | "!=" ) comparison } ;
comparison       = shift { ( "<" | "<=" | ">" | ">=" ) shift } ;
shift            = additive { ( "<<" | ">>" ) additive } ;
additive         = multiplicative { ( "+" | "-" ) multiplicative } ;
multiplicative   = unary { ( "*" | "/" | "%" ) unary } ;
unary            = ( "+" | "-" | "!" | "~" ) unary
                 | "borrow" [ "mut" ] unary
                 | primitiveCast | postfix ;
primitiveCast    = "(" primitiveType ")" unary ;
primitiveType    = "byte" | "char" | "int" | "long" | "double" ;
postfix          = primary { "." identifier [ arguments ] | "?" } ;
primary          = literal | "this" | identifier [ arguments ]
                 | "(" expression ")" | "()"
                 | "(" expression "," expression ")"
                 | newValue | genericStaticCall ;
newValue         = "new" qualifiedName [ typeArguments ]
                   [ "." identifier ] arguments ;
genericStaticCall = qualifiedName typeArguments "." identifier arguments ;
arguments        = "(" [ expression { "," expression } ] ")" ;
literal          = integer | long | double | character | string
                 | "true" | "false" ;
```

The grammar's repeated operators are left-associative. The precedence is therefore
`||`, `&&`, `|`, `^`, `&`, equality, comparisons, shifts, addition/subtraction,
then multiplication/division/remainder. Unary `+`, `-`, `!`, and `~` bind more
tightly than binary operators. Unary `+` and `-` require numeric operands,
`!` requires `boolean`, and `~` requires an integer operand.

Binary operations are type-checked: arithmetic and relational operators require
numeric operands; bitwise operators require integers; `&&` and `||` require
booleans; equality accepts numeric or boolean operands. Shift counts must be
`byte`, `char`, or `int`. There is no text or object equality operator.

`borrow place` and `borrow mut place` require a valid stable place after type
checking. Postfix `?` consumes an owned built-in `Result`, yielding its `Ok`
payload or propagating its `Err` from a compatible Result-returning function.
Primitive casts use `(byte)`, `(char)`, `(int)`, `(long)`, or `(double)`. Class and
interface casts are not available. The parser also recognizes
`System.out.println(expression)`, `move(owner)`, and `drop(owner)` as built-ins.



## Language constraints

- Exactly one entry point is required: a free `void main()` function or a static
  `void main()` method. Functions and methods cannot be overloaded.
- Interfaces contain method signatures only. `implements` may list multiple
  interfaces, but every signature must match exactly; interfaces do not extend
  interfaces. See [INTERFACES.md](INTERFACES.md).
- `extends`, class inheritance, nested classes, static fields, arrays (`T[]`),
  indexing, `null`, exceptions, and class casts are unsupported.
- Borrowed types name nominal types only; primitives and `void` cannot be
  borrowed. Borrowed returns are restricted to receiver-derived instance-method
  results. Local views cannot escape their lexical scope.
- Enums are affine inline values. They have variants and positional payloads,
  but no ordinary fields, methods, or `implements` clause. `Option<T>` and
  `Result<T,E>` are reserved built-in enums.
- `String` is a static literal view. Dynamic UTF-8 text is owned `Text`; binary
  bytes use `ByteBuffer`.

The standard-library signatures and ownership contracts are in
[STDLIB.md](STDLIB.md).
