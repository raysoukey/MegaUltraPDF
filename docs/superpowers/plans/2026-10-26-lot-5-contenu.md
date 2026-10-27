# Lot 5 — Édition de contenu : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Éditer le contenu des pages (texte, images, objets, caviardage, filigrane, numérotation,
recadrage) par des `EditCommand`, et solder les dettes `lopdf` des Lots 3-4 (`/InkList`, `/AP`
des notes, élagage, pages tournées).

**Architecture:** `mega-core::content` (objets de page via pdfium-render : lecture, insertion,
déplacement, suppression, texte ; caviardage par objets), `mega-core::overlay` (filigrane,
numérotation par flux `lopdf`), `mega-core::cleanup` (passe `lopdf` à l'enregistrement),
`geometry` étendu aux pages tournées ; frontend : outils « Contenu », dialogues.

**Tech Stack:** pdfium-render 0.9.4 (sûr), lopdf 0.46, Svelte 5, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-26-lot-5-contenu.md`

## Global Constraints

- `#![forbid(unsafe_code)]` ; toute édition = `EditCommand` ; écriture atomique ; aucune
  logique métier en TS ; chaînes dans `strings.ts` ; SPDX ; `///` ; `[Apprentissage]`.
- Toutes les coordonnées exposées par l'API (annotations, champs, objets, commandes) sont
  dans l'espace de la **page affichée** (après `/Rotate`) ; la conversion vers l'espace
  utilisateur se fait en Rust (`geometry::to_user_space` / `from_user_space`).
- Texte inséré : polices Standard 14 (WinAnsi) ; un caractère hors WinAnsi → `InvalidOperation`
  explicite (pas de police embarquée dans ce lot).
- Le caviardage **supprime** les objets texte et image dont l'emprise touche la zone
  (sur-caviardage sûr) ; les chemins restent ; un rectangle noir est posé ; les annotations
  de la zone sont supprimées.

## Review Focus

1. Page `/Rotate 90` : une annotation posée à l'écran est relue au même endroit et rendue au
   bon endroit (test `annotations_on_rotated_page_round_trip`, Task 2).
2. Caviardage d'un mot au milieu d'une ligne : l'extraction de texte ne contient plus **aucun**
   caractère de l'objet touché (test `redaction_removes_text_from_extraction`, Task 5).
3. Enregistrement après N rendus : moins d'objets, pas de `/PDFIUM_HasGeneratedAP`, `/InkList`
   présent sur nos formes (test `save_cleanup`, Task 1).
4. Suppression d'un objet dont l'index a changé (document modifié entre-temps) : erreur typée,
   document inchangé (test Task 3).
5. Filigrane sur un document sans `/Resources` de page : la police est ajoutée, le rendu montre
   le texte (test `watermark_on_bare_page`, Task 6).

---

### Task 1 — Passe `lopdf` à l'enregistrement : `/InkList`, `/AP` des notes, élagage

**Files:** Create `crates/mega-core/src/cleanup.rs`, `tests/cleanup.rs` ; Modify `edit.rs`
(`save_as` : `cleanup::before_save(bytes) -> Vec<u8>` avant `write_atomically`), `lib.rs`.

**Produces:** `pub fn before_save(bytes: &[u8]) -> CoreResult<Vec<u8>>` : pour chaque
annotation `/Subtype /Text` portant `/PDFIUM_HasGeneratedAP` → retirer `/AP` et la clé ; pour
chaque `/Subtype /Ink` sans `/InkList` dont l'`/AP /N` est un flux : `/InkList` déduit des
opérateurs `m`/`l`/`c` du flux (polylignes ; Bézier : points de contrôle extrêmes) ;
`prune_objects` + `renumber_objects` ; documents chiffrés : octets inchangés.

- [ ] Tests (RED) : `save_cleanup_strips_note_ap_and_adds_inklist` (note + rectangle + 5 rendus
  → `save_as` → octets : pas de `PDFIUM_HasGeneratedAP`, `/InkList` présent, nombre d'`obj`
  inférieur au `to_bytes` brut ; réouverture : 2 annotations, kinds conservés, rendu du
  rectangle identique) ; `cleanup_keeps_encrypted_bytes`.
- [ ] Implémenter ; verts ; commit `feat(core): lopdf cleanup pass on save`.

### Task 2 — Pages tournées : espace affiché ↔ espace utilisateur

**Files:** Modify `geometry.rs` (`pub fn to_user_space(r: Rect, rotate: u16, media: PageSize) -> Rect`,
`from_user_space`, idem points), `document.rs` (`page_rotate(page) -> u16`, `page_size` reste
la taille affichée), `annotations/mod.rs` (conversion à l'écriture et à la lecture : rect,
quads, géométrie), `forms/mod.rs` (rects des champs), `forms/signature.rs`, `testing.rs`
(`rotated_pdf()` : `/Rotate 90`, texte « Page 1 »), `xtask` (`fixtures/rotated.pdf`), tests.

- [ ] Tests (RED) : `annotations_on_rotated_page_round_trip` (rect posé en coordonnées affichées
  → relu identique ; octets : `/Rect` différent ; pixel rendu à l'endroit attendu),
  `form_fields_on_rotated_page` (fixture tournée avec un widget), `signature_on_rotated_page`,
  `geometry::unrotate` unitaires (0/90/180/270).
- [ ] Implémenter ; verts ; commit `feat(core): rotated pages share one displayed coordinate space`.

### Task 3 — Objets de page : lecture, suppression, déplacement, texte

**Files:** Create `crates/mega-core/src/content/mod.rs`, `tests/content.rs` ; Modify `edit.rs`,
`service/mod.rs` (`Request::PageObjects`), `lib.rs`.

**Produces:**
```rust
#[serde(rename_all = "snake_case")] pub enum ObjectKind { Text, Image, Path, Form, Shading, Other }
pub struct PageObject { pub index: u16, pub kind: ObjectKind, pub bounds: Rect, pub text: Option<String>, pub font_size: Option<f32> }
impl Document { pub fn page_objects(&self, page: u16) -> CoreResult<Vec<PageObject>>; pub fn delete_object(&mut self, page: u16, index: u16) -> CoreResult<()>; pub fn move_object(&mut self, page: u16, index: u16, dx: f32, dy: f32) -> CoreResult<()>; pub fn set_object_text(&mut self, page: u16, index: u16, text: &str) -> CoreResult<()>; }
EditCommand::{DeleteObject { page, index }, MoveObject { page, index, dx, dy }, SetObjectText { page, index, text }}
```
Bornes : 5000 objets lus par page ; `set_object_text` refuse un texte vide ou > 4096 ; après
chaque modification, `regenerate_content`.

- [ ] Tests (RED) : `reads_page_objects` (`sample_pdf` : 1 objet texte « Page 1 », bounds
  plausibles), `delete_object_changes_rendering_and_undo`, `move_object_shifts_bounds`,
  `set_object_text_round_trips` (extraction contient le nouveau texte), `unknown_object_index_is_refused`.
- [ ] Implémenter ; verts ; commit `feat(core): page objects read, delete, move, retext`.

### Task 4 — Insérer du texte et des images

**Files:** Modify `content/mod.rs`, `edit.rs`, tests.

**Produces:**
```rust
#[serde(rename_all = "snake_case")] pub enum FontFamily { Helvetica, Times, Courier }
pub struct TextStyle { pub family: FontFamily, pub bold: bool, pub italic: bool, pub size: f32, pub color: Color }
impl Document { pub fn insert_text(&mut self, page: u16, at: [f32; 2], text: &str, style: &TextStyle) -> CoreResult<()>; pub fn insert_image(&mut self, page: u16, rect: Rect, path: &str) -> CoreResult<()>; }
EditCommand::{InsertText { page, at, text, style }, InsertImage { page, rect, path }}
```
Multi-ligne : une ligne par `\n`, interligne 1,2 × taille. Caractère hors WinAnsi →
`InvalidOperation("caractère non pris en charge : …")`. Image : réutilise `signature::fit`
et les bornes du Lot 4 (extraire dans `content/image.rs` partagé).

- [ ] Tests (RED) : `insert_text_is_extractable_and_undoable`, `insert_text_rejects_non_winansi`,
  `insert_image_fits_rect`, `insert_text_style_bold_italic` (police relue `Helvetica-BoldOblique`).
- [ ] Implémenter ; verts ; commit `feat(core): insert text and images`.

### Task 5 — Caviardage réel

**Files:** Create `content/redact.rs` ; Modify `edit.rs`, tests.

`Document::redact(page, rect) -> CoreResult<RedactReport { texts: u16, images: u16, annotations: u16 }>` :
supprime les objets texte dont l'emprise **intersecte** `rect`, les objets image dont
l'emprise intersecte, les annotations dont le `/Rect` intersecte ; ajoute un rectangle noir
(chemin rempli) ; `regenerate_content`. `EditCommand::Redact { page, rect }`.

- [ ] Tests (RED) : `redaction_removes_text_from_extraction` (fixture `rich_pdf()` : deux
  lignes « Secret 123 » et « Public » ; caviarder la première → extraction sans « Secret » et
  sans « 123 », avec « Public » ; pixels noirs dans la zone ; réouverture identique),
  `redaction_removes_images_and_annotations`, `redaction_outside_page_is_refused`.
- [ ] Implémenter ; verts ; commit `feat(core): real redaction at object granularity`.

### Task 6 — Filigrane, numérotation, recadrage

**Files:** Create `crates/mega-core/src/overlay.rs`, `tests/overlay.rs` ; Modify `edit.rs`, `pages.rs` ou `content` (crop), tests.

**Produces:**
```rust
pub struct Watermark { pub text: String, pub size: f32, pub opacity: f32, pub angle_deg: f32, pub color: Color, pub pages: Option<Vec<u16>> }
#[serde(rename_all = "snake_case")] pub enum Corner { TopLeft, TopCenter, TopRight, BottomLeft, BottomCenter, BottomRight }
pub struct PageNumbering { pub template: String /* {n} {total} {titre} */, pub corner: Corner, pub margin: f32, pub size: f32, pub start: u16, pub pages: Option<Vec<u16>> }
pub struct CropSpec { pub pages: Option<Vec<u16>>, pub margins: [f32; 4] /* gauche, bas, droite, haut */ }
impl Document { pub fn add_watermark(&mut self, w: &Watermark) -> CoreResult<()>; pub fn add_page_numbers(&mut self, n: &PageNumbering) -> CoreResult<()>; pub fn crop_pages(&mut self, c: &CropSpec) -> CoreResult<()>; }
EditCommand::{Watermark { spec }, PageNumbers { spec }, CropPages { spec }}
```
Filigrane et numéros : `lopdf` (`add_page_contents` d'un flux `q /GSmega gs BT /Fmega size Tf …
ET Q`, police Helvetica ajoutée aux `/Resources /Font` de chaque page si absente, `ExtGState`
pour l'opacité), puis rechargement. Recadrage : `boundaries_mut().set_crop` (PDFium), marges
en points depuis la MediaBox ; marges nulles = réinitialiser.

- [ ] Tests (RED) : `watermark_is_extractable_on_every_page`, `watermark_on_bare_page`
  (page sans `/Resources`), `page_numbers_follow_template`, `crop_changes_page_size_and_resets`.
- [ ] Implémenter ; verts ; commit `feat(core): watermark, page numbers, crop`.

### Task 7 — Acteur, Tauri, `api.ts`, CLI

- Requêtes `PageObjects { id, page }` ; Tauri `page_objects` ; `api.ts` : `PageObject`,
  `ObjectKind`, `TextStyle`, `Watermark`, `PageNumbering`, `CropSpec`, `RedactReport`,
  `EditCommand` étendu ; CLI `mega watermark <f> --text --opacity -o`, `mega number <f>
  --template --corner -o`, `mega crop <f> --margins l,b,r,t -o`, `mega redact <f> --page --rect
  x,y,w,h -o`, `mega objects <f> --page`.
- [ ] Tests CLI (RED) sur `sample_pdf`/`rich_pdf` ; verts ; `cargo check -p mega-app` ;
  `pnpm check` ; commit `feat(cli,app): content commands and IPC`.

### Task 8 — Frontend

**Files:** Create `components/ContentToolbar.svelte`, `ObjectLayer.svelte`, `WatermarkDialog.svelte`,
`NumberingDialog.svelte`, `CropDialog.svelte`, `TextInsertDialog.svelte` ; Modify `logic/tools.ts`
(outils `objects`, `text-content`, `image`, `redact`), `state/documents.svelte.ts`
(`pageObjects` cache, `selectedObject`, actions), `PageView.svelte`, `Toolbar.svelte` (menu
« Contenu »), `+page.svelte`, `strings.ts`, `shortcuts` (aucun nouveau), tests Vitest de la logique pure.

Comportement : outil Objets → survol surligne (hit test sur `bounds`), clic sélectionne,
glisser déplace (`MoveObject` au relâchement), Suppr supprime, double-clic sur un texte →
saisie (`SetObjectText`) ; outil Texte-contenu → clic → dialogue (texte, police, taille,
couleur) → `InsertText` ; outil Image → tracé d'un rectangle → fichier → `InsertImage` ;
outil Caviarder → tracé → confirmation → `Redact` ; dialogues filigrane / numérotation /
recadrage depuis le menu « Contenu » de la barre principale.

- [ ] Implémenter ; `pnpm check/lint/test/format` ; commit `feat(app): content editing tools and dialogs`.

### Task 9 — Vérification, documentation, relecture

- Scénario CDP `contenu.json` (checklist de la spec) + `rotation.json` (fixture tournée :
  annotation posée et relue) ; `mega objects`/`mega text` sur la copie.
- ADR-0010 (contenu, caviardage par objets, filigranes `lopdf`, espace affiché), ADR-0003 mis à
  jour (double moteur effectif), `docs/apprentissage/lot-5.md`, `/concept matrices-et-transformations`,
  `/concept flux-de-contenu-pdf`, FONCTIONNALITES / ROADMAP / README / CLAUDE.md.
- `/verif`, relecture Fable + `pdf-expert`, passe de correction, PR, CI, fusion.
