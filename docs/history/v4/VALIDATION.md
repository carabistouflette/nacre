# Validation de livraison — JavaNative V4 / 0.5.0

## Statut réel

**Le compilateur Rust n'a pas été compilé dans cet environnement. Aucun de ses
766 tests Rust n'a été exécuté ici.** `rustc` et `cargo` sont absents ; la tentative
`cargo test --offline --locked` échoue avant compilation faute d'exécutable.
Les tentatives de récupération d'une toolchain n'ont pas abouti.

Le nombre 766 est un **inventaire statique**, et non une sortie de
`cargo test --list` ou un nombre de réussites. Deux de ces tests sont ignorés par
défaut parce qu'ils demandent explicitement Clang/LLVM. Des erreurs de compilation
Rust, d'intégration, d'analyse ou de génération C peuvent subsister. La V4 est un
prototype source non validé de bout en bout, pas un compilateur de production.

La présence de `Cargo.lock` et l'absence de dépendances externes ne dispensent pas
d'installer le compilateur et la bibliothèque standard Rust.

Tentative enregistrée : [end-to-end-attempt.json](end-to-end-attempt.json).

## Vérifications réellement exécutées

| Vérification | Résultat observé | Limite |
|---|---|---|
| Runtime C exact avec GCC et Clang | Réussi | Aucun frontend Rust impliqué. |
| Fixture C manuelle d'objets possédés | Réussie, avec sanitizers | Ne teste pas le code généré. |
| Fixture C manuelle d'emprunts/vues | Réussie, avec sanitizers | Ne teste pas l'analyse des prêts. |
| Nouvelle fixture C manuelle d'interfaces | Réussie avec GCC/Clang, sans et avec ASan/UBSan | Ne teste pas les conversions ou contrats du frontend. |
| Fixture d'interfaces : C → LLVM → exécutable | Réussie via Clang | Ce n'est pas une exécution de la commande Rust `emit-llvm`. |
| Synchronisation des suites JSON/Rust V2/V3/V4 | Réussie | Ne vérifie pas les résultats attendus des cas. |
| JSON, scripts Python, chemins `include_str!`, inventaire | Réussis | Ne compile pas le Rust. |
| Audit limité des chaînes de format Rust | Réussi | Ne couvre pas toute la syntaxe des macros. |
| Audit de délimiteurs Rust | Réussi | Pas un parser Rust complet, pas de typage. |
| Compilateur Rust, tests Cargo, véritables sorties `.jvn` | **Non exécutés** | Validation indispensable sur une machine équipée. |
| CI GitHub fournie | **Non exécutée ici** | Aucune exécution distante revendiquée. |

La fixture d'interfaces est **écrite à la main** dans
`tests/fixtures/interface_support.c`. Le script lui préfixe le vrai runtime du
projet. Elle couvre le dispatch typé, une destruction dynamique, des retours et
paramètres d'interface possédés, des champs propriétaires, des enveloppes
empruntées et des structures spécialisées scalaires.

Chaque exécution de cette fixture réalise **50 004 allocations, 50 004 libérations
et zéro objet vivant en sortie**. Les cinq exécutions observées correspondent à
GCC normal/sanitizers, Clang normal/sanitizers et Clang via LLVM. Ce comptage ne
provient pas d'un exemple `.jvn` compilé par JavaNative.

Sous Linux, les fixtures avec sanitizers utilisent `-fno-pie -no-pie` pour éviter
les collisions ASan/PIE en conteneur. ASan, UBSan et la demande de détection de
fuites restent actifs. Une réussite sur ces fixtures ne prouve pas l'absence de
tout défaut mémoire et encore moins la correction de l'analyse statique Rust.

## Rapports joints

- [Runtime C](runtime-validation.json).
- [Fixture d'objets possédés](object-support-validation.json).
- [Fixture d'emprunts/vues](borrow-support-validation.json).
- [Fixture d'interfaces et aller-retour LLVM](interface-support-validation.json).
- [Audit structurel des délimiteurs Rust](rust-structure-audit.json).
- [Audit limité des chaînes de format](format-string-audit.json).
- [Audit de packaging](package-audit.txt).
- [Inventaire des tests non exécutés](test-inventory.json).

Les scripts de fixtures annoncent explicitement leur périmètre. Ils n'exécutent
pas une implémentation de référence à la place d'un compilateur en échec.
Les guides et rapports V3 sont conservés dans `docs/history/` à titre historique,
et ne décrivent pas le statut courant de la V4.

## Tests livrés

Les 585 tests de la V3 sont conservés. Les nouveaux fichiers ajoutent 109 tests de
frontend, 53 tests natifs et 19 tests d'outils, dont deux LLVM ignorés par défaut.
Les suites générées sont synchronisées avec `tests/v4-cases.json`.

Leur couverture prévue comprend les contrats d'interfaces, conversions nominales,
déplacements, destruction dynamique, emprunts conservant leur origine, aliasing,
champs propriétaires et empruntés, spécialisation imbriquée, arité et bornes des
génériques, variantes primitives/objets, inspection, CLI et niveaux d'optimisation.
« Couverture prévue » ne signifie pas que ces comportements ont été observés.

Aucune mesure de performance, preuve formelle de sûreté, équivalence avec Rust,
ni compatibilité complète avec Java n'est revendiquée.

## Valider le vrai compilateur

Depuis la racine du projet, avec Rust/Cargo et les outils C installés :

```bash
cargo test --offline --locked
JVN_CC=clang cargo test --offline --locked

# Les deux tests LLVM explicitement ignorés par défaut :
cargo test --offline --locked --test features_v4_tools -- --ignored

# Frontend réel → C, puis compilation/exécution avec sanitizers ;
# les exemples V4 passent également par l'export LLVM :
python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm \
  --report docs/local-validation.json
```

`validate.py` compile le **vrai projet Rust**, utilise son binaire pour vérifier
les exemples négatifs et générer le C, puis vérifie les sorties et les bilans
mémoire. Avec `--llvm`, les exemples V4 sont aussi exportés en IR par la vraie
commande `emit-llvm`, compilés avec Clang et exécutés. Une toolchain manquante,
un test échoué ou un diagnostic sanitizer interrompt cette validation.

Pour ne lancer que les analyses frontend, sans compilateur C :

```bash
cargo test --offline --locked --lib --test frontend --test objects_frontend \
  --test ownership_frontend --test flow_diagnostics \
  --test borrowing_frontend --test borrowing_diagnostics \
  --test features_v4_frontend
```

Pour reproduire séparément le test C manuel effectué ici :

```bash
python3 scripts/test_interface_support.py --cc gcc --cc clang
```

Les plateformes initialement visées sont Linux et WSL. macOS n'a pas été testé.
Le minimum Rust 1.74 est déclaré, non vérifié ; la matrice CI Rust 1.74/stable et
GCC/Clang est fournie pour le vérifier. Les optimisations et l'export LLVM ne sont
pas une validation supplémentaire du frontend.
