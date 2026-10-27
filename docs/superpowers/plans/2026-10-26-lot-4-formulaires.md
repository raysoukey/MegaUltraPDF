# Lot 4 — Formulaires et signature visuelle : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lire, remplir (texte, cases, radios), aplatir un formulaire AcroForm et apposer une
signature visuelle, via des `EditCommand`, avec des champs HTML natifs superposés à la page.

**Architecture:** `mega-core::forms` (modèle, lecture par les widgets, écriture + rechargement,
aplatir, signature en objets de page) ; 3 variantes `EditCommand` ; requêtes de l'acteur ;
Tauri + `api.ts` ; `FormLayer` (inputs), `FormPanel`, `SignatureDialog` ; CLI `fields`/`flatten`.

**Tech Stack:** pdfium-render 0.9.4 (API sûre), `image` (png + jpeg), Svelte 5, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-26-lot-4-formulaires.md`

## Global Constraints

- `#![forbid(unsafe_code)]` ; toute édition = `EditCommand` via `Document::apply` ; écriture via
  `write_atomically` ; aucune logique métier en TS ; chaînes dans `strings.ts` ; SPDX ; `///`.
- Après `set_value`/`set_checked`, **recharger** le document (`reload_from_bytes(to_bytes())`)
  pour que PDFium régénère les apparences (`NeedAppearances`).
- Image de signature : fichier ≤ 20 Mo, décodage par `image::load_from_memory` (png/jpeg),
  dimensions ≤ 8192 px ; erreurs typées (`InvalidOperation`).
- Feature Cargo `image/jpeg` ajoutée à `mega-core` (raison dans le commit).

## Review Focus

1. Formulaire **sans** `/NeedAppearances` : valeur écrite dans `/V`, apparence PDFium inchangée ;
   le calque HTML montre la valeur ; documenté (test `value_is_written_without_need_appearances`, Task 2).
2. Champ lecture seule : `set_field_value` refusé (`InvalidOperation`), rien d'écrit (test Task 2).
3. Radio : cocher B décoche A dans le fichier relu (test Task 2).
4. Aplatir un document sans formulaire : succès sans changement (test Task 3).
5. Image de signature corrompue ou énorme : erreur propre, document inchangé (test Task 3).

---

### Task 1 — Modèle, fixture, lecture

**Files:** Create `crates/mega-core/src/forms/mod.rs`, `tests/forms.rs` ; Modify `lib.rs`
(`pub mod forms; pub use forms::{FieldKind, FieldValue, FormField, FormInfo, FormType}`),
`testing.rs` (`form_pdf()`, `xfa_pdf()`), `xtask/src/main.rs` (`fixtures/form.pdf`).

**Produces:**
```rust
#[serde(rename_all = "snake_case")] pub enum FormType { None, AcroForm, Xfa }
#[serde(rename_all = "snake_case")] pub enum FieldKind { Text, Checkbox, Radio, Combo, List, Button, Signature, Unknown }
pub struct FieldOption { pub label: String, pub selected: bool }
pub struct FormField { pub page: u16, pub index: u16, pub name: String, pub kind: FieldKind, pub value: Option<String>, pub checked: Option<bool>, pub options: Vec<FieldOption>, pub rect: Rect, pub read_only: bool, pub required: bool, pub multiline: bool, pub password: bool }
pub struct FormInfo { pub form_type: FormType, pub field_count: u32 }
impl Document { pub fn form_info(&self) -> CoreResult<FormInfo>; pub fn form_fields(&self, page: u16) -> CoreResult<Vec<FormField>>; }
```
`index` = position de l'annotation `Widget` dans `/Annots` de la page (comme les annotations).

- [ ] **Step 1 : fixture** `form_pdf()` (voir spec) et `xfa_pdf()` ; `gen-fixtures` écrit `fixtures/form.pdf`.
- [ ] **Step 2 : tests (RED)** `reads_form_fields` (7 champs ; noms, genres, `ville` : 2 options dont « Paris » sélectionnée, `ref.read_only`, `email.required`, `adresse.multiline`, `ok.checked == Some(false)`, rects), `form_info_detects_acroform_and_xfa` (`form.pdf` → AcroForm/7 ; `sample_pdf` → None/0 ; `xfa_pdf` → Xfa), `form_fields_on_page_without_widgets_is_empty`.
- [ ] **Step 3 :** implémenter ; `cargo test -p mega-core --test forms` vert. **Commit** `feat(core): form model, fixture and reading`.

### Task 2 — Écriture des valeurs, aplatir, commandes

**Files:** Modify `forms/mod.rs`, `edit.rs`, `service/mod.rs`, `tests/forms.rs`, `tests/edit.rs`, `tests/service.rs`.

**Produces:**
```rust
#[serde(tag = "kind", rename_all = "snake_case")] pub enum FieldValue { Text { text: String }, Checked { checked: bool } }
impl Document { pub fn set_field_value(&mut self, page: u16, index: u16, value: &FieldValue) -> CoreResult<()>; pub fn flatten_all(&mut self) -> CoreResult<()>; }
EditCommand::SetFieldValue { page: u16, index: u16, value: FieldValue }  // libellé « Remplir : <nom> »
EditCommand::FlattenForm                                                // « Aplatir le formulaire »
ServiceHandle::form_fields(id, page), form_info(id)
```
Règles : texte sur un `Text` seulement, `Checked` sur `Checkbox`/`Radio` ; lecture seule →
`InvalidOperation` ; après écriture, `reload_from_bytes(self.to_bytes()?)`.

- [ ] **Step 1 : tests (RED)** `set_text_value_round_trips_and_renders` (valeur relue après `to_bytes`/réouverture ; rendu avant/après différent), `check_box_and_radio_group` (cocher B décoche A ; `is_checked`), `read_only_field_is_refused`, `wrong_value_kind_is_refused`, `value_is_written_without_need_appearances` (fixture variante sans `NeedAppearances` : `/V` relu, rendu éventuellement inchangé — assertion sur `/V` seulement), `flatten_removes_fields_and_keeps_rendering` (0 champ après ; rendu ≠ vide ; `flatten_all` sur `sample_pdf` : Ok), `edit.rs::form_commands_are_undoable` (libellés, undo), `service.rs::form_requests`.
- [ ] **Step 2 :** implémenter ; verts. **Commit** `feat(core): fill, flatten and form edit commands`.

### Task 3 — Signature visuelle (cœur) + CLI + Tauri + `api.ts`

**Files:** Create `forms/signature.rs`, `crates/mega-cli/src/commands/{fields,flatten}.rs` ; Modify `edit.rs`, `Cargo.toml` (`image` features `png`, `jpeg`), `cli.rs`, `main.rs`, `commands/mod.rs`, `tests/cli.rs`, `app/src-tauri/src/commands.rs`, `lib.rs`, `app/src/lib/api.ts`.

**Produces:**
```rust
#[serde(tag = "kind", rename_all = "snake_case")] pub enum SignatureSource { Image { path: String }, Strokes { strokes: Vec<Vec<[f32; 2]>> } }
impl Document { pub fn insert_signature(&mut self, page: u16, rect: Rect, source: &SignatureSource) -> CoreResult<()>; }
EditCommand::InsertSignature { page: u16, rect: Rect, source: SignatureSource }   // « Apposer une signature »
```
Image : ajustée dans `rect` (proportions gardées, centrée) ; traits : chemins noirs 2 pt arrondis, points en PDF ; `page.regenerate_content()` après ajout. CLI : `mega fields <f> [--password]` (une ligne par champ `p<page> <nom> [<genre>] = <valeur|coché|->`), `mega flatten <f> -o <out>`. Tauri : `form_fields`, `form_info` ; `api.ts` : types + `formFields`, `formInfo`, `EditCommand` étendu, `pickImageFile()` (dialogue, filtres png/jpg).

- [ ] **Step 1 : tests (RED)** `signature_strokes_change_rendering_and_survive_reopen`, `signature_image_is_fitted` (PNG généré en test par `image` 20×10 → objet image dont l'emprise tient dans `rect` avec le bon ratio ; lu via `page.objects()` image object bounds), `corrupt_or_huge_image_is_refused` (fichier texte ; image 9000×10 → `InvalidOperation`, aucun objet ajouté), CLI `fields_and_flatten_commands` (`fields form.pdf` contient `nom`, `ville`, `Paris` ; `flatten -o` puis `fields` → `0 champ`).
- [ ] **Step 2 :** implémenter ; `cargo test --workspace`, `cargo check -p mega-app`, `pnpm check`. **Commit** `feat(core,cli,app): visual signature, fields/flatten commands, form IPC`.

### Task 4 — Logique d'interface pure (Vitest)

**Files:** Create `app/src/lib/logic/forms.ts`, `forms.test.ts`.

`missingRequired(fields): FormField[]` ; `canvasStrokesToPdf(strokes: {x,y}[][], canvas: {w,h}, rect: Rect): [number,number][][]` (origine canvas en haut-gauche → PDF, proportions gardées, bornes) ; `fitRect(imageW, imageH, rect): Rect` (ajustement proportionnel centré — même règle que Rust, testée des deux côtés) ; `fieldInputType(field): 'text'|'password'|'textarea'|'checkbox'|'radio'|'select'|'none'`.

- [ ] Tests RED → GREEN → **Commit** `feat(app): pure form logic`.

### Task 5 — Calque de champs, panneau, signature, raccourcis

**Files:** Create `components/FormLayer.svelte`, `FormPanel.svelte`, `SignatureDialog.svelte` ; Modify `PageView.svelte` (calque sous le calque d'annotations), `Sidebar.svelte` (onglet « Formulaire » si `formInfo.field_count > 0`), `documents.svelte.ts` (`formFields` par page, `formInfo`, `setFieldValue`, `flattenForm`, `insertSignature`, invalidation dans `afterEdit`), `AnnotationToolbar.svelte` (outil `signature` : placement par glisser → dialogue), `logic/tools.ts` (tool `signature` : brouillon rectangle), `+page.svelte` (bandeau XFA, confirmation d'aplatissement), `strings.ts`, `shortcuts` (aucun nouveau).

Comportement : `FormLayer` positionne chaque champ (`pdfRectToCss`), valeur locale liée, envoi au `change`/`blur` (texte) ou au `change` (case/radio) ; champ lecture seule → `readonly`/`disabled` ; requis → bordure ; combo/liste → `<select disabled>` avec `title={t.formListReadOnly}` ; après `afterEdit` les valeurs se resynchronisent (cache vidé). `SignatureDialog` : onglet « Image » (bouton fichier → `pickImageFile`) / « Dessiner » (canvas 400×150, pointer events, traits) ; validation → `insertSignature` sur le rectangle tracé.

- [ ] Implémenter ; `pnpm check/lint/test/format`. **Commit** `feat(app): form layer, panel and visual signature`.

### Task 6 — Vérification, documentation, relecture

- Scénario CDP `formulaire.json` (checklist 1-8 ; `xfa.pdf` via `annotation-tierce`-like scénario `formulaire-xfa.json`) ; `mega fields` sur la copie.
- ADR-0009 (champs HTML natifs, rechargement après écriture, limites : NeedAppearances, combos, aplatissement global), `docs/apprentissage/lot-4.md`, `/concept drop-et-ressources-natives`, FONCTIONNALITES / ROADMAP / README / CLAUDE.md.
- `/verif`, relecture Fable + `pdf-expert` (AcroForm), passe de correction, PR, CI, fusion.
