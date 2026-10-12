# Sources, projects, and LSP — V8

Toutes les commandes de compilation acceptent un ou plusieurs chemins après la
commande, avec les options aux emplacements usuels :

```sh
nacre check src
nacre run src/Main.ncr src/Statistics.ncr --memory-stats
nacre build src -O2 -o out/app
nacre emit-c src -o out/app.c
nacre emit-llvm src -o out/app.ll
nacre run src -- argument1 "argument avec espaces"
nacre lsp
```

Les dossiers sont parcourus récursivement dans un ordre trié. Les fichiers `.ncr`
sont sélectionnés ; `target`, `out` et les noms cachés sont ignorés. Les liens
symboliques sources sont refusés. Un fichier fourni explicitement est lu tel quel.
Les doublons de chemins canonisés sont refusés. Limites : 64 fichiers, 1 Mio
cumulé, profondeur de dossiers 32. Les fichiers doivent être en UTF-8.

## Pas encore de modules

Les déclarations sont fusionnées après le lexing et le parsing séparés de chaque
fichier. Toutes les classes, interfaces et fonctions partagent un espace de noms.
Une déclaration peut utiliser un type déclaré plus loin. Les noms en double et
les points d’entrée multiples sont des erreurs. Il n’y a ni `import`, ni
`package`, ni compilation incrémentale ou séparée, ni résolution de dépendances.

Les diagnostics de compilation remappent la position principale et chacune de
leurs notes vers le fichier d’origine. Les erreurs d’exécution disposent d’une
petite table de sources lorsqu’il y a plusieurs fichiers. Les exports d’analyses
`emit-flow` et `emit-borrows` utilisent encore les lignes du texte cumulé ; ils
ne constituent pas un format de diagnostic stabilisé.

## Init et doctor

`nacre init nouveau-dossier` exige un chemin inexistant. Il crée deux fichiers
sources, un README, un `.gitignore` et un marqueur `nacre.json` contenant
`{"sources":["src"]}`. Aucune commande externe ni gestion de paquets n’est lancée.
Une erreur d’écriture peut laisser un dossier partiel :
aucun chemin existant n’est supprimé automatiquement.

`nacre doctor` vérifie le frontend puis compile et exécute une courte application
List/Text. Il exige la sortie `test=42` et un bilan de 4 allocations/libérations.
Son succès est un test de fumée, pas une validation exhaustive du langage.

## API Rust

```rust
use nacre::project::SourceBundle;

let sources = SourceBundle::from_sources(vec![
    ("C.ncr".into(), "class C { readonly int value(){return 42;} }".into()),
    ("Main.ncr".into(), "void main(){C c=new C();println(c.value());}".into()),
]).expect("sources invalides");

let c = sources.compile_to_c().unwrap_or_else(|error| {
    panic!("{}", sources.render(&error));
});
let result = nacre::native::compile_c(&c).unwrap().output().unwrap();
assert!(result.status.success());
assert_eq!(result.stdout, b"42\n");
```

`from_paths` charge les chemins disque. `validate`, `emit_flow`, `emit_borrows`
et `emit_types` utilisent le même bundle et les mêmes analyses.
L’API à source unique reste disponible. `locate(span)` expose le fichier, le
texte local et la plage en octets utilisés pour construire les diagnostics LSP.

## Sorties et protections

Tous les fichiers sources chargés sont protégés contre l’écrasement par une
sortie. Sur Unix, le contrôle prend aussi en compte les hard links en comparant
périphérique et inode. Ce contrôle n’est pas une garantie contre un adversaire
modifiant simultanément l’arborescence pendant la compilation.
Les processus C sont lancés avec des arguments séparés, jamais via un shell.

## Portée des diagnostics d’éditeur

Un marqueur à la racine déclare la compilation multifichier :

```json
{ "sources": ["src"] }
```

Les chemins sont relatifs au marqueur ; ils désignent des fichiers `.ncr` ou des
dossiers parcourus récursivement. Les chemins absolus et ceux sortant de la
racine sont refusés. Les limites de sources et l’interdiction de liens symboliques
s’appliquent aussi au LSP. `examples/project/nacre.json` déclare l’exemple multifichier.
Sans marqueur, chaque document ouvert est indépendant : les exemples ayant chacun
un `main` ne sont pas fusionnés par simple voisinage.

Le serveur accepte les snapshots complets de `didOpen`/`didChange`, les sauvegardes,
fermetures et changements de dossiers workspace. Les versions périmées ne remplacent
pas un buffer plus récent. Les buffers ouverts remplacent les fichiers disque,
y compris un nouveau fichier non encore sauvegardé.
Le premier diagnostic du frontend est publié avec sa plage UTF-16 et ses notes
liées ; les diagnostics résolus sont effacés. Une configuration ou un chargement
de sources invalide est signalé par `window/logMessage`, pas présenté comme une
validation réussie. Les diagnostics sont testés headless via `nacre lsp`.

