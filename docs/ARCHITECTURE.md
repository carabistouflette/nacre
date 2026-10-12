# Architecture Nacre V7

Cette note décrit l'implémentation source livrée. Le périmètre de validation
du compilateur Rust et de ses programmes générés est documenté dans
[VALIDATION.md](VALIDATION.md).

## Pipeline

```text
fichier(s) .ncr ou dossier
  → project.rs      collecte, limites et unités source distinctes
  → lexer.rs        tokens et positions source de chaque fichier
  → parser.rs       AST puis fusion des déclarations
  → stdlib.rs       injection des contrats natifs réservés
  → generics.rs     instanciations nominales et substitution des paramètres
  → checker.rs      noms, types, visibilité, contrats d'interfaces, IR typée
  → ownership.rs    CFG : initialisation et déplacements, point fixe
  → borrowing.rs    prêts lexicaux, origines et incompatibilités d'accès
  → codegen.rs      C11, tables de méthodes et nettoyages déterministes
  → stdlib_codegen.rs / stdlib/*.c.in : corps C des méthodes natives
  → native.rs       GCC/Clang → exécutable ; ou Clang → IR LLVM textuelle
```

Le frontend utilise la bibliothèque standard Rust ; `serde_json` encode et décode
les messages du LSP. `#![forbid(unsafe_code)]` interdit Rust `unsafe` dans
ses sources ; cela ne prouve ni la correction du compilateur ni la sûreté du C
qu'il est censé produire. Le programme cible utilise le runtime C fourni et la
bibliothèque C du système, sans JVM ni collecteur de déchets prévu.

## AST et spécialisation générique

L'AST distingue `Named`, `Applied { name, args }` et `Borrow { name, args, mutable }`.
Les interfaces portent `is_interface = true` ; les enums portent une liste de
variantes et de payloads. `is_builtin` distingue les déclarations du prelude des
types utilisateur, sans accorder de privilèges à un nom de paramètre comme `T`.
Une classe conserve ses paramètres et sa liste `implements` ; les constructions
ordinaires et de variantes conservent leurs arguments de type explicites.

`generics.rs` indexe les déclarations puis réécrit les types utilisés. Chaque
application de type demandée possède une clé canonique, par exemple `Box<int>`.
Une file déterministe permet de découvrir les instances imbriquées ; un cache
évite de produire plusieurs instances de la même application. La substitution
traverse champs, signatures, variables, constructions et corps des méthodes.
Elle ne remplace pas les identifiants de variables par des noms de types.

Les instances deviennent des types nominaux concrets avant le typage.
Les analyses suivantes n'ont donc pas de variable de type à résoudre. Les
méthodes d'un template non instancié ne sont pas toutes vérifiées ; toutes celles
d'une instance demandée le sont, même si elles ne sont jamais appelées. Cette
sémantique est décrite dans [GENERICS.md](GENERICS.md).

Des bornes sur la profondeur, le nombre et la longueur des instances limitent
les expansions récursives. Ce n'est pas une inférence de paramètres, ni un
système de contraintes génériques avec bornes de type.

## IR nominale et interfaces

L’IR remplace les noms par des identifiants nominaux. `Object(id)` représente un
propriétaire : pointeur de classe, interface par valeur ou somme affine par valeur.
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

Virtual method signatures expose the same contracts as concrete methods; every
implementation body is checked separately. Borrow analysis need not select a
dynamic implementation to determine whether an argument owns or borrows.
V7 records borrowed returns in those signatures and proves their receiver origin;
caller-side result loans survive call cleanup. Local views remain non-exportable.

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

### Sommes par valeur

Une enum C est une structure avec un tag `int32_t` et une union de payloads.
Les sommes imbriquées sont définies dans un ordre topologique ; les cycles de
représentation par valeur sont refusés avant émission. Les pointeurs de classes
et interfaces permettent des structures récursives ayant un cas de base fini.

Une construction ne crée pas d’allocation d’enveloppe. Un déplacement remplace
le tag du propriétaire source par `-1`, état interne non constructible par le
programme. Le destructeur consulte uniquement la variante active et détruit ses
payloads possédés dans l’ordre inverse. Un `match` transfère ces payloads aux
variables de branche et invalide son temporaire ; le CFG et les nettoyages
lexicaux couvrent retours et sorties de boucle. Il est émis comme une chaîne
`if`/`else if`, pour qu’un `break` continue de viser la boucle source.

V7 enum references point to inline storage. Borrowed matching preserves the tag
and binds primitive copies or receiver/scrutinee-derived nominal references;
payloads containing borrowed fields/local views remain forbidden.


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

`emit-llvm` invokes Clang with `-S -emit-llvm` on the same generated C. V7 has no
direct LLVM backend written in Rust. `NACRE_CLANG` selects that Clang executable;
`NACRE_CC`/`CC` separately select the compiler used by `run` and `build`.

## Inspection et tests

`emit-types` expose les types nominaux après spécialisation, `emit-flow` le CFG
d'ownership, `emit-borrows` les événements vérifiés par l'analyse des prêts, et
`emit-c` le code C. Ces commandes passent par les vérifications du frontend ;
elles ne contournent pas un diagnostic de typage ou d'emprunt.

Les manifestes JSON V2, V3, V4 et V5 servent à générer les suites Rust conservées
dans le dépôt. Le script `validate.py` utilise le **vrai binaire Rust**, lorsqu'il
peut être compilé, pour tester ses sorties C et LLVM. Les fixtures C indépendantes
ne sont pas un substitut à cette validation.


## Classes natives et mémoire des collections

`ast::Class.native` n'est jamais produit par le parser. Le prelude interne
associe ce marqueur à `List<T>` et `Text`. La spécialisation remplace le type
d'élément puis le checker résout `ir::NativeClass` et les `NativeMethod`.
Unused native classes are not materialized. Copied `get` is removed for affine
elements; `getRef`/`getMut` are removed for primitive elements before typing.
Borrowed accessors derive their checked object/enum/interface ABI from typed IR.

Les fonctions natives reçoivent des signatures typées normales, mais leurs corps
IR vides sont explicitement exemptés d'analyse. `ownership.rs` et `borrowing.rs`
ne leur attribuent pas une preuve de sûreté. Les appels utilisateur sont vérifiés
selon les mêmes contrats que les fonctions ordinaires.

Le stockage d'une liste comporte un pointeur de buffer, une taille et une
capacité. Seule la plage initialisée possède des valeurs. Le type concret de
l'élément détermine son destructeur, y compris un destructeur dynamique pour les
interfaces. Les opérations `pop`/`remove` retirent l'élément avant de transférer sa
représentation à l'appelant. Le texte contient un buffer d'octets possédé ; aucune
vue de ce buffer n'est retournée comme `String` statique.

Le backend injecte aux appels natifs deux arguments cachés ligne/colonne et
substitue leurs identifiants numériques dans les templates C. Les buffers
passent par les mêmes compteurs que les allocations d'objets. Ils n'utilisent ni
GC ni comptage de références dans ce modèle. Voir [STDLIB.md](STDLIB.md).

La vérification historique des cycles de champs reste appliquée aux champs
obligatoires. Une liste native ne comporte pas de champ obligatoire de son type
élément : sa valeur vide fournit un cas de base. Un `Node` qui possède une
`List<Node>` est donc représentable ; un graphe avec plusieurs propriétaires
pour le même objet ne l'est pas.

## Unités source et outils de projet

`project::SourceBundle` conserve les noms, le texte et les offsets de chaque
unité. Les fichiers sont lexés et parsés séparément pour qu'un commentaire ou une
déclaration ne puisse franchir une frontière de fichier. Les positions sont
ensuite exprimées dans un texte cumulé, et les AST sont fusionnés avant les
passes communes. Il n'y a pas de compilation séparée, de packages ou d'import.

Les diagnostics primaires et leurs notes sont remappés sur le fichier d'origine.
Pour les erreurs runtime multifichiers, le C contient une table compacte des
lignes de début de fichier. Les commandes d'export `emit-flow`/`emit-borrows`
conservent les lignes cumulées pour l'instant.

`doctor` utilise le véritable pipeline, pas une fixture prédéfinie. `init` crée
seulement un squelette local neuf. `--sanitize` transmet les options de détection
mémoire au compilateur C. Le pipeline natif et `doctor` ont été exécutés sous
Linux avec GCC et Clang ; voir le rapport de validation.

## LSP and recoverable I/O

`nacre lsp` échange des messages JSON-RPC avec framing `Content-Length`.
Il réutilise `SourceBundle::validate` et `SourceBundle::locate`, sans compilateur C,
sur des snapshots bornés de sources disque et de buffers non sauvegardés.
Les positions en octets du frontend sont converties en lignes/colonnes UTF-16
pour le protocole ; les notes deviennent des informations liées.
`nacre.json` déclare les chemins source d’un projet ; sans marqueur, un document
est compilé seul. Les clients VS Code et Zed lancent ce même serveur.

Les corps C d’E/S produisent les véritables spécialisations IR de `Option`/`Result`.
Ils valident UTF-8, ranges et conversions int32, avec erreurs textuelles possédées.
Les buffers de console/fichier sont transférés au `Text` retourné sans recopie
finale. L’entrée C reçoit `argc/argv` ; `Args` expose les arguments sans `argv[0]`.
Les échecs d’allocation et les erreurs fatales historiques gardent leur arrêt sans
déroulement de pile ; les retours `Err` suivent les nettoyages normaux.

