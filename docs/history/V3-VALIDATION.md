# Validation de livraison — JavaNative V3 / 0.4.0

## Statut réel

**Le compilateur Rust n'a pas été compilé dans cet environnement. Aucun de ses
585 tests Rust n'a été exécuté ici.** `rustc` et `cargo` sont absents. La tentative
`cargo test --offline --locked` échoue avant toute compilation, faute d'exécutable.
L'accès réseau direct n'a pas permis de récupérer une toolchain. Le frontend GCC
Rust n'est pas disponible non plus.

Les 585 tests constituent un **inventaire statique**, pas une liste obtenue par
`cargo test --list`, ni 585 réussites. Il peut rester des erreurs Rust, des erreurs
d'intégration ou des défauts dans les analyses et l'émission C. Le projet est un
prototype source, pas un compilateur de production validé.

Rapport de tentative : [end-to-end-attempt.json](end-to-end-attempt.json).

## Vérifications exécutées

| Vérification | Résultat observé | Ce que cela ne vérifie pas |
|---|---|---|
| Runtime C exact avec GCC et Clang | Réussi | Parser, typage, ownership, emprunts, génération C. |
| Fixture C manuelle d'objets possédés | Réussi avec ASan/UBSan et détection des fuites, GCC/Clang | Code produit par le compilateur Rust. |
| Nouvelle fixture C manuelle d'emprunts/vues | Réussi avec et sans sanitizers, GCC/Clang | Analyse des références ou émission C par Rust. |
| Synchronisation des suites générées V2/V3 | Réussie | Validité des résultats attendus de tous les tests. |
| Manifestes, JSON, scripts Python, chemins `include_str!` | Réussis | Compilation ou type-check Rust. |
| Équilibrage des délimiteurs des fichiers Rust | Réussi | Grammaire complète, emprunts Rust, types ou exécution. |
| Compilateur Rust et programmes générés | **Non exécutés** | Validation de bout en bout à effectuer. |
| CI GitHub fournie | **Non exécutée ici** | Aucune exécution distante revendiquée. |

La nouvelle fixture d'emprunts effectue **20 001 allocations et 20 001 libérations**
par exécution, sans objet vivant à la sortie. Elle vérifie notamment qu'une vue
peut détruire son enfant possédé sans détruire sa cible empruntée. La fixture est
**écrite à la main**, avec le vrai `src/runtime.c` préfixé par le script. Ce nombre
ne provient pas d'un programme `.jvn` compilé par la V3.

Pour les tests de cette fixture sous Linux, les builds avec sanitizers utilisent
`-fno-pie -no-pie` afin d'éviter les collisions d'espace d'adressage ASan/PIE en
conteneur. Les vérifications ASan/UBSan restent actives ; la détection des fuites
est demandée par `ASAN_OPTIONS=detect_leaks=1:halt_on_error=1`.

## Rapports joints

- [Runtime C](runtime-validation.json).
- [Fixture d'objets possédés](object-support-validation.json).
- [Fixture de pointeurs empruntés](borrow-support-validation.json).
- [Audit structurel limité aux délimiteurs Rust](rust-structure-audit.json).
- [Audit de packaging](package-audit.txt).
- [Inventaire des tests non exécutés](test-inventory.json).

Les scripts `test_object_support.py` et `test_borrow_support.py` annoncent eux-mêmes
leur périmètre limité. Ils ne remplacent jamais le résultat d'un compilateur Rust
par celui d'une implémentation de référence. L'audit de délimiteurs n'est pas un
parser Rust et ne doit pas être présenté comme une validation syntaxique complète.

## Tests livrés

Les suites de la V2 sont conservées. La V3 ajoute 133 cas de syntaxe/typage/emprunts,
46 vérifications d'exécutables natifs et 17 tests de diagnostics, inspection et CLI.
Total : 585 tests selon le comptage statique du dépôt.

Les cas V3 couvrent les modes partagé/exclusif, la libération des prêts en sortie
de bloc et d'appel, les réemprunts, les conflits entre paramètres aliasés, `readonly`,
les réservations de receveur, les boucles, les champs empruntés, les vues locales,
les tentatives d'échappement et les nettoyages attendus.

Aucune affirmation de sûreté formelle, de performance ou d'équivalence au borrow
checker de Rust n'est faite. La V3 est un modèle lexical plus restrictif.

## Valider sur une machine équipée

Depuis la racine du projet :

```bash
cargo test --offline --locked
JVN_CC=clang cargo test --offline --locked

python3 scripts/validate.py --cc gcc --cc clang --sanitizers \
  --report docs/local-validation.json
```

Le dernier script compile le **vrai** projet Rust, puis utilise son exécutable
`jvn` pour produire du C et le soumettre aux compilateurs C et aux sanitizers.
Les cas négatifs doivent être rejetés avec le diagnostic attendu. Les cas natifs
doivent produire leur sortie et leur bilan mémoire attendus. Un outil manquant,
un test échoué ou un défaut détecté interrompt la validation.

Pour vérifier seulement le frontend, sans compilateur C :

```bash
cargo test --offline --locked --lib --test frontend --test objects_frontend \
  --test ownership_frontend --test flow_diagnostics \
  --test borrowing_frontend --test borrowing_diagnostics
```

Les plateformes initialement visées sont Linux et WSL. macOS n'a pas été testé.
Le minimum Rust 1.74 est déclaré, non vérifié dans cet environnement ; une matrice
CI Rust 1.74/stable et GCC/Clang est fournie pour l'établir.
