# Validation de livraison — Nacre V6 / 0.7.0

## Nacre 0.7.1 stabilization

Current local proof is recorded in [nacre-v6-stabilization.json](nacre-v6-stabilization.json).
The renamed compiler again completed 412 sanitized native successes, 14 expected
fatal runs, 424 invalid-source rejections, and 11 LLVM round trips. Rust 1.74
all-target compilation and the locked Zed WASI build pass.

Actual VS Code and Zed desktop sessions exercised Nacre highlighting, live
integer/boolean diagnostics and correction clearing, unsaved cross-file symbol
changes, language-server replacement on restart, and saving/closing/reopening.
VS Code also recovered through its restart command after a deliberately missing
server executable was corrected. Zed's isolated Xvfb session showed a benign
missing desktop-portal notification, separate from the language extension.

The bounded real [frontend and LSP framing campaigns](fuzz-stabilization.json)
completed without observed crashes or timeouts. This is finite fuzz evidence,
not a general safety proof.

## Remote CI — final result

The complete workflow passed on commit `915e09c` in [GitHub Actions run 38283995876](https://github.com/carabistouflette/nacre/actions/runs/38283995876). All six jobs succeeded: the editor build and tests, the Rust 1.74.0/stable × GCC/Clang matrix, and the nightly fuzz job. The ignored V4 tools test is Clang-only by workflow configuration and passed in both Clang jobs.

The final bounded campaigns executed 612,925 frontend inputs and 2,295,090
LSP-framing inputs in 31 seconds per target, with no observed crash or timeout.
These finite runs are not a general safety proof.

Earlier failures remain part of the record. Run [38281468493](https://github.com/carabistouflette/nacre/actions/runs/38281468493) failed on interface-fixture compiler warnings and a stale Zed lockfile; the fixture’s argument-storage path was repaired and the Zed lockfile regenerated. Its fuzz campaigns nevertheless completed without observed crashes or timeouts (787,489 frontend and 3,050,701 LSP-framing inputs). Run [38282654415](https://github.com/carabistouflette/nacre/actions/runs/38282654415) then failed on a stale template-substitution assertion in `scripts/test_stdlib_support.py`. The fixture was repaired to instantiate the shipped templates with inline Result layouts and owning Text errors, without stubs. Subsequent GCC and Clang O2 and ASan/UBSan runs passed with 13,561 allocations and frees per successful run, `live=0`, and all 16 expected error scenarios.

The V6 results below describe the original 0.7.0 run; captured historical reports
have not been rewritten as new Nacre executions.

## Statut réel

Le compilateur et ses exécutables natifs ont été réellement exécutés sous Linux
x86_64. La suite complète passe avec GCC puis Clang : **941 tests réussis et deux
tests LLVM ignorés par défaut** par exécution. Les deux tests LLVM passent aussi
lorsqu’ils sont lancés explicitement.

`python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm` passe :
**412 exécutions natives sous ASan/UBSan**, **14 erreurs fatales attendues**
vérifiées séparément sans sanitizers, **424 rejets de sources invalides** et
**11 allers-retours C → LLVM → exécutable**. Ces nombres sont les totaux du rapport,
pas des estimations de couverture.

Les scénarios V6 supplémentaires exécutent le véritable compilateur et ses
programmes générés : **22 exécutions sous GCC/Clang avec ASan/UBSan**, toutes avec
allocations/libérations équilibrées et `live=0`. Elles couvrent console/EOF/UTF-8
invalide, int32 minimum, sommes inline sans allocation, sommes dans les champs
et collections, arguments, fichiers à frontières de buffer, NUL/UTF-8,
modification des buffers retournés et Sudoku.
Le jeu a également été lancé dans un vrai terminal : coups, erreurs, indices
immuables, sauvegarde avec espaces, chargement et résolution ont été observés.

Clippy passe avec les avertissements traités en erreurs. **Rust 1.74.0 a été
vérifié par `cargo check --all-targets --offline --locked`**, avec son propre
répertoire de build ; cette vérification est une compilation, pas une deuxième
exécution de la suite sous Rust 1.74. La toolchain principale est Rust/Cargo
1.99.0-nightly. La livraison reste expérimentale, sans preuve générale de sûreté.

Rapports : [validation complète](v6-validation.json),
[scénarios réels V6](v6-smoke.json), [inventaire statique](test-inventory.json).
Le [rapport V5](end-to-end-attempt.json) est historique ; il ne certifie pas la V6.

## Contrôles par surface

| Surface | Vérification exercée |
|---|---|
| Types somme et ownership | Transferts par retour/appel/match, objets/interfaces/payloads imbriqués, nettoyage des boucles, refus des réutilisations, variantes non exhaustives et cycles inline. Une somme de primitifs s’exécute avec zéro allocation. |
| Console et texte | EOF distinct d’une ligne vide, LF/CRLF/dernière ligne, UTF-8 invalide, NUL, conversions int32 strictes et plages à frontières UTF-8. |
| Fichiers et arguments | Round-trip UTF-8/NUL, erreurs récupérables, indices/arguments invalides, croissance à 4096 octets, buffers retournés modifiables, arguments avec espaces/flags/Unicode. |
| Sudoku | Coups/remplacements/effacement, protection des indices, reconnaissance d’une grille complète, restauration après recherche impossible, sauvegarde/chargement, fichiers invalides sans mutation et fin de flux propre. |
| LSP réel | Processus `nacre lsp`, framing JSON-RPC, plages UTF-16, URI Unicode, éditions non sauvegardées, overlays multifichiers, effacement des diagnostics, JSON invalide, méthodes inconnues et shutdown/exit. Une requête `exit` invalide n’arrête plus le serveur. |
| VS Code | Vrai tokenizer TextMate/Oniguruma sur Sudoku et cas enum/match ; 13 snippets vérifiés par `nacre check` ; client LSP bundlé et VSIX `0.2.0` empaqueté. |
| Zed | 37 exemples analysés sans erreur par le parseur natif ; quatre requêtes compilées/exécutées ; adaptateur Rust et distribution préparée compilés en `wasm32-wasip2`. |
| Compatibilité | Suite GCC/Clang, Clippy et compilation de tous les targets Rust sous 1.74.0. |

Les extensions ont été vérifiées **headless** : ni VS Code ni Zed n’était installé.
Leur rendu visuel et le lancement du serveur depuis leurs interfaces n’ont pas
été observés. Le serveur a été exercé directement et les adaptateurs ont été
construits, sans faux serveur ni compilateur de remplacement.

## Reproduire

Un premier téléchargement des dépendances est nécessaire avant le mode offline :

```sh
cargo fetch --locked
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings
python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm \
  --report docs/local-validation.json
cargo run --offline --locked -- run examples/sums.ncr --sanitize --memory-stats
cargo run --offline --locked -- run examples/sudoku.ncr --sanitize --memory-stats
```

Contrôles ciblés :

```sh
cargo test --offline --locked --test sums_v6 --test io_v6 --test lsp_v6 \
  --test sudoku --test sudoku_v6
rustup toolchain install 1.74.0 --profile minimal
cargo +1.74.0 fetch --locked
cargo +1.74.0 check --offline --locked --all-targets --target-dir out/msrv-target
npm --prefix editors/vscode ci
npm --prefix editors/vscode test
npm --prefix editors/vscode run package
npm --prefix editors/tree-sitter-nacre ci
npm --prefix editors/tree-sitter-nacre run generate
npm --prefix editors/tree-sitter-nacre test
uv run --python 3.13 --with tree-sitter==0.25.2 python editors/zed/check.py
python3 editors/zed/prepare.py
rustup target add wasm32-wasip2
cargo build --manifest-path editors/zed/dist/nacre/Cargo.toml \
  --target wasm32-wasip2 --release --locked
python3 scripts/audit_package.py
```

Le workflow CI est fourni, mais n’a pas été exécuté sur un service CI pendant
cette livraison. Les contrôles Python historiques des templates C restent des
contrôles indépendants du frontend ; leurs rapports antérieurs ne sont pas
présentés comme une validation du nouveau runtime d’E/S.

## Risques encore ouverts

Pas d’audit indépendant ni de preuve formelle du modèle d’ownership/emprunts.
Les corps natifs appartiennent à la base de confiance. Les destructions récursives
peuvent épuiser la pile. Un arrêt fatal d’allocation ou de bounds ne déroule pas
les destructeurs ; un `Err` récupérable suit en revanche les nettoyages normaux.
`Files.writeText` n’est pas atomique et peut laisser un fichier tronqué/partiel.

Les programmes non fiables ne sont pas isolés par une sandbox. Les limites de
sources, messages LSP et parcours de dossiers ne sont pas une certification de
résistance aux entrées hostiles. Windows natif, macOS et les architectures 32 bits
ne sont pas validés ici. La coloration et les diagnostics ne remplacent ni
l’analyse indépendante de sûreté ni la vérification visuelle des deux éditeurs.
