# ADR-0017 — Limites de ressources et isolement des pannes

Date : 2026-10-30 · Statut : accepté (Lot 13, PR A)

## Contexte

L'audit du Lot 13 (`docs/audit/2026-10-30-robustesse-compatibilite.md`) a cherché ce qu'un
fichier malformé ou hostile peut faire à l'application. Les parcours de graphes étaient déjà
bornés, mais quatre pannes réelles ont été reproduites ou démontrées :

- une chaîne de 5 000 formulaires XObject imbriqués faisait **déborder la pile** pendant un
  caviardage : le processus était tué (`STATUS_STACK_OVERFLOW`) ;
- une **bombe de décompression** (quelques centaines de Ko qui deviennent 300 Mio) était
  décompressée en entier à chaque enregistrement, deux fois ;
- un **Tesseract bloqué** figeait l'acteur PDFium, donc toute l'application ;
- une **panique** dans l'acteur l'arrêtait pour de bon : plus aucune opération jusqu'au
  redémarrage.

Ensuite, une page impossible à rendre restait blanche sans explication, et aucune trace ne
permettait de comprendre une panne chez l'utilisateur.

## Décisions

1. **Une seule porte d'entrée pour `lopdf`** : `crypt::load_limited` (avec
   `LoadOptions::max_decompressed_size`), `crypt::page_content`
   (`get_page_content_with_limit`) et `crypt::stream_bytes` plafonnent toute décompression à
   **256 Mio par flux** (`MAX_DECOMPRESSED`). Au-delà :
   - la passe d'enregistrement écrit les octets de PDFium tels quels ;
   - une édition `lopdf` refuse avec une erreur typée.
2. **Pas de récursion sur une structure du fichier** : le parcours des formulaires, motifs et
   masques (`content/raster.rs`) utilise une pile explicite, comme `reading_order::xy_cut`.
   Au-delà de 64 niveaux, la réponse est prudente : « peint une image », donc le caviardage
   refuse.
3. **Images JPEG** : `content::raster::decode_jpeg` fixe les `image::Limits` (dimensions
   annoncées par le dictionnaire, allocation) **avant** le décodage. Un en-tête menteur est
   refusé sans rien allouer.
4. **Taille des fichiers** : refus au-delà de 2 Gio moins un octet (`MAX_FILE_BYTES`,
   `CoreError::FileTooLarge`), avant toute lecture. PDFium reçoit la longueur d'un document en
   mémoire dans un `int`.
5. **Programmes externes avec délai** : `ocr::output_within` lance Tesseract, lit ses sorties
   dans deux threads (un tube plein bloquerait le processus) et le tue au délai :
   - 300 s par page ;
   - 15 s pour `--list-langs` ;
   - erreur `CoreError::Timeout`.
6. **Isolement des paniques** : `Actor::step` entoure chaque requête de `catch_unwind`.
   - Les documents que la requête a touchés (`Docs` en garde la trace) sont fermés : leur état
     PDFium est douteux.
   - L'appelant reçoit `CoreError::Aborted`, et les autres documents restent ouverts.
   - Le profil `release` garde `panic = "unwind"` (commentaire dans `Cargo.toml`).
7. **Échec de rendu visible** :
   - la commande `render_failure` rejoue une URL `mega://` refusée et rend son message ;
   - `PageView` affiche un cadre « Page impossible à afficher » ;
   - 409 et 404 (passagers) n'affichent rien.
8. **Journal de diagnostic local** (`app/src-tauri/src/diagnostic.rs`) :
   - une ligne par erreur renvoyée à l'interface, et une par panique ;
   - 1 Mo, avec une rotation ;
   - messages passés par `sanitize` : texte entre guillemets et chemins masqués, longueur
     bornée ;
   - jamais de contenu de document, rien d'envoyé ;
   - ouvert depuis Aide ▸ À propos.
9. **Fuzzing sans dépendance** (`testing::fuzz`) :
   - mutations déterministes de vraies fixtures (xorshift64 initialisé par SplitMix64) ;
   - 200 cas à chaque `cargo test` ;
   - campagnes longues avec `cargo xtask fuzz --iterations N --seed S` (graine du cas en cours
     dans `target/fuzz-graine.txt`).

   S'y ajoutent :
   - un test de propriété sur `reading_order::layout` ;
   - le corpus malformé `fixtures/robustesse/` (`tests/robustness.rs`).

## Conséquences

- Un fichier peut toujours être refusé, mais avec une erreur typée, jamais par une fermeture
  de l'application. Exception : un plantage **natif** de PDFium ferme le processus, car il n'y
  a pas d'isolation par processus. Le coût d'un second processus de rendu a été jugé
  disproportionné.
- `cargo fuzz` (guidé par la couverture) n'est pas utilisé : il exige un compilateur nightly
  et ne tourne pas sur toutes nos plateformes de CI. Notre fuzzing est plus naïf mais tourne
  partout.
- Les seuils (256 Mio, 64 niveaux, 2 Gio, 300 s) sont des constantes documentées, pas des
  préférences.
