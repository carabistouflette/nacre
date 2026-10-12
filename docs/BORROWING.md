# Modèle d’emprunts — Nacre V6

Ce document décrit le sous-ensemble implémenté, pas une équivalence avec Rust
ni une preuve formelle de sûreté. Le projet Rust et ses tests ont été exécutés ;
voir [VALIDATION.md](VALIDATION.md) pour la portée des contrôles.

## Contrats de types

`C` est un propriétaire unique, `borrow C` un accès partagé et `borrow mut C`
un accès exclusif. Ces deux derniers types sont réservés aux classes/interfaces,
pas aux primitives, enums ou vues locales. Le compilateur insère une expression IR `Borrow`
lorsqu'un paramètre, une variable ou un champ attend ce type. Les arguments doivent
être des places nommées : variable, paramètre, `this` ou projection de champs.
Une allocation temporaire doit d'abord recevoir un propriétaire local.

Il n'y a ni copie de l'objet, ni transfert de propriété, ni comptage de références.
En C, l'emprunt est simplement un pointeur non possédé. Le compilateur ne transmet
pas de `restrict` ou de `const` C supposé prouver les garanties : celles-ci reposent
sur ses propres analyses avant émission.

Les enums sont des propriétaires affines **par valeur** : `match` consomme la
somme et donne la propriété des payloads à ses variables de branche. Elles ne
peuvent contenir de référence ni de vue locale, même à travers un autre payload.
La V6 ne fournit pas de `match` emprunté ; les classes/interfaces conservent
leurs règles d’emprunts lexicaux.


## Domaine d'alias et capacités

Un objet possédé et tous ses champs objets possédés forment un même domaine.
Un emprunt d'un enfant bloque donc les accès incompatibles au parent et à ses
autres enfants. Ce choix conservatif évite de promettre une analyse des chemins
projetés indépendante que la V3 ne possède pas.

Chaque emprunt garde un identifiant de racine et éventuellement le prêt parent
à travers lequel il a été obtenu. Un accès autorisé via un réemprunt utilise les
capacités de ses ancêtres ; il doit encore être compatible avec les autres prêts
actifs de la même racine. Deux prêts partagés peuvent coexister. Un accès exclusif
ou une destruction est incompatible avec un autre prêt actif.

Un propriétaire n'utilise pas automatiquement la capacité d'un emprunt mutable
créé depuis lui : pendant cet emprunt, le code doit passer par la référence.
Les paramètres empruntés sont des racines abstraites dans le corps de fonction.
Le site d'appel vérifie les conflits réels, y compris quand plusieurs arguments
ont des noms différents mais la même origine.

## Durée de vie et contrôle de flux

Une variable empruntée a une seule initialisation et aucune réaffectation. Elle
reste dans son bloc et ne peut être retournée. Un prêt local se termine à la fin
du bloc ; un prêt d'argument se termine au retour de l'appel, sauf capture par une
vue locale. Il n'existe pas de primitive de libération anticipée des prêts.

Cette restriction évite des unions d'origines réaffectables aux jonctions du CFG.
Chaque branche a ses propres prêts locaux et les prêts d'un corps de boucle ne
peuvent en sortir. La passe structurelle peut donc restaurer le même ensemble de
prêts externes à la sortie d'un bloc. `break`, `continue` et `return` ne transportent
pas de référence empruntée hors de ce bloc.

L'initialisation des propriétaires et leurs déplacements restent vérifiés par
le **point fixe sur CFG de la V2**. Une référence ne peut être créée depuis une
variable déplacée ou non initialisée. L'analyse des prêts interdit ensuite de
libérer ou remplacer la cible tant qu'une référence la maintient empruntée.

Une référence inutilisée reste active. Une condition constante ne supprime pas la
vérification de sa branche. Des programmes sûrs peuvent être refusés ; c'est une
limite documentée, pas une analyse équivalente aux NLL de Rust.

## Méthodes et deux phases

Les méthodes non annotées ont un receveur modifiant ; `readonly` donne un receveur
en lecture seule. Le type checker vérifie transitivement l'accès à travers les
champs et refuse un appel modifiant sur un receveur partagé. `readonly` n'est pas
un effet de pureté globale : les paramètres mutables distincts restent modifiables.

Le receveur d'un appel est évalué avant les arguments. Sa racine reçoit d'abord une
**réservation de durée de vie**, qui interdit déplacement, destruction et remplacement,
mais autorise encore des appels imbriqués qui ne l'invalident pas. L'accès partagé
ou exclusif du receveur est vérifié après les arguments, contre les prêts de ces
arguments et les prêts locaux encore actifs.

Cela accepte `counter.add(counter.get())`, où l'appel intérieur se termine avant
l'appel extérieur. En revanche, un appel modifiant `counter.method(counter)` dont
le paramètre emprunte le même objet est refusé. Les arguments empruntés ordinaires
n'ont pas cette réservation en deux phases : ils acquièrent immédiatement leur prêt.

Les champs objets possédés ne peuvent toujours pas être remplacés, ce qui contribue
à maintenir les projections de receveur stables pendant l'évaluation des arguments.

## Vues locales : première forme de champs empruntés

Une classe possédant un champ `borrow C` est une vue. Seuls les champs partagés sont
admis. Leur constructeur doit les initialiser directement depuis un paramètre
`borrow C` de même type ; ni propriétaire local, ni paramètre possédé, ni expression
calculée, ni paramètre mutable ne convient dans cette version.

La passe de typage enregistre, pour chaque champ emprunté, **l'indice du paramètre
constructeur qui le fournit**. Les autres paramètres et les champs peuvent être
dans un ordre différent. Le site `View v = new View(...)` garde les prêts des
arguments effectivement capturés jusqu'à la fin du bloc de `v`. Plusieurs champs
peuvent partager un paramètre ; plusieurs paramètres partagés peuvent désigner
la même cible.

L'accès à `v.target` retrouve l'origine capturée, plutôt que de traiter le champ
comme un nouvel objet indépendant. Dans une méthode de vue, les champs empruntés
ont des racines abstraites partagées ; les prêts du site d'appel empêchent qu'un
paramètre modifiant/possédé aliase ces cibles.

Une vue peut être instanciée, lire ses champs empruntés, modifier ses propres
primitives et objets possédés, et appeler ses méthodes. Elle ne peut être copiée,
déplacée, passée en paramètre, retournée, réaffectée, empruntée en tant que vue,
détruite explicitement ou contenue dans un autre objet. Sa destruction lexicale
libère ses propres sous-objets possédés, **jamais ses champs empruntés**.

Ces contraintes excluent, entre autres, les objets autoréférentiels et les graphes
arbitraires de références persistantes. Les retours empruntés, les paramètres de
durée de vie et les vues exportables nécessiteraient des contrats supplémentaires.

## Diagnostics

| Code | Sens |
|---|---|
| `E2001` | Propriétaire déplacé ou potentiellement déplacé, hérité de la V2. |
| `E2002` | Variable non initialisée sur tous les chemins. |
| `E3001` | Lecture ou accès exclusif incompatible avec un prêt actif. |
| `E3002` | Déplacement, destruction ou remplacement pendant un prêt/réservation. |
| `E3003` | Modification via un accès partagé ou un receveur `readonly`. |
| `E3004` | Retour de référence empruntée non pris en charge. |
| `E3005` | Liaison empruntée non initialisée ou réaffectée. |
| `E3007` | Restriction du modèle de vues locales. |
| `E3099` | Limite de ressources du borrow checker. |

Certaines incompatibilités nominales sont des erreurs de typage générales, sans code
E300x. Une note sur le lieu du prêt accompagne les conflits dynamiques du graphe
lexical ; les erreurs de type `readonly` sont établies avant cette passe.


## Extensions V5 : interfaces et génériques

Les règles lexicales introduites en V3 restent inchangées. Un paramètre
`borrow I` ou `borrow mut I`, où `I` est une interface, emprunte l'objet concret
sans transférer sa propriété. L'objet doit implémenter explicitement cette
interface. Le compilateur conserve l'origine concrète du prêt : changer de type
visible ne permet pas de contourner les conflits d'accès.

Les méthodes d'interface doivent annoncer leur mode de receveur (`readonly` ou
modifiant), leurs paramètres possédés/empruntés et leur retour. L'implémentation
est tenue de reprendre exactement ce contrat. Ainsi, l'analyse d'un appel ne
dépend pas d'une recherche du corps qui sera choisi à l'exécution.

Un champ `borrow I` reste un champ emprunté partagé, avec les mêmes restrictions
de construction et de vue locale. Une vue ne peut pas implémenter une interface,
pour ne pas être cachée dans un propriétaire effacé puis exportée hors de son
bloc. Les interfaces propriétaires peuvent, en revanche, être placées dans les
champs possédés d'une classe ordinaire.

Les types génériques sont spécialisés **avant** cette analyse. `Holder<Counter>`
possède donc réellement son `Counter`, tandis que `View<Counter>` contenant un
champ `borrow T` devient une vue locale ordinaire. Un type primitif substitué à
`T` dans `borrow T` est rejeté : le modèle d'emprunts reste limité aux objets.

Voir [INTERFACES.md](INTERFACES.md) et [GENERICS.md](GENERICS.md) pour les exemples
et les limites de ces ajouts. La validation locale couvre la compilation Rust,
les analyses et l'exécution des cas de test, sans prouver leur correction générale.

## Référence conceptuelle

Les règles « plusieurs accès partagés ou un accès exclusif » sont notamment
présentées dans [The Rust Programming Language — References and Borrowing](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html).
Le présent prototype utilise sa propre analyse lexicale, plus restrictive, et
**ne réutilise ni le borrow checker ni le système de lifetimes de rustc**.


## Bibliothèque native V5

Les appels de `List<T>` et `Text` suivent les mêmes contrats de paramètres et de
receveurs. Par exemple, `Text.appendText(borrow Text)` exige un prêt partagé
distinct du receveur mutable, alors que `Text.equals(borrow Text)` utilise un
receveur partagé et permet de comparer un texte à lui-même.

Les méthodes natives ne produisent aucune référence retournée : `List<T>.get`
existe seulement pour `int`, `boolean` et `String`. Les autres éléments sont
accessibles par transfert avec `pop`/`remove`. Les vues locales sont interdites
dans les listes, même lorsque le type est imbriqué. Leurs corps C sont de
confiance ; seuls les appels utilisateur sont analysés par le frontend.
