# Interfaces — V5

Ce document décrit l'implémentation source livrée, non validée de bout en bout.
Les tests et leurs résultats réels sont séparés dans `VALIDATION.md`.

## Déclaration et compatibilité

```java
interface Readable { readonly int read(); }
interface Editable { void set(int n); }

class Number implements Readable, Editable {
    private int value = 0;
    readonly int read() { return value; }
    void set(int n) { value = n; }
}
```

Une interface ne contient que des signatures publiques d'instance terminées par
`;`. Pas de champ, constructeur, corps, méthode statique, méthode privée ou
méthode `default`. Les noms de méthodes doivent être distincts, sans surcharge.
Les interfaces vides sont autorisées.

Une classe doit déclarer chaque interface après `implements`. La conformité est
vérifiée une fois toutes les signatures enregistrées, donc une interface peut
être déclarée après sa classe d'implémentation. La liste `implements` ne doit pas
contenir de doublons ni de nom de classe concrète.

Chaque méthode doit avoir exactement le contrat déclaré : nombre et type des
paramètres, `borrow`/`borrow mut`/propriété, retour et `readonly`, ainsi qu'une
visibilité publique et un statut non statique. La V5 ne fait pas de sous-typage
covariant ou contravariant des signatures. Même le renforcement d'une méthode
mutable en `readonly` est refusé : l'égalité stricte est volontaire pour cette
première version.

Il n'y a pas d'héritage d'interface, de conversion d'une interface vers une autre,
ni de récupération du type concret par cast ou `instanceof`.

## Propriété et conversion

À une frontière possédée (`let`, affectation, paramètre, retour, champ), un objet
concret d'une classe implémentant l'interface peut être converti implicitement :

```java
Number n = new Number();
Readable r = n; // Déplace n : utiliser n ensuite doit échouer.
```

Le type statique de `r` ne donne accès qu'aux méthodes de `Readable`. L'objet reste
un `Number` à l'exécution. `Readable other = r` déplace la valeur d'interface ; il
ne crée pas deux propriétaires. Le remplacement ou la fin de portée de `r` doit
invoquer la destruction concrète, y compris ses champs possédés.

Une classe possédant un champ d'interface suit les mêmes règles : champ fixé à
l'initialisation, pas d'extraction ni de remplacement ultérieur de sa propriété.
Les branches, boucles, sorties précoces, `move` et `drop` utilisent le même CFG que
pour les types concrets.

## Emprunts

À une frontière empruntée, la conversion conserve la place source :

```java
void display(borrow Readable r) { println(r.read()); }
```

`display(n)` emprunte `n` ; `n` peut rester utilisable après l'appel. Un local
`borrow Readable r = n` garde l'emprunt jusqu'à la sortie de son bloc. Un
`borrow mut Editable` est exclusif. Une conversion ne permet ni de promouvoir un
emprunt partagé en mutable ni de transformer un emprunt en propriétaire.

Les deux accès au même objet — via son nom concret et via un emprunt d'interface
— doivent rester dans le même domaine d'aliasing. Les origines, liens de
réemprunt et réservations de receveur sont conservés par `borrowing.rs`.
Les arguments partagés peuvent aliaser ; les combinaisons conflictuelles avec un
accès exclusif doivent être rejetées même si les types statiques diffèrent.

Les champs `borrow Interface` sont autorisés dans les vues locales de la V3,
initialisés directement depuis un paramètre emprunté du constructeur.
**Une vue locale ne peut pas implémenter une interface en V5.** Elle ne peut donc
pas devenir un propriétaire polymorphe transportable qui masquerait ses prêts.

## Représentation C

Schéma, pas ABI public stable :

```c
struct interface_value {
    void *data;
    const struct interface_vtable *vtable;
};
struct interface_vtable {
    void (*destroy)(void *);
    int32_t (*read)(void *);
};
```

L'interface est passée par valeur dans le C généré. Le frontend distingue une
valeur possédée d'une valeur empruntée, même si elles partagent cette forme.
Un déplacement copie les deux pointeurs puis remet la source à `{NULL, NULL}`.
Ces nulls sont un état interne de nettoyage, pas une valeur `null` disponible dans
le langage. La destruction teste `data` avant d'utiliser la table.

Les tables sont des constantes statiques. Les conversions ne créent ni objet
d'enveloppe ni compteur de références. Une table fournit un adaptateur typé pour
chaque méthode concrète et pour sa destruction. Les adaptateurs reçoivent un
`void *`, le convertissent en pointeur du vrai objet, et appellent une fonction
ayant la bonne signature. Aucun cast entre pointeurs de fonctions incompatibles
n'est utilisé.

Les méthodes synthétiques des interfaces délèguent à ces tables. Leurs paramètres
possédés sont transférés à l'implémentation concrète ; le wrapper ne les détruit
pas une seconde fois. Un appel indirect peut avoir un coût et peut ou non être
optimisé par le compilateur C. Aucun gain de performance n'est mesuré ici.

## Diagnostics nouveaux

`E4001` : déclaration ou liste `implements` invalide.
`E4002` : méthode requise manquante.
`E4003` : signature incompatible avec le contrat.
`E4004` : tentative de `new` sur une interface.
`E4005` : tentative d'implémentation par une vue locale.

Les erreurs de déplacement restent `E2001`/`E2002` ; les conflits d'emprunts
restent `E3001`/`E3002`/`E3003`. Les incompatibilités nominales simples utilisent le
diagnostic de typage existant, sans nouveau code dédié.


## Interfaces dans les listes (V5)

Une spécialisation `List<Interface>` stocke les représentations d'interface
possédées, pas un second propriétaire parallèle. `add` consomme la valeur,
`pop`/`remove` la transfèrent à l'appelant, et `clear`, `set` ou la destruction de
la liste utilisent la destruction dynamique de chaque élément restant.
`get` n'existe pas pour ces éléments : les retours empruntés sont encore absents.
Voir [STDLIB.md](STDLIB.md).
