# Lot 3 — Annotations : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Créer, lire, déplacer, modifier et supprimer des annotations (marquages de texte, note,
texte, encre, formes, tampons) via des `EditCommand`, rendues par PDFium et lisibles ailleurs.

**Architecture:** `mega-core::annotations` (modèle `serde`, lecture, création avec apparences
construites par objets chemin/texte pour `Ink`/`Stamp`, détection de forme à la lecture) ;
4 nouvelles variantes `EditCommand` ; requête `Annotations` de l'acteur ; commande Tauri et
CLI `annots` ; frontend : machine à états des outils (pure), calque SVG par page, barre
d'outils d'annotation, panneau latéral.

**Tech Stack:** pdfium-render 0.9.4 (API sûre uniquement), chrono (dates), Svelte 5, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-26-lot-3-annotations.md`

## Global Constraints

- `#![forbid(unsafe_code)]` : aucun appel aux bindings bruts de pdfium-render.
- Toute édition = variante `EditCommand` appliquée via `Document::apply` (ADR-0007) ; toute
  écriture via `write_atomically`. Aucune logique métier dans `app/src-tauri` ni en TS (sauf
  logique d'interface pure et testée : outils, poignées, conversions de coordonnées).
- Couleurs posées **avant** toute apparence (`FPDFAnnot_SetColor` échoue sinon) ; modifier =
  supprimer + recréer.
- Chaînes visibles dans `strings.ts` ; SPDX ; docs `///` en français ; `[Apprentissage]`.
- Nouvelle dépendance `chrono` (déjà dans l'arbre via pdfium-render) : raison dans le commit.
- Coordonnées PDF (points, origine bas-gauche) dans tout le modèle ; conversion écran ↔ PDF
  uniquement dans `app/src/lib/logic/geometry.ts` (existant) et `logic/annotations.ts`.

## Review Focus

1. Page tournée à l'affichage (Lot 1, `doc.rotation = cw90`) : tracer un rectangle doit
   produire un `/Rect` correct en coordonnées PDF (test Vitest `toPdfRect` avec rotation, Task 5).
2. Annotation d'un PDF tiers sans `/AP` ni couleur (`/C` absent) : la lecture ne doit pas
   échouer, `color: None` (test `read_annotation_without_color`, Task 1).
3. Suppression d'une annotation dont l'index n'existe plus (double clic sur Suppr) :
   `PageOutOfRange`-like `InvalidOperation`, document inchangé (test `delete_unknown_annotation_is_refused`, Task 2).
4. Texte libre contenant des caractères hors Latin-1 (« été », « — ») : l'apparence Helvetica
   doit rester lisible ou remplacer, jamais planter ; `/Contents` conserve l'UTF-8 exact
   (test `text_annotation_round_trips_unicode_contents`, Task 2).
5. Opacité 0 ou épaisseur 0 saisies : refusées côté Rust (`InvalidOperation`), pas d'annotation
   invisible créée (test `style_validation`, Task 2).

---

### Task 1 — Modèle, lecture, fixture, rendu des annotations

**Files:**
- Create: `crates/mega-core/src/annotations.rs` (modèle + lecture), `crates/mega-core/tests/annotations.rs`
- Modify: `crates/mega-core/src/lib.rs` (mod + `pub use annotations::{Annotation, AnnotationKind, Color, Geometry, NewAnnotation, Style}`),
  `crates/mega-core/src/testing.rs` (`pub fn annotated_pdf() -> Vec<u8>`), `xtask/src/main.rs` (`fixtures/annotated.pdf`),
  `crates/mega-core/src/render.rs:79-82` (`.render_annotations(true)`), `crates/mega-core/Cargo.toml` (`chrono = "0.4"`).

**Interfaces:**
- Produces:
  ```rust
  #[serde(rename_all = "snake_case")] pub enum AnnotationKind { Note, Highlight, Underline, StrikeOut, Squiggly, Rect, Ellipse, Line, Arrow, Ink, Text, Stamp, Link, Widget, Other }
  pub struct Color { pub r: u8, pub g: u8, pub b: u8 }   // Copy, serde
  pub struct Annotation { pub index: u16, pub kind: AnnotationKind, pub rect: Rect, pub color: Option<Color>, pub opacity: f32, pub author: Option<String>, pub contents: Option<String>, pub created: Option<String>, pub modified: Option<String>, pub quads: Vec<Rect> }
  impl Document { pub fn annotations(&self, page: u16) -> CoreResult<Vec<Annotation>>; }
  ```
  `opacity` = alpha de `stroke_color()` / 255 (1.0 si absent). `kind` : sous-type pdfium →
  variante ; `Ink`/`Stamp` passent par `classify_appearance` (Task 2, stub `Ink`/`Stamp` ici).

- [ ] **Step 1: Fixture** — dans `testing.rs`, `annotated_pdf()` : 1 page A4, `/Annots` avec
  (a) `Text` : `/Rect [50 750 70 770] /Contents (Note de test) /T (Alice) /CreationDate (D:20261026120000Z) /C [1 0.8 0]`,
  (b) `Highlight` : `/Rect [72 700 200 720] /QuadPoints [72 720 200 720 72 700 200 700] /C [1 1 0] /CA 0.5 /T (Bob)`,
  (c) `Square` : `/Rect [100 500 300 600] /C [1 0 0] /IC [0 0 1]` **avec** `/AP << /N 9 0 R >>` (flux de forme `/BBox [0 0 200 100]`, contenu `1 0 0 RG 0 0 1 rg 2 w 1 1 198 98 re B`),
  (d) `Underline` **sans** `/C` (Review Focus 2). Réutiliser `assemble_pdf`/`pdf_text_string`.
  Ajouter l'écriture de `fixtures/annotated.pdf` dans `xtask gen-fixtures` et l'exécuter.
- [ ] **Step 2: Tests (RED)** — `tests/annotations.rs` :
  `reads_third_party_annotations` (4 annotations ; kinds `[Note, Highlight, Rect, Underline]` ; note : author `Some("Alice")`, contents `Some("Note de test")`, created commence par `D:20261026` ; highlight : `opacity ≈ 0.5`, `quads.len() == 1`, quad `x≈72, w≈128` ; square : color `Some(Color{255,0,0})`) ;
  `read_annotation_without_color` (underline : `color == None`, `opacity == 1.0`) ;
  `annotations_out_of_range_page_is_error` ;
  `render_includes_annotations` (`render_page(0, 1.0, Rotation::None)` de `annotated_pdf()` ≠ rendu de `sample_pdf(1, "x")` ; et ≠ du même fichier **sans** `/Annots` : ajouter `annotated_pdf_without_annots()` ou comparer au rendu avant `render_annotations(true)` : choisir la première option).
- [ ] **Step 3:** `cargo test -p mega-core --test annotations` → échec de compilation (module absent).
- [ ] **Step 4: Implémenter** `annotations.rs` (lecture : `page.annotations().iter()`, `annotation_type()`, `bounds()`, `stroke_color()`, `creator()`, `contents()`, `creation_date()`, `modification_date()`, `attachment_points()` pour les marquages) sous `engine::guard()` ; `Rect` depuis `PdfRect` (helper existant de `geometry`/`links` à réutiliser) ; `render.rs` : `.render_annotations(true)`.
- [ ] **Step 5:** tests verts ; `cargo test --workspace` vert (vérifier que les tests de rendu du Lot 1 ne changent pas : les fixtures existantes n'ont pas d'annotations sauf `outlined.pdf` qui a des **liens** — s'assurer que le rendu des liens reste visuellement vide ou mettre à jour l'attente).
- [ ] **Step 6: Commit** `feat(core): annotation model, reading and rendering (chrono for dates)`.

### Task 2 — Création, apparences, suppression, déplacement

**Files:**
- Create: `crates/mega-core/src/annotations/appearance.rs` (déplacer `annotations.rs` en `annotations/mod.rs`)
- Modify: `crates/mega-core/src/annotations/mod.rs`, `tests/annotations.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Style { pub color: Color, pub fill: Option<Color>, pub opacity: f32, pub stroke_width: f32, pub font_size: f32 }
  #[serde(tag = "kind", rename_all = "snake_case")]
  pub enum Geometry { Rect { rect: Rect }, Quads { rects: Vec<Rect> }, Line { from: [f32; 2], to: [f32; 2] }, Strokes { strokes: Vec<Vec<[f32; 2]>> } }
  pub struct NewAnnotation { pub kind: AnnotationKind, pub geometry: Geometry, pub style: Style, pub contents: String, pub author: String }
  impl Document {
      pub fn add_annotation(&mut self, page: u16, new: &NewAnnotation) -> CoreResult<u16>;   // index créé
      pub fn delete_annotation(&mut self, page: u16, index: u16) -> CoreResult<()>;
      pub fn set_annotation_rect(&mut self, page: u16, index: u16, rect: Rect) -> CoreResult<()>;
  }
  pub(crate) fn classify_appearance(ann: &PdfPageAnnotation) -> AnnotationKind;  // Ink → Rect/Ellipse/Line/Arrow/Ink ; Stamp → Text/Stamp
  ```
  Correspondance kind → sous-type pdfium : `Note→Text`, `Highlight/Underline/StrikeOut/Squiggly` → idem (quads obligatoires), `Rect/Ellipse/Line/Arrow/Ink → Ink` (objets chemin), `Text/Stamp → Stamp` (objets texte, + cadre pour Stamp). Autres kinds → `InvalidOperation`.
  Validation : `opacity ∈ (0, 1]`, `stroke_width > 0` (sauf marquages), `font_size ≥ 4`, `Quads` non vide, `Strokes` ≥ 2 points, `Line` from ≠ to.
  Note : `rect` 20×20 pt ancré au coin haut-gauche de `Geometry::Rect`. Flèche : segment + pointe (2 traits à 30°, longueur `max(8, 4×width)`). Ellipse : 4 Bézier (k = 0.5523). Texte : une ligne par `\n`, interligne `font_size × 1.2`, police `fonts_mut().helvetica()`, couleur = `style.color`, rect = géométrie fournie. Tampon : cadre arrondi 2 pt + texte centré (taille adaptée à la largeur).
  Dates : `set_creation_date(Utc::now())` ; auteur : `set_creator`.

- [ ] **Step 1: Tests (RED)** — dans `tests/annotations.rs`, un test aller-retour par kind
  (créer → `to_bytes` → `Document::from_bytes` → `annotations(0)`) :
  `highlight_round_trip` (kind, quads, color, opacity 0.5, author « Moi », created `Some`),
  `note_round_trip` (contents « Bonjour », rect 20×20),
  `shapes_round_trip` (Rect/Ellipse/Line/Arrow détectés après relecture, rect englobant correct à 1 pt près),
  `ink_round_trip` (kind `Ink`, rect = englobant des points ± épaisseur/2),
  `text_annotation_round_trips_unicode_contents` (contents « été — ok », kind `Text`),
  `stamp_round_trip` (kind `Stamp`, contents « BROUILLON »),
  `annotated_render_differs_from_plain` (rendu avant/après ajout d'un Rect ≠),
  `delete_and_move_annotation` (suppression → len−1 ; `set_annotation_rect` → nouveau `rect`),
  `delete_unknown_annotation_is_refused`, `style_validation` (opacity 0 / width 0 / quads vides → `InvalidOperation`).
- [ ] **Step 2:** RED (fonctions absentes).
- [ ] **Step 3: Implémenter** `appearance.rs` (constructeurs d'objets + `classify_appearance` par analyse des `segments()` du premier objet chemin : 2 segments → Line ; 4 Bézier → Ellipse ; 4-5 lignes axiales fermées → Rect ; 2 objets dont le second à 3 segments → Arrow ; texte seul → Text ; texte + chemin → Stamp ; sinon Ink/Stamp) et les trois méthodes de `Document` (une prise de jeton chacune ; `delete_annotation` via `page.annotations_mut().delete_annotation(ann)`).
- [ ] **Step 4:** verts ; clippy ; `cargo doc`.
- [ ] **Step 5: Commit** `feat(core): create, move and delete annotations with portable appearances`.

### Task 3 — Commandes d'édition et acteur

**Files:**
- Modify: `crates/mega-core/src/edit.rs` (variantes + labels + apply), `crates/mega-core/src/service/mod.rs` (`Request::Annotations`, `ServiceHandle::annotations(id, page) -> CoreResult<Vec<Annotation>>`), `tests/edit.rs`, `tests/service.rs`

**Interfaces:**
- Produces: `EditCommand::AddAnnotation { page: u16, annotation: NewAnnotation }`, `DeleteAnnotation { page: u16, index: u16 }`, `UpdateAnnotation { page: u16, index: u16, annotation: NewAnnotation }` (= delete puis add), `MoveAnnotation { page: u16, index: u16, rect: Rect }`.
  Libellés : « Ajouter : <nom du kind> », « Supprimer : <kind> », « Modifier : <kind> », « Déplacer : <kind> » avec noms FR (`AnnotationKind::label_fr()` : Note, Surlignage, Soulignement, Barré, Ondulé, Rectangle, Ellipse, Ligne, Flèche, Encre, Texte, Tampon, Lien, Champ, Annotation).

- [ ] **Step 1: Tests (RED)** — `tests/edit.rs::annotation_commands_undo_and_labels` (add → 1 annotation, label « Ajouter : Rectangle », undo → 0, redo → 1 ; move → rect ; update → couleur changée ; delete → 0 ; `is_modified`) ; `tests/service.rs::annotations_request_and_cache_invalidation` (apply Add → `annotations(id, 0).len() == 1`, cache du doc vidé : `render_png` après édition ≠ avant).
- [ ] **Step 2:** RED. **Step 3:** implémenter. **Step 4:** verts. **Step 5: Commit** `feat(core): annotation edit commands and actor request`.

### Task 4 — CLI `annots`, commande Tauri, `api.ts`, préférence auteur

**Files:**
- Create: `crates/mega-cli/src/commands/annots.rs`
- Modify: `crates/mega-cli/src/{cli,main}.rs`, `crates/mega-cli/src/commands/mod.rs`, `crates/mega-cli/tests/cli.rs`, `app/src-tauri/src/commands.rs` (+ `lib.rs`), `app/src/lib/api.ts`, `app/src/lib/state/settings.svelte.ts` (`author: string`, défaut `''`)

**Interfaces:**
- CLI : `mega annots <file> [--page N] [--password]` → une ligne par annotation : `p<page> #<index> <kind> [x y w h] auteur="…" "<contenu tronqué à 40>"` ; sans `--page`, toutes les pages.
- Tauri : `annotations(id: DocId, page: u16) -> Result<Vec<Annotation>, AppError>`.
- `api.ts` : `AnnotationKind` (union de chaînes snake_case), `Color`, `Style`, `Geometry` (union `kind`), `NewAnnotation`, `Annotation`, `annotations(id, page): Promise<Annotation[]>`, `EditCommand` étendu des 4 variantes.

- [ ] **Step 1: Test CLI (RED)** `annots_lists_annotations` sur `fixtures/annotated.pdf` : stdout contient `#0 note`, `auteur="Alice"`, `Note de test`, et 4 lignes `p1 #`.
- [ ] **Step 2:** RED → implémenter → vert. Tauri : `cargo check -p mega-app` ; `pnpm check`.
- [ ] **Step 3: Commit** `feat(cli,app): annots command, annotations IPC and typed mirrors`.

### Task 5 — Logique d'interface pure (Vitest)

**Files:**
- Create: `app/src/lib/logic/tools.ts`, `tools.test.ts`, `app/src/lib/logic/annotations.ts`, `annotations.test.ts`

**Interfaces:**
- `tools.ts` : `type Tool = 'select'|'note'|'highlight'|'underline'|'strikeout'|'text'|'ink'|'rect'|'ellipse'|'line'|'arrow'|'stamp'` ;
  `type Draft = { tool: Tool; points: Pt[] }` (coordonnées **PDF**) ;
  `beginDraft(tool, p): Draft | null` (null pour `select`/`highlight`/`underline`/`strikeout` qui agissent sur la sélection) ; `extendDraft(d, p): Draft` (ink : ajoute si distance ≥ 1 pt ; autres : remplace le 2ᵉ point) ; `finishDraft(d, style, contents, author): NewAnnotation | null` (null si trop petit : < 2 pt) ; `isDrawingTool(tool)`.
- `annotations.ts` : `handlesFor(rect: Rect): { id: 'nw'|'ne'|'sw'|'se'|'move'; at: Pt }[]`, `resize(rect, handle, delta): Rect` (taille min 4 pt), `hitTest(annots: Annotation[], p: Pt): number | null` (dernier dessiné en premier), `quadsFromSelection(rects: Rect[]): Rect[]` (fusion des rects de même ligne), `noteIconRect(a: Annotation): Rect`, `toPdfRect(cssRect, page, scale, rotation): Rect` (réutilise `cssPointToPdf` ; **Review Focus 1** : test avec `cw90`).

- [ ] **Step 1: Tests (RED)** nommés d'après chaque fonction ; cas pinnés : `extendDraft` ink ignore un point à 0.5 pt ; `finishDraft` rect trop petit → null ; `resize` ne passe pas sous 4 pt ; `hitTest` préfère l'annotation la plus récente ; `toPdfRect` avec rotation `cw90` : un rect CSS en haut à gauche devient un rect PDF en haut à droite (valeurs numériques sur une page 100×200 à l'échelle 1).
- [ ] **Step 2:** RED → implémenter → `pnpm vitest run` vert. **Step 3: Commit** `feat(app): pure tool state machine and annotation geometry`.

### Task 6 — État, calque SVG, barre d'outils d'annotation

**Files:**
- Create: `app/src/lib/components/AnnotationLayer.svelte`, `AnnotationToolbar.svelte`, `StylePicker.svelte`
- Modify: `app/src/lib/state/documents.svelte.ts` (`OpenDoc.annotations: Record<number, Annotation[]>`, `OpenDoc.selectedAnnotation: { page, index } | null`, `ensureAnnotations(doc, page)`, `afterEdit` vide le cache ; état global `toolState = $state({ tool: 'select', style: {...défauts}, stamp: 'BROUILLON' })` dans `state/tools.svelte.ts`), `PageView.svelte` (calque au-dessus du texte), `+page.svelte`, `strings.ts`

Comportement : outil ≠ `select` → le calque capte les pointeurs ; `pointerdown/move/up` → `beginDraft/extendDraft/finishDraft` → `applyEdit(doc, { kind: 'add_annotation', … })` ; `text`/`note` ouvrent la saisie du contenu (Task 7) avant l'envoi ; `highlight`/`underline`/`strikeout` : bouton de la barre → `quadsFromSelection(selectionRects(...))` sur `doc.selection`. Outil `select` : clic = `hitTest` → sélection ; glisser = `move_annotation` à la fin du geste ; poignées = `resize` ; `Suppr` = `delete_annotation`. Icônes de note dessinées pour chaque `kind === 'note'`. Défauts de style : couleur `#ffd400` (marquages) / `#e53935` (formes, encre) / `#1e88e5` (texte), opacité 1 (0.5 pour surlignage), épaisseur 2, police 14.

- [ ] **Step 1:** implémenter (composants Svelte, validés par le scénario CDP de la Task 8) ; `pnpm check`, `lint`, `format`.
- [ ] **Step 2: Commit** `feat(app): annotation layer, tools and style picker`.

### Task 7 — Panneau Annotations, éditeur de contenu, tampons, raccourcis

**Files:**
- Create: `app/src/lib/components/AnnotationPanel.svelte`, `AnnotationEditor.svelte`, `StampPicker.svelte`
- Modify: `Sidebar.svelte` (3ᵉ onglet « Annotations »), `logic/shortcuts.ts` (+ test : `n` note, `h` surligner, `u` souligner, `k` barrer, `t` texte, `d` dessiner, `Delete` supprimer, `Escape` → select), `strings.ts`, `settings.svelte.ts` (auteur, champ dans le panneau)

Panneau : liste par page (`t.annotationKind[kind]`, auteur, date formatée `toLocaleString`, extrait 60 car.) ; clic → `goToPage` + sélection ; bouton supprimer. Éditeur : dialogue avec `textarea` (contenu), sélecteur de couleur, opacité, épaisseur, taille de police, auteur → `update_annotation`. Tampons : 5 libellés de la spec.

- [ ] **Step 1:** test `shortcuts.test.ts` (RED → GREEN) ; implémenter ; `pnpm check/lint/test/format`.
- [ ] **Step 2: Commit** `feat(app): annotation panel, editor, stamps and shortcuts`.

### Task 8 — Vérification, documentation, relecture

- Scénario CDP `scripts/scenarios/annotation.json` (checklist 1-8 de la spec, copie de `simple.pdf`, puis `mega annots` et `mega render` sur la copie) ; `lecteur-*.json` toujours verts (rendu des annotations activé).
- ADR-0008 (sous-types choisis, apparences, limites pdfium-render, révision Lot 5), `docs/apprentissage/lot-3.md`, `/concept enums-et-match-exhaustif` et `/concept pattern-builder`, `FONCTIONNALITES.md` (section Annotations), `ROADMAP.md`, README, CLAUDE.md (règle : couleurs avant apparence ; pas de bindings bruts).
- `/verif` ; relecture indépendante (Fable, lecture seule) + `pdf-expert` sur la conformité des annotations ; une passe de correction ; PR ; CI 3 OS ; fusion.
