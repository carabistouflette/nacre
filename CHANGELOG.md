# Nacre — historique

## V6 stabilization / 0.7.1

- Clean Nacre cutover: `nacre` crate and CLI, `.ncr` sources, `nacre.json`
  project marker, Nacre editor language IDs, and `NACRE_*` environment selectors.
- `Files.writeTextAtomic` writes a sibling temporary file and commits using
  `rename`; Sudoku saves now use it. Atomic visibility does not imply crash
  durability; replacement files use `mkstemp` permissions.
- Real frontend and LSP framing libFuzzer targets with bounded campaigns and
  intentional seed corpora.
- VS Code lifecycle operations serialize activation, restart, and shutdown.
  Restart after failed server startup no longer attempts to stop an inactive
  client.
- Process-argument storage is emitted with the generated executable, rather
  than the standalone runtime; strict GCC/Clang fixture builds remain valid.
- Regenerated the renamed Zed adapter lockfile for locked WASI builds.
- Historical captured validation reports and original legal attribution are
  retained without relabeling their recorded outputs.

## V6 / 0.7.0

- Enums nominaux génériques, variantes positionnelles et `match` exhaustif
  consommant. `Option<T>` et `Result<T,E>` préinjectés ; sommes affines par
  valeur sans allocation d’enveloppe, destruction de la variante active.
- Validation des layouts inline, cas de base des structures récursives,
  payloads possédés et nettoyage sur retours/sorties de boucle. Les paramètres
  internes du prelude ne masquent plus des types utilisateur `T`/`E`.
- Console UTF-8, EOF explicite, affichage sans saut de ligne, fichiers texte
  avec erreurs récupérables et arguments natifs. Les buffers lus sont
  transférés au `Text` sans copie finale ; les erreurs d’allocation restent fatales.
- `Text.parseInt` strict avec limites int32 et `sliceBytes` à frontières UTF-8
  vérifiées. Pas d’exceptions ni de conversion silencieuse octets/caractères.
- Serveur `nacre lsp` utilisant le frontend : snapshots complets, buffers non
  sauvegardés, diagnostics UTF-16, projets `nacre.json` et effacement des erreurs
  résolues. Clients VS Code/Zed `0.2.0`, grammaires et snippets V6.
- Sudoku interactif : coups, protection des indices, résolution, sauvegardes
  séparant indices/coups et chargement validé avant mutation ; chemins avec espaces.
- Tests de forme du C émis supprimés au profit des exécutions natives ; tests
  de transitions, erreurs, limites, ownership et protocole LSP conservés/ajoutés.
- Résultats de livraison et limites dans [VALIDATION.md](docs/VALIDATION.md).


## Corrections de maintenance V5

- Support `.ncr` pour VS Code : grammaire TextMate, commentaires, délimiteurs,
  indentation et snippets ; empaquetage VSIX local et validation du tokenizer.
- Support Zed : grammaire Tree-sitter Nacre V5, coloration, outline,
  indentation, délimiteurs et snippets partagés. Préparation d'une extension de
  développement avec dépôt de grammaire local, sans publication ni serveur de langage.
- Exemple `sudoku.ncr` : grille 9×9, validation des chiffres et des contraintes,
  résolution par backtracking sans copie de grille, affichage de la solution.
  Tests natifs des unités résolues, de la conservation des indices et de la
  restauration d'une grille sans solution. Pas d'entrée console en V5.
- Calcul saturant de la taille des états d'ownership via `saturating_mul`,
  sans changement de la limite mémoire ni du diagnostic de dépassement.
- Noms des tests V5 conformes à snake_case, y compris leur source JSON ;
  suppression d'une copie de chemin inutile dans les tests multifichiers.
- Compilation et exécution réelles sous Linux : 916 tests Rust réussis,
  deux tests LLVM ignorés par défaut, validation native GCC/Clang réussie et
  `cargo clippy --offline --locked --all-targets -- -D warnings` sans avertissement.
- Documentation de validation actualisée ; les statuts des livraisons initiales
  ci-dessous sont conservés comme historique.

## V5 / 0.6.0

- Bibliothèque native `List<T>` : croissance, réserve, lecture de valeurs
  copiables, remplacement, extraction possédée, nettoyage des éléments.
- `Text` possédé : construction UTF-8 dynamique, ajout de valeurs, clone explicite,
  égalité de contenu, affichage et accès aux octets. `String` reste statique.
- Propriété d'objets/interfaces/listes dans les collections ; types récursifs
  `Node → List<Node>` avec cas de base vide. Interdiction des vues locales comme
  éléments et des getters d'objets qui créeraient des références non suivies.
- Prelude de contrats et templates C natifs, explicitement dans la base de
  confiance. Instanciation à la demande, donc pas de bibliothèque inutilisée
  ajoutée à la génération des programmes historiques.
- Compilation multifichier/dossiers via `SourceBundle`, analyse séparée de
  chaque unité puis espace de noms partagé ; diagnostics et erreurs runtime
  rattachés aux fichiers. Pas de packages ni compilation séparée.
- CLI `init`, `doctor`, `--sanitize` ; protection des sources multiples contre
  l'écrasement. Rendu des diagnostics robuste aux positions UTF-8 mal alignées.
- Exemples de collections, objets extraits, arbre possédé, texte dynamique et
  projet multifichier. 918 tests Rust déclarés au total, **non exécutés ici**.
- Validation locale étendue, tests C des templates réels avec opérations
  aléatoires et ASan/UBSan. Correction d'une assertion historique sensible à
  l'espacement dans une signature C de destructeur ; pas de changement de son
  contrat mémoire.

**Validation de livraison :** Rust/Cargo toujours absents. Quatre exécutions
positives des templates C (GCC/Clang, normal et sanitizers), 50 000 opérations
aléatoires et 13 561 allocations/libérations équilibrées par exécution ; 16 cas
négatifs contrôlés. Runtime arithmétique C testé séparément. **Ce n'est pas une
validation du compilateur Rust ni de son C réellement généré.**

**Limites :** emprunts toujours lexicaux, pas de référence retournée ni getter
emprunté d'élément, pas de remplacement de champ possédé, `Option`/`Result`, E/S,
modules ou backend LLVM direct. Voir `docs/ROADMAP.md`.

---

## V4 / 0.5.0

- `interface` et `implements` : contrats nominaux, méthodes publiques d'instance,
  mode `readonly`, paramètres possédés ou empruntés et retours possédés.
- Conversion implicite classe → interface, déplacement du propriétaire conservé,
  destruction dynamique et origines d'emprunts préservées.
- Backend C avec enveloppes d'interface à deux pointeurs, tables constantes et
  adaptateurs typés ; pas d'allocation d'enveloppe ni de comptage de références.
- Templates de classes et d'interfaces, arguments de type explicites,
  spécialisation nominale via le nouveau `src/generics.rs` et bornes d'expansion.
- Commande `emit-types`, commande `emit-llvm` **via le C et Clang**, niveaux
  `-O0`, `-O1`, `-O2`, `-O3`, `-Os` ; pas de backend LLVM direct.
- Sept exemples positifs V4, cas négatifs dédiés, 109 cas frontend, 53 cas natifs
  et 19 tests d'outils. Inventaire : 766 tests Rust, dont 2 tests LLVM ignorés par
  défaut ; **aucun de ces tests n'a été exécuté ici**.
- Validation locale scriptée étendue aux cas V4 et option `--llvm`. CI fournie,
  non exécutée dans cet environnement. Documentation V3 archivée dans
  `docs/history/` avant réécriture des guides courants.

**Validation de livraison :** Rust/Cargo toujours indisponibles. Runtime et
fixtures C manuelles exécutés avec GCC/Clang et sanitizers ; aller-retour C → LLVM
→ exécutable effectué sur la fixture d'interfaces. Ces résultats ne valident pas
le compilateur Rust ni le code qu'il génère. Voir `docs/VALIDATION.md`.

**Limites :** pas d'héritage, de corps de méthode d'interface, de conversions
dynamiques, de génériques de fonctions, de bornes/variance/inférence de type,
ni de backend LLVM direct. Le corps d'un template non demandé n'est pas
entièrement vérifié. Les emprunts restent lexicaux et les restrictions V3 sur
les retours de références et les vues locales sont conservées.

---

## V3 / 0.4.0

- Types `borrow C` et `borrow mut C` pour variables et paramètres ; emprunts implicites au site d'appel.
- Méthodes `readonly`, vérifiées transitivement sur le receveur et ses champs possédés.
- Nouveau `src/borrowing.rs` : prêts lexicaux, capacités de réemprunt, conflits partagés/exclusifs et diagnostics E300x.
- Réservations de receveur en deux phases pour conserver les appels imbriqués V2 sans autoriser l'invalidation du receveur.
- Vues locales à champs `borrow` partagés, avec capture des origines depuis les paramètres du constructeur.
- Distinction `Object` / `Borrow` dans l'IR et le backend : aucun nettoyage du référent pour un paramètre ou champ emprunté.
- Commande `emit-borrows` et nouvelles vérifications CLI.
- 133 cas frontend V3, 46 cas natifs V3, matrices d'aliasing et diagnostics ; 585 tests Rust au total selon inventaire statique.
- Exemple principal, réemprunts, composition readonly, emprunts dans les boucles et vues locales.
- Conservation des suites V0/V1/V2 ; mise à jour du test de version de la CLI.

**Validation de livraison :** Rust/Cargo indisponibles ; aucun test Rust exécuté.
Runtime C et fixtures C manuelles vérifiés avec GCC/Clang et sanitizers. Les fichiers
`docs/VALIDATION.md` et les rapports JSON distinguent ces vérifications.

**Hors périmètre :** retours empruntés, NLL, références réaffectables, champs
empruntés mutables et vues exportables. Les champs possédés restent non remplaçables.

---

## V2 — version Cargo 0.3.0

Nouvelle passe `ownership.rs` : graphe de contrôle explicite, états non initialisé /
disponible / déplacé, et propagation au point fixe. Les transferts dans les boucles
sont autorisés lorsque les chemins sont sûrs, avec prise en compte des sorties
`break`, `continue`, `return` et du court-circuit logique.

Déclarations locales différées, intrinseques `move(...)` et `drop(...)`, diagnostics
`E2001` / `E2002` avec notes, commande `emit-flow`, option `--memory-stats`.
Le backend initialise les emplacements différés pour rendre leurs nettoyages sûrs.
Les anciens transferts implicites, les classes et les commandes V1 sont conservés.

Les interdictions V1 de déplacer puis sortir d’une boucle ont été remplacées par des
tests d’acceptation ; les tests de lecture après déplacement sont conservés. Le test
de déclaration sans initialiseur devient un test d’acceptation. Les champs objets
restent non remplaçables ; aucun borrow checker général n’est annoncé.

Cinq exemples positifs, quatre exemples négatifs, un manifeste de 81 cas V2 et des
tests du solveur, des diagnostics et de la CLI. Le script de validation compile
les sorties du vrai frontend avec sanitizers lorsqu’une toolchain est disponible.

**Statut : Rust/Cargo non disponibles ici ; tests Rust non exécutés.** Les contrôles
du runtime C et de la fixture C indépendante ont été relancés avec succès.
Voir `docs/VALIDATION.md` pour le périmètre exact.

## V1 — version Cargo 0.2.0

Ajout des classes instanciables, constructeurs, `new`, `this`, champs, méthodes
d'instance et statiques, appels qualifiés, plusieurs classes par fichier, et visibilité
`public`/`private`. Les champs peuvent posséder d'autres objets pour la composition.

Un socle de propriété unique accompagne ces objets : transferts de variables,
paramètres et retours ; emprunt temporaire des receveurs ; libération lexicale et
nettoyage des retours/sorties de boucle. Ce socle anticipe une partie du chantier
mémoire initialement prévu après la V1, sans prétendre être un borrow checker complet.

Les champs doivent être initialisés ; les champs objets sont fixes après leur
initialisation ; les déplacements d'objets extérieurs à une boucle sont refusés à
l'intérieur. Pas encore d'héritage, d'interfaces, de références persistantes ou de
comptage de références.

Six nouveaux exemples valides et quatre exemples invalides ; 139 tests supplémentaires
par rapport aux 107 tests de la V0, soit 246 tests Rust écrits. Les tests V0
concernés par les objets et les classes ont été adaptés aux nouvelles règles. Une CI et un
script de validation de bout en bout sont fournis, mais n'ont pas été exécutés ici.

**Statut : prototype non compilé en Rust dans l'environnement de livraison.** Seul
le runtime C et une fixture C indépendante ont été exécutés ici. Consulter le rapport
de validation avant d'interpréter les tests comme une preuve de fonctionnement.

## V0 — version Cargo 0.1.0

Syntaxe Java limitée à un conteneur de méthodes statiques ou à des fonctions libres,
types scalaires, chaînes statiques, contrôle de flux, génération C11 et pilote natif.
