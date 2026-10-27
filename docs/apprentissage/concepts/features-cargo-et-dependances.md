# Features Cargo et dépendances : n'embarquer que ce qui sert

## L'idée en 3 phrases

Une dépendance Cargo peut exposer des **features** : des morceaux optionnels que l'on
active dans `Cargo.toml`, et qui sinon ne sont ni compilés ni liés. Le binaire n'alourdit
donc que de ce dont il a besoin, et chaque feature activée est une décision écrite dans le
dépôt, relue par la revue. Une dépendance **transitive** (apportée par une autre) n'est pas
utilisable directement : pour l'appeler, on la déclare, même si elle est déjà dans l'arbre.

## Dans ce projet

### 1. `image` sans ses décodeurs par défaut (`crates/mega-core/Cargo.toml`)

```toml
# png : rendu ; jpeg : images de signature fournies par l'utilisateur.
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
```

`default-features = false` retire la vingtaine de formats que la crate sait lire (WebP,
TIFF, AVIF…) ; on ne garde que PNG (rendu, export) et JPEG (signatures, export,
recompression). Chaque format en moins, c'est du code en moins dans le binaire et une
surface d'attaque en moins face à un fichier piégé (`content/image.rs` borne déjà les tailles).

### 2. `chrono` réduit à l'horloge

```toml
chrono = { version = "0.4", default-features = false, features = ["clock"] }
```

Les dates des annotations et `/ModDate` n'ont besoin que de « maintenant » : pas de fuseaux
horaires, pas de sérialisation. Le commentaire au-dessus de la ligne dit **pourquoi** : c'est
la règle du projet (pas de dépendance sans raison écrite).

### 3. `rand`, déjà là mais à déclarer (`security.rs`)

```toml
# Clé de fichier AES-256 de la protection par mot de passe (Lot 6) ; déjà dans l'arbre via lopdf.
rand = "0.10"
```

`lopdf` dépend de `rand` pour ses propres sels ; nous voulons la même source d'aléa pour la
clé de fichier (`rand::rng().fill(&mut key[..])`). Sans cette ligne, `use rand::…` ne
compile pas : une dépendance transitive n'est pas une dépendance. L'ajouter ne compile rien
de nouveau (même version, résolue une fois dans `Cargo.lock`), mais rend l'usage **explicite**.

### 4. Notre propre feature : `testing` (`lib.rs`)

```rust
#[cfg(feature = "testing")]
pub mod testing;
```

Les générateurs de fixtures (`sample_pdf`, `rotated_pdf`, `heavy_images_pdf`…) ont le droit
de paniquer et tirent `lopdf` dans tous les sens : ils n'ont rien à faire dans le binaire
livré. La feature est activée par les `dev-dependencies` du crate lui-même
(`mega-core = { path = ".", features = ["testing"] }`) et par `xtask`, jamais par `mega-cli`
ni `mega-app`. `cfg(feature = …)` est le cousin de `cfg!(debug_assertions)` vu au Lot 0 :
une condition de **compilation**, pas d'exécution.

## Ce qui se passerait sans

Avec `image` par défaut, le binaire embarquerait des décodeurs jamais appelés, et une
vulnérabilité dans l'un d'eux nous concernerait quand même. Sans la feature `testing`, chaque
`unwrap` des fixtures serait refusé par le lint du crate (ADR-0005), ou devrait être excusé.

## Erreurs fréquentes

- Croire qu'une feature désactivée « n'est juste pas appelée » : elle n'est pas **compilée**,
  d'où des erreurs `unresolved import` qui disparaissent en l'activant.
- Activer une feature dans un crate et l'oublier dans un autre : Cargo unifie les features
  d'une même dépendance dans tout le workspace, ce qui peut cacher le problème en local et le
  révéler en CI (ou l'inverse).
- Ajouter une dépendance « parce qu'elle est déjà là » sans écrire pourquoi : la revue ne peut
  plus juger.

## Pour aller plus loin

Le Cargo Book, chapitre « Features » ; Rust Book, chapitre 14.3 (workspaces).

## Exercice de lecture

`xtask/Cargo.toml` : quelle feature de `mega-core` active-t-il, et que se passerait-il si
`mega-cli` l'activait aussi ? (Indice : le lint `clippy::unwrap_used` et `#![cfg_attr(not(test), …)]`.)
