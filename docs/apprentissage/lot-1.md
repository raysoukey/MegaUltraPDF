# Lot 1 — Lecteur : note d'apprentissage

## Ce que le lot a construit

Un vrai lecteur : plusieurs documents en onglets, rendu net à tout zoom sans figer
l'interface, vignettes, signets, liens, sélection et copie de texte, recherche avec
surbrillance, propriétés, thème clair/sombre, raccourcis, préférences et positions de lecture
mémorisées, glisser-déposer. Côté Rust, le changement de fond est l'**acteur PDFium**
(`crates/mega-core/src/service/`) : un thread unique possède les documents, reçoit des
messages priorisés, met les rendus en cache et annule ceux qui sont périmés.

## Concepts Rust rencontrés

### 1. Threads et canaux : le patron acteur

`PdfService::spawn` (`service/mod.rs`) crée un thread nommé `pdfium-actor` avec
`std::thread::Builder`. Il lui donne le **récepteur** d'un canal `mpsc::channel`. Chaque
`ServiceHandle` possède un **émetteur** cloné ; `send` est non bloquant. Pour la réponse,
chaque requête emporte un `sync_channel(1)` : un canal à une place qui joue le rôle de
« réponse unique ». La méthode `ServiceHandle::call` montre tout le mécanisme en six lignes.

- Fiche à venir : `/concept canaux-et-acteur`. Lire aussi `Request` : une variante par message,
  le `match` de `Actor::handle` est exhaustif.

### 2. `BinaryHeap`, `Ord` et `Reverse`

`service/queue.rs` : la file renvoie toujours la priorité la plus haute, puis la plus
ancienne. `BinaryHeap` sort le plus grand élément ; `Reverse(seq)` inverse l'ordre du numéro
de séquence pour que « plus ancien » soit « plus grand ». L'`impl Ord` manuel compare la
priorité puis la séquence (`then_with`).

### 3. Un cache LRU avec `HashMap` + `VecDeque` et `Arc<Vec<u8>>`

`service/cache.rs` : la `HashMap` retrouve une image en O(1), la `VecDeque` garde l'ordre
d'utilisation (fin = plus récent). Les PNG sont partagés par `Arc` : le cache et la réponse
pointent les mêmes octets ; `Arc::ptr_eq` dans le test prouve qu'il n'y a pas de copie.
L'échelle `f32` n'est pas `Hash` : on la quantifie en `u32` (`scale_x100`).

### 4. Générations : annuler sans tuer

Un compteur par document (`Entry.generation`). Le frontend l'incrémente au changement de
zoom/rotation ; un rendu qui arrive avec une génération périmée reçoit `CoreError::Cancelled`
immédiatement. Pas de thread à interrompre, pas de drapeau partagé : la requête porte
elle-même la preuve qu'elle est encore utile.

### 5. `async` côté Tauri, blocage côté acteur : `spawn_blocking`

`app/src-tauri/src/commands.rs` : les commandes sont `async fn` ; comme le handle **bloque**
en attendant la réponse, on l'appelle dans `tauri::async_runtime::spawn_blocking`, un pool
réservé aux opérations bloquantes, puis on `.await` le résultat. La fonction `blocking` évite
de répéter ce schéma. Le protocole `mega://` fait pareil avec `UriSchemeResponder`.

### 6. Structures récursives et listes chaînées

`outline.rs` : `OutlineNode` contient `Vec<OutlineNode>` (possible grâce à l'indirection
du `Vec`) ; les signets PDFium sont une liste chaînée (`next_sibling`), parcourue avec
`while let Some(b) = current`.

### 7. Durées de vie et fermetures : l'erreur E0515

Dans `outline.rs::target_page`, une première version renvoyait une destination qui
empruntait une action temporaire : « cannot return value referencing function parameter ».
Correction : extraire l'entier (`Copy`) avant que l'action ne soit détruite. À relire.

### 8. Côté TypeScript (pour mémoire)

Les runes Svelte 5 (`$state`, `$derived`, `$effect`, `$props`) dans `app/src/lib/state/` ;
la logique pure testée dans `app/src/lib/logic/` (géométrie avec rotation, zoom, sélection,
virtualisation). Règle appliquée : un composant enfant ne mute jamais une prop ; il appelle
une fonction du module d'état (`setSelection`, `notePage`).

## Décisions de conception du lot

ADR-0006 (acteur, file priorisée, cache, générations). Voir aussi la spec du lot pour les
choix d'interface (sélection maison, recherche par l'acteur, vignettes via `mega://`).

## Pièges rencontrés

- **Itérateur de signets** : `iter_siblings` de pdfium-render ne donnait qu'un élément au
  premier niveau ; suivre `next_sibling` à la main a résolu le problème (test `outline_tree_has_child`).
- **Mutations de props en Svelte** : avertissement `ownership_invalid_mutation` quand
  `TextLayer` modifiait `doc.selection` ; passer par une fonction du module d'état.
- **Assertions trop tôt** dans les scénarios CDP : un `$effect` s'exécute après la micro-tâche ;
  attendre avant de lire le DOM.
- **Commande groupée refusée** : le classifieur de sécurité a refusé un `git rm` mêlé à des
  éditions ; des étapes séparées et lisibles passent.

## Vérification sans souris

`scripts/ui-check.ps1 -Pdf fixtures/simple.pdf -Scenario scripts/scenarios/lecteur-simple.json`
lance l'application avec le port de débogage WebView2, déroule le scénario (clics, touches,
glisser, captures) et affiche ✔/✘ par assertion. Les captures sont dans `target/ui-check/`.

## Exercices de lecture

1. `service/mod.rs`, `Actor::run` : pourquoi faire `recv()` **puis** une rafale de `try_recv()`
   avant de traiter ? Que se passerait-il avec un simple `for req in rx` ?
2. `service/cache.rs`, `insert` : la boucle d'éviction s'arrête à `order.len() > 1`. Pourquoi
   garder au moins une entrée même si elle dépasse le budget ?
3. `app/src/lib/components/PageView.svelte` : expliquez comment l'ancienne image reste
   affichée pendant qu'une nouvelle génération se rend, et ce qui se passe si le backend
   répond 409.

## Prérequis pour le Lot 2

Comprendre le patron acteur (les éditions de pages passeront par lui), les traits objets
(`Box<dyn Command>` pour annuler/rétablir) et l'écriture atomique de fichiers.
