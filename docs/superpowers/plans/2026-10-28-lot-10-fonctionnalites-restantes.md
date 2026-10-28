# Lot 10 — Fonctionnalités restantes : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** police Unicode incorporée, caviardage d'images partiel, signature visible, nettoyage des
documents chiffrés, OCR en un pas d'annulation, dialogues unifiés, mineurs du Lot 9 ; v0.10.0.

**Architecture:** un module `fonts::unicode` (DejaVu Sans via `ttf-parser`) et un écrivain de
police `Type0` `lopdf` sous-ensemble (`subsetter`) branchés sur les passes `lopdf` existantes
(overlay, OCR, signature) ; PDFium charge la même police pour ses objets texte. Le caviardage
remplace une image partielle par une nouvelle image PDFium noircie. Une « session lopdf »
déchiffre puis rechiffre à l'identique pour toutes les réécritures.

**Tech Stack:** Rust (pdfium-render 0.9.4, lopdf 0.46, ttf-parser 0.25, subsetter 0.2), Tauri 2,
Svelte 5, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-lot-10-fonctionnalites-restantes.md`

## Global Constraints

- Règles de `CLAUDE.md` : SPDX, `forbid(unsafe_code)`, docs `///` françaises disant pourquoi,
  `[Apprentissage]`, pas d'`unwrap` hors tests, chaînes UI dans `strings.ts` fr **et** en.
- Toute méthode publique qui modifie le document appelle `self.mark_changed()` en tête.
- `engine::guard()` non réentrant : PDFium d'abord (mesures), puis `lopdf`, puis recharger.
- Nouvelle dépendance : raison écrite dans le message de commit.
- TDD : test vu échouer avant le code ; `clippy` lancé seul avant chaque commit.
- Chemins de test PDFium : `crate::testing` et `fixtures/`.

## Review Focus

1. Texte mixte (« Prix : 10 € — Цена ») : Helvetica ne peut pas, la police Unicode doit couvrir
   les deux ; largeur et centrage cohérents avec le rendu.
2. Caviardage d'une image sur une page tournée ou d'une image retournée (matrice négative) : la
   zone noircie doit tomber sous le rectangle affiché.
3. Document chiffré AES-256 réécrit : se rouvre avec le même mot de passe, permissions `/P`
   inchangées, aucune chaîne en clair.
4. Signature visible avec un nom de signataire non latin et sur une page dont `/Annots` est
   inline ou absent.
5. Arrêt de l'OCR en cours de route : le groupe d'annulation se ferme quand même.

Chaque point a son test dans la tâche qui possède le code (marqué **RF**).

---

### Task 1: Police Unicode embarquée (`fonts::unicode`)

**Files:** Create `crates/mega-core/assets/fonts/{DejaVuSans.ttf,LICENSE-DejaVu.txt}`,
`crates/mega-core/src/fonts/unicode.rs` ; Move `fonts.rs` → `fonts/mod.rs` ; Modify
`crates/mega-core/Cargo.toml`, `Cargo.lock`.

**Interfaces — Produces:**
- `pub(crate) static DEJAVU_SANS: &[u8]` (`include_bytes!`).
- `pub(crate) struct UnicodeFont` ; `UnicodeFont::embedded() -> &'static UnicodeFont`
  (`OnceLock`, analyse unique ; `#[allow(clippy::expect_used)]` justifié : police embarquée
  vérifiée par test) ; `glyph(&self, c: char) -> Option<u16>` ; `advance(&self, gid: u16) -> u16`
  (millièmes de la taille, arrondi) ; `text_width(&self, text: &str, size: f32) -> f32`
  (glyphe absent = avance du glyphe 0) ; `ascent()/descent()/cap_height()/bbox()` en millièmes ;
  `units_per_em()`.
- `pub fn missing_glyphs(text: &str) -> Vec<char>` (dans `fonts/mod.rs`, réexport public :
  caractères visibles absents, dédupliqués dans l'ordre ; ignore `\n`, `\r`, `\t`).
- `pub(crate) fn needs_unicode(text: &str) -> bool` : un caractère non WinAnsi
  (`content::win_ansi_byte`).

- [ ] Tests (RED) dans `fonts/unicode.rs` : `embedded_font_maps_cyrillic_greek_and_latin_ext`
  (`glyph('Ж')`, `glyph('Ω')`, `glyph('Ł')` sont `Some`), `cjk_is_missing`
  (`missing_glyphs("a你b好你") == ['你','好']`), `advance_of_space_matches_hmtx`
  (`advance(glyph(' '))` == 318 pour DejaVu Sans, 651/2048 arrondi),
  `needs_unicode_only_outside_win_ansi` (`"été €"` → false, `"Ł"` → true).
- [ ] Implémenter ; deps `ttf-parser = { version = "0.25", default-features = false, features = ["std"] }`.
- [ ] `cargo test -p mega-core fonts` vert ; commit `feat(core): embed DejaVu Sans with a glyph table` (raison des deps).

### Task 2: Écrivain de police Type0 (`lopdf`)

**Files:** Modify `crates/mega-core/src/fonts/unicode.rs`, `Cargo.toml` (`subsetter`).

**Interfaces — Produces:**
- `pub(crate) struct Type0Writer` : `new() -> Self` ; `encode(&mut self, text: &str) -> String`
  (chaîne hexa `<…>` de CID 2 octets ; un CID par caractère distinct, attribués 1, 2, … dans
  l'ordre d'apparition) ; `width(&self, text: &str, size: f32) -> f32` ;
  `write(self, doc: &mut lopdf::Document) -> CoreResult<lopdf::ObjectId>` (objet police
  `/Type0`, `/BaseFont /AAAAAA+DejaVuSans` — préfixe de sous-ensemble dérivé des CID,
  `/Encoding /Identity-H`, descendant `CIDFontType2` avec `/CIDSystemInfo (Adobe/Identity/0)`,
  `/W` par CID, `/DW` = avance du glyphe 0, `/CIDToGIDMap` flux (CID → nouveau GID du
  sous-ensemble, 0 si absent), `/FontDescriptor` (`/Flags 32`, `/FontBBox`, `/Ascent`,
  `/Descent`, `/CapHeight`, `/ItalicAngle 0`, `/StemV 80`, `/FontFile2` flux du sous-ensemble
  comprimé Flate) et `/ToUnicode` CMap `bfchar` (UTF-16BE, paires de substitution au-delà du BMP).
- Repli consigné si `subsetter` échoue : police entière, `CIDToGIDMap` vers les GID d'origine.

- [ ] Tests (RED) : `type0_font_round_trips_cyrillic` (squelette `testing::minimal_pdf`, page
  avec `BT /U1 24 Tf 50 700 Td <hex> Tj ET` écrit par `Type0Writer`, rechargé dans
  `Document::from_bytes` ; `page_text(0)` contient « Привет Ελλάδα Łódź ») ;
  `type0_subset_is_small` (police écrite < 60 Ko pour ces 17 caractères) ;
  `unmapped_char_keeps_its_unicode` (« 你 » encodé, texte extrait == « 你 ») ;
  `qpdf --check` manuel sur le fichier de test (noté dans le registre).
- [ ] Implémenter ; dep `subsetter = { version = "0.2", default-features = false }`.
- [ ] `cargo test -p mega-core fonts` ; commit `feat(core): write subset Type0 fonts with lopdf`.

### Task 3: Filigrane, numérotation et couche OCR en Unicode

**Files:** Modify `crates/mega-core/src/overlay.rs`, `crates/mega-core/tests/overlay.rs` (ou
le fichier de tests existant des filigranes), `crates/mega-core/src/ocr.rs` (doc des langues).

**Interfaces — Consumes:** Task 1-2. **Produces:** `check_text` n'échoue que sur
`missing_glyphs` (message : « caractères absents de la police : 你, 好 ») ; `Placement` porte un
`FontChoice { Helvetica, Unicode }` ; `apply_placements` crée au plus une police de chaque sorte
par passe (`MegaF1` Helvetica, `MegaU1` Type0) ; `add_invisible_words` écrit les mots non WinAnsi
en Type0 (`MegaU1`), les autres en Helvetica ; plus de `?` dans la couche OCR.

- [ ] Tests (RED) : `watermark_with_greek_is_searchable` (`find_text("Ελλάδα")` trouve 1
  résultat) ; `ascii_watermark_still_uses_helvetica` (aucun `/Type0` dans le fichier) ;
  `mixed_text_is_one_unicode_run` **RF1** (« Prix : 10 € — Цена » centré : boîte du texte
  extrait centrée à ±2 pt) ; `visible_cjk_text_is_refused_with_list` ;
  `page_numbers_in_cyrillic` (« Страница 1 ») ; `ocr_layer_keeps_cyrillic`
  (`add_invisible_words` « Привет » → `page_text` contient « Привет ») ;
  `ocr_layer_keeps_unmapped_glyphs_searchable` (« 你好 » → retrouvé).
- [ ] Implémenter ; `cargo test -p mega-core overlay` ; commit `feat(core): Unicode watermarks, numbering and OCR layer`.

### Task 4: Texte PDFium (insertion, retouche, texte libre, tampon) en Unicode

**Files:** Modify `crates/mega-core/src/content/mod.rs` (`FontFamily::Unicode`, choix
automatique), `annotations/{mod.rs,appearance.rs}`, `cleanup.rs` (fusion des polices
identiques), `app/src/lib/api.ts`, `app/src/lib/strings.ts`,
`app/src/lib/components/TextInsertDialog.svelte`.

**Interfaces — Produces:**
- `FontFamily::Unicode` (sérialisé `"unicode"`) ; `insert_text` avec Helvetica/Times/Courier et
  un texte non WinAnsi bascule **automatiquement** sur Unicode (gras/italique ignorés, noté dans
  la doc) ; refus seulement si `missing_glyphs` non vide.
- `Document::unicode_font_token_unlocked(&mut self) -> CoreResult<PdfFontToken>` : charge
  `DEJAVU_SANS` (`load_true_type_from_bytes(.., true)`), mis en cache dans `Document`
  (`Option<PdfFontToken>`), remis à `None` par `reload_from_bytes`.
- Annotations `Text`/`Stamp` : validation sur `missing_glyphs` ; `appearance::advance` mesure
  avec `UnicodeFont::text_width` si `needs_unicode`.
- `cleanup::dedupe_font_files(doc: &mut lopdf::Document) -> usize` appelé dans `before_save`
  avant `prune` : les flux `/FontFile2` d'octets identiques sont fusionnés (références réécrites).

- [ ] Tests (RED) : `insert_greek_text_is_extractable` ; `insert_text_auto_switches_to_unicode`
  (style Helvetica + « Ł » → OK) ; `free_text_annotation_in_cyrillic` (créée, `page_text` ou
  `annotations()` contents == « Привет », rendu non blanc dans le rect) ;
  `stamp_in_greek_fits_the_frame` ; `two_unicode_insertions_embed_one_font_after_save`
  (enregistrer : un seul `/FontFile2` DejaVu) ; Vitest strings fr/en déjà vérifiés.
- [ ] Implémenter, UI : option « Unicode (DejaVu Sans) » dans `TextInsertDialog`.
- [ ] `cargo test -p mega-core content annotations cleanup` ; `pnpm --dir app check` ; commit `feat: Unicode text insertion and annotations`.

### Task 5: Formulaires et licences

**Files:** `crates/mega-core/src/forms/write.rs` (test), `xtask/src/licenses.rs`,
`THIRD_PARTY_LICENSES.md`, `app/src/lib/licenses.json`.

- [ ] Test `cyrillic_field_value_is_utf16` (`/V` commence par `FE FF`, relu « Привет ») — s'il
  passe d'emblée, garde-fou consigné (B18 du Lot 9).
- [ ] `licenses.rs` : entrée « DejaVu Sans 2.37 — Bitstream Vera License + domaine public »
  (texte de `LICENSE-DejaVu.txt`), test `render_markdown_mentions_dejavu` (RED) ; `cargo xtask
  licenses`.
- [ ] Commit `docs: DejaVu licence in third-party notices`.

### Task 6: Fixture `image-mosaic.pdf` et caviardage partiel d'image

**Files:** `xtask/src/fixtures.rs`, `fixtures/image-mosaic.pdf`, `crates/mega-core/src/content/redact.rs`,
`crates/mega-core/tests/redact.rs` (ou tests existants du caviardage).

**Interfaces — Produces:** `RedactReport { texts, images_removed, images_redacted,
paths_removed, paths_kept, annotations }` (u16) ; `fn pixel_zone(matrix: PdfMatrix, zone: Rect,
w: u32, h: u32) -> Option<(u32,u32,u32,u32)>` (pure : inverse la matrice, projette les quatre
coins, boîte englobante arrondie vers l'extérieur + 1 px, bornée à l'image ; `None` si vide).

Fixture : page 0 = image 64×64 RGB (quadrants rouge, vert, bleu, jaune ; octets du quadrant
haut-gauche = motif répété `0xC0 0xFF 0xEE`) posée à `100 100 200 200` + image 32×32 avec SMask à
`350 100 100 100` ; page 1 = même XObject image ; page 2 tournée `/Rotate 90` avec la même image.

- [ ] Tests (RED) : `pixel_zone_projects_through_the_matrix` (unitaire, matrice
  `[200,0,0,200,100,100]`, zone `100,200,100,100` → quadrant haut-gauche `0,0,33,33`) ;
  `pixel_zone_handles_flipped_matrix` **RF2** ; `partial_image_redaction_blacks_only_the_zone`
  (rendu : centre du quadrant visé noir, centre des trois autres couleur d'origine) ;
  `partial_redaction_on_rotated_page` **RF2** ; `redacted_pixels_are_not_in_the_saved_file`
  (enregistrer, décompresser tous les flux `lopdf`, motif `C0FFEE…` absent) ;
  `shared_image_stays_intact_on_other_page` ; `smask_zone_becomes_opaque` (rendu sur fond :
  la zone est noire, pas transparente) ; adapter `redaction_removes_images_and_annotations`.
- [ ] Implémenter : image partiellement couverte → `get_raw_image()` → noircir (`RGBA` : noir,
  alpha 255) → `PdfPageImageObject::new` → `set_matrix` (matrice d'origine) →
  `insert_object_at_index(i, …)` après `remove_object_at_index(i)`.
- [ ] `cargo test -p mega-core redact` ; commit `feat(core): redact the covered part of an image`.

### Task 7: Tracés, vignettes et rapport dans l'interface

**Files:** `content/redact.rs`, `cleanup.rs` ou passe `lopdf` dans `redact`, `app/src/lib/api.ts`,
`strings.ts`, composant qui affiche le résultat du caviardage (`RedactDialog`/`+page.svelte`),
`crates/mega-cli` (affichage du rapport).

- [ ] Tests (RED) : `path_inside_zone_is_removed` ; `crossing_path_is_counted_kept` ;
  `thumbnail_and_piece_info_are_dropped` (page avec `/Thumb` et `/PieceInfo` écrits à la main) ;
  CLI `redact_reports_paths_kept`.
- [ ] Implémenter ; chaînes `redactDone` (compteurs) et `redactPathsKept` fr/en.
- [ ] Commit `feat: redact paths, drop page thumbnails, report kept paths`.

### Task 8: Session lopdf chiffrée (nettoyage et réécritures)

**Files:** Create `crates/mega-core/src/crypt.rs` ; Modify `cleanup.rs`, `overlay.rs`,
`content/redact.rs`, `pages.rs`, `document.rs`/`edit.rs` (appelants de `before_save`),
`mega-cli`.

**Interfaces — Produces:**
- `pub(crate) struct LopdfSession { pub doc: lopdf::Document, state: Option<EncryptionState> }` ;
  `LopdfSession::load(bytes: &[u8], password: Option<&str>) -> CoreResult<Self>` (déchiffre ;
  garde l'état ; filtre inconnu → `CoreError::InvalidOperation`) ;
  `finish(self) -> CoreResult<Vec<u8>>` (rechiffre avec le même état, même `/ID`, puis
  `save_to`).
- `pub fn before_save(bytes: &[u8], password: Option<&str>) -> CoreResult<Vec<u8>>` ;
  `rewrite_is_sane(pages_before, out, password)`.
- Levée des refus « PDF chiffré » du caviardage, du filigrane, de la numérotation et de la
  couche OCR (la signature reste refusée).

- [ ] Tests (RED) : `encrypted_document_is_cleaned_and_stays_encrypted` (`encrypted.pdf` +
  note supprimée : le popup disparaît, ouverture sans mot de passe → `PasswordRequired`, avec →
  OK) ; `owner_only_stays_protected` ; `aes256_cleanup_keeps_password_and_permissions` **RF3**
  (chiffrer via `Document::protect`, nettoyer, `/P` identique, aucune chaîne d'origine en clair) ;
  `watermark_on_encrypted_document` ; `redact_on_encrypted_document_drops_the_text`.
- [ ] Implémenter ; `qpdf --check --password=…` sur les fichiers produits (registre).
- [ ] Commit `feat(core): clean and rewrite encrypted documents, re-encrypting identically`.

### Task 9: Signature visible (cœur)

**Files:** `crates/mega-core/src/digital_signature/{pdf.rs,mod.rs}`, `service/mod.rs`
(invalidation du cache), `mega-cli` (`sign --visible page,x,y,w,h [--image]`).

**Interfaces — Produces:**
- `pub struct SignAppearance { pub page: u16, pub rect: Rect, pub image: Option<Vec<u8>>,
  pub show_text: bool }` ; `SignOptions.appearance: Option<SignAppearance>` (`#[serde(default)]`).
- `sign_to` : convertit `rect` (affiché → utilisateur) et lit la rotation de la page **avant**
  `sign_bytes` ; `sign_bytes(base, identity, options, now, placement: Option<Placement>)` où
  `Placement { page_index, user_rect, rotate, w, h }`.
- Apparence : Form XObject `/BBox [0 0 w h]` (dimensions affichées), `/Matrix` de rotation
  ramenant le cadre droit, image à gauche (carré de hauteur `h`, `image` crate décode puis
  XObject Flate RGB + SMask), texte à droite : « Signé numériquement par <CN> », date
  `JJ/MM/AAAA HH:MM UTC`, motif ; Helvetica, ou `Type0Writer` si non WinAnsi.

- [ ] Tests (RED) : `visible_signature_verifies_intact` ;
  `visible_signature_widget_has_appearance_and_page_annots` (widget `/Rect` ≠ 0, `/AP /N` non
  vide, page visée référence le widget, `/Annots` inline et absent **RF4**) ;
  `visible_signature_on_rotated_page_is_upright` (rendu PDFium : pixels sombres dans le rect
  affiché) ; `signer_name_non_latin_uses_type0` **RF4** (identité de test avec CN cyrillique
  générée en test, ou nom forcé) ; `pyhanko sign validate` manuel (registre).
- [ ] Commit `feat(core): visible digital signatures`.

### Task 10: Signature visible (Tauri et interface)

**Files:** `app/src-tauri/src/commands.rs`, `app/src/lib/api.ts`, `strings.ts`,
`components/SignDialog.svelte`, `AnnotationLayer.svelte` (rectangle de l'outil signature).

- [ ] Test Rust `sign_options_deserialize_appearance` (RED) ; implémentation ; case « Signature
  visible » + image facultative ; chaînes fr/en.
- [ ] Commit `feat(app): visible signature option`.

### Task 11: OCR = un seul pas d'annulation

**Files:** `crates/mega-core/src/edit.rs`, `service/mod.rs`, `app/src-tauri/src/commands.rs`,
`app/src/lib/api.ts`, la boucle OCR du frontend (`OcrDialog`/`documents.svelte.ts`).

**Interfaces — Produces:** `Document::begin_edit_group(&mut self, label: String)`,
`Document::end_edit_group(&mut self)` ; pendant un groupe, la première modification pousse un
seul instantané (libellé du groupe), les suivantes n'en poussent aucun ; `undo`/`redo` refusés
groupe ouvert (`CoreError::InvalidOperation("opération en cours")`) ; `HistoryState.group_open`.
Frontend : `try { begin; boucle } finally { end }`.

- [ ] Tests (RED) : `group_of_three_edits_undoes_in_one_step` ; `undo_refused_while_group_open` ;
  `empty_group_adds_no_step` ; `stopped_group_still_closes` **RF5** (Vitest sur la boucle si
  elle est extraite en `logic/`, sinon test Rust de `end` sans édition).
- [ ] Commit `feat: OCR of several pages is one undo step`.

### Task 12: Dialogues de `+page.svelte` et `MenuButton`

**Files:** Create `app/src/lib/logic/dialogs.ts` (+ `.test.ts`),
`app/src/lib/components/MenuButton.svelte` ; Modify `+page.svelte`, `FileMenu.svelte`,
`ContentToolbar.svelte`.

**Interfaces — Produces:** `type Dialog = 'open' | 'password' | … ` (union des dialogues
existants) ; `NO_DOC: ReadonlySet<Dialog>` (dialogues permis sans document) ;
`canOpen(dialog, hasDoc): boolean` ; `escapeTarget(state): 'dialog' | 'search' | 'menu' | null`.

- [ ] Vitest (RED) : un seul dialogue ouvert à la fois, `NO_DOC`, ordre Échap.
- [ ] Refonte ; scénarios CDP existants verts ; commit `refactor(app): single dialog state and shared MenuButton`.

### Task 13: Mineurs reportés du Lot 9

Un test par point (RED), ou ruling consigné si sans effet :
XMP `x-default` et guillemets simples ; PDFDocEncoding 0x7F/0xAD ; `parse_pdf_date` accepte les
dates partielles (`D:2024`, `D:202403`) ; CAdES (SHA-384/512 dans `signed_attrs`, doublon
`signing-certificate-v2` refusé) ; case à cocher `/DV` sans `/V` ni `/AS` ; overlay `rect`
fini ; `header_minor` cherche `%PDF-` dans les 1024 premiers octets ; membres `/RT /Group`
supprimés avec le parent ; `mega meta -o même.pdf` cohérent ; `/Matrix`→`BBox` (§12.5.5)
pour `/InkList` ; I3 (repli `before_save` élagué) ; I6 (`/NM` et `/IRT` conservés à la
recréation) ; WinAnsi qui bloque le déplacement d'un FreeText tiers (levé par Task 4).
- [ ] Commits `fix: …` par groupe.

### Task 14: Scénarios CDP, documentation, version

- [ ] Scénarios `scripts/scenarios/{unicode-texte,caviardage-image,signature-visible,dialogues}.json` ;
  `run-all-scenarios.ps1` : 22/22.
- [ ] ADR-0014 ; CLAUDE.md (pièges Lot 10) ; ROADMAP (Lot 10 ✅) ; `docs/FONCTIONNALITES.md` ;
  aide intégrée si elle liste les fonctions ; versions 0.10.0 (`Cargo.toml` ×4,
  `tauri.conf.json`, `package.json`).
- [ ] `/note-apprentissage` (lot-10.md), `/verif` 8/8.
