# Lot 13 — Robustesse et compatibilité : ce que vous venez de voir

## Ce que le lot a construit

- Un **audit** écrit (`docs/audit/2026-10-30-robustesse-compatibilite.md`) : 27 risques, chacun
  avec sa preuve, sa correction et son statut.
- Un fichier abîmé ou piégé ne ferme plus l'application et ne la fige plus. Exemples : bombe de
  décompression, 5 000 formulaires imbriqués, JPEG menteur, Tesseract bloqué, panique interne.
  Une page impossible à afficher dit pourquoi, et un journal local garde la trace des erreurs.
- Chaque fichier que nous écrivons passe le contrôle de **qpdf**, un validateur indépendant.
  Ce contrôle a trouvé deux vrais défauts de signature.
- Mieux pris en charge : les anciens chiffrements et les mots de passe dans n'importe quelle
  écriture, l'hébreu, l'arabe et le japonais vertical, une recherche qui suit l'ordre de
  lecture, les espaces des PDF LaTeX, la déclaration PDF/A.
- Côté plateforme : miniatures réordonnables sous Windows, pages nettes sur les écrans à 150 %
  et 200 %, messages d'enregistrement clairs, chemins de plus de 260 caractères, `.dmg`
  universel. Version 0.13.0.

## Concepts Rust rencontrés

1. **Pile explicite au lieu de la récursion** : `hidden_images_in` range le travail à faire dans
   un `Vec` et le reprend dans une boucle `while let Some(…) = pending.pop()`.
   - La mémoire vient alors du tas, et la profondeur se compte (`MAX_NESTING`).
   - Une fonction qui s'appelle elle-même 5 000 fois épuisait la pile du thread
     (`crates/mega-core/src/content/raster.rs:416` et `:432`).
2. **Une porte d'entrée unique** : toutes les lectures `lopdf` passent par `load_limited`,
   `page_content` et `stream_bytes`, qui plafonnent la décompression
   (`crates/mega-core/src/crypt.rs:30`, `:39`, `:92`). Un seul endroit où penser à la
   sécurité, au lieu de treize.
3. **Attendre un processus avec un délai** : `output_within`
   (`crates/mega-core/src/ocr.rs:50`).
   - Deux threads lisent les sorties du programme pendant que le thread principal surveille
     sa fin avec `try_wait`, qui ne bloque pas.
   - Au délai, `kill_tree` arrête aussi les petits-enfants (`:112`).
   - Fiche : [canaux et patron acteur](concepts/canaux-et-acteur.md).
4. **`catch_unwind`** : une panique remonte normalement jusqu'au bout du thread et le tue.
   - `std::panic::catch_unwind` l'arrête à une frontière choisie : une requête de l'acteur
     (`crates/mega-core/src/service/mod.rs:833`).
   - `AssertUnwindSafe` est une promesse : on ne réutilisera pas un état à moitié modifié. Elle
     est tenue parce que les documents touchés sont fermés aussitôt.
   - Le profil `release` doit garder `panic = "unwind"` (`Cargo.toml`).
5. **Variantes d'erreur qui disent quoi faire** : `Timeout`, `Aborted`, `FileTooLarge`,
   `UnsupportedEncryption`, `FileReadOnly`, `FileLocked`, `FolderNotWritable`
   (`crates/mega-core/src/error.rs`).
   - L'interface montre un message utile au lieu de « Erreur d'accès au fichier ».
   - Fiche : [enums et match exhaustif](concepts/enums-et-match-exhaustif.md) (la conversion
     vers `ErrorKind`, `app/src-tauri/src/error.rs`, refuse de compiler si une variante est
     oubliée).
6. **Générateur pseudo-aléatoire reproductible** : `XorShift`
   (`crates/mega-core/src/testing/fuzz.rs:20`) mélange les bits d'un entier.
   - Le test de reproductibilité a trouvé un vrai bogue : `seed ^ C | 1` donnait le même état
     pour 42 et 43, car `^` passe avant `|`.
   - D'où le mélange SplitMix64 à l'initialisation (`:26`).
7. **Sous-tranche retournée en place** : `line[k..=end].reverse()` remet de gauche à droite un
   nombre pris dans une ligne arabe, sans copie
   (`crates/mega-core/src/reading_order.rs:187`, `logical_order`).
8. **Arithmétique de calendrier sans dépendance** : `utc_timestamp` convertit une date Unix en
   jour du calendrier grégorien, par ères de 400 ans
   (`app/src-tauri/src/diagnostic.rs:228`).
9. **`#[cfg(unix)]` / `#[cfg(windows)]`** dans les tests (`crates/mega-core/tests/saving.rs`) :
   - un fichier verrouillé n'a de sens que sous Windows ;
   - un lien physique ou un nom non UTF-8 ne se teste que sous Unix ;
   - la CI fait tourner les trois.

   Fiche : [features Cargo et dépendances](concepts/features-cargo-et-dependances.md).

Côté TypeScript :
- les événements **pointeur** (`pointerdown`, `pointermove`, `pointerup`, `setPointerCapture`)
  remplacent le glisser-déposer HTML5 (`app/src/lib/components/Thumbnails.svelte:152`) ;
- `devicePixelRatio`, suivi par `matchMedia`, fixe l'échelle de rendu
  (`app/src/lib/pageUrl.ts:17`).

## Décisions de conception

- **ADR-0017**, limites de ressources et isolement des pannes :
  - 256 Mio par flux, 64 niveaux d'imbrication, 2 Gio par fichier, 300 s par page d'OCR ;
  - une panique ne ferme que les documents que la requête pouvait **modifier** ;
  - journal local masqué (texte entre guillemets, chemins).
- **ADR-0018**, compatibilité :
  - qpdf comme juge indépendant, obligatoire en CI ;
  - signer seulement ce que lopdf voit comme PDFium ;
  - mots de passe invariants par SASLprep (PDFium ne normalise pas) ;
  - recherche dans l'ordre de lecture ;
  - PDF/A déclaré mais pas vérifié.
- **ADR-0007** (complément) :
  - erreurs d'enregistrement typées, dossier canonique, liens physiques écrits en place ;
  - limite assumée : les ACL et flux alternatifs Windows sont perdus (il faudrait du code
    `unsafe`).

## Pièges rencontrés

- **Un test qui simule ce qu'il devrait vérifier** : les scénarios CDP fabriquaient des
  `DragEvent` à la main, directement dans la page. Ils passaient alors que le glisser-déposer
  réel ne marchait pas sous Windows. Un test doit passer par le même chemin que l'utilisateur.
- **Ne jamais piloter la vraie souris** : un essai qui visait la fenêtre de l'application a
  cliqué dans une autre fenêtre ouverte sur le poste. Les vérifications restent dans la page
  (CDP).
- **lopdf lit « sans erreur » une table xref décalée, mais sans aucune page** : signer
  échouait. On compare désormais à PDFium (`crates/mega-core/src/digital_signature/pdf.rs:902`).
- **Les espaces invisibles de LaTeX** : pdfTeX sépare les mots par du crénage, sans caractère
  espace. PDFium ajoute une espace sans largeur, que l'ordre de lecture écartait (« Helloworld »).
  `with_word_spaces` lui donne la largeur du blanc, mais pas au-delà d'un caractère : sinon
  elle comblait la gouttière entre deux colonnes
  (`crates/mega-core/src/text.rs:174`).
- **Chiffres arabo-indiens** (« ٢٠٢٤ ») : ils sont dans le bloc Unicode arabe, mais un nombre
  se lit toujours de gauche à droite (`direction`, `reading_order.rs`).
- **Chemins de plus de 260 caractères** sous Windows : le renommage final de `tempfile`
  échouait ; le dossier canonique `\\?\…` lève la limite
  (`crates/mega-core/src/edit.rs:851`).
- **Message de panique et vie privée** : la bibliothèque standard cite parfois la chaîne fautive
  entre accents graves ; `sanitize` les masque aussi (`diagnostic.rs:149`).

## Exercices de lecture

1. `crates/mega-core/src/cleanup.rs:71` (`page_contents`) : pourquoi comparer les octets
   **encore compressés** des contenus suffit-il pour vérifier la réécriture, et pourquoi
   est-ce plus sûr devant une bombe de décompression ?
2. `crates/mega-core/src/service/mod.rs:757` (`Docs`) : pourquoi `get` ne note-t-il pas le
   document touché, alors que `get_mut` le fait ? Que perdrait l'utilisateur sinon ?
3. `crates/mega-core/src/security.rs:79` (`check_password`) : un mot de passe contenant une
   espace insécable est refusé. Que se passerait-il au moment de rouvrir le fichier si on
   l'acceptait ?

## Prérequis pour la suite

- Comprendre `Result` et les variantes d'erreur
  ([fiche](concepts/result-et-operateur-point-d-interrogation.md)).
- Savoir lire un test qui échoue avant d'écrire le code : chaque correction de ce lot commence
  par là.
- Pour la signature des paquets (prochaine étape possible) : notions de certificat et de chaîne
  de confiance, côté système plutôt que Rust.
