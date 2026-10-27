# Complexité et collections : choisir `Vec`, `HashMap` ou `BTreeSet`

## L'idée en 3 phrases

Une même question (« ai-je déjà vu cet objet ? ») coûte très différemment selon la structure
qui range les données : un `Vec` doit être parcouru en entier, une `HashMap` ou un `BTreeSet`
répond presque tout de suite. On note ce coût en « O » : O(n) veut dire « proportionnel au
nombre d'éléments », O(log n) « grandit très lentement », O(1) « constant ». La bibliothèque
standard de Rust offre ces structures (`std::collections`) ; choisir la bonne évite un
programme qui devient lent sur un gros fichier sans jamais être faux sur un petit.

## Dans ce projet

**1. Parcourir un graphe d'objets PDF** (`crates/mega-core/src/cleanup.rs:67`)

```rust
let mut seen = BTreeSet::new();          // objets déjà visités, triés, sans doublon
let mut todo = Vec::new();               // pile des objets à visiter (on ajoute/retire au bout)
// …
while let Some(id) = todo.pop() {        // O(1) : on retire le dernier élément de la pile
    if seen.insert(id)                    // O(log n) : vrai seulement à la première visite
        && let Some(object) = doc.objects.get(&id)
    {
        refs_of(object, &mut todo);       // on empile ce que cet objet référence
    }
}
```

`insert` répond à deux questions d'un coup : « déjà vu ? » (il rend `false`) et « retiens-le ».
L'ancien élagage de `lopdf` gardait les objets vus dans un `Vec` et appelait `contains` à chaque
référence : O(n) par question, donc O(n²) au total (Lot 9, C22). Le `BTreeSet` protège aussi
contre les références cycliques : un objet déjà vu n'est pas revisité.

**2. Le cache des pages rendues** (`crates/mega-core/src/service/cache.rs:46-47` et `103-111`)

```rust
entries: HashMap<PageKey, (Arc<Vec<u8>>, u64)>,  // page → (image, « tic » d'utilisation)
order: BTreeMap<u64, PageKey>,                   // tic → page, trié : le premier est le plus ancien

fn touch(&mut self, key: &PageKey) {
    let Some((_, tick)) = self.entries.get_mut(key) else { return };  // O(1)
    self.order.remove(tick);              // O(log n) : on retire l'ancien tic
    self.tick += 1;
    *tick = self.tick;
    self.order.insert(self.tick, *key);   // O(log n) : la page devient la plus récente
}
```

Deux structures coopèrent : la `HashMap` retrouve une image par sa clé, le `BTreeMap` donne le
moins récent avec `pop_first()`. Avant le Lot 9, l'ordre était une `VecDeque` : retrouver la
place d'une page demandait un parcours (`iter().position`), O(n) à chaque affichage.

**3. Trier et dédoublonner** (`crates/mega-core/src/pagespec.rs:21`)

```rust
let set: BTreeSet<u16> = groups.into_iter().flatten().collect();  // trié, sans doublon
Ok(set.into_iter().collect())                                       // → Vec<u16> ordonné
```

« 3,1-2,2 » devient `[0, 1, 2]` : le `BTreeSet` fait le tri et retire les doublons, sans code
de tri écrit à la main.

**4. Mémoriser un calcul coûteux** (`crates/mega-core/src/digital_signature/pdf.rs:283`)

```rust
type DigestCache = HashMap<(usize, usize, usize), [u8; 32]>;  // plage → empreinte SHA-256
```

Deux champs de signature qui désignent la même plage d'octets ne font hacher le fichier
qu'une fois : `digests.entry(plage).or_insert_with(|| …)` ne calcule que si la clé manque.

## `HashMap` ou `BTreeMap` ?

- `HashMap` / `HashSet` : accès en O(1) en moyenne, **sans ordre**. La clé doit implémenter
  `Hash` et `Eq`.
- `BTreeMap` / `BTreeSet` : accès en O(log n), **éléments triés** (premier, dernier, parcours
  dans l'ordre). La clé doit implémenter `Ord`. Le projet les préfère quand l'ordre sert
  (pages, plus ancien du cache) ou pour des sorties stables d'une exécution à l'autre.

## Ce qui se passerait sans

Rien ne casserait dans les tests : le résultat est le même avec un `Vec`. Mais sur un PDF de
100 000 objets, O(n²) fait 10 milliards de comparaisons au lieu d'environ 1,7 million : un
enregistrement qui prend des minutes au lieu d'une fraction de seconde. Un bug de lenteur,
invisible sur les petits fichiers de test.

## Erreurs fréquentes

- **Clé qui n'implémente pas le trait voulu.** Un `f32` n'implémente ni `Eq`, ni `Hash`, ni
  `Ord` (à cause de `NaN`) : `HashMap<f32, _>` ne compile pas (« the trait bound `f32: Eq` is
  not satisfied »). Le projet stocke l'échelle multipliée par 100 en entier
  (`crates/mega-core/src/service/cache.rs:16-21`, `scale_x100`).
- **Modifier une collection pendant qu'on la parcourt.** `for (k, v) in map.iter() {
  map.remove(k) }` ne compile pas (emprunt partagé puis exclusif). On collecte d'abord les clés
  à retirer, puis on retire (`invalidate_doc`, même fichier).
- **`contains` dans une boucle sur un `Vec`.** Ça compile, et c'est le piège de l'exemple 1.

## Pour aller plus loin

- Rust Book, chapitre 8 « Common Collections » :
  <https://doc.rust-lang.org/book/ch08-00-common-collections.html>

## Exercice de lecture

Relisez `crates/mega-core/src/service/cache.rs:66-80` (`insert`). Pourquoi faut-il retirer
l'ancien tic de `order` quand une page déjà présente est remplacée ? Que verrait-on, plus tard,
au moment de l'éviction, si on l'oubliait ?
