# Standard Library V9

`List`, `Map`, `Text`, `Option`, `Result`, `Console`, `Files`, and `Args` are reserved
and pre-injected without imports. The two sum types use the ordinary enum
representation; native methods belong to the trusted base. Specializations are
emitted on demand with all their signatures. The scope of real validation is
described in [VALIDATION.md](VALIDATION.md).

## List<T>

`new List<T>()` creates an empty list. `T` may be `int`, `boolean`, a static
`String`, an owned class or interface, an enum, `Text`, or another list.
Borrow-containing views and borrows are not valid list elements, including when
nested. Lists are invariant: `List<Dog>` is not a `List<Animal>`. However,
`List<Animal>.add(new Dog())` accepts an upcast to an interface explicitly
implemented by `Dog`.

| Method | Contract |
|---|---|
| `void add(T value)` | Copies a primitive or consumes an owned object; grows the buffer as needed. |
| `readonly int size()` | Number of initialized elements. |
| `readonly int capacity()` | Current buffer capacity. |
| `readonly boolean isEmpty()` | Whether `size() == 0`. |
| `readonly T get(int index)` | Copies an element of `int`, `boolean`, or `String`; checks bounds. |
| `readonly Option<T> tryGet(int index)` | Copies an `int`, `boolean`, or `String` into `Some`; invalid indices return `None` without mutation. |
| `readonly borrow T getRef(int index)` | Returns a shared borrow of an affine nominal element; checks bounds and does not allocate or transfer ownership. |
| `borrow mut T getMut(int index)` | Returns an exclusive borrow of an affine nominal element; checks bounds and does not allocate or transfer ownership. |
| `void set(int index, T value)` | Replaces an element and destroys the old owned value; checks bounds. |
| `T pop()` | Removes and transfers ownership of the last element; fatal if the list is empty. |
| `T remove(int index)` | Removes an element, preserves order, and transfers ownership; checks bounds. |
| `Option<T> tryPop()` | Transfers the last element into `Some`; an empty list returns `None`. |
| `Option<T> tryRemove(int index)` | Transfers the indexed element into `Some`, preserving survivor order; invalid indices return `None`. |
| `void clear()` | Destroys remaining elements and retains the buffer. |
| `void reserve(int capacity)` | Ensures at least this capacity without changing the size; a negative value is fatal. |

The buffer grows geometrically, with overflow checks before size multiplication.
Reallocation relocates stored representations without destroying them; only
`[0, size())` owns elements. `remove` shifts subsequent elements and clears the
old final slot to prevent a second destruction. List destruction releases its
elements in reverse order, then its buffer and the list object. Extracted
elements are no longer released by the list.

`getRef` and `getMut` borrow from the entire list, not just the selected index.
Their returned reference is scoped lexically. While either borrow is active,
`add`, `set`, `pop`, `remove`, `clear`, `reserve`, moving the list, or destroying
it is forbidden; these operations could replace an element or relocate storage.
A mutable element borrow also prevents independent reads of the list. The
borrowed element itself may be read through `getRef` or mutated through
`getMut`. Ordinary objects return their stored object pointer, inline enums
return the address of their inline slot, and interfaces use their existing
allocation-free borrowed fat-pointer representation.

Call arguments retain the language's receiver reservations: `xs.add(xs.get(0))`
for `List<int>` evaluates the copied argument before mutation. A named shared
borrow prevents mutation of the same owner until the end of its lexical scope;
the same rule prevents moving it during that scope.

Recoverable methods return inline options without allocating. Failed extraction
does not change contents, size, or capacity. Successful extraction clears the
vacated slot and transfers exactly one cleanup obligation. Existing strict
methods still fail fatally at invalid boundaries. Allocation failure remains
fatal; no `Option<borrow T>` method is exposed.

`for (int item : numbers)` copies primitive elements; nominal elements use
`for (borrow T item : list)` or `for (borrow mut T item : list)`. The collection
is evaluated once and borrowed for the whole loop. Element bindings end each
iteration, while structural mutation through another alias stays forbidden.
See [V8.md](V8.md) for traversal and recoverable-operation contracts.

## Map<int,V>

`new Map<int,V>()` creates an empty dictionary. V9 accepts only signed `int`
keys, including zero, negative values, INT32_MIN and INT32_MAX. Value types follow
List ownership rules; borrows and local views cannot be stored. Generic types
remain invariant. There is no map traversal, custom hashing, or text-key API.

| Method | Contract |
|---|---|
| `readonly int size()` | Live key/value pairs. |
| `readonly boolean isEmpty()` | Whether the live count is zero. |
| `readonly boolean containsKey(int key)` | Presence query with no retained reference. |
| `Option<V> put(int key, V value)` | Copies a primitive or consumes an owner; returns None for a new key or Some of the previous value for replacement. |
| `Option<V> tryRemove(int key)` | Removes/transfers the value in Some, or returns None without mutation. |
| `readonly Option<V> tryGet(int key)` | Copies int/boolean/String into Some; absent key returns None. Only copied value specializations expose it. |
| `readonly borrow V getRef(int key)` | Shared affine-value reference; absent key is fatal. |
| `borrow mut V getMut(int key)` | Exclusive affine-value reference; absent key is fatal. |
| `void clear()` | Destroys remaining values once and retains reusable entry storage. |

Options are inline wrappers. Replacement transfers the previous cleanup
obligation rather than destroying that value. Existing-key replacement never
grows the buffer; unsuccessful removal/lookup preserves contents and size.
Insertion alone may allocate for growth; allocation/size failure remains fatal.

Storage is one entry buffer with open addressing and backward-shift deletion.
Growth relocates representations without destroying owners; deleted slots no
longer own a value. Expected lookup/update is constant time, with linear
worst-case collision chains. No hash-flooding resistance is promised.

References borrow the entire map root, not an independent key. A conflicting
put, removal, clear, movement, replacement or destruction is forbidden while
either reference is active, even if the call would be a no-op. Exclusive value
access blocks independent map reads. Objects return stored pointers, inline
enums return slot addresses, and interfaces retain their borrowed fat-pointer
ABI. Bounds/presence observations do not grant a reference or lifetime refinement.
See [V9.md](V9.md) for the exact milestone contract.

## Text

`new Text(String initial)` copie les octets de la chaîne statique dans un buffer
possédé. Une chaîne initiale vide n’alloue pas de buffer avant le premier ajout.
`Text` n’est pas convertible en `String`. Ses déplacements, paramètres propriétaires
et retours propriétaires utilisent les règles ordinaires du langage.

| Méthode | Contrat |
|---|---|
| `void append(String value)` | Ajoute les octets statiques, sans conserver de référence à un objet. |
| `void appendInt(int value)` | Ajoute la représentation décimale signée, y compris INT32_MIN. |
| `void appendBoolean(boolean value)` | Ajoute `true` ou `false`. |
| `void appendText(borrow Text other)` | Copie le contenu d’un autre texte. L’auto-aliasing est refusé par l’analyse. |
| `readonly int byteLength()` | Taille en octets UTF-8, pas en caractères. |
| `readonly int byteAt(int index)` | Octet entre 0 et 255 ; vérifie les limites. |
| `readonly boolean isEmpty()` | Teste si le contenu est vide. |
| `void clear()` | Vide le contenu sans réduire la capacité. |
| `readonly void print()` | Affiche le contenu suivi d’un saut de ligne. |
| `readonly Text clone()` | Nouvelle allocation et copie indépendante explicite. |
| `readonly boolean equals(borrow Text other)` | Comparaison des octets ; deux lectures du même texte sont autorisées. |
| `readonly boolean equalsString(String other)` | Comparaison avec une chaîne statique. |
| `readonly Result<int,Text> parseInt()` | Signe `+`/`-` facultatif puis chiffres ASCII ; analyse tous les octets, vérifie `[-2147483648,2147483647]`. Vide, espaces, suffixes et overflow donnent `Err` sans consommer le propriétaire. |
| `readonly Result<Text,Text> sliceBytes(int start, int end)` | Copie indépendante de `[start,end)` ; plage valide et frontières de points de code UTF-8 obligatoires, sinon `Err`. |

Aucun `toString`, emprunt de slice ou pointeur n’est exposé. `sliceBytes` retourne
un nouveau propriétaire, pas une vue qui pourrait survivre au buffer dynamique.
Les NUL sont conservés. Les indices restent des **octets**, pas des caractères ou
graphèmes ; la vérification de frontières UTF-8 ne les transforme pas en indices Unicode.

## Option<T> et Result<T,E>

```java
enum Option<T> { None; Some(T); }
enum Result<T,E> { Ok(T); Err(E); }
```

Ces déclarations sont des contrats préinjectés réservés. Construction explicite :
`new Option<int>.Some(42)`, `new Option<int>.None()`,
`new Result<int,Text>.Err(new Text("échec"))`.
Une somme n’alloue pas d’enveloppe ; seuls ses payloads peuvent allouer.
Toutes les sommes sont affines, même `Option<int>`. `match` les consomme et exige
toutes les variantes, avec les variables de payload correspondantes.
Les payloads possédés sont détruits sur les sorties normales, y compris les
retours précoces et sorties de boucle. Les emprunts et vues locaux sont interdits
dans les payloads ; il n’y a pas de déballage implicite ni d’exceptions.

## Console

| Méthode statique | Contrat |
|---|---|
| `Result<Option<Text>,Text> readLine()` | `Ok(Some(line))` pour une ligne, `Ok(None)` pour EOF sans octets restants, `Err(error)` pour erreur de lecture, taille excessive ou UTF-8 invalide. |
| `void print(String value)` | Affiche les octets sans saut de ligne et flush stdout. |
| `void printText(borrow Text value)` | Même affichage pour un texte possédé, sans le consommer. |

`readLine` enlève LF et le CR qui le précède ; une dernière ligne sans LF est
retournée avant EOF. Une ligne vide est `Some(Text vide)`, pas EOF.
Une ligne UTF-8 invalide est consommée avant `Err`. En revanche, un flux C ayant
une erreur persistante n’est pas réinitialisé : l’appelant ne doit pas le réessayer
indéfiniment. Le Sudoku termine proprement sur une erreur du flux console.
Le buffer lu est transféré au `Text` retourné, sans recopie intégrale.

## Files

| Méthode statique | Contrat |
|---|---|
| `Result<Text,Text> readText(borrow Text path)` | Lecture complète UTF-8, NUL de contenu conservés ; échec d’ouverture/lecture/fermeture, UTF-8 invalide ou taille excessive → `Err`. |
| `Result<boolean,Text> writeText(borrow Text path, borrow Text content)` | Écrit les octets exacts et remplace le fichier existant ; `Ok(true)` après écriture, flush et fermeture réussis, sinon `Err`. |
| `Result<boolean,Text> writeTextAtomic(borrow Text path, borrow Text content)` | Writes exact bytes to a sibling temporary file, flushes and closes it, then atomically replaces the destination using `rename`; returns `Ok(true)` only after commit. |

Un NUL dans un chemin est refusé. Les chemins sont UTF-8 ; les espaces sont
acceptés. Les buffers lus sont transférés au texte retourné sans copie finale.
`writeText` n’est pas atomique : un échec peut laisser un fichier partiel ou
tronqué. Aucun `Err` ne prétend annuler les écritures déjà effectuées.

`writeTextAtomic` guarantees atomic visibility, not power-loss durability: it
does not call `fsync`. Before a successful commit, a failure preserves the
destination and attempts to remove the temporary file. A replacement uses
`mkstemp` permissions (0600 subject to umask), not the old file's permissions.
Renaming over a symlink replaces the symlink entry rather than its referent.
There is no fallible operation after the successful rename that can produce
an `Err` claiming the old destination was preserved.

## Args

| Méthode statique | Contrat |
|---|---|
| `int count()` | Nombre d’arguments utilisateur, sans le nom de l’exécutable. |
| `Result<Text,Text> get(int index)` | Copie possédée de l’argument UTF-8 ; indice invalide ou octets non UTF-8 → `Err`. |

L’entrée reste `void main()`. Les arguments sont ceux de l’exécutable construit,
ou ceux transmis par `nacre run sources -- arguments...`.
`Console`, `Files` et `Args` ne sont pas constructibles et ne sont pas des types
de variables ou de payloads.


## Frontière de confiance

Les corps vides de `stdlib/prelude.ncr` sont des contrats internes, pas du code
utilisateur valable pour les méthodes non-void. `src/stdlib.rs` est le seul
injecteur du marqueur natif. Le parseur ne peut pas produire ce marqueur à partir
du source. Après spécialisation, le checker résout les signatures, puis les appels
utilisateur sont analysés normalement. Les corps natifs ne sont pas présentés
comme ayant été prouvés par les analyses d’ownership et d’emprunts.

Le backend ajoute aux appels natifs une ligne et une colonne cachées pour les
contrôles de limites. Les primitives mémoire C et les templates natifs font
partie de la base de confiance ; ils doivent être audités indépendamment.
Les scripts C testent les templates réels, mais pas leur intégration Rust.

## Erreurs fatales

Les opérations historiques de listes/texte hors limites, `pop` vide, capacité
négative, taille impossible dans les opérations fatales ou échec d’allocation
arrêtent toujours le processus. Pas de déroulement normal des destructeurs sur
ce chemin. Les nouvelles E/S, conversions et copies de plages renvoient des
erreurs récupérables via `Result` ; l’échec d’allocation reste fatal.
