# JavaNative V4 — interfaces, polymorphisme et génériques

Compilateur expérimental **écrit en Rust**, sans dépendance Cargo externe.
La syntaxe reste proche de Java ; les objets suivent les règles de propriété
unique et d'emprunts des V2/V3. La V4 ajoute des interfaces, leur dispatch
dynamique et des classes/interfaces génériques spécialisées à la compilation.

**Statut réel : le Rust de cette livraison n'a pas été compilé.** Rust et Cargo
sont absents de l'environnement de livraison. Les 766 tests Rust répertoriés
statiquement n'ont pas été exécutés ; deux tests LLVM sont explicitement ignorés
par défaut car ils exigent Clang. Seuls le runtime C et des fixtures C **écrites
à la main** ont été exécutés ici. Aucune garantie de sûreté ou de bon
fonctionnement du compilateur complet n'est revendiquée.
Voir [le rapport de validation](docs/VALIDATION.md).

## Démarrage

Prérequis : Rust/Cargo (minimum déclaré : 1.74, non vérifié ici) et GCC ou Clang
avec les en-têtes C et l'éditeur de liens. Clang est facultatif, sauf pour
`emit-llvm`. Linux et WSL sont les cibles initiales ; macOS n'a pas été testé.
Aucune JVM n'est nécessaire, ni pour le compilateur Rust ni pour les programmes.

```bash
unzip javanative-v4.zip
cd javanative-v4

# Commencer par vérifier la compilation et les tests du vrai compilateur.
cargo test --offline --locked

# Interfaces et dispatch dynamique.
cargo run --offline --locked -- run examples/interfaces.jvn --memory-stats

# Classes génériques.
cargo run --offline --locked -- run examples/generic_box.jvn

# Les deux mécanismes ensemble.
cargo run --offline --locked -- run examples/generic_interfaces.jvn --memory-stats
```

Sortie **attendue**, non observée via ce compilateur ici, pour `interfaces.jvn` :

```text
Ouaf !
Ouaf !
Miaou !
```

Sur stderr avec `--memory-stats` :

```text
JVN_MEMORY allocations=2 frees=2 live=0
```

La conversion vers l'interface est prévue sans allocation supplémentaire :
il y a un Dog puis un Cat, pas un objet d'enveloppe alloué pour chaque conversion.

## 1. Interfaces et polymorphisme

```java
interface Animal {
    readonly void speak();
}

class Dog implements Animal {
    public readonly void speak() { println("Ouaf !"); }
}

class Cat implements Animal {
    public readonly void speak() { println("Miaou !"); }
}

void introduce(borrow Animal animal) {
    animal.speak();
}

void main() {
    Animal animal = new Dog();
    introduce(animal);     // Emprunt implicite selon la signature.
    animal = new Cat();   // Détruit l'ancien objet possédé.
    introduce(animal);
}
```

`implements` est **nominal** : avoir les bonnes méthodes ne suffit pas, la classe
doit déclarer l'interface. Plusieurs interfaces peuvent être implémentées.
Les méthodes requises doivent être publiques, non statiques, avec exactement
les mêmes types de paramètres, modes d'emprunt, type de retour et `readonly`.
Il n'y a ni covariance des retours ni surcharge dans cette version.

Une valeur d'interface possède l'objet ou l'emprunte selon son type, comme une
valeur de classe. Sa représentation contient un pointeur de données et un
pointeur de table de méthodes. La table comporte aussi la destruction du type
concret. Le pointeur partagé vers cette table statique n'est pas un partage de
propriété des données et ne nécessite pas de comptage de références.

Les interfaces fonctionnent dans les variables, les paramètres, les retours et
les champs possédés. Les champs empruntés partagés de vues locales peuvent
aussi utiliser une interface, avec les restrictions de durée de vie de la V3.

## 2. Les emprunts survivent à l'effacement du type concret

```java
interface CounterAccess {
    void increment();
    readonly int getValue();
}

class Counter implements CounterAccess {
    private int value = 0;
    public void increment() { this.value = this.value + 1; }
    public readonly int getValue() { return this.value; }
}

void bump(borrow mut CounterAccess counter) { counter.increment(); }

void main() {
    Counter counter = new Counter();
    bump(counter); // Ni transfert de propriété, ni allocation d'enveloppe.
    {
        borrow CounterAccess reader = counter;
        println(reader.getValue());
        // drop(counter);        // Doit être rejeté : objet emprunté.
        // counter.increment();  // Doit être rejeté : accès en écriture.
    }
    counter.increment();
}
```

Un passage vers une interface **ne doit pas effacer l'origine de l'emprunt**.
La même analyse d'aliasing contrôle les accès via le nom concret et via
l'interface. Les emprunts restent lexicaux, jusqu'à la fin du bloc, et non jusqu'à
la dernière utilisation comme dans des analyses plus avancées.

Une classe contenant des champs empruntés reste une vue locale non transférable.
Elle ne peut pas implémenter une interface en V4 : masquer cette vue pourrait
faire perdre les informations nécessaires pour empêcher son échappement.

## 3. Classes et interfaces génériques

```java
class Box<T> {
    private T value;
    public Box(T value) { this.value = value; }
    public readonly T get() { return this.value; }
}

void main() {
    Box<int> number = new Box<int>(42);
    Box<String> text = new Box<String>("Bonjour !");
    println(number.get());
    println(text.get());
}
```

Chaque spécialisation demandée est produite une seule fois : `Box<int>` et
`Box<String>` ont leurs propres types de champs et méthodes. Les arguments sont
explicites, y compris après `new` ; pas de diamant `<>` ni d'inférence en V4.
Les arguments `int`, `boolean`, `String`, classe et interface sont acceptés.
Les types imbriqués comme `Holder<Box<int>>` sont possibles.

**Ce sont des modèles vérifiés lors de leur instanciation, pas les génériques
bornés de Java ou de Rust.** Le corps d'un modèle inutilisé n'est pas entièrement
vérifié sémantiquement. Pour chaque spécialisation demandée, tous ses champs,
constructeurs et méthodes passent ensuite par le typage, l'ownership et le borrow
checker, y compris les méthodes qui ne seront pas appelées. Pas de `extends`
comme borne de type, de wildcard, de covariance, ou de fonctions génériques
indépendantes dans cette version.

Conséquence importante : le `Box<T>.get()` ci-dessus fonctionne pour les
primitives et les chaînes statiques. **`Box<Counter>` avec ce getter sera rejeté** :
retourner le champ possédé déplacerait son objet hors de la boîte, une opération
que le modèle mémoire actuel ne prend pas en charge. Pour conserver un objet,
utiliser une composition qui expose ses opérations sans extraire sa propriété,
comme dans `examples/generic_composition.jvn`.

Les interfaces peuvent elles aussi être génériques :

```java
interface Source<T> { readonly T read(); }

class Constant<T> implements Source<T> {
    private T value;
    Constant(T value) { this.value = value; }
    public readonly T read() { return this.value; }
}

void main() {
    Source<int> number = new Constant<int>(42);
    println(number.read());
}
```

## 4. Commandes disponibles

```bash
# Vérification, sans compilateur C.
cargo run --offline --locked -- check examples/interfaces.jvn

# Exécutable natif conservé ; optimisation -O2 par défaut.
cargo run --offline --locked -- build examples/interfaces.jvn -O3 -o out/animals
./out/animals

# Source intermédiaire C11.
cargo run --offline --locked -- emit-c examples/interfaces.jvn -o out/animals.c

# Types concrets et spécialisations effectivement vérifiés.
cargo run --offline --locked -- emit-types examples/generic_interfaces.jvn

# Analyses mémoire héritées des V2/V3.
cargo run --offline --locked -- emit-flow examples/interfaces.jvn
cargo run --offline --locked -- emit-borrows examples/interface_borrowing.jvn

# LLVM textuel via Clang (optionnel).
cargo run --offline --locked -- emit-llvm examples/interfaces.jvn -O2 -o out/animals.ll
clang out/animals.ll -o out/animals-from-llvm
```

`run`, `build` et `emit-llvm` acceptent `-O0`, `-O1`, `-O2`, `-O3` et `-Os`.
Ces options sont transmises au compilateur C/Clang ; la V4 n'introduit pas un
nouveau passe d'optimisation propre à JavaNative. `emit-c` produit le C avant
optimisation. `--memory-stats` est disponible pour `run`, `build`, `emit-c` et
`emit-llvm`. Les commandes d'inspection écrivent sur stdout, ou dans `-o chemin`.

Choix des compilateurs : `JVN_CC` puis `CC`, sinon recherche de `cc`, `clang`, `gcc`.
Pour `emit-llvm` : `JVN_CLANG`, sinon `clang`. Ces variables contiennent le chemin
d'un exécutable, pas une commande shell avec options.

**Le backend principal reste C11.** `emit-llvm` suit `.jvn → C11 → Clang → .ll` ;
ce n'est pas un backend LLVM direct. Les exécutables utilisent libc. « Sans JVM
ni GC » ne signifie ni « sans runtime » ni « entièrement lié statiquement ».

## 5. Validation sur ta machine

```bash
# Toute la suite par défaut : un compilateur C suffit.
cargo test --offline --locked

# Nouvelles règles de langage, sans compilateur C.
cargo test --offline --locked --test features_v4_frontend

# Validation réelle des programmes générés, et détection d'erreurs mémoire.
python3 scripts/validate.py --cc gcc --cc clang --sanitizers --llvm \
  --report docs/local-validation.json

# Seulement les deux tests optionnels d'émission LLVM.
cargo test --offline --locked --test features_v4_tools -- --ignored
```

`validate.py` ne remplace jamais le vrai compilateur par une implémentation de
référence : un outil absent ou un test échoué interrompt la validation. La CI
fournie est un workflow à exécuter dans un dépôt ; elle n'a pas tourné ici.

## 6. Périmètre et limites

Les V0–V3 sont conservées : fonctions et méthodes, classes, composition, `int`
32 bits avec débordement défini, `boolean`, chaînes UTF-8 statiques, branches,
boucles, `move`, `drop`, initialisation différée, emprunts et vues locales.

Restent absents : héritage de classes **et d'interfaces**, méthodes `default`,
casts/downcasts, génériques de fonctions, bornes de types, lifetimes explicites,
références retournées, NLL, extraction/remplacement des champs objets, tableaux,
collections standard, chaînes dynamiques, `null`, exceptions, threads, modules et
interopérabilité Java. Une méthode exige encore un receveur nommé, pas un objet
temporaire. Aucun chiffre de performance comparative n'est établi.

Bornes de protection du prototype : source de 1 Mio, 128 déclarations nominales,
128 types concrets après expansion, 64 spécialisations, 8 paramètres génériques,
profondeur de type générique 32. Certaines structures sûres restent refusées.

## Fichiers à explorer

| Fichier | Rôle |
|---|---|
| `src/parser.rs` | Syntaxe `interface`, `implements` et arguments de types. |
| `src/generics.rs` | Instanciation et substitution des modèles. |
| `src/checker.rs` | Contrats d'interfaces et conversions typées. |
| `src/ownership.rs`, `src/borrowing.rs` | Analyses mémoire appliquées après spécialisation. |
| `src/codegen.rs` | Tables de méthodes, adaptateurs et destruction dynamique. |
| `src/native.rs` | Compilation native et émission LLVM via Clang. |
| `tests/v4-cases.json` | Cas positifs/négatifs et sorties attendues. |
| `docs/INTERFACES.md`, `docs/GENERICS.md` | Règles détaillées et limites. |

[Architecture](docs/ARCHITECTURE.md) · [Grammaire](docs/GRAMMAR.md) ·
[Emprunts](docs/BORROWING.md) · [Changements](CHANGELOG.md) ·
[Validation](docs/VALIDATION.md)

Licence MIT. Prototype de langage, pas compilateur de production.
