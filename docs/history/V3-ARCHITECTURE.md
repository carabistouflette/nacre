# Architecture JavaNative V3

## Pipeline

```text
.jvn
  → lexer.rs : tokens et positions source
  → parser.rs : AST nominal, types borrow, modificateur readonly
  → checker.rs : noms, types, visibilité, contrats de vues, IR typée
  → ownership.rs : CFG, point fixe, initialisation et déplacements
  → borrowing.rs : prêts lexicaux et incompatibilités d'accès
  → codegen.rs : séquencement C11 et nettoyages déterministes
  → runtime.c + GCC/Clang + libc
  → exécutable natif
```

Le compilateur lui-même est du Rust standard sans dépendances externes.
`#![forbid(unsafe_code)]` interdit l'utilisation de blocs Rust `unsafe` dans le projet.
Cette propriété ne constitue pas une preuve que l'analyse du langage cible est
correcte ou que le C émis est sûr. Le projet Rust n'a pas été compilé ici.

## Représentations

L'AST contient les types `Named(String)` et `Borrow { name, mutable }`. L'IR remplace
les noms par des identifiants nominaux : `Object(class_id)` possède son pointeur,
`Borrow { class, mutable }` ne le possède pas. `Type::is_object()` signifie
strictement **possédé**. `object_class()` sert aux accès aux champs et méthodes,
indépendamment du mode de propriété.

Les fonctions gardent leur mode de receveur `readonly`. Les classes gardent un
tableau `borrowed_from` reliant chaque champ emprunté à son paramètre constructeur.
Les expressions `Borrow` sont produites contextuellement depuis les types attendus.
Les expressions `Move`/`Drop` explicites de l'IR V2 sont conservées.

## Typage et frontières d'abstraction

Les signatures exposent le contrat consommé/partagé/exclusif. La vérification d'un
appel n'analyse pas le corps du callee pour deviner s'il conserve un objet : ce
comportement relève de sa signature. Les méthodes `readonly` sont vérifiées dans
leur corps ; elles ne peuvent pas modifier leur receveur via un helper modifiant.

Le type checker rejette les retours empruntés, la réaffectation d'une référence,
les champs empruntés mutables et les échappements de vues locales. Il impose une
initialisation explicite et directe des champs de constructeur. Les références
aux objets incomplets ne peuvent être passées à un appel avant initialisation.

## Propriétaires et CFG

`ownership.rs` est conservé depuis la V2, avec la lecture de places empruntées
ajoutée. Chaque variable a un ensemble possible d'états : non initialisée (`U`),
disponible (`R`) ou déplacée (`M`). Les jonctions sont des unions ; un worklist
calcule un point fixe sur les boucles. Une utilisation exige l'état `R` sur tous
les chemins entrants.

Une variable empruntée a une initialisation et des lectures dans le graphe, mais
n'est pas un propriétaire à détruire. Le CFG reste une représentation d'analyse,
pas un MIR exécutable complet. `emit-flow` affiche ce graphe.

## Emprunts

`borrowing.rs` travaille sur la structure typée des blocs, après le CFG. Les
références locales ont des cibles fixes et ne peuvent s'échapper ; l'ensemble des
prêts externes est restauré à la fin de chaque bloc. Les prêts d'argument ont une
durée temporaire, sauf ceux capturés par une vue.

Une origine combine une racine et une capacité parent. Un prêt porte son mode,
son site source, son détenteur et son état actif. Les accès partagés, exclusifs et
destructifs sont comparés aux autres prêts actifs. Les réservations de receveur
protègent la durée de vie avant l'activation de leur accès après les arguments.

La passe trace les événements réellement vérifiés pour `emit-borrows`. Elle ne
réalise ni inférence NLL, ni séparation des champs possédés, ni calcul de lifetimes
génériques. Voir [BORROWING.md](BORROWING.md) pour les invariants et restrictions.

## Backend C

Les deux sortes d'objets sont représentées par des pointeurs vers des structures
C nominales. Les signatures de méthodes reçoivent un pointeur de receveur caché.
Le backend met les expressions dans des temporaires pour maintenir l'ordre
gauche-droite ; `&&` et `||` deviennent des blocs conditionnels.

Seuls les types `Object` participent au nettoyage : propriétaires locaux, paramètres
consommés et champs possédés. Les types `Borrow` ne sont jamais détruits. La fin
d'un prêt ne génère donc aucun travail de gestion mémoire à l'exécution. Les vues
restent des objets alloués et libérés, mais leurs cibles empruntées ne le sont pas.

Les remplacements, retours, `break` et `continue` émettent les nettoyages lexicaux
déjà présents en V2. Les variables déplacées deviennent `NULL` côté C, ce qui rend
les destructions conditionnelles idempotentes. Les sorties fatales du runtime
n'effectuent pas le déroulement des propriétaires.

## Tests et limites de validation

Les suites V0/V1/V2 sont conservées. Les cas V3 comprennent la syntaxe, les
mutations `readonly`, les conflits entre paramètres, les réemprunts, les boucles,
les vues, les diagnostics, l'inspection du C et les exécutables natifs attendus.
Les tests pré-générés proviennent des manifestes JSON versionnés.

Le script `validate.py` exécute le **vrai** compilateur Rust puis soumet le C généré
à GCC/Clang et, sur demande, aux sanitizers. Son exécution n'a pas été possible ici.
Les fixtures C manuelles et les audits de packaging livrés sont distincts de cette
validation. Voir [VALIDATION.md](VALIDATION.md).

## Évolutions non implémentées

Des lifetimes non lexicaux demanderaient une analyse de vivacité des prêts sur CFG.
Des références réaffectables demanderaient aussi la propagation d'ensembles
d'origines. Des retours empruntés ou vues exportables demanderaient des relations
de durée de vie dans les signatures. Des champs mutables empruntés demanderaient
des règles de transfert des capacités exclusives. L'héritage, les interfaces et
les génériques restent des chantiers séparés.
