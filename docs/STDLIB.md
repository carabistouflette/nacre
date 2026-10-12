# Bibliothèque standard V6

`List`, `Text`, `Option`, `Result`, `Console`, `Files` et `Args` sont réservés et
préinjectés, sans imports. Les deux types somme utilisent la représentation
ordinaire des enums ; les méthodes natives appartiennent à la base de confiance.
Les spécialisations sont émises à la demande, avec toutes leurs signatures.
La portée de la validation réelle est décrite dans [VALIDATION.md](VALIDATION.md).

## List<T>

`new List<T>()` crée une liste vide. `T` peut être `int`, `boolean`, une `String`
statique, une classe/interface possédée, une enum, `Text` ou une autre liste.
Les vues à champs empruntés et les emprunts eux-mêmes sont interdits, même imbriqués.
Les listes sont invariantes : `List<Dog>` ne devient pas `List<Animal>`.
En revanche, `List<Animal>.add(new Dog())` accepte une conversion vers une
interface `Animal` explicitement implémentée par `Dog`.

| Méthode | Contrat |
|---|---|
| `void add(T value)` | Copie une primitive ; consomme un objet propriétaire. Agrandit le buffer au besoin. |
| `readonly int size()` | Nombre d’éléments initialisés. |
| `readonly int capacity()` | Capacité actuelle du buffer. |
| `readonly boolean isEmpty()` | `size() == 0`. |
| `readonly T get(int index)` | Disponible seulement pour les types copiables ; vérifie les limites. |
| `void set(int index, T value)` | Remplace un élément ; détruit l’ancien si propriétaire. |
| `T pop()` | Retire le dernier élément et transfère sa propriété. Fatal si vide. |
| `T remove(int index)` | Retire un élément, maintient l’ordre, transfère sa propriété. |
| `void clear()` | Détruit les éléments restants, conserve le buffer. |
| `void reserve(int capacity)` | Assure au moins cette capacité ; ne change pas la taille. Valeur négative fatale. |

Croissance géométrique, vérifications d’overflow avant multiplication de tailles.
Une réallocation déplace les représentations des valeurs sans les détruire ;
seule la plage `[0, size())` possède les éléments. `remove` décale la suite et
invalide l’ancienne dernière case pour ne pas la détruire deux fois.
La destruction d’une liste libère ses éléments dans l’ordre inverse, son buffer
puis l’objet liste. Un élément extrait n’est plus libéré avec la liste.

`get` est volontairement absent pour les objets : une copie de pointeur casserait
la propriété unique. Aucun emprunt vers un élément ne peut actuellement s’échapper.
Les futurs itérateurs et références d’éléments devront empêcher `add`, `reserve`,
`clear`, `remove`, `set` et la destruction pendant leur durée de vie.

Les appels conservent les emprunts V3 et les réservations de receveur :
`xs.add(xs.get(0))` pour `List<int>` évalue l’argument avant la mutation.
Un emprunt partagé nommé interdit toute mutation du même propriétaire jusqu’à la
fin du bloc. Le même principe interdit un déplacement pendant l’appel.

## Text

`new Text(String initial)` copie les octets de la chaîne statique dans un buffer
possédé. Une chaîne initiale vide n’alloue pas de buffer avant le premier ajout.
`Text` n’est pas convertible en `String`. Ses déplacements, paramètres propriétaires
et retours propriétaires utilisent les règles ordinaires du langage.

| Méthode | Contrat |
|---|---|
| `void append(String value)` | Ajoute les octets statiques, sans conserver de référence à un objet. |
| `void appendInt(int value)` | Ajoute la représentation décimale signée, y compris INT32_MIN. |
| `void appendBoolean(boolean value)` | Ajoute `true` ou `false`. |
| `void appendText(borrow Text other)` | Copie le contenu d’un autre texte. L’auto-aliasing est refusé par l’analyse. |
| `readonly int byteLength()` | Taille en octets UTF-8, pas en caractères. |
| `readonly int byteAt(int index)` | Octet entre 0 et 255 ; vérifie les limites. |
| `readonly boolean isEmpty()` | Teste si le contenu est vide. |
| `void clear()` | Vide le contenu sans réduire la capacité. |
| `readonly void print()` | Affiche le contenu suivi d’un saut de ligne. |
| `readonly Text clone()` | Nouvelle allocation et copie indépendante explicite. |
| `readonly boolean equals(borrow Text other)` | Comparaison des octets ; deux lectures du même texte sont autorisées. |
| `readonly boolean equalsString(String other)` | Comparaison avec une chaîne statique. |
| `readonly Result<int,Text> parseInt()` | Signe `+`/`-` facultatif puis chiffres ASCII ; analyse tous les octets, vérifie `[-2147483648,2147483647]`. Vide, espaces, suffixes et overflow donnent `Err` sans consommer le propriétaire. |
| `readonly Result<Text,Text> sliceBytes(int start, int end)` | Copie indépendante de `[start,end)` ; plage valide et frontières de points de code UTF-8 obligatoires, sinon `Err`. |

Aucun `toString`, emprunt de slice ou pointeur n’est exposé. `sliceBytes` retourne
un nouveau propriétaire, pas une vue qui pourrait survivre au buffer dynamique.
Les NUL sont conservés. Les indices restent des **octets**, pas des caractères ou
graphèmes ; la vérification de frontières UTF-8 ne les transforme pas en indices Unicode.

## Option<T> et Result<T,E>

```java
enum Option<T> { None; Some(T); }
enum Result<T,E> { Ok(T); Err(E); }
```

Ces déclarations sont des contrats préinjectés réservés. Construction explicite :
`new Option<int>.Some(42)`, `new Option<int>.None()`,
`new Result<int,Text>.Err(new Text("échec"))`.
Une somme n’alloue pas d’enveloppe ; seuls ses payloads peuvent allouer.
Toutes les sommes sont affines, même `Option<int>`. `match` les consomme et exige
toutes les variantes, avec les variables de payload correspondantes.
Les payloads possédés sont détruits sur les sorties normales, y compris les
retours précoces et sorties de boucle. Les emprunts et vues locaux sont interdits
dans les payloads ; il n’y a pas de déballage implicite ni d’exceptions.

## Console

| Méthode statique | Contrat |
|---|---|
| `Result<Option<Text>,Text> readLine()` | `Ok(Some(line))` pour une ligne, `Ok(None)` pour EOF sans octets restants, `Err(error)` pour erreur de lecture, taille excessive ou UTF-8 invalide. |
| `void print(String value)` | Affiche les octets sans saut de ligne et flush stdout. |
| `void printText(borrow Text value)` | Même affichage pour un texte possédé, sans le consommer. |

`readLine` enlève LF et le CR qui le précède ; une dernière ligne sans LF est
retournée avant EOF. Une ligne vide est `Some(Text vide)`, pas EOF.
Une ligne UTF-8 invalide est consommée avant `Err`. En revanche, un flux C ayant
une erreur persistante n’est pas réinitialisé : l’appelant ne doit pas le réessayer
indéfiniment. Le Sudoku termine proprement sur une erreur du flux console.
Le buffer lu est transféré au `Text` retourné, sans recopie intégrale.

## Files

| Méthode statique | Contrat |
|---|---|
| `Result<Text,Text> readText(borrow Text path)` | Lecture complète UTF-8, NUL de contenu conservés ; échec d’ouverture/lecture/fermeture, UTF-8 invalide ou taille excessive → `Err`. |
| `Result<boolean,Text> writeText(borrow Text path, borrow Text content)` | Écrit les octets exacts et remplace le fichier existant ; `Ok(true)` après écriture, flush et fermeture réussis, sinon `Err`. |
| `Result<boolean,Text> writeTextAtomic(borrow Text path, borrow Text content)` | Writes exact bytes to a sibling temporary file, flushes and closes it, then atomically replaces the destination using `rename`; returns `Ok(true)` only after commit. |

Un NUL dans un chemin est refusé. Les chemins sont UTF-8 ; les espaces sont
acceptés. Les buffers lus sont transférés au texte retourné sans copie finale.
`writeText` n’est pas atomique : un échec peut laisser un fichier partiel ou
tronqué. Aucun `Err` ne prétend annuler les écritures déjà effectuées.

`writeTextAtomic` guarantees atomic visibility, not power-loss durability: it
does not call `fsync`. Before a successful commit, a failure preserves the
destination and attempts to remove the temporary file. A replacement uses
`mkstemp` permissions (0600 subject to umask), not the old file's permissions.
Renaming over a symlink replaces the symlink entry rather than its referent.
There is no fallible operation after the successful rename that can produce
an `Err` claiming the old destination was preserved.

## Args

| Méthode statique | Contrat |
|---|---|
| `int count()` | Nombre d’arguments utilisateur, sans le nom de l’exécutable. |
| `Result<Text,Text> get(int index)` | Copie possédée de l’argument UTF-8 ; indice invalide ou octets non UTF-8 → `Err`. |

L’entrée reste `void main()`. Les arguments sont ceux de l’exécutable construit,
ou ceux transmis par `nacre run sources -- arguments...`.
`Console`, `Files` et `Args` ne sont pas constructibles et ne sont pas des types
de variables ou de payloads.


## Frontière de confiance

Les corps vides de `stdlib/prelude.ncr` sont des contrats internes, pas du code
utilisateur valable pour les méthodes non-void. `src/stdlib.rs` est le seul
injecteur du marqueur natif. Le parseur ne peut pas produire ce marqueur à partir
du source. Après spécialisation, le checker résout les signatures, puis les appels
utilisateur sont analysés normalement. Les corps natifs ne sont pas présentés
comme ayant été prouvés par les analyses d’ownership et d’emprunts.

Le backend ajoute aux appels natifs une ligne et une colonne cachées pour les
contrôles de limites. Les primitives mémoire C et les templates natifs font
partie de la base de confiance ; ils doivent être audités indépendamment.
Les scripts C testent les templates réels, mais pas leur intégration Rust.

## Erreurs fatales

Les opérations historiques de listes/texte hors limites, `pop` vide, capacité
négative, taille impossible dans les opérations fatales ou échec d’allocation
arrêtent toujours le processus. Pas de déroulement normal des destructeurs sur
ce chemin. Les nouvelles E/S, conversions et copies de plages renvoient des
erreurs récupérables via `Result` ; l’échec d’allocation reste fatal.
