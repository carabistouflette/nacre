# Generic specialization — V9

## Modèle choisi

Nacre specializes explicitly instantiated class, interface, and enum templates
without runtime type erasure. This is neither Java's generic implementation nor
Rust's bounded trait system.

```java
class Pair<A, B> {
    A first;
    B second;
    Pair(A a, B b) { first = a; second = b; }
}
void main() {
    Pair<int, String> p = new Pair<int, String>(42, "ok");
    println(p.first);
    println(p.second);
}
```

`Pair<int,String>` est un type nominal concret, distinct de
`Pair<String,int>`. Les spécialisations sont invariantes. Les espaces autour de
`<`, `>` et `,` n'affectent pas leur identité. Les types imbriqués fonctionnent ;
le lexer n'ayant pas d'opérateur de décalage, `>>` fournit deux fermetures de type.

Les déclarations acceptent jusqu'à huit paramètres distincts. Un paramètre ne
peut pas masquer un nom de type déclaré ou certains noms réservés. Les paramètres
internes du prelude ne masquent pas une classe utilisateur nommée `T` ou `E`.
Les arguments peuvent être `int`, `boolean`, `String` ou un type nominal possédé,
y compris interface et enum. Ni `void`, ni un type d’emprunt comme argument.
Pour emprunter une boîte, écrire `borrow Box<int>` ; un modèle peut contenir
`borrow T` si T est une classe/interface compatible avec les règles des vues locales.

Pas de diamant, d'inférence d'arguments de type, de valeur générique brute sans
arguments, de bornes `extends`, de wildcard ou de variance. Il n'y a pas encore de
fonctions ou de méthodes ayant leurs propres paramètres génériques.
L'accès statique qualifié `Box<int>.method()` et `T.method()` dans un modèle ne
sont pas pris en charge ; des appels statiques non qualifiés à l'intérieur d'une
classe spécialisée restent possibles.

## Quand le code est-il vérifié ?

Le fichier entier est lexé et parsé. Les doublons de déclarations nominales et les
paramètres génériques invalides sont rejetés même si un modèle n'est pas utilisé.
En revanche, le corps d'un modèle inutilisé n'est pas entièrement vérifié en types.

Une spécialisation est demandée quand un type appliqué apparaît dans les
signatures, champs, variables ou `new` de code concret, ou dans une spécialisation
déjà demandée. Toutes les méthodes de cette spécialisation sont vérifiées, même
si elles ne sont pas appelées. « À la demande » concerne donc la spécialisation
de type nominal, pas une élimination des méthodes inaccessibles.

Par exemple, ce modèle non instancié peut être conservé :

```java
class Example<T> {
    T value() { return 42; }
}
void main() {}
```

`Example<int>` est compatible avec son retour ; `Example<boolean>` doit être
rejeté. Les besoins du corps — méthode disponible, opération arithmétique, contrat
d'interface — sont contrôlés sur les types substitués, et non à partir de bornes
universelles déclarées sur T. C'est une limite majeure d'ergonomie et de modularité.

## Propriété après substitution

La spécialisation ne contourne aucune analyse mémoire. Un paramètre `T` substitué
par `int` est copié comme un entier. Substitué par `Counter`, il possède l’objet
et le passage à une méthode consomme sa propriété. Substitué par une enum,
il possède une somme affine par valeur ; il n’est pas copié implicitement.
`borrow T` can specialize to a class, interface, or enum reference, but not a
primitive. Returned borrows must also satisfy the receiver-origin contract.

Le getter suivant ne convient pas à tous les arguments :

```java
class Box<T> {
    T value;
    Box(T value) { this.value = value; }
    readonly T get() { return this.value; }
}
```

Pour `Box<int>` et `Box<String>`, le retour est une copie de valeur primitive ou
de vue sur une chaîne statique. Pour `Box<Counter>`, le corps tente de déplacer un
champ possédé ; il est refusé, même si `get()` n'est jamais appelé.
Pour exposer une opération d'un objet contenu, utiliser par exemple
`readonly int read() { return this.value.getValue(); }` dans un modèle dont les
spécialisations fournissent effectivement `getValue()`.

A nominal-only template may instead expose `readonly borrow T getRef() { return
this.value; }`. Its caller keeps a lexical loan of the template instance.
Specializing that method with a primitive is rejected; only trusted `List`
conditionally prunes its copied/reference accessors by element category.

## Interfaces génériques

```java
interface Source<T> { readonly T read(); }
class Constant<T> implements Source<T> {
    T value;
    Constant(T value) { this.value = value; }
    readonly T read() { return this.value; }
}
```

`Source<int>` est une interface concrète avec ses propres signatures et sa table
de méthodes. `Constant<int>` déclare son implémentation après substitution.
`Source<String>` est une autre interface nominale. Il n'y a pas de conversion
entre les deux, même si certains layouts natifs se ressemblent.

## Algorithme et bornes

`src/generics.rs` travaille sur l'AST, pas sur le texte par remplacement global.
Il conserve un registre des modèles et une file FIFO des spécialisations
normalisées. Les arguments sont résolus et substitués dans les types, signatures,
champs et constructions. Une spécialisation déjà demandée est réutilisée ; les
noms de valeurs locales ne sont pas réécrits par substitution de types.

Les types concrets produits passent par `checker.rs`, puis `ownership.rs` et
`borrowing.rs`. Les symboles C sont basés sur des identifiants numériques, pas sur
les noms contenant `<` ou `>`.

Une expansion comme `Chain<T>` contenant un `Chain<Chain<T>>` peut engendrer une
suite infinie de spécialisations. Elle n'est pas supposée terminer : le compilateur
impose un maximum de 64 spécialisations, 128 types concrets, une profondeur
syntaxique de type 32 et une longueur de nom spécialisé 4096. Atteindre une borne
produit `E4199`, pas une allocation illimitée intentionnelle. Ces bornes ne sont
pas une preuve générale de résistance aux entrées hostiles.

Les types récursifs possédés directs restent soumis aux restrictions V3.

## Diagnostics

`E4101` : paramètres génériques invalides ou diamant non pris en charge.
`E4102` : modèle inconnu, arguments manquants, arité incorrecte.
`E4103` : argument interdit/inconnu, emprunt ou construction devenu primitif.
`E4199` : limite d'expansion atteinte.

Les autres erreurs utilisent les diagnostics habituels du type concret obtenu.
Les spans source sont conservés, mais les notes de spécialisation ne couvrent pas
encore tous les diagnostics des passes ultérieures.


## Native collection templates

List<T> and Map<K,V> are reserved nominal templates, specialized on demand along
with Text and the sum types used by their signatures. Unused native classes are
not emitted. Native requests share the existing specialization bounds and do
not count as user-defined concrete classes.

The copied native `get` and `tryGet` methods exist only for `int`, `boolean`, and
`String`. Nominal elements expose shared/exclusive `getRef`/`getMut`; all supported
elements expose owned `tryPop`/`tryRemove` with inline specialized `Option<T>`.
For-each header and collection types are substituted before ordinary checking.
An empty `List<Node>` permits finite owned recursion, unlike a required
`Node next` field without a base case. `List<T>` remains invariant.

V9 Map has exactly two arguments and accepts only int keys. Value methods are
pruned before ordinary checking: copied primitives retain tryGet, while nominal
values retain getRef/getMut. Put/tryRemove return specialized inline Option<V>
for every supported value representation. Borrows/local views cannot be values.
Map<int,Dog> is not Map<int,Animal>; an individual owned or borrowed upcast still
obeys the ordinary interface rules.
Les contrats et limites de cette bibliothèque sont dans [STDLIB.md](STDLIB.md).
