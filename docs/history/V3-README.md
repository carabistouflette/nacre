# JavaNative V3 — emprunts et vues locales

**Version Cargo : `0.4.0`.** Compilateur écrit en Rust, sans dépendance Cargo
externe. Source `.jvn` → vérification statique → C11 → GCC/Clang → programme natif,
sans JVM, GC, comptage de références ou vérificateur d'emprunts à l'exécution.

La V3 prolonge la V2 : les classes, constructeurs, méthodes, champs possédés,
déplacements, `drop`, initialisation différée et analyses de boucles sont conservés.
Elle ajoute **des emprunts lexicaux partagés/exclusifs, des paramètres empruntés,
des méthodes `readonly` et des objets locaux à champs empruntés en lecture seule**.

> **Prototype non validé de bout en bout dans l'environnement de livraison.**
> Rust et Cargo y sont absents. Le compilateur Rust n'a donc pas été compilé et les
> **585 tests Rust livrés n'y ont pas été exécutés**. Le nombre est un inventaire
> statique des tests, pas un résultat de `cargo test`. Le runtime C et des fixtures
> C indépendantes, écrites à la main, ont été exécutés avec GCC/Clang et sanitizers.
> Cela ne valide pas le parser, l'analyse d'emprunts ou le C produit par le compilateur.
> Détails et rapports : [VALIDATION.md](docs/VALIDATION.md).

## Démarrage

Prérequis : Rust/Cargo et un compilateur C11 (GCC ou Clang, en-têtes et linker inclus).
Le minimum déclaré est Rust **1.74**, mais ce minimum n'a pas été testé ici.
La cible de livraison est Linux ou WSL ; macOS est envisagé, non validé ici.
Aucun accès à crates.io n'est nécessaire pour construire le projet.

```bash
unzip javanative-v3.zip
cd javanative-v3

# Premier contrôle : compiler le vrai projet Rust et exécuter ses tests.
cargo test --offline --locked

# Programme principal V3.
cargo run --offline --locked -- run examples/borrowing.jvn --memory-stats
```

Sorties **attendues, non observées ici pour le programme généré** :

```text
# stdout
42
84
43

# stderr
JVN_MEMORY allocations=1 frees=1 live=0
```

Le binaire cible utilise la bibliothèque C du système : « natif » ne signifie pas
« entièrement lié statiquement ». `--memory-stats` compte les allocations/libérations,
mais n'intervient pas dans les durées de vie.

## 1. Emprunter sans transférer

```java
class Counter {
    private int value;

    public Counter(int value) { this.value = value; }
    public void increment() { this.value = this.value + 1; }
    public readonly int getValue() { return this.value; }
}

void incrementTwice(borrow mut Counter counter) {
    counter.increment();
    counter.increment();
}

int read(borrow Counter counter) {
    return counter.getValue();
}

void main() {
    Counter counter = new Counter(40);
    incrementTwice(counter);
    println(read(counter)); // 42 : counter reste possede par main.
}
```

| Déclaration | Contrat |
|---|---|
| `Counter counter` | Propriétaire unique ; un argument de ce type est consommé. |
| `borrow Counter counter` | Accès partagé en lecture seule ; l'objet n'est pas consommé. |
| `borrow mut Counter counter` | Accès exclusif autorisant la modification ; l'objet n'est pas consommé. |
| `readonly int getValue()` | Méthode qui ne modifie pas son receveur, directement ou via ses champs possédés. |

L'emprunt est **implicite au site d'appel**, à partir de la signature :
`incrementTwice(counter)` ne requiert ni `&` ni `borrow(...)`. Le contrat de la
fonction, lui, reste explicite. Le borrow checker de Rust n'est pas réutilisé :
les règles du langage sont implémentées dans `src/borrowing.rs`.

`readonly` porte sur le receveur, **pas sur toute la fonction**. Une telle méthode
peut allouer et modifier ses objets locaux ou modifier un autre paramètre déclaré
`borrow mut`, à condition de respecter les règles d'aliasing. Une méthode non
annotée est considérée modifiante, même si son corps ne fait actuellement que lire.

## 2. Variables empruntées et réemprunts

```java
Counter counter = new Counter(40);

{
    borrow Counter first = counter;
    borrow Counter second = counter;
    println(first.getValue() + second.getValue());
} // Les deux emprunts partages se terminent ici.

{
    borrow mut Counter editor = counter;
    editor.increment();

    {
        borrow Counter reader = editor;
        println(reader.getValue());
        // editor.increment(); // Rejete : reader est encore actif.
    }

    editor.increment(); // Le reemprunt partage est termine.
}

println(counter.getValue()); // Le proprietaire est de nouveau accessible.
```

Un emprunt local doit être initialisé immédiatement. Sa **liaison n'est pas
réaffectable**, y compris avec `borrow mut` : ce modificateur autorise à modifier
l'objet référencé, pas à changer la cible de la variable.

Les durées de vie sont **lexicales**, jusqu'à la fin du bloc, et non jusqu'à la
dernière utilisation. Les emprunts de paramètres durent le temps de l'appel.
Les réemprunts conservent l'origine et la capacité de leur parent ; un accès
incompatible par le parent ou un autre alias est refusé pendant leur vie.

```java
Counter counter = new Counter(0);
borrow Counter reader = counter;
drop(counter); // E3002, meme si reader n'est pas utilise plus tard.
```

Pour raccourcir un emprunt, utiliser un bloc. `drop(reader)` ne termine pas un
emprunt : c'est une erreur, car `drop` ne reçoit que des objets possédés.

## 3. Références dans des objets : vues locales

Les champs empruntés existent sous une forme restreinte mais explicite :

```java
class CounterView {
    private borrow Counter target;

    CounterView(borrow Counter target) {
        this.target = target;
    }

    readonly int read() {
        return this.target.getValue();
    }
}

void main() {
    Counter counter = new Counter(42);
    {
        CounterView view = new CounterView(counter);
        println(view.read());
        // counter.increment(); // Rejete pendant la vie de view.
    } // Detruit view, pas counter.
    counter.increment();
}
```

Une classe contenant un champ `borrow` est une **vue locale**. La relation entre
le champ et le paramètre source est enregistrée par le compilateur ; l'emprunt
est maintenu jusqu'à la fin du bloc de la vue, pas seulement pendant son constructeur.

Les restrictions V3 sont les suivantes : les champs empruntés sont **partagés
uniquement** et initialisés une seule fois, directement depuis un paramètre
`borrow` du constructeur. Une vue s'initialise dans une variable locale par `new`.
Elle ne peut être déplacée, remplacée, passée en paramètre, retournée, empruntée en
tant que vue, détruite par `drop` ou stockée dans un autre objet. Elle peut contenir
des primitives et des sous-objets ordinaires possédés ; ses méthodes sont appelables.

Ces restrictions interdisent les références pendantes par échappement de vue sans
nécessiter encore de paramètres de durée de vie. Ce n'est **pas** un système général
de références persistantes dans des graphes d'objets.

## Commandes

```bash
# Vérifier sans compilateur C : syntaxe, types, ownership et emprunts.
cargo run --offline --locked -- check examples/borrowing.jvn

# Exécuter les autres exemples V3.
cargo run --offline --locked -- run examples/reborrowing.jvn
cargo run --offline --locked -- run examples/borrowed_fields.jvn --memory-stats
cargo run --offline --locked -- run examples/borrowed_loop.jvn
cargo run --offline --locked -- run examples/readonly_composition.jvn

# Conserver l'exécutable natif.
cargo run --offline --locked -- build examples/borrowing.jvn -o out/borrowing
./out/borrowing

# Inspecter le C, le graphe d'ownership et les événements d'emprunts.
cargo run --offline --locked -- emit-c examples/borrowing.jvn -o out/borrowing.c
cargo run --offline --locked -- emit-flow examples/borrowing.jvn -o out/borrowing.flow
cargo run --offline --locked -- emit-borrows examples/borrowing.jvn -o out/borrowing.borrows
```

`emit-borrows` affiche les créations/fins d'emprunts, leur origine, leurs parents et
les réservations/activations de receveurs. C'est la trace de l'analyse réellement
utilisée, pas une preuve formelle. Les commandes d'analyse rejettent les sources
invalides. La CLI conserve la protection contre l'écrasement du fichier source.

`run`, `build` et `emit-c` acceptent `--memory-stats`. `JVN_CC=clang` ou `JVN_CC=gcc`
choisit le compilateur C ; la valeur est un chemin d'exécutable, pas une commande shell.

## Tests et validation reproductible

```bash
# Frontend uniquement : pas de GCC/Clang nécessaire.
cargo test --offline --locked --lib --test frontend --test objects_frontend \
  --test ownership_frontend --test flow_diagnostics \
  --test borrowing_frontend --test borrowing_diagnostics

# Tous les tests, y compris les exécutables natifs et la CLI.
cargo test --offline --locked
JVN_CC=clang cargo test --offline --locked

# Valider le C réellement généré avec ASan/UBSan et les compteurs mémoire.
python3 scripts/validate.py --cc gcc --cc clang --sanitizers \
  --report docs/local-validation.json
```

Les 133 cas V3 sont décrits dans `tests/v3-cases.json` ; 46 ont aussi un test natif
avec sortie et bilan mémoire attendus. Les autres tests couvrent les diagnostics,
les traces, la matrice des modes de paramètres et les versions précédentes.

```bash
python3 scripts/generate_v2_tests.py --check
python3 scripts/generate_v3_tests.py --check
```

Les tests Rust sont pré-générés : Python n'est pas nécessaire pour `cargo test`.
Les scripts ne remplacent jamais le compilateur Rust par une implémentation de
référence ; un outil manquant fait échouer la validation de bout en bout.
La CI fournie n'a pas été exécutée dans cet environnement.

## Limites importantes

La V3 n'offre **pas** les lifetimes non lexicaux de Rust, les retours empruntés,
les emprunts réaffectables, les champs `borrow mut`, les vues exportables ou les
emprunts de primitives. Emprunter un sous-objet verrouille conservativement
**tout l'arbre de son propriétaire** ; deux champs possédés distincts ne sont pas
considérés indépendants.

Les arguments empruntés sont protégés dès leur évaluation jusqu'à la fin de
l'appel. Les receveurs sont réservés, puis activés après les arguments, pour
conserver `counter.add(counter.get())` sans autoriser leur destruction entre-temps.

Les limites antérieures restent présentes : pas d'interfaces, d'héritage, de
génériques, de tableaux, de `null`, d'import multifichier, de concurrence, de FFI,
de destructeurs utilisateur ou d'exceptions. Les champs objets possédés sont
fixés au constructeur. `String` reste une vue sur un littéral statique, sans
concaténation. Les sorties fatales du runtime ne déroulent pas les nettoyages.

Les limites de ressources comprennent un source de 1 Mio, 128 classes, une profondeur
syntaxique de 128, un CFG de 50 000 nœuds/16 Mio de cellules d'état, et une analyse
d'emprunts plafonnée à 8192 créations et 4 millions d'unités de travail par fonction.

## Organisation

```text
src/parser.rs      grammaire Java-like, borrow / borrow mut / readonly
src/checker.rs     types, visibilité, readonly, contrats et vues locales
src/ir.rs          distinction Object / Borrow et origines des champs empruntés
src/ownership.rs   CFG, initialisation certaine et déplacements dans les boucles
src/borrowing.rs   emprunts lexicaux, réemprunts, aliasing et réservations
src/codegen.rs     C11, pointeurs non possédés et nettoyages des propriétaires
src/runtime.c      arithmétique, affichage, allocation/libération
```

[Architecture](docs/ARCHITECTURE.md) · [Grammaire](docs/GRAMMAR.md) ·
[Modèle d'emprunts](docs/BORROWING.md) · [Validation](docs/VALIDATION.md) ·
[Historique](CHANGELOG.md)
