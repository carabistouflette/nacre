# Nacre V6 — types somme, E/S récupérables et diagnostics d’éditeur

Prototype de compilateur écrit en **Rust** ; `serde_json` sert au protocole LSP.
Le programme cible est du code natif obtenu par **Nacre → C11 → GCC/Clang**,
sans JVM, sans GC et sans comptage de références. La bibliothèque C du système reste utilisée.
The Cargo package version is `0.7.1`; the original V0 began at `0.1.0`.

> **Validation locale : les programmes V6 et le LSP ont été réellement exécutés.**
> Les contrôles incluent transferts de propriété, erreurs d’E/S récupérables,
> Sudoku interactif, sanitizers et positions LSP UTF-16.
> Actual VS Code and Zed verification now covers highlighting, live diagnostics,
> unsaved cross-file overlays, server restart, and close/reopen. See
> [VALIDATION.md](docs/VALIDATION.md); these checks do not prove general safety.

## Démarrage

Il faut Rust/Cargo (version minimale déclarée : Rust 1.74) et un compilateur C11
avec ses en-têtes et son éditeur de liens. Cibles initiales : Linux/WSL.
Le frontend Rust et les exécutables générés ont été testés sous Linux. macOS et
Windows natif ne sont pas validés.

```sh
# Depuis le dossier du projet ; télécharger les dépendances avant le mode offline.
cargo fetch --locked
cargo test --offline --locked
cargo run --offline --locked -- doctor
cargo run --offline --locked -- run examples/collections.ncr --memory-stats
```

Sortie **attendue** du dernier programme :

```text
Somme = 42
20
12
1
```

Bilan attendu sur stderr : `NACRE_MEMORY allocations=4 frees=4 live=0`.
Les compteurs incluent les objets **et** leurs buffers, pas seulement les objets.

`doctor` compile le frontend et vérifie ses analyses lorsqu’on le lance via
`cargo run`, puis produit et exécute un petit programme natif utilisant `List`
et `Text`. Il vérifie la sortie et le bilan mémoire. Il échoue si le compilateur
C est absent ou si le résultat ne correspond pas. Ce n’est pas une preuve de sûreté générale.

## Un programme de collections complet

```java
void main() {
    List<int> scores = new List<int>();
    scores.add(10);
    scores.add(20);
    scores.add(12);

    int total = 0;
    int i = 0;
    while (i < scores.size()) {
        total = total + scores.get(i);
        i = i + 1;
    }

    Text message = new Text("Somme = ");
    message.appendInt(total);
    message.print();
}
```

Les listes et le texte sont des objets possédés : les sorties normales de portée
et les transferts utilisent le même modèle que les classes utilisateur.

## Nouveautés V6

| Ajout | Contrat |
|---|---|
| `enum`, `Option<T>`, `Result<T,E>` | Unions taguées par valeur, sans allocation d’enveloppe ; toutes les valeurs sont affines. |
| `match` exhaustif | Consomme la somme une fois et transfère ses payloads aux variables de branche. |
| Console et fichiers texte | Lecture de lignes, UTF-8, chargement/sauvegarde avec erreurs explicites, sans exceptions. |
| Texte | `parseInt` vérifie les limites int32 ; `sliceBytes` copie une plage à frontières UTF-8 valides. |
| Arguments | `Args.count/get` ; `nacre run ... -- arguments...` transmet les arguments au programme natif. |
| LSP et éditeurs | Diagnostics réels, buffers non sauvegardés, positions UTF-16, projets `nacre.json`, clients Zed/VS Code. |
| Sudoku | Jeu interactif, indices immuables, résolution, chargement et sauvegarde des coups. |

Voir [la grammaire](docs/GRAMMAR.md), [la bibliothèque](docs/STDLIB.md)
et [les projets](docs/PROJECTS.md). Exemple complet : [sums.ncr](examples/sums.ncr).

### Acquis des versions précédentes

| Ajout | Contrat conservé |
|---|---|
| `List<T>` | Liste dynamique de `int`, `boolean`, `String` statiques et objets possédés, y compris interfaces, `Text` et autres listes. |
| Extraction d’éléments | `pop` et `remove` transfèrent la propriété ; `set` détruit l’ancien objet ; `clear` détruit les éléments restants. |
| `Text` | Texte UTF-8 possédé : construction, ajout, formatage d’entiers/booléens, copie explicite, comparaison et affichage. |
| Plusieurs fichiers | Liste de fichiers ou dossier récursif `.ncr`, analyses communes et diagnostics rattachés au bon fichier. |
| Outils | `init`, `doctor`, `--sanitize`, rapports de validation explicitement marqués bloqués/échoués/réussis. |
| Robustesse | Diagnostics résistants aux offsets UTF-8 invalides, protection de tous les fichiers d’entrée contre l’écrasement. |

L’implémentation de `List` et `Text` est **native et privilégiée** : les contrats
sont dans `stdlib/prelude.ncr`, le code C dans `stdlib/*.c.in`. Ce n’est pas encore
une bibliothèque entièrement écrite dans Nacre. Les appels utilisateur
passent néanmoins par les mêmes vérifications de types, de propriété et d’emprunts.

### Listes d’objets : pas de copie cachée

```java
class Counter {
    int value;
    Counter(int value) { this.value = value; }
    readonly int getValue() { return this.value; }
}

void main() {
    List<Counter> counters = new List<Counter>();
    Counter counter = new Counter(42);
    counters.add(counter);       // Transfert, pas copie de référence.
    // counter.getValue();       // Doit être rejeté : counter est déplacé.

    Counter extracted = counters.pop();
    drop(counters);
    println(extracted.getValue()); // extracted reste propriétaire de son objet.
}
```

**`get` est réservé aux types copiables (`int`, `boolean`, `String` statique).**
`List<Counter>.get` n’est pas proposé : retourner un emprunt d’élément requiert
un modèle de durées de vie plus complet. Utiliser `pop`/`remove` pour extraire un objet.
Aucun pointeur vers un buffer redimensionnable n’est exposé.

La liste peut être vide : une classe `Node` contenant `List<Node>` est donc
possible, contrairement à un champ propriétaire direct `Node next` obligatoire.
Voir [owned_tree.ncr](examples/owned_tree.ncr). Les arbres très profonds peuvent
épuiser la pile lors d’une destruction récursive ; V6 ne garantit pas une pile bornée.

### Texte dynamique : distinct des chaînes statiques

```java
void main() {
    Text text = new Text("Réponse : ");
    text.appendInt(42);
    Text copy = text.clone();
    text.clear();
    copy.print();
}
```

`String` garde la sémantique précédente : vue copiable sur des littéraux statiques.
`Text` possède son buffer. `Text b = a` est un déplacement ; `a.clone()` est une
copie explicite. Il n’y a pas de `Text.toString()` qui laisserait s’échapper une
référence potentiellement pendante. `byteLength` et `byteAt` travaillent en
**octets UTF-8**, pas en caractères ou graphèmes. Les NUL intégrés sont conservés.

## Sudoku en Nacre

```sh
cargo run --offline --locked -- run examples/sudoku.ncr --memory-stats --sanitize
```

Le programme accepte des commandes au clavier. Lignes et colonnes vont de 1 à 9 ;
les indices initiaux ne peuvent être ni modifiés ni effacés.

```text
set 1 3 4
clear 1 3
save partie.sud
load partie.sud
solve
reset
show
help
quit
```

Le chemin de `save`/`load` occupe le reste de la ligne, sans guillemets ; les
espaces internes sont conservés. EOF quitte proprement. Les erreurs de commandes,
de nombres et de fichiers laissent la partie utilisable ; une erreur du flux
console est signalée puis termine la session, sans boucle de réessai infinie.

Charger une sauvegarde dès le lancement :

```sh
cargo run --offline --locked -- run examples/sudoku.ncr -- partie.sud
```

Format : exactement trois lignes ASCII terminées par LF (`172` octets) :
`SUDOKU1`, 81 chiffres d’indices, puis 81 chiffres de coups. `0` signifie absent ;
un coup doit être nul à une position d’indice. Le chargement valide tout avant de
modifier la partie. La recherche sans solution restaure la grille courante.
Tests : `cargo test --offline --locked --test sudoku --test sudoku_v6`.

## Éditeurs : VS Code et Zed

Les fichiers `.ncr` sont reconnus comme **Nacre**, avec coloration des
mots-clés V6 (`enum`, `match`, `borrow`, `mut`, `readonly`), types, variantes,
appels, littéraux et commentaires. Les deux intégrations fournissent commentaires,
délimiteurs, indentation et treize snippets. Zed ajoute un outline des déclarations.
Les deux clients lancent **`nacre lsp`** pour les diagnostics du véritable frontend.
Le serveur publie la première erreur du compilateur et efface les diagnostics
résolus ; il ne fournit ni autocomplétion sémantique, ni références, ni formateur.

### VS Code

Node.js 22 ou plus récent et npm sont requis pour empaqueter l'extension :

```sh
cd editors/vscode
npm ci
npm run package
code --install-extension nacre-0.2.0.vsix
```

Sans commande `code`, utiliser **Extensions → Install from VSIX…** et sélectionner
le fichier produit. Ouvrir ensuite `examples/sudoku.ncr` ; le mode de langage
doit être **Nacre**. Les snippets sont proposés avec les préfixes
`main`, `class`, `interface`, `constructor`, `readonly-method`, `while`, `if`,
`list`, `text`, `borrow`, `enum`, `variant`, `match`.
La grammaire TextMate est lexicale : les noms de types utilisateur sont
reconnus par contexte et conventions de nommage, pas par résolution sémantique.

Installer le compilateur dans le PATH avec `cargo install --locked --path .`
depuis la racine, ou configurer `nacre.serverPath` avec son chemin absolu
(`target/debug/nacre` après compilation). La commande **Nacre: Restart Language
Server** applique un changement de chemin sans réinstaller l’extension.

### Zed

Python 3, Git et Rust via rustup sont requis pour préparer et installer l’extension.
Le parseur Tree-sitter généré est inclus ; Node.js n’est pas requis pour cette installation.

```sh
python3 editors/zed/prepare.py
```

Dans Zed, lancer **`zed: install dev extension`** et sélectionner le dossier
`editors/zed/dist/nacre` indiqué par le script. Zed compile le parseur
en WebAssembly et télécharge son SDK WASI si nécessaire
([documentation Zed](https://zed.dev/docs/extensions/developing-extensions)).
Conserver `editors/zed/dist` en place. Après déplacement du projet ou modification
du parseur/des requêtes, relancer le script puis réinstaller l'extension de développement.
Le script crée un dépôt Git local du parseur dans le dossier généré ; il ne
modifie pas l'historique Git du projet et ne publie rien.

Le LSP utilise `nacre` du PATH de Zed. Pour choisir un binaire explicitement :

```json
{
  "lsp": {
    "nacre": {
      "binary": { "path": "/chemin/absolu/nacre", "arguments": ["lsp"] }
    }
  }
}
```

Pour les projets multifichiers, ajouter `nacre.json` à leur racine, par exemple
`{"sources":["src"]}`. Sans marqueur, le serveur ne compile que le document ouvert :
les programmes indépendants du dossier `examples` ne sont pas fusionnés.

### Vérification et modification des grammaires

```sh
cargo build --offline --locked
npm --prefix editors/vscode ci
npm --prefix editors/vscode test
npm --prefix editors/tree-sitter-nacre ci
npm --prefix editors/tree-sitter-nacre run generate
npm --prefix editors/tree-sitter-nacre test
uv run --python 3.13 --with tree-sitter==0.25.2 python editors/zed/check.py
```

Le contrôle VS Code exécute le vrai tokenizer TextMate/Oniguruma et vérifie les
snippets avec le compilateur local (`NACRE_BIN` permet de choisir un autre binaire).
Le contrôle Zed compile le parseur natif, analyse les exemples valides et
exécute les quatre requêtes utilisées par l'éditeur. Ces contrôles sont headless ;
ils ne remplacent pas une vérification visuelle dans les deux éditeurs.

## Projets multifichiers

```sh
cargo run --offline --locked -- run examples/project
cargo run --offline --locked -- check examples/project/Main.ncr examples/project/Statistics.ncr
cargo run --offline --locked -- build examples/project -O2 -o out/demo
./out/demo
```

Créer un nouveau projet, sans écraser un dossier existant :

```sh
cargo run --offline --locked -- init mon-projet
cargo run --offline --locked -- run mon-projet/src
```

Tous les fichiers partagent **un espace de noms**. Il ne s’agit pas encore de
packages, d’un système d’imports ou de compilation séparée. Chaque fichier est
lexé et parsé indépendamment : un commentaire ou une accolade manquante ne peut
pas être complété par le fichier suivant.

Limites : 64 fichiers et 1 Mio de texte source cumulé, séparateurs compris.
Les parcours de dossiers sont triés ; les dossiers cachés, `out` et `target` sont
ignorés. Les liens symboliques sources rencontrés sont refusés. Les références
entre types peuvent être résolues vers des fichiers déclarés plus loin.
Voir [PROJECTS.md](docs/PROJECTS.md).

## Commandes

```sh
cargo run --offline --locked -- check examples/collections.ncr
cargo run --offline --locked -- emit-c examples/collections.ncr -o out/demo.c
cargo run --offline --locked -- emit-flow examples/collections.ncr
cargo run --offline --locked -- emit-borrows examples/collections.ncr
cargo run --offline --locked -- emit-types examples/collections.ncr
cargo run --offline --locked -- emit-llvm examples/collections.ncr -O2 -o out/demo.ll
cargo run --offline --locked -- run examples/owned_list.ncr --memory-stats --sanitize
```

`emit-llvm` exige Clang et passe **par le C**, pas par un backend LLVM direct.
Niveaux : `-O0`, `-O1`, `-O2` (défaut), `-O3`, `-Os`.
`NACRE_CC` puis `CC` sélectionnent un exécutable C, sans arguments shell ;
`NACRE_CLANG` sélectionne Clang pour `emit-llvm`.

`--sanitize` ajoute AddressSanitizer et UndefinedBehaviorSanitizer au C généré.
Cette option dépend de leur disponibilité dans la toolchain C.
Les erreurs de limites, listes vides, division par zéro ou d’allocation sont
**fatales** (code de sortie 70), sans déroulement des destructeurs. Il n’y a pas
encore d’erreurs récupérables ; le nettoyage automatique est garanti par le
modèle prévu uniquement sur les chemins normaux, pas lors de ces arrêts.

## Tests et validation

```sh
cargo test --offline --locked
python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm \
  --report docs/local-validation.json
```

Le script utilise le véritable compilateur Rust, ses programmes générés et les
cas positifs/négatifs. Il n’utilise **aucun substitut** si Cargo est absent.
Le rapport livré [end-to-end-attempt.json](docs/end-to-end-attempt.json) consigne
la validation locale du véritable compilateur et de ses programmes générés.

Contrôles indépendants du Rust, exécutables avec Python et un compilateur C :

```sh
python3 scripts/test_stdlib_support.py --cc gcc --cc clang
python3 scripts/test_runtime.py --cc gcc --cc clang
python3 scripts/audit_package.py
```

Le premier script réutilise exactement les templates C livrés, mais les instancie
lui-même. Cela ne valide **ni le frontend Rust, ni son mécanisme de génération**.
Le workflow CI est fourni dans `.github/workflows/ci.yml` ; aucun passage distant
réussi n’est affirmé pour cette livraison.

## Limites qui restent réelles

L’analyse d’emprunts reste lexicale : pas de retour d’emprunts, de paramètres de
lifetime ou de fin d’emprunt à la dernière utilisation. Les champs propriétaires
ordinaires restent fixés à la construction. Les enums n’ont pas de patterns
empruntés. V6 ne fournit pas encore tableaux `T[]`, itérateurs empruntés, maps,
réseau, threads, FFI, héritage, surcharge, packages ou compilation séparée.

Une revue indépendante du modèle de sûreté reste nécessaire.
Les plateformes autres que Linux/WSL restent à valider. Voir [ROADMAP.md](docs/ROADMAP.md).
Ne pas employer ce prototype pour du code sensible ou en production.

## Organisation

`src/stdlib.rs` injecte les contrats natifs ; `src/stdlib_codegen.rs` instancie
leurs templates. `src/project.rs` charge les sources et les localise.
`src/checker.rs`, `src/ownership.rs` et `src/borrowing.rs` restent les trois analyses
principales. Les versions antérieures sont documentées dans `docs/history/`.
La licence du projet est MIT.
