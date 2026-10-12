# Architecture JavaNative V4

Cette note décrit l'implémentation source livrée, **pas une validation du
compilateur**. Rust/Cargo sont absents de l'environnement de livraison ; voir
[VALIDATION.md](VALIDATION.md).

## Pipeline

```text
.jvn
  → lexer.rs        tokens et positions source
  → parser.rs       AST : classes, interfaces, types appliqués, emprunts
  → generics.rs     instanciations nominales et substitution des paramètres
  → checker.rs      noms, types, visibilité, contrats d'interfaces, IR typée
  → ownership.rs    CFG : initialisation et déplacements, point fixe
  → borrowing.rs    prêts lexicaux, origines et incompatibilités d'accès
  → codegen.rs      C11, tables de méthodes et nettoyages déterministes
  → native.rs       GCC/Clang → exécutable ; ou Clang → IR LLVM textuelle
```

Le compilateur utilise la bibliothèque standard Rust, sans dépendances Cargo
externes. `#![forbid(unsafe_code)]` interdit l'utilisation de Rust `unsafe` dans
ses sources ; cela ne prouve ni la correction du compilateur ni la sûreté du C
qu'il est censé produire. Le programme cible utilise le runtime C fourni et la
bibliothèque C du système, sans JVM ni collecteur de déchets prévu.

## AST et spécialisation générique

L'AST distingue `Named`, `Applied { name, args }` et `Borrow { name, args, mutable }`.
Les interfaces réutilisent la représentation des déclarations nominales avec
`is_interface = true`. Une classe conserve ses paramètres de type et sa liste
`implements` ; une construction `new` conserve ses arguments de type explicites.

`generics.rs` indexe les déclarations puis réécrit les types utilisés. Chaque
application de type demandée possède une clé canonique, par exemple `Box<int>`.
Une file déterministe permet de découvrir les instances imbriquées ; un cache
évite de produire plusieurs instances de la même application. La substitution
traverse champs, signatures, variables, constructions et corps des méthodes.
Elle ne remplace pas les identifiants de variables par des noms de types.

Les instances deviennent des classes ou interfaces ordinaires avant le typage.
Les analyses suivantes n'ont donc pas de variable de type à résoudre. Les
méthodes d'un template non instancié ne sont pas toutes vérifiées ; toutes celles
d'une instance demandée le sont, même si elles ne sont jamais appelées. Cette
sémantique est décrite dans [GENERICS.md](GENERICS.md).

Des bornes sur la profondeur, le nombre et la longueur des instances limitent
les expansions récursives. Ce n'est pas une inférence de paramètres, ni un
système de contraintes génériques avec bornes de type.

## IR nominale et interfaces

L'IR remplace les noms par des identifiants nominaux. `Object(id)` représente un
propriétaire, qu'il désigne une classe concrète ou une interface.
`Borrow { class: id, mutable }` représente un accès non possédé. Ainsi,
`Type::is_object()` signifie **possédé**, et non « pointeur vers une classe ».

Une classe IR conserve son nom, ses interfaces et, pour chacune, la liste des
méthodes concrètes correspondant aux emplacements du contrat. Une interface
conserve ses méthodes dans l'ordre de déclaration. Une signature virtuelle a
un `virtual_slot` ; une expression `Upcast` enregistre la conversion possédée
classe → interface. Pour une conversion empruntée, l'expression `Borrow` conserve
la place concrète d'origine et porte le type d'interface attendu.

Les contrats sont nominaux : une méthode de même nom ne suffit pas. Une clause
`implements` doit exister. Le typage vérifie visibilité, mode de receveur
`readonly`, paramètres, modes d'emprunt et retour. Les contrats sont actuellement
exacts, sans covariance ni assouplissement implicite des effets. Une classe à
champs empruntés, donc une vue locale, ne peut pas implémenter d'interface :
l'effacement de son type ne doit pas contourner ses restrictions de durée de vie.

## Ownership et emprunts

`ownership.rs` suit les états possibles de chaque variable : non initialisée,
disponible ou déplacée. Les jonctions sont des unions d'états ; une file de
travail calcule un point fixe à travers les boucles. Une lecture exige un état
disponible sur tous les chemins entrants. Une conversion possédée en interface
traverse le déplacement de sa valeur source, sans créer un second propriétaire.

`borrowing.rs` travaille ensuite sur les blocs typés. Les emprunts locaux restent
lexicaux, leurs cibles sont fixes, et les prêts d'argument durent le temps de
l'appel, sauf capture par une vue. Les origines conservent racine et capacité
parent pour les réemprunts. Le passage à une interface ne change pas cette origine :
il ne permet pas d'obtenir un accès mutable concurrent à un emprunt partagé.

Les signatures des méthodes virtuelles exposent les mêmes contrats que les
méthodes concrètes ; le corps de chaque implémentation est vérifié séparément.
Le borrow checker n'a pas à choisir une implémentation dynamique pour déterminer
si un argument est consommé ou emprunté. Les restrictions V3 sur les vues,
retours empruntés et champs possédés restent applicables.

## Représentation C

Une classe concrète reste un pointeur vers sa structure nominale. Une interface,
possédée ou empruntée, est une structure de deux pointeurs passée par valeur :

```c
struct interface_value {
    void *data;
    const struct interface_vtable *vtable;
};
```

La table contient une fonction de destruction et une fonction typée par méthode.
Une table constante est émise par couple classe/interface. Les adaptateurs
reçoivent `void *`, convertissent le pointeur de données en son type concret, puis
appellent la bonne fonction. Ils évitent les conversions entre pointeurs de
fonctions de signatures incompatibles.

L'enveloppe d'interface ne nécessite pas d'allocation supplémentaire. Un transfert
possédé met l'ancienne enveloppe à `{ NULL, NULL }`. Une interface propriétaire
est nettoyée via la fonction de destruction de sa table, qui connaît le type
concret et ses sous-objets possédés. Une interface empruntée n'est jamais détruite
comme propriétaire. Une copie physique de ses deux pointeurs pour un emprunt
n'est pas une copie de l'objet ni un comptage de références.

Les fonctions de dispatch virtuel transmettent les paramètres à l'implémentation
sans les nettoyer une seconde fois : les paramètres possédés sont consommés par
la fonction concrète. Le nettoyage des retours, remplacements et sorties de
boucle suit le modèle lexical des versions précédentes.

Le backend séquence les expressions pour préserver l'ordre gauche-droite. Les
opérateurs `&&` et `||` restent des blocs conditionnels, pas des évaluations
anticipées de leurs deux opérandes. Les sorties fatales du runtime ne déroulent
pas les propriétaires ; les compteurs mémoire ne sont donc pas une garantie de
nettoyage après une erreur fatale.

## Code natif et LLVM

`run` et `build` émettent du C puis appellent GCC ou Clang avec le niveau
`-O0`, `-O1`, `-O2` (défaut), `-O3` ou `-Os`. Aucun gain de performance n'a été
mesuré. Un niveau d'optimisation plus élevé n'est pas présenté comme une garantie
de code plus rapide.

`emit-llvm` appelle Clang sur ce même C avec `-S -emit-llvm` pour récupérer l'IR
textuelle. **Il n'existe pas de backend LLVM direct écrit en Rust dans cette V4.**
La commande nécessite `clang`, ou `JVN_CLANG`. `JVN_CC`/`CC` sélectionne séparément
le compilateur C de `run` et `build`.

## Inspection et tests

`emit-types` expose les types nominaux après spécialisation, `emit-flow` le CFG
d'ownership, `emit-borrows` les événements vérifiés par l'analyse des prêts, et
`emit-c` le code C. Ces commandes passent par les vérifications du frontend ;
elles ne contournent pas un diagnostic de typage ou d'emprunt.

Les manifestes JSON V2, V3 et V4 servent à générer les suites Rust conservées
dans le dépôt. Le script `validate.py` utilise le **vrai binaire Rust**, lorsqu'il
peut être compilé, pour tester ses sorties C et LLVM. Les fixtures C indépendantes
ne sont pas un substitut à cette validation.
