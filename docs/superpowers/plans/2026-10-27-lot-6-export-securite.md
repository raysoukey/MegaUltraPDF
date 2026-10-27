# Lot 6 — Export, impression, sécurité : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Exporter (images, texte, impression), décrire (métadonnées), protéger / déprotéger, optimiser, gérer les pièces jointes et comparer deux versions, en CLI et dans l'application.

**Architecture:** Un module `mega-core` par sujet (`export`, `security`, `optimize`, `attachments`, `compare`, métadonnées dans `properties`), chacun soit PDFium (rendu, texte, pièces jointes, comparaison) soit `lopdf` (métadonnées, chiffrement, optimisation) selon ADR-0003 ; chaque décision sur le document est une `EditCommand` ; acteur et Tauri restent des passe-plats ; l'interface ajoute un menu « Fichier », des dialogues et une vue d'impression.

**Tech Stack:** Rust (pdfium-render 0.9.4, lopdf 0.46 avec `encrypt`/`decrypt`, image 0.25, `rand` déjà présent via lopdf), Tauri 2, Svelte 5, Vitest.

**Spec:** docs/superpowers/specs/2026-10-27-lot-6-export-securite.md

## Global Constraints

- CLAUDE.md : frontières des crates, docs `///` en français sur tout `pub`, `[Apprentissage]`,
  pas d'`unwrap`/`expect`/`panic` hors tests, `write_atomically` pour toute écriture, toute
  édition par `EditCommand`/`apply`, chaînes dans `strings.ts`, aucun appel réseau.
- ADR-0010 §7 : une passe `lopdf` s'abstient sur un fichier chiffré **ou déchiffré en silence**
  (`is_encrypted() || was_encrypted()`), sauf `protect`/`unprotect` qui en sont l'objet ;
  réécriture validée (`rewrite_is_sane`) avant de remplacer les octets PDFium.
- Bornes de la spec : DPI 36–600 et ≤ 50 Mpx par image rendue ; qualité JPEG 10–100 ;
  `max_image_px` 256–8192 ; pièce jointe ≤ 100 Mo ; mots de passe ≤ 127 octets, ASCII / Latin-1.
- Nouvelle dépendance directe autorisée : `rand` (déjà dans l'arbre via lopdf) pour la clé de
  fichier AES-256 ; motif écrit dans le commit.

## Review Focus

1. Protéger puis enregistrer : le fichier exige le mot de passe à la réouverture, les
   permissions sont celles choisies, le document reste ouvert et navigable (Task 3).
2. Document protégé par mot de passe propriétaire seul : `unprotect` sans mot de passe refuse
   avec `WrongPassword` ; avec le mot de passe propriétaire, le fichier enregistré n'a plus
   d'`/Encrypt` (Task 3).
3. Optimisation d'un PDF sans image et déjà compressé : rapport « rien à gagner », octets
   identiques au rendu près, jamais plus gros (Task 4).
4. Export d'une plage dans un dossier contenant déjà `<préfixe>-1.png` : refus sans `--force`,
   aucun fichier écrasé (Task 1).
5. Comparaison de pages de tailles différentes : pas de panique, différence calculée sur
   l'union, compteur cohérent (Task 6).

---

### Task 1 — Export d'images et de texte

**Files:** Create `crates/mega-core/src/export.rs`, `crates/mega-core/tests/export.rs` ; Modify `lib.rs`, `text.rs` (`page_text`).

**Produces:**
```rust
#[serde(tag = "kind", rename_all = "snake_case")] pub enum ImageFormat { Png, Jpeg { quality: u8 } }
pub struct ImageExport { pub pages: Option<Vec<u16>>, pub format: ImageFormat, pub dpi: u16, pub with_annotations: bool }
pub const MAX_EXPORT_PIXELS: u64 = 50_000_000;
impl Document {
    pub fn export_images(&self, spec: &ImageExport, dir: &Path, prefix: &str, force: bool) -> CoreResult<Vec<PathBuf>>;
    pub fn page_text(&self, page: u16) -> CoreResult<String>;
    pub fn export_text(&self, pages: Option<&[u16]>, separators: bool) -> CoreResult<String>;
}
```
Échelle = dpi / 72 ; nom `<prefix>-<N>.<ext>` (N 1-based) ; `write_atomically` ; dossier créé
si absent ; sans `force`, un fichier existant → `InvalidOperation` **avant** d'écrire quoi que
ce soit ; extension `jpg`. Texte : `page.text().all()`, séparateur `\n===== Page N =====\n`.

- [ ] Tests (RED) : `export_images_writes_one_png_per_page` (3 pages, 72 dpi → 3 fichiers
  PNG de 595 × 842 décodables), `export_images_jpeg_quality_and_dpi` (150 dpi → 1240 × 1754,
  JPEG décodable, qualité 30 plus petit que 90), `export_refuses_overwrite_without_force`
  (Review Focus 4), `export_rejects_bad_dpi_and_pixel_budget` (dpi 20, dpi 700, 600 dpi sur
  une page 2000 × 2000 pt → `InvalidOperation`), `page_text_and_export_text`
  (`sample_pdf(2)` → « Page 1 », séparateurs présents / absents, plage `[1]`).
- [ ] Implémenter ; verts ; commit `feat(core): export pages as images and text`.

### Task 2 — Métadonnées modifiables

**Files:** Modify `crates/mega-core/src/properties.rs`, `edit.rs`, `tests/document.rs` (ou nouveau `tests/metadata.rs`).

**Produces:**
```rust
#[derive(Default)] pub struct MetadataPatch { pub title: Option<String>, pub author: Option<String>, pub subject: Option<String>, pub keywords: Option<String> }
impl Document { pub fn set_metadata(&mut self, patch: &MetadataPatch) -> CoreResult<()>; }
EditCommand::SetMetadata { patch: MetadataPatch }   // libellé « Modifier les métadonnées »
```
`lopdf` : `/Info` créé si absent (référencé par le trailer), chaînes via `forms::write::pdf_text`
(rendu `pub(crate)`), chaîne vide = clé retirée, `/ModDate` = maintenant (`D:YYYYMMDDHHmmSS+00'00'`),
≤ 1024 caractères par champ, refus si chiffré (`is_encrypted() || was_encrypted()`), puis
`reload_from_bytes`.

- [ ] Tests (RED) : `set_metadata_round_trips_unicode` (titre « Résumé — été », auteur
  « Zoë » relus par `properties()` et après réouverture), `set_metadata_empty_removes_key`,
  `set_metadata_is_undoable_and_updates_mod_date`, `set_metadata_refused_on_encrypted`
  (`owner-only.pdf`).
- [ ] Implémenter ; verts ; commit `feat(core): editable document metadata`.

### Task 3 — Protection et retrait du mot de passe

**Files:** Create `crates/mega-core/src/security.rs`, `tests/security.rs` ; Modify `edit.rs`, `document.rs` (`set_password`), `lib.rs`, `Cargo.toml` (`rand`).

**Produces:**
```rust
pub struct Permissions { pub print: bool, pub copy: bool, pub modify: bool, pub annotate: bool }
pub struct Protection { pub user_password: String, pub owner_password: String, pub permissions: Permissions }
pub struct SecurityInfo { pub encrypted: bool, pub user_password_required: bool, pub permissions: Permissions }
impl Document {
    pub fn security(&self) -> CoreResult<SecurityInfo>;   // PDFium : security_handler_revision, permissions() ; user_password_required = self.password.is_some()
    pub fn protect(&mut self, p: &Protection) -> CoreResult<()>;
    pub fn unprotect(&mut self) -> CoreResult<()>;
}
EditCommand::Protect { protection: Protection }  // « Protéger le document »
EditCommand::Unprotect                            // « Retirer la protection »
```
`protect` : refus si déjà chiffré ; mots de passe validés (≤ 127 octets, chars ≤ U+00FF, propriétaire
vide → = utilisateur) ; `lopdf` `encrypt(EncryptionState::try_from(EncryptionVersion::V5 { … }))`
avec clé de 32 octets `rand`, filtres `StdCF` AES-V3 pour flux et chaînes, `Permissions` de lopdf
construites depuis les nôtres ; `save_to` ; `reload_from_bytes` après `self.password = Some(user)`
(ou `None` si utilisateur vide) ; l'historique garde des instantanés **chiffrés** : `undo`
recharge avec le mot de passe courant (vérifier) ; `unprotect` : `load_mem_with_password(bytes,
password connu)` ; si le document n'a pas de mot de passe connu (propriétaire seul) →
`WrongPassword` ; `decrypt` → `save_to` → `password = None` → reload.

- [ ] Tests (RED) : `protect_then_reopen_requires_password_and_keeps_permissions` (Review
  Focus 1 : `Document::open(None)` → `PasswordRequired`, avec « secret » OK, `security()` dit
  `print: false`), `protect_refuses_bad_passwords` (vide des deux côtés, 200 octets, « 🙂 »),
  `protect_is_undoable` (undo → `security().encrypted == false`), `unprotect_owner_only_needs_password`
  (Review Focus 2 : `owner-only.pdf` ouvert sans mot de passe → `WrongPassword` ; ouvert avec
  « secret » → OK, enregistré sans `/Encrypt`), `unprotect_user_password_document`
  (`encrypted.pdf` + « secret »), `edits_after_protect_are_refused_by_lopdf_passes` (filigrane →
  `InvalidOperation`).
- [ ] Implémenter ; verts ; commit `feat(core): AES-256 protection and password removal` (motif
  de `rand` dans le message).

### Task 4 — Optimisation

**Files:** Create `crates/mega-core/src/optimize.rs`, `tests/optimize.rs` ; Modify `testing.rs` (`heavy_images_pdf`), `edit.rs`, `lib.rs`.

**Produces:**
```rust
pub struct OptimizeOptions { pub image_quality: Option<u8>, pub max_image_px: Option<u32>, pub compress_streams: bool }
pub struct OptimizeReport { pub bytes_before: u64, pub bytes_after: u64, pub images_recompressed: u16, pub objects_removed: u32 }
impl Document { pub fn optimize(&mut self, opts: &OptimizeOptions) -> CoreResult<OptimizeReport>; }
EditCommand::Optimize { options: OptimizeOptions }  // « Optimiser »
```
`lopdf` : refus si chiffré ; `objects_removed` = objets avant − après `prune_objects` ;
`compress_streams` : `Stream::compress()` sur les flux sans `/Filter` (hors images) ; images :
pour chaque objet `/Subtype /Image` éligible (DeviceRGB ou DeviceGray, BPC 8, pas de `/SMask`,
`/Mask`, `/Decode`, `/ImageMask`, filtre ∅ / Flate / DCT) : décoder (brut ou JPEG via `image`),
réduire si `max(w, h) > max_image_px` (`resize` Triangle), encoder JPEG à `image_quality`, et ne
**remplacer** que si plus petit (`/Filter /DCTDecode`, `/Width`, `/Height`, `/Length`, suppression
de `/DecodeParms`) ; `renumber_objects` ; `rewrite_is_sane` ; si résultat ≥ octets d'entrée →
rapport inchangé (`bytes_after == bytes_before`) et document intact ; sinon `reload_from_bytes`.

- [ ] Tests (RED) : `optimize_recompresses_flate_rgb_image` (`heavy_images_pdf` : octets après <
  60 % avant, `images_recompressed == 1`, rendu encore proche : pixel central de l'image de la
  couleur attendue ± 12), `optimize_downsamples_large_image` (`max_image_px: 256` → `/Width`
  256 relu par lopdf), `optimize_never_grows_and_reports_nothing_to_gain` (Review Focus 3 sur
  `sample_pdf` : `bytes_after == bytes_before`, pas d'édition enregistrée dans l'historique si
  rien ne change → `apply` renvoie OK mais `is_modified()` vrai : documenter ; choix : la
  commande applique quand même), `optimize_skips_masked_or_cmyk_images` (fixture avec `/SMask` :
  image intacte), `optimize_is_refused_on_encrypted`.
- [ ] Implémenter ; verts ; commit `feat(core): optimize (prune, compress, recompress images)`.

### Task 5 — Pièces jointes

**Files:** Create `crates/mega-core/src/attachments.rs`, `tests/attachments.rs` ; Modify `testing.rs` (`attachments_pdf`), `xtask` (`fixtures/attachments.pdf`), `edit.rs`, `lib.rs`.

**Produces:**
```rust
pub struct Attachment { pub index: u16, pub name: String, pub size: u64 }
pub const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;
impl Document {
    pub fn attachments(&self) -> CoreResult<Vec<Attachment>>;
    pub fn save_attachment(&self, index: u16, path: &Path) -> CoreResult<()>;
    pub fn add_attachment(&mut self, path: &Path) -> CoreResult<()>;
    pub fn delete_attachment(&mut self, index: u16) -> CoreResult<()>;
}
EditCommand::AddAttachment { path: String }      // « Joindre : <nom> »
EditCommand::DeleteAttachment { index: u16 }     // « Retirer la pièce jointe : <nom> » (label_in)
```
PDFium `attachments()` ; nom = nom de fichier (`display_safe` côté CLI) ; `save_attachment` via
`write_atomically` ; `add_attachment` : taille bornée, nom dédoublonné `nom (2).ext`.

- [ ] Tests (RED) : `lists_embedded_files` (`attachments_pdf` : 2 fichiers, noms et tailles),
  `save_attachment_writes_exact_bytes`, `add_and_delete_attachment_are_undoable`,
  `attachment_too_big_is_refused` (fichier creux de 101 Mo : `set_len`), `unknown_attachment_index`.
- [ ] Implémenter ; verts ; commit `feat(core): list, extract, add and delete attachments`.

### Task 6 — Comparaison de pages

**Files:** Create `crates/mega-core/src/compare.rs`, `tests/compare.rs` ; Modify `lib.rs`.

**Produces:**
```rust
pub struct PageDiff { pub image: RenderedPage, pub differing_pixels: u64, pub total_pixels: u64 }
pub fn compare_pages(a: &Document, page_a: u16, b: &Document, page_b: u16, scale: f32) -> CoreResult<PageDiff>;
```
Rendu des deux pages (annotations comprises) à `scale` (bornée : image ≤ 50 Mpx) ; image de
sortie = union des tailles, fond blanc ; pixel identique → gris clair (mélange 70 % blanc) ;
pixel différent (écart max sur un canal > 24) → rouge opaque ; pixel hors de l'une des deux pages
→ différent.

- [ ] Tests (RED) : `identical_pages_have_no_difference`, `modified_page_is_flagged`
  (`sample_pdf(1)` vs la même avec un texte inséré : `differing_pixels > 0`, pixels rouges là où
  le texte est), `different_sizes_do_not_panic` (Review Focus 5 : A4 vs `blank_pdf(200, 200)`),
  `compare_rejects_scale_over_budget`.
- [ ] Implémenter ; verts ; commit `feat(core): visual page comparison`.

### Task 7 — Acteur, Tauri, `api.ts`, CLI

- Requêtes `PageText`, `ExportImages`, `ExportText` (écrit le fichier : chemin fourni),
  `Security`, `Attachments`, `SaveAttachment`, `ComparePages { id_a, page_a, id_b, page_b, scale }`
  (répond `Vec<u8>` PNG) ; Tauri : commandes homonymes (`compare_pages` renvoie
  `tauri::ipc::Response`), `pick_any_file`, `pick_text_save_path` ; `api.ts` : types de la spec +
  `EditCommand` étendu + fonctions ; CLI : `mega export-images <f> -o <dir> [--pages] [--dpi] [--jpeg
  --quality] [--prefix] [--force]`, `mega export-text <f> -o <txt> [--pages] [--no-separators]`,
  `mega meta <f> [--title --author --subject --keywords -o]`, `mega protect <f> --user --owner
  [--no-print --no-copy --no-modify --no-annotate] -o`, `mega unprotect <f> --password -o`,
  `mega optimize <f> [--quality --max-px --no-compress] -o`, `mega attachments <f> [--extract <idx>
  -o]`, `mega compare <a> <b> [--page --scale] -o diff.png`.
- [ ] Tests CLI (RED) sur `simple.pdf`, `encrypted.pdf`, `owner-only.pdf`, `attachments.pdf` ;
  verts ; `cargo check -p mega-app` ; `pnpm check` ; commit `feat(cli,app): export, security,
  optimize, attachments, compare commands and IPC`.

### Task 8 — Frontend

**Files:** Create `components/FileMenu.svelte`, `ExportDialog.svelte`, `PrintView.svelte`,
`ProtectDialog.svelte`, `OptimizeDialog.svelte`, `AttachmentsPanel.svelte`, `CompareDialog.svelte` ;
Modify `Toolbar.svelte` (menu Fichier), `PropertiesDialog.svelte` (champs modifiables +
sécurité), `Sidebar.svelte` (onglet Pièces jointes), `state/documents.svelte.ts`
(`security`, `attachments` caches ; actions), `logic/export.ts` (noms de fichiers, bornes,
portée → pages ; tests Vitest), `logic/shortcuts.ts` (`print` : Ctrl+P), `+page.svelte`,
`strings.ts`.

Comportement : Fichier ▾ (Ouvrir, récents, Exporter…, Imprimer…, Propriétés…, Protéger…,
Optimiser…, Comparer…) ; `PrintView` : overlay plein écran, images `mega://` à 150/300 dpi
(`priority: 'visible'`, génération courante), `@media print` masque tout le reste,
`window.print()` ; Propriétés : formulaire + « Enregistrer » → `SetMetadata` + bloc Sécurité
(protégé / permissions) ; Protéger : deux mots de passe + confirmation + cases permissions,
bouton « Retirer la protection » si protégé ; Optimiser : options + rapport (`t.bytes`) ;
Pièces jointes : liste, Extraire (dialogue natif), Ajouter, Supprimer ; Comparer : sélecteur
d'onglet ou fichier, page, `<img>` depuis `blob:` (PNG renvoyé), compteur, ‹ ›.

- [ ] Implémenter ; `pnpm check/lint/test/format` ; commit `feat(app): file menu, export, print
  view, metadata, protection, optimize, attachments, compare`.

### Task 9 — Vérification, documentation, relecture

- Scénarios CDP `export-securite.json` (checklist de la spec sur une copie de `simple.pdf`,
  `window.print` espionné) et `pieces-jointes.json` (copie d'`attachments.pdf`) ; CLI sur les
  fichiers produits.
- ADR-0011 (export, impression WebView, chiffrement lopdf V5, optimisation conservatrice,
  comparaison visuelle), `docs/apprentissage/lot-6.md`, `/concept read-write-et-cursor`,
  `/concept features-cargo-et-dependances`, FONCTIONNALITES / ROADMAP / README / CLAUDE.md.
- `/verif`, relecture Fable + `pdf-expert`, passe de correction, PR, CI, fusion.
