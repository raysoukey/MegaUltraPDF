# `Read`, `Write` et `Cursor` : des octets sans se soucier d'où ils viennent

## L'idée en 3 phrases

Beaucoup de bibliothèques ne veulent pas savoir si elles écrivent dans un fichier, dans la
mémoire ou sur le réseau : elles demandent « quelque chose qui implémente `std::io::Write` »
(ou `Read` pour lire). `Vec<u8>` implémente `Write`, `&[u8]` implémente `Read`, et `Cursor`
ajoute à un tampon mémoire la notion de **position** (`Seek`) que certains encodeurs exigent.
On peut donc encoder une image, comprimer un flux ou décoder un PDF **entièrement en mémoire**,
puis décider à la fin ce que l'on fait des octets.

## Dans ce projet

### 1. Encoder un JPEG en mémoire (`crates/mega-core/src/export.rs`, `encode`)

```rust
let mut out = Cursor::new(Vec::new());
image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
    .write_image(&rgb, page.width_px, page.height_px, image::ExtendedColorType::Rgb8)?;
Ok(out.into_inner())
```

Ligne par ligne : `Cursor::new(Vec::new())` crée un « fichier en mémoire » vide ; l'encodeur
reçoit `&mut out` parce qu'il ne demande qu'un `impl Write + Seek` ; `write_image` écrit les
octets JPEG dedans ; `into_inner()` rend le `Vec<u8>` final, que `write_atomically` écrira
sur disque. Pour le PNG (`render.rs`, `to_png`), un simple `Vec<u8>` suffit : l'encodeur PNG
n'a pas besoin de revenir en arrière.

### 2. Lire une image depuis des octets (`content/image.rs`, `load_bounded`)

```rust
let reader = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
let (w, h) = reader.into_dimensions()?;
```

Ici `Cursor` enveloppe un **emprunt** (`&bytes`) : rien n'est copié. `with_guessed_format`
lit les premiers octets pour reconnaître PNG ou JPEG, puis **revient au début** grâce à
`Seek` : c'est précisément pour cela qu'un `&[u8]` nu (qui implémente `Read` mais pas `Seek`)
ne suffirait pas.

### 3. `lopdf` écrit dans n'importe quel `Write` (`cleanup.rs`, `overlay.rs`, `security.rs`)

```rust
let mut out = Vec::new();
doc.save_to(&mut out)?;
self.reload_from_bytes(out)
```

`save_to` prend un `&mut impl Write` : on lui donne un `Vec<u8>` et le PDF réécrit reste en
mémoire, prêt à être rechargé par PDFium ou validé (`rewrite_is_sane`) avant de remplacer
quoi que ce soit sur disque. Le même appel écrirait dans un `File` ; c'est le **trait** qui
rend ce choix indifférent à `lopdf`.

### 4. Le tampon brut de la comparaison (`app/src-tauri/src/commands.rs`, `compare_pages`)

```rust
let mut body = Vec::with_capacity(compared.png.len() + 24);
body.extend_from_slice(&compared.differing_pixels.to_le_bytes());
// … puis le PNG
Ok(tauri::ipc::Response::new(body))
```

Pas de `Write` ici, mais la même idée : des octets assemblés en mémoire, et un format
minuscule (seize octets de compteurs devant l'image) lu côté TypeScript par un `DataView`
(`logic/export.ts`, `parseCounters`). Le contrat est écrit noir sur blanc des deux côtés.

## Ce qui se passerait sans

Sans `Write`, chaque encodeur devrait proposer « vers un fichier », « vers la mémoire », …
et nous ne pourrions pas valider un PDF réécrit **avant** de l'écrire : `write_atomically`
(ADR-0007) repose sur « j'ai tous les octets d'avance ».

## Erreurs fréquentes

- Passer `&[u8]` à une API qui exige `Seek` : l'erreur de compilation parle de borne de trait
  (`the trait Seek is not implemented`) ; la réponse est `Cursor::new(...)`.
- Oublier `into_inner()` et garder le `Cursor` : on transporte une position inutile.
- Lire un flux inconnu sans borne (`decompressed_content`) : `lopdf` propose
  `decompressed_content_with_limit` ; c'est ce que fait `optimize.rs` pour les images.

## Pour aller plus loin

Rust Book, chapitre 10 (traits comme paramètres) ; documentation de `std::io::{Read, Write,
Seek, Cursor}`.

## Exercice de lecture

`export.rs`, fonction `encode` : pourquoi le JPEG passe-t-il par un `Cursor` alors que
`RenderedPage::to_png` (`render.rs`) se contente d'un `Vec<u8>` ? Cherchez dans la doc
d'`image` la borne de trait de `JpegEncoder::new_with_quality` et celle de `PngEncoder::new`.
