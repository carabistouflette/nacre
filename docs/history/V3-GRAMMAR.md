# Grammaire V3

Grammaire indicative du sous-ensemble implémenté. Les contraintes sémantiques
ci-dessous font partie du langage, même lorsqu'une forme est acceptée par le parser.
Identifiants ASCII ; sources et chaînes UTF-8 ; commentaires `//` et `/* ... */`.

```ebnf
program       = { function | class } ;
class         = [ "public" ] "class" identifier "{" { member } "}" ;
member        = modifiers ( constructor | method | field ) ;
modifiers     = { "public" | "private" | "static" | "readonly" } ;
function      = modifiers returnType identifier parameters block ;
method        = returnType identifier parameters block ;
constructor   = className parameters block ;
field         = type identifier [ "=" expression ] ";" ;
parameters    = "(" [ parameter { "," parameter } ] ")" ;
parameter     = type identifier ;
returnType    = type | "void" ;
type          = "int" | "boolean" | "String" | identifier | borrowedType ;
borrowedType  = "borrow" [ "mut" ] identifier ;
block         = "{" { statement } "}" ;
statement     = type identifier [ "=" expression ] ";"
              | place "=" expression ";"
              | callOrNew ";"
              | "if" "(" expression ")" block [ "else" ( block | ifStatement ) ]
              | "while" "(" expression ")" block
              | "return" [ expression ] ";"
              | "break" ";" | "continue" ";" | block ;
place         = ( identifier | "this" ) { "." identifier } ;
call          = identifier "(" [ arguments ] ")"
              | place "." identifier "(" [ arguments ] ")" ;
new           = "new" identifier "(" [ arguments ] ")" ;
arguments     = expression { "," expression } ;
```

Les expressions comprennent les entiers décimaux, booléens, chaînes, places,
appels, `new`, parenthèses, les unaires `+`, `-`, `!`, et les binaires ci-dessous :

| Priorité croissante | Opérateurs |
|---|---|
| 1 | `||` |
| 2 | `&&` |
| 3 | `==`, `!=` |
| 4 | `<`, `<=`, `>`, `>=` |
| 5 | `+`, `-` |
| 6 | `*`, `/`, `%` |
| 7 | unaires `+`, `-`, `!` |

`println(expr)` et `System.out.println(expr)` sont intégrés, comme `move(objet)`
et `drop(objet)`. `move` et `drop` exigent une valeur possédée. Il n'existe pas
d'expression `borrow(...)` : l'emprunt est inséré à partir du type attendu.

## Contraintes de déclarations

Une seule entrée sans paramètres et de retour `void` est exigée : fonction libre
`main` ou méthode `static main`. Une classe a au plus un constructeur, sans surcharge.
Les méthodes de même nom ne peuvent pas être surchargées. Les champs statiques,
classes imbriquées, interfaces, héritage, génériques et imports sont absents.

`readonly` est réservé aux méthodes d'instance. Il est interdit sur un champ,
constructeur, classe, fonction libre ou méthode statique. Les doublons de
modificateurs sont rejetés. L'absence de visibilité signifie public dans cette
unité source unique ; il n'y a pas de packages.

Les déclarations de propriétaires et primitives sans initialiseur sont autorisées,
avec une analyse d'initialisation certaine. Les variables de type `borrow` exigent
un initialiseur nommé et ne sont pas réaffectables. L'ombrage d'une variable dans
une portée déjà active reste interdit.

Les retours `borrow` sont reconnus syntaxiquement mais rejetés sémantiquement en
V3. `borrow int`, `borrow String` et `borrow void` ne sont pas des types valides.
`borrow`, `mut` et `readonly` sont de nouveaux mots réservés.

## Objets et références

Les objets ont des champs et méthodes, mais pas de `null`, références forgées,
conversions de pointeurs, FFI, tableaux ou objets récursifs possédés. Les champs
possédés sont initialisés directement au constructeur, sans remplacement ultérieur.
Les primitives contenues dans un objet restent modifiables via un accès mutable.

Les champs `borrow` sont en lecture seule et font de leur classe une vue locale.
Ils doivent être initialisés directement depuis un paramètre `borrow` partagé du
constructeur. La vue s'initialise par `new` et ne peut quitter sa variable/bloc de
déclaration par transfert, paramètre ou retour. Voir [BORROWING.md](BORROWING.md).

Les emprunts et les receveurs exigent des places stables. Les appels sur receveur
temporaire (`new Counter().get()`) demandent encore une variable intermédiaire.
Les appels `get().field` ne constituent pas des emprunts exportables.

## Contrôle et valeurs

Les accolades sont obligatoires. `break` et `continue` exigent une boucle.
Une fonction non-`void` doit retourner sur tous les chemins ; les boucles ne sont
pas utilisées pour prouver le retour. Les instructions après un terminal certain
sont rejetées. Les deux issues d'une condition restent considérées possibles pour
les analyses, même avec un littéral constant.

`int` est signé sur 32 bits avec arithmétique modulo 2^32. Le littéral `2147483648`
n'est accepté que directement précédé du signe moins. Les divisions par zéro
terminent le programme avec un diagnostic du runtime. Les tests d'égalité sont
limités à `int` et `boolean`. Les chaînes sont des vues sur littéraux statiques,
sans concaténation ou allocation dynamique.
