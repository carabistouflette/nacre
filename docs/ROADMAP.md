# Ce qu’il reste à faire après la V6

## Acquis de V6

La V6 ajoute enums génériques, `Option`/`Result`, branches `match` exhaustives
consommant, E/S console/fichiers UTF-8, conversions int32, copies de plages par
octets et arguments natifs. Le Sudoku est interactif et conserve indices/coups
séparément dans ses sauvegardes.

`nacre lsp` réutilise le frontend pour les diagnostics UTF-16 de buffers non
sauvegardés et de projets déclarés par `nacre.json`. VS Code et Zed ont des clients,
grammaires et snippets. Le LSP reste limité aux diagnostics ; les éditeurs ont
été vérifiés headless, pas visuellement dans leurs interfaces.

La validation native GCC/Clang, ASan/UBSan, LLVM via C, Clippy et la compilation
des targets Rust sous 1.74.0 sont consignées dans [VALIDATION.md](VALIDATION.md).
Ce sont des contrôles exécutés, pas une preuve générale de sûreté.

## 1. Maintenir et auditer la base

Maintenir la suite complète et les scénarios réels d’E/S/Sudoku/LSP en CI.
Le workflow fourni doit encore être exécuté sur un service CI ; vérifier aussi
les plateformes supplémentaires. Ajouter du fuzzing du lexer/parser, des
analyses et du framing LSP, avec des limites de ressources explicites.

Une revue indépendante des invariants de déplacement/destruction reste
nécessaire, surtout pour les combinaisons sommes/interfaces/génériques/emprunts.
`forbid(unsafe_code)` dans le frontend ne prouve pas la sûreté du C généré.

## 2. Retours empruntés et lecture de collections

Le manque mémoire/ergonomique principal reste la lecture d’un objet dans
`List<User>` sans extraction. Définir les références retournées et leur lien
au receveur avant d’exposer un pointeur d’élément.

Une lecture empruntée doit bloquer retrait, destruction et invalidation ; une vue
vers un buffer doit bloquer ses redimensionnements. Les contrats doivent rester
valables à travers les interfaces, fonctions et boucles. Un getter C direct
sans suivi d’origine n’est pas une solution.

Commencer par des contrats explicites aux frontières de fonctions/interfaces,
puis étudier fin d’emprunt à la dernière utilisation et projections distinctes.
Critère de sortie : consultation sans transfert, avec rejets adversariaux des
invalidations et alias incompatibles.

## 3. Extraction de champs et patterns empruntés

Les champs possédés restent fixés à la construction. Les enums peuvent les
contenir, mais la V6 ne fournit ni extraction/remplacement de champ propriétaire,
ni `match` emprunté. Une primitive d’échange typée ou un contrat de remplacement
avec état valide peut être plus simple que des déplacements partiels arbitraires.

`Option`/`Result` existent ; les opérations historiques comme `List.pop` vide
restent fatales. Étendre séparément les API récupérables et choisir une politique
d’allocation faillible. Aucun déroulement de pile en cas d’exception/panique
n’est promis par le modèle actuel.

## 4. Bibliothèque quotidienne

Ajouter selon des usages réels : itérateurs, dictionnaires, flottants,
opérations de texte plus riches et éventuellement E/S en flux. Distinguer
indices par octets, points de code et graphèmes ; `sliceBytes` vérifie les
frontières UTF-8 mais n’indexe pas des caractères.

Définir limites configurables et stratégie de destruction des arbres très
profonds. Une sauvegarde atomique demande une API et des garanties spécifiques ;
`Files.writeText` peut aujourd’hui laisser une sortie partielle après erreur.
La concurrence et la FFI exigent leurs propres contrats de sûreté.

## 5. Outils et projets

Étendre le LSP seulement après stabilisation des positions/snapshots : complétion,
définitions/références et informations de types. Ajouter un formateur et vérifier
les surfaces réelles Zed/VS Code. Les déclarations de fichiers partagent toujours
un espace de noms ; `nacre.json` ne fournit ni modules ni gestion de dépendances.

Modules/imports, visibilité intermodules, compilation séparée/incrémentale et
gestion de paquets demandent une conception explicite des contrats d’emprunt et
des modèles génériques exportés. `init` reste un générateur de projet.

## 6. Performances mesurées, puis options de produit

Mesurer compilation, taille des exécutables, allocations et performances sur
des programmes comparables avant toute promesse « comme Rust ».
La monomorphisation et l’optimisation C ne sont pas des benchmarks.

Un backend LLVM direct n’est pas nécessaire pour produire du natif sans JVM/GC ;
il ne doit pas précéder sûreté et bibliothèque utilisable. Héritage, exceptions
à la Java, réflexion et compatibilité avec l’écosystème Java restent des choix
séparés, pas des exigences automatiques.

## Ordre proposé

**Validation/audit → retours empruntés et collections → extraction de champs →
bibliothèque et outils enrichis → optimisation mesurée.**
