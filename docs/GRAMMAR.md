# Grammaire V6

Grammaire indicative du sous-ensemble implémenté. Les contraintes sémantiques
ci-dessous font partie du langage. Identifiants ASCII ; sources et chaînes UTF-8 ;
commentaires `//` et `/* ... */`.

```ebnf
program       = { function | class | interface | enum } ;
class         = [ "public" ] "class" identifier [ typeParameters ]
                [ "implements" nominalType { "," nominalType } ]
                "{" { member } "}" ;
interface     = [ "public" ] "interface" identifier [ typeParameters ]
                "{" { interfaceMethod } "}" ;
enum          = [ "public" ] "enum" identifier [ typeParameters ]
                "{" variant { variant } "}" ;
variant       = identifier [ "(" [ type { "," type } ] ")" ] ";" ;
typeParameters = "<" identifier { "," identifier } ">" ;
typeArguments = "<" type { "," type } ">" ;
nominalType   = identifier [ typeArguments ] ;
member        = modifiers ( constructor | method | field ) ;
modifiers     = { "public" | "private" | "static" | "readonly" } ;
function      = modifiers returnType identifier parameters block ;
method        = returnType identifier parameters block ;
interfaceMethod = [ "public" ] [ "readonly" ] returnType identifier
                  parameters ";" ;
constructor   = className parameters block ;
field         = type identifier [ "=" expression ] ";" ;
parameters    = "(" [ parameter { "," parameter } ] ")" ;
parameter     = type identifier ;
returnType    = type | "void" ;
type          = "int" | "boolean" | "String" | nominalType | borrowedType ;
borrowedType  = "borrow" [ "mut" ] nominalType ;
block         = "{" { statement } "}" ;
statement     = type identifier [ "=" expression ] ";"
              | place "=" expression ";"
              | callOrNew ";"
              | "if" "(" expression ")" block [ "else" ( block | ifStatement ) ]
              | "while" "(" expression ")" block
              | "return" [ expression ] ";"
              | "break" ";" | "continue" ";" | matchStatement | block ;
matchStatement = "match" "(" expression ")" "{" matchArm { matchArm } "}" ;
matchArm      = identifier [ "(" [ identifier { "," identifier } ] ")" ] block ;
place         = ( identifier | "this" ) { "." identifier } ;
call          = identifier "(" [ arguments ] ")"
              | place "." identifier "(" [ arguments ] ")" ;
new           = "new" nominalType [ "." identifier ] "(" [ arguments ] ")" ;
arguments     = expression { "," expression } ;
```

Les modificateurs de méthode peuvent aussi être écrits dans l'ordre inverse ;
les doublons et les combinaisons interdites sont rejetés. `className` dans un
constructeur désigne le nom non appliqué : `Box(T value)`, et non `Box<T>(...)`.

Les expressions comprennent littéraux, places, appels, constructions, parenthèses,
unaires et binaires suivants :

| Priorité croissante | Opérateurs |
|---|---|
| 1 | `||` |
| 2 | `&&` |
| 3 | `==`, `!=` |
| 4 | `<`, `<=`, `>`, `>=` |
| 5 | `+`, `-` |
| 6 | `*`, `/`, `%` |
| 7 | unaires `+`, `-`, `!` |

Les `>` de `Holder<Box<int>>` ferment deux applications de type. Il n'existe pas
d'opérateur de décalage `>>`. `println(expr)` et `System.out.println(expr)` sont
intégrés, comme `move(objet)` et `drop(objet)`. Il n'existe pas d'expression
`borrow(...)` : le type attendu détermine l'emprunt.

## Déclarations et interfaces

Une seule entrée sans paramètres de retour `void` est exigée : fonction libre
`main` ou méthode `static main`. Une classe a au plus un constructeur ; méthodes
et fonctions ne sont pas surchargeables. L'absence de constructeur entraîne une
construction par défaut seulement si les règles d'initialisation le permettent.
Les champs statiques, classes imbriquées, imports et packages sont absents.

Les interfaces ne contiennent que des signatures de méthodes publiques d'instance,
avec ou sans `readonly`. Pas de champs, constructeurs, méthodes statiques, corps
par défaut ou héritage d'interfaces. Une interface ne peut pas être construite
avec `new`. `implements` accepte plusieurs interfaces mais exige des contrats
exacts, sans surcharge ni variance des signatures. Voir [INTERFACES.md](INTERFACES.md).

`readonly` est réservé aux méthodes d'instance et à leurs contrats d'interface.
Il est interdit sur les champs, constructeurs, classes, fonctions libres et
méthodes statiques. L'absence de visibilité signifie public dans cet espace de noms
partagé entre unités source. `extends`, donc l'héritage de classes, reste absent.

## Génériques

Classes, interfaces et enums peuvent déclarer des paramètres de type. Les applications
exigent tous leurs arguments, y compris après `new` : `new Box<int>(42)`. Ni
inférence, ni `<>` vide, ni bornes, ni wildcards, ni variance. Les fonctions et
méthodes ne déclarent pas leurs propres paramètres génériques.

Les arguments valides sont les trois types scalaires et les types nominaux
possédés, éventuellement appliqués. `void` et les types empruntés sont exclus
comme arguments. Un paramètre de type peut être utilisé dans `borrow T`, mais
une instance où `T` est scalaire est alors rejetée. Les types sont spécialisés
avant l'analyse des corps. Un template non instancié n'est pas intégralement
vérifié ; voir [GENERICS.md](GENERICS.md).

La syntaxe d'appel statique qualifié ne comprend pas de type appliqué :
`Box<int>.method()` n'est pas pris en charge. Les appels non qualifiés à une
méthode statique depuis le corps de la même classe spécialisée restent possibles.
Un paramètre de type ne peut masquer un autre nom de type déclaré.

## Types somme et branches exhaustives

```java
enum Parcel<T> { Empty; Value(T); }
void main() {
    Parcel<int> parcel = new Parcel<int>.Value(42);
    match (parcel) {
        Empty { println(0); }
        Value(number) { println(number); }
    }
}
```

Une enum est une union taguée nominale passée par valeur, sans allocation
d’enveloppe. Chaque variante possède zéro ou plusieurs payloads positionnels.
`new Parcel<int>.Empty()` construit une variante vide ; `new Parcel<int>()`
n’est pas une construction valide. Les enums n’ont pas de champs ordinaires,
de méthodes ni de clause `implements`.

Toutes les enums sont **affines**, même lorsqu’elles ne contiennent que des
entiers : affectation, passage, retour et `match` transfèrent la propriété.
Le `match` évalue puis consomme son expression une seule fois. Il exige exactement
une branche par variante, sans doublon ni wildcard ; chaque payload a une variable
locale de branche. Les objets liés sont détruits en sortie de portée, y compris
sur `return`, `break` et `continue`, sauf transfert de leur propriété.
`None { ... }` et `None() { ... }` sont équivalents pour une variante vide.

Les payloads ne peuvent contenir ni emprunt ni vue locale. Les emprunts d’enums
sont rejetés en V6 ; pas de `match` emprunté. Les cycles de sommes imbriquées par
valeur sont rejetés, même avec une variante vide : leur taille C serait infinie.
Un cycle passant par un objet alloué peut avoir un cas de base réel, par exemple
`enum Link { End; Next(Node); }` avec un champ `Link` dans `Node`.

`Option<T>` et `Result<T,E>` sont préinjectés comme enums réservées :
`None`/`Some(T)` et `Ok(T)`/`Err(E)`. Il n’existe pas de déballage implicite,
d’exception ou d’opérateur de propagation d’erreur en V6.


## Objets et références

Les variables possédées et scalaires peuvent être déclarées sans initialiseur ;
l'analyse exige une initialisation certaine avant utilisation. Les variables
`borrow` exigent un initialiseur nommé et ne sont pas réaffectables. L'ombrage
d'une variable dans une portée déjà active reste interdit.

Les retours `borrow` sont reconnus syntaxiquement mais rejetés sémantiquement.
`borrow int`, `borrow String` et `borrow void` ne sont pas des types valides.
Les propriétaires peuvent être convertis implicitement vers une interface
implémentée, en transférant leur propriété. Les emprunts peuvent être convertis
vers cette interface sans changement d'origine ni de mode d'accès.

Il n'y a pas de `null`, références forgées, FFI, conversions explicites de types,
tableaux ou champs possédés remplaçables. Les champs possédés sont initialisés
au constructeur ; les champs scalaires restent modifiables via un accès mutable.

Les champs `borrow` partagés rendent leur classe locale à son bloc. Ils sont
initialisés directement depuis un paramètre partagé du constructeur. Une vue ne
peut être déplacée, passée en paramètre, retournée, contenue dans un autre objet
ou effacée derrière une interface. Voir [BORROWING.md](BORROWING.md).

Les emprunts et receveurs exigent des places stables. Un appel sur un temporaire,
comme `new Counter().get()`, demande encore une variable intermédiaire.

## Contrôle et valeurs

Les accolades sont obligatoires. `break` et `continue` exigent une boucle.
Une fonction non-`void` doit retourner sur tous les chemins ; les boucles ne sont
pas utilisées pour prouver le retour. Les instructions après un terminal certain
sont rejetées. Les deux issues d'une condition restent considérées possibles,
même lorsque sa valeur est un littéral constant.

`int` est signé sur 32 bits avec arithmétique modulo 2^32. Le littéral `2147483648`
n'est accepté que directement précédé du signe moins. La division par zéro
termine le programme avec un diagnostic du runtime. Les tests d'égalité sont
limités à `int` et `boolean`. Les `String` restent des vues sur des littéraux
statiques, sans concaténation ni construction dynamique.


## Bibliothèque native et plusieurs fichiers

`List<T>`, `Text`, `Console`, `Files`, `Args`, `Option<T>` et `Result<T,E>`
sont des noms réservés préinjectés, pas de nouveaux mots-clés.
Leurs contrats sont décrits dans [STDLIB.md](STDLIB.md).
`String` reste statique ; `Text` est la valeur possédée modifiable. La syntaxe
`T[]` et l'indexation `xs[i]` restent absentes.

Chaque fichier est analysé indépendamment, puis ses déclarations rejoignent
un espace de noms global. Une déclaration ou un commentaire ne peut pas
commencer dans un fichier et se terminer dans un autre. Les règles de projet
et la CLI sont décrites dans [PROJECTS.md](PROJECTS.md).
