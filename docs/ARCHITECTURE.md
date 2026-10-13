# Nacre architecture

This note summarizes the compiler and runtime structure. Validation evidence is
recorded separately in [VALIDATION.md](VALIDATION.md).

## Pipeline

```text
one or more .ncr files or directories
  → project.rs       source collection, limits, source-unit boundaries
  → lexer.rs         tokens and file-local source positions
  → parser.rs        one AST unit per source file
  → resolver.rs      modules, imports, and qualified names
  → stdlib.rs        injection of reserved native declarations
  → generics.rs      nominal instantiation and type substitution
  → checker.rs       names, types, visibility, interfaces, and typed IR
  → ownership.rs     definite initialization and move-flow analysis
  → borrowing.rs     lexical loans, origins, and access conflicts
  → codegen.rs       C11, interface tables, and deterministic cleanup
  → stdlib_codegen.rs / stdlib/*.c.in   native C method bodies
  → native.rs        GCC/Clang to executable, or Clang to textual LLVM IR
```

The Rust frontend uses the standard library; `serde_json` encodes and decodes LSP
messages. `#![forbid(unsafe_code)]` forbids Rust `unsafe` in the compiler, but
does not prove compiler correctness or generated-C safety. The native target
uses the supplied C runtime and system C library. It has no garbage collector or
implicit reference counting for ordinary language values; sharing synchronization
controls requires explicit mutex/channel handles.

## AST et spécialisation générique

L'AST distingue `Named`, `Applied { name, args }` et `Borrow { name, args, mutable }`.
Les interfaces portent `is_interface = true` ; les enums portent une liste de
variantes et de payloads. `is_builtin` distingue les déclarations du prelude des
types utilisateur, sans accorder de privilèges à un nom de paramètre comme `T`.
Une classe conserve ses paramètres et sa liste `implements` ; les constructions
ordinaires et de variantes conservent leurs arguments de type explicites.

`generics.rs` indexe les déclarations puis réécrit les types utilisés. Chaque
application de type demandée possède une clé canonique, par exemple `Box<int>`.
Une file déterministe permet de découvrir les instances imbriquées ; un cache
évite de produire plusieurs instances de la même application. La substitution
traverse champs, signatures, variables, constructions et corps des méthodes.
Elle ne remplace pas les identifiants de variables par des noms de types.

Les instances deviennent des types nominaux concrets avant le typage.
Les analyses suivantes n'ont donc pas de variable de type à résoudre. Les
méthodes d'un template non instancié ne sont pas toutes vérifiées ; toutes celles
d'une instance demandée le sont, même si elles ne sont jamais appelées. Cette
sémantique est décrite dans [GENERICS.md](GENERICS.md).

Des bornes sur la profondeur, le nombre et la longueur des instances limitent
les expansions récursives. Ce n'est pas une inférence de paramètres, ni un
système de contraintes génériques avec bornes de type.

`ForEach` AST nodes preserve typed bindings, the collection, body, and source
spans: maps have two bindings; lists and sets have one. The checker evaluates a
collection expression once and lowers traversal to typed block/while IR. Lists
use indexed accessors; maps and sets use compiler-only capacity, occupancy, key,
and value expressions. Knowledge of their C layout stays in `stdlib_codegen`.
The cursor advances before the original body is checked by reference, not cloned.
Existing flow, borrowing, and cleanup passes govern joins and exits. Traversal
does not allocate iterator objects, copy affine owners, or rehash keys to read
stored values.

Recoverable list intrinsics return inline `Option<T>` layouts obtained from
ordinary specialization. Native enum IDs/tags are looked up, not fixed in the
backend. Successful extraction reuses strict transfer/slot-clearing operations;
invalid boundaries return `None` before touching storage. List element validity
is checked before compiler-generated Option payloads, preserving primary
list-view diagnostics.

## IR nominale et interfaces

L’IR remplace les noms par des identifiants nominaux. `Object(id)` représente un
propriétaire : pointeur de classe, interface par valeur ou somme affine par valeur.
`Borrow { class: id, mutable }` représente un accès non possédé. Ainsi,
`Type::is_object()` signifie **possédé**, et non « pointeur vers une classe ».

Une classe IR conserve son nom, ses interfaces et, pour chacune, la liste des
méthodes concrètes correspondant aux emplacements du contrat. Une interface
conserve ses méthodes dans l'ordre de déclaration. Une signature virtuelle a
un `virtual_slot` ; une expression `Upcast` enregistre la conversion possédée
classe → interface. Pour une conversion empruntée, l'expression `Borrow` conserve
la place concrète d'origine et porte le type d'interface attendu.

Les contrats sont nominaux : une méthode de même nom ne suffit pas. Une clause
`implements` doit exister. Le typage vérifie visibilité, mode de receveur
`readonly`, paramètres, modes d'emprunt et retour. Les contrats sont actuellement
exacts, sans covariance ni assouplissement implicite des effets. Une classe à
champs empruntés, donc une vue locale, ne peut pas implémenter d'interface :
l'effacement de son type ne doit pas contourner ses restrictions de durée de vie.

## Ownership et emprunts

`ownership.rs` suit les états possibles de chaque variable : non initialisée,
disponible ou déplacée. Les jonctions sont des unions d'états ; une file de
travail calcule un point fixe à travers les boucles. Une lecture exige un état
disponible sur tous les chemins entrants. Une conversion possédée en interface
traverse le déplacement de sa valeur source, sans créer un second propriétaire.

`borrowing.rs` travaille ensuite sur les blocs typés. Les emprunts locaux restent
lexicaux, leurs cibles sont fixes, et les prêts d'argument durent le temps de
l'appel, sauf capture par une vue. Les origines conservent racine et capacité
parent pour les réemprunts. Le passage à une interface ne change pas cette origine :
il ne permet pas d'obtenir un accès mutable concurrent à un emprunt partagé.

Virtual method signatures expose the same contracts as concrete methods; every
implementation body is checked separately. Borrow analysis need not select a
dynamic implementation to determine whether an argument owns or borrows.
The checker records borrowed returns in those signatures and proves their receiver origin;
caller-side result loans survive call cleanup. Local views remain non-exportable.

## Représentation C

Une classe concrète reste un pointeur vers sa structure nominale. Une interface,
possédée ou empruntée, est une structure de deux pointeurs passée par valeur :

```c
struct interface_value {
    void *data;
    const struct interface_vtable *vtable;
};
```

La table contient une fonction de destruction et une fonction typée par méthode.
Une table constante est émise par couple classe/interface. Les adaptateurs
reçoivent `void *`, convertissent le pointeur de données en son type concret, puis
appellent la bonne fonction. Ils évitent les conversions entre pointeurs de
fonctions de signatures incompatibles.

L'enveloppe d'interface ne nécessite pas d'allocation supplémentaire. Un transfert
possédé met l'ancienne enveloppe à `{ NULL, NULL }`. Une interface propriétaire
est nettoyée via la fonction de destruction de sa table, qui connaît le type
concret et ses sous-objets possédés. Une interface empruntée n'est jamais détruite
comme propriétaire. Une copie physique de ses deux pointeurs pour un emprunt
n'est pas une copie de l'objet ni un comptage de références.

### Sommes par valeur

Une enum C est une structure avec un tag `int32_t` et une union de payloads.
Les sommes imbriquées sont définies dans un ordre topologique ; les cycles de
représentation par valeur sont refusés avant émission. Les pointeurs de classes
et interfaces permettent des structures récursives ayant un cas de base fini.

Une construction ne crée pas d’allocation d’enveloppe. Un déplacement remplace
le tag du propriétaire source par `-1`, état interne non constructible par le
programme. Le destructeur consulte uniquement la variante active et détruit ses
payloads possédés dans l’ordre inverse. Un `match` transfère ces payloads aux
variables de branche et invalide son temporaire ; le CFG et les nettoyages
lexicaux couvrent retours et sorties de boucle. Il est émis comme une chaîne
`if`/`else if`, pour qu’un `break` continue de viser la boucle source.

Enum references point to inline storage. Borrowed matching preserves the tag
and binds primitive copies or receiver/scrutinee-derived nominal references;
payloads containing borrowed fields/local views remain forbidden.


Les fonctions de dispatch virtuel transmettent les paramètres à l'implémentation
sans les nettoyer une seconde fois : les paramètres possédés sont consommés par
la fonction concrète. Le nettoyage des retours, remplacements et sorties de
boucle suit le modèle lexical des versions précédentes.

Le backend séquence les expressions pour préserver l'ordre gauche-droite. Les
opérateurs `&&` et `||` restent des blocs conditionnels, pas des évaluations
anticipées de leurs deux opérandes. Les sorties fatales du runtime ne déroulent
pas les propriétaires ; les compteurs mémoire ne sont donc pas une garantie de
nettoyage après une erreur fatale.

## Code natif et LLVM

`run` et `build` émettent du C puis appellent GCC ou Clang avec le niveau
`-O0`, `-O1`, `-O2` (défaut), `-O3` ou `-Os`. Aucun gain de performance n'a été
mesuré. Un niveau d'optimisation plus élevé n'est pas présenté comme une garantie
de code plus rapide.

`emit-llvm` invokes Clang with `-S -emit-llvm` on the same generated C. Nacre has no
direct LLVM backend written in Rust. `NACRE_CLANG` selects that Clang executable;
`NACRE_CC`/`CC` separately select the compiler used by `run` and `build`.

## Inspection et tests

`emit-types` expose les types nominaux après spécialisation, `emit-flow` le CFG
d'ownership, `emit-borrows` les événements vérifiés par l'analyse des prêts, et
`emit-c` le code C. Ces commandes passent par les vérifications du frontend ;
elles ne contournent pas un diagnostic de typage ou d'emprunt.

Les manifestes JSON V2, V3, V4 et V5 servent à générer les suites Rust conservées
dans le dépôt. Le script `validate.py` utilise le **vrai binaire Rust**, lorsqu'il
peut être compilé, pour tester ses sorties C et LLVM. Les fixtures C indépendantes
ne sont pas un substitut à cette validation.


## Classes natives et mémoire des collections

`ast::Class.native` is never produced by source parsing. Trusted prelude
injection marks built-in collection templates, scalar/text types, and system
APIs for I/O, filesystem, regex, math/random/time, processes, synchronization, and
networking. Generic specialization classifies `Map<K,V>` and `Set<K>` keys as
`int` or owned `Text` before ordinary type checking. The checker resolves native
declarations and typed methods without granting native privileges to a
user-defined spelling.

Unused native classes are not materialized. Copying accessors are pruned for
affine elements; borrowed accessors are pruned for copyable elements. Their ABI
is derived from typed IR.
Native functions have ordinary typed signatures, but their empty IR bodies are
exempt from analysis. `ownership.rs` and `borrowing.rs` do not prove native
method safety; user calls are checked against the same contracts as ordinary
functions.

List storage contains a buffer pointer, size, and capacity. Only initialized
slots own values. The concrete element type selects the destructor, including
dynamic destruction for interface values. `pop` and `remove` clear the stored
slot before transferring its value to the caller. `Text` owns a byte buffer;
that buffer is not returned as a static `String` view.

Map storage is a single buffer of entries containing an `int` or owned `Text`
key, a value, occupancy, and (for Text keys) a cached hash. Entry structure
definitions follow complete ordinary/interface/enum layouts and precede drop
bodies, so inline values have their real C size. Unsigned hashing and
open-addressing support every signed integer key and Text keys. Checked growth
raw-relocates owners; backward-shift deletion repairs probe chains without
tombstones or allocation. Replacing an entry returns the previous value rather
than dropping it. Text lookup borrows the query; borrowed value lookup returns
an object pointer, enum slot address, or interface fat pointer according to the
concrete value layout.

Each map remains one alias domain. Native argument sequencing and projected
receiver freezing remain in force; separate keys do not provide disjoint
borrowing. Map and Set traversal does not add a first-class iterator or a
per-entry lifetime model. Native collection code remains part of the trusted
base.

The backend injects source line/column arguments into native calls and substitutes
numeric identifiers into C templates. Buffers use the same accounting as object
allocations. Ordinary values do not use garbage collection or implicit reference
counts; mutexes and channels explicitly share their synchronization control
blocks. See [STDLIB.md](STDLIB.md).

The existing check for cycles of required fields still applies. A native list
does not require an element field, so its empty state supplies a base case. A
`Node` that owns a `List<Node>` is representable; multiple owners for the same
object are not.

## Source units and project tools

`project::SourceBundle` retains each source name, its text, and source offsets.
Files are lexed and parsed independently so a comment or declaration cannot
cross a file boundary. Their AST units are then resolved together: each
`module` header supplies a namespace, and each file's imports bind declarations
from the selected source bundle. This is whole-program compilation, not
separate compilation, package linking, or external dependency resolution.

Primary diagnostics and their notes map back to the original file. Multi-file C
output includes a compact table of file starting lines for runtime errors.
`emit-flow` and `emit-borrows` currently retain lines from the combined text.

`doctor` exercises the real compiler pipeline rather than a fixed fixture.
`init` creates a new local project skeleton. `--sanitize` passes memory-safety
instrumentation options to the C compiler; `doctor` is a smoke check, not an
exhaustive proof. See [VALIDATION.md](VALIDATION.md) for the recorded evidence.

## LSP and recoverable I/O

`nacre lsp` exchanges JSON-RPC messages with `Content-Length` framing. It reuses
`SourceBundle::validate` and `SourceBundle::locate`, without invoking the C
compiler, on bounded snapshots of disk sources and open buffers. Frontend byte
positions are converted to LSP UTF-16 positions; notes become related
information.

The nearest ancestor `nacre.json` declares project source roots. Within a project
scope, open buffers override disk files, including unsaved new source files under
the configured roots. Stale document versions do not replace newer snapshots.
Without a marker, each document is validated alone; nearby files are not merged.
VS Code and Zed launch the same server. See [PROJECTS.md](PROJECTS.md) for the
configuration and overlay contract.

Les corps C d’E/S produisent les véritables spécialisations IR de `Option`/`Result`.
Ils valident UTF-8, ranges et conversions int32, avec erreurs textuelles possédées.
Les buffers de console/fichier sont transférés au `Text` retourné sans recopie
finale. L’entrée C reçoit `argc/argv` ; `Args` expose les arguments sans `argv[0]`.
Les échecs d’allocation et les erreurs fatales historiques gardent leur arrêt sans
déroulement de pile ; les retours `Err` suivent les nettoyages normaux.

