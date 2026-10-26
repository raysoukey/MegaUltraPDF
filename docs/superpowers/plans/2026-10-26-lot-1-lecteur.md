# Lot 1 — Lecteur : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (native) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Un lecteur PDF complet (onglets, rendu asynchrone priorisé avec cache, zoom, navigation, vignettes, signets, liens, sélection, recherche, propriétés, thème, raccourcis, préférences) sur la fondation du Lot 0.

**Architecture:** Acteur PDFium dans `mega-core::service` (thread unique propriétaire des documents, file priorisée, cache LRU, générations) exposé par un `ServiceHandle` bloquant ; backend Tauri en commandes `async` + protocole `mega://` asynchrone ; frontend SvelteKit avec logique pure testée et composants fins.

**Tech Stack:** inchangée + `tauri-plugin-store`, `tauri-plugin-opener`, `@tauri-apps/plugin-store`, `@tauri-apps/plugin-opener`.

**Spec:** `docs/superpowers/specs/2026-10-26-lot-1-lecteur.md`

## Global Constraints

Celles du Lot 0 (SPDX, français/anglais, `forbid(unsafe_code)`, docs `///`, `[Apprentissage]`,
chaînes dans `strings.ts`, aucun réseau) **plus** ADR-0005 (lints anti-panique actifs) et :
`pdfium_render` reste confiné à `mega-core` ; le frontend ne décide rien sur le document ;
toute nouvelle commande Tauri a son miroir typé dans `api.ts`.

## Review Focus

1. PDF de plusieurs centaines de pages : l'ouverture ne rend pas toutes les pages ; seules les
   visibles (± 1) et les vignettes à l'écran sont demandées (test `virtualization` + ledger).
2. Zoom rapide répété : les rendus périmés sont annulés (test `stale_generation_is_cancelled`).
3. Fermeture d'un onglet pendant un rendu en cours : pas de panique, réponse `NotFound`
   (test `render_after_close_is_not_found`).
4. Page sans texte (page vide) : calque vide, recherche sans résultat, aucune erreur
   (test `text_boxes_empty_on_blank_page`).
5. Lien URI avec schéma non http(s) (`file:`, `javascript:`) : jamais ouvert, affiché comme
   inactif (test `external_link_policy` côté TS).

---

### Task 1 — `mega-core` : texte, recherche, signets, liens, propriétés, rotation

**Files:** Create `crates/mega-core/src/text.rs`, `outline.rs`, `links.rs`, `properties.rs` ;
Modify `document.rs`, `render.rs`, `testing.rs`, `lib.rs`, `tests/document.rs` ; Create `fixtures/outlined.pdf` (via `xtask gen-fixtures`).

**Interfaces (Produces):**
- `#[derive(Serialize, Clone, Copy)] pub enum Rotation { None, Cw90, Cw180, Cw270 }` + `Document::render_page_with(index, scale, rotation) -> CoreResult<RenderedPage>` (`render_page` = `render_page_with(.., Rotation::None)`).
- `#[derive(Serialize, Clone, PartialEq)] pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }` (points, origine bas-gauche) ; `pub struct TextBox { pub ch: char, pub rect: Rect }` ; `Document::text_boxes(index) -> CoreResult<Vec<TextBox>>` (ordre de lecture PDFium, `loose_bounds`).
- `pub struct SearchOptions { pub match_case: bool, pub whole_word: bool }` ; `pub struct SearchHit { pub page: u16, pub rects: Vec<Rect> }` ; `Document::search_page(index, query, &SearchOptions) -> CoreResult<Vec<SearchHit>>`.
- `pub struct OutlineNode { pub title: String, pub page: Option<u16>, pub children: Vec<OutlineNode> }` ; `Document::outline() -> CoreResult<Vec<OutlineNode>>`.
- `pub enum LinkTarget { Page(u16), Uri(String) }` ; `pub struct PageLink { pub rect: Rect, pub target: LinkTarget }` ; `Document::links(index) -> CoreResult<Vec<PageLink>>`.
- `pub struct DocumentProperties { pub title, author, subject, keywords, creator, producer, creation_date, modification_date: Option<String>, pub pdf_version: String, pub page_count: u16 }` ; `Document::properties() -> CoreResult<DocumentProperties>`.
- `testing::outlined_pdf() -> Vec<u8>` : 3 pages A4 avec texte « Page N », `/Outlines` (« Chapitre 1 » → p.1 avec enfant « Section 1.1 » → p.2 ; « Chapitre 2 » → p.3), page 1 avec un lien `/Dest` vers p.3 (rect 72,700,200,720) et un lien `/URI https://example.org` (rect 72,650,200,670).

- [ ] Tests (RED) dans `tests/document.rs` : `text_boxes_read_page_label` (page 2 de `sample_pdf(3,..)` contient les caractères « P a g e 2 » en ordre, rects avec `h > 0`), `text_boxes_empty_on_blank_page` (`blank_pdf`), `search_finds_each_page_once` (« Page » sur 3 pages → 3 hits de 1 rect), `search_respects_case` (« page » avec `match_case` → 0), `outline_tree_has_child` (outlined : 2 racines, première a 1 enfant page 1), `links_internal_and_uri` (page 0 : 1 lien `Page(2)`, 1 lien `Uri("https://example.org")`), `properties_expose_title_and_version` (titre, `pdf_version` commence par « 1. », `page_count`), `render_rotated_swaps_dimensions` (A4 Cw90 → largeur > hauteur).
- [ ] Implémenter ; `outlined_pdf` via `assemble_pdf` (objets : Catalog avec `/Outlines`, `/PageMode /UseOutlines`, Pages, 3 Pages avec `/Contents` texte Helvetica via ressource `/Font`, Outlines + 3 items, 2 annotations Link). `search_page` via `page.text().search(query, &PdfSearchOptions)` et `iter(Forward)` ; rects des segments.
- [ ] `xtask gen-fixtures` écrit aussi `fixtures/outlined.pdf` ; tests verts ; fmt/clippy/doc ; commit `feat(core): text layer, search, outline, links, properties, rotated render`.

### Task 2 — `mega-core::service` : acteur PDFium

**Files:** Create `crates/mega-core/src/service/{mod.rs, cache.rs, queue.rs}`, `tests/service.rs` ; Modify `lib.rs`, `Cargo.toml` (aucune dépendance nouvelle).

**Interfaces (Produces):**
- `pub type DocId = u32` (déplacé ici ; le backend l'importe).
- `#[derive(Serialize, Clone)] pub struct DocumentSummary { pub id: DocId, pub path: Option<String>, pub title: Option<String>, pub page_count: u16, pub pages: Vec<PageSize> }`.
- `#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)] pub enum Priority { Thumbnail = 0, Prefetch = 1, Visible = 2 }`.
- `pub struct RenderRequest { pub doc: DocId, pub page: u16, pub scale: f32, pub rotation: Rotation, pub generation: u64, pub priority: Priority }`.
- `pub struct ServiceHandle` (Clone + Send + Sync) : `open_path(path, password) -> CoreResult<DocumentSummary>`, `open_bytes(bytes, password)`, `close(id) -> CoreResult<()>`, `bump_generation(id) -> CoreResult<u64>` (appelé au changement de zoom/rotation ; invalide le cache du doc), `render_png(req) -> CoreResult<Arc<Vec<u8>>>`, `text_boxes(id, page)`, `search(id, query, opts) -> CoreResult<Vec<SearchHit>>` (toutes pages), `outline(id)`, `links(id, page)`, `properties(id)`, `summary(id)`.
- Erreurs : `CoreError::NotFound { id }` (nouvelle variante, « Document inconnu »), `CoreError::Cancelled` (« Rendu annulé (génération périmée) »), `CoreError::ServiceStopped`.
- `PdfService::spawn() -> ServiceHandle` ; `PdfService::spawn_with(cache_budget_bytes)`.
- Interne : `queue.rs` : `PriorityQueue` (BinaryHeap de `(Priority, Reverse(seq))`) ; l'acteur fait `recv()` bloquant puis `try_recv()` en rafale pour remplir la file avant de traiter le plus prioritaire ; `cache.rs` : `PngCache` LRU par budget (défaut 256 Mo) clé `(doc, page, (scale*100) as u32, rotation)`, invalidation par doc.

- [ ] Tests (RED) `tests/service.rs` : `open_and_summary`, `close_then_use_is_not_found`, `render_png_is_cached` (2e appel renvoie le même `Arc` pointeur), `bump_generation_invalidates_cache`, `stale_generation_is_cancelled` (requête avec génération < courante → `Cancelled`), `render_after_close_is_not_found`, `search_whole_document`, `handles_are_cloneable_across_threads` (4 threads × handle cloné), `visible_requests_served_before_thumbnails` (bloquer l'acteur avec un rendu, empiler 1 vignette puis 1 visible, vérifier l'ordre des réponses via timestamps/compteur partagé).
- [ ] Implémenter ; `[Apprentissage]` sur l'enum de messages, le `sync_channel(1)` comme oneshot, `BinaryHeap`+`Ord`, `Arc<Vec<u8>>` partagé entre cache et réponse.
- [ ] Verts, fmt/clippy/doc ; commit `feat(core): PDFium actor service with priority queue, LRU cache and generations`.

### Task 3 — Backend Tauri sur l'acteur

**Files:** Modify `app/src-tauri/src/{lib.rs, commands.rs, protocol.rs, error.rs}`, delete `registry.rs`, `Cargo.toml`, `capabilities/default.json`, `tauri.conf.json`.

**Interfaces (Produces, miroir dans `api.ts` en Task 4):**
- Commandes `async` : `open_document(path, password) -> DocumentSummary`, `close_document(id)`, `bump_generation(id) -> u64`, `text_layer(id, page) -> Vec<TextBox>`, `search_document(id, query, match_case, whole_word) -> Vec<SearchHit>`, `outline(id) -> Vec<OutlineNode>`, `page_links(id, page) -> Vec<PageLink>`, `document_properties(id) -> DocumentProperties`, `startup_file()`, `file_size(path) -> u64`.
- Toutes via `tauri::async_runtime::spawn_blocking(move || handle.xxx(..)).await` (erreur de jointure → `Internal`).
- `ErrorKind` += `Cancelled`, `NotFound` existe.
- Protocole : `mega://page/<id>/<n>?scale=S&rot=R&gen=G&prio=P` asynchrone ; `Cancelled` → HTTP 409 ; `respond` devient `respond(handle, path, query) -> Response` exécuté dans `spawn_blocking`, `responder.respond(...)`.
- Plugins `store` (`tauri_plugin_store::Builder::new().build()`) et `opener` ; capabilities `store:default`, `opener:default`, `dialog:default` (confirm). Retirer le `Mutex<DocumentRegistry>`.

- [ ] Tests (RED) : `parse_page_request` accepte `rot`/`gen`/`prio` avec défauts ; `respond_cancelled_is_409` ; `respond_png_for_valid_page` via handle ; adaptation des tests existants.
- [ ] Implémenter ; CDP : ouvrir simple.pdf, image page 1, `bump_generation` puis URL avec ancienne `gen` → 409.
- [ ] Commit `feat(app): async commands and asynchronous mega:// protocol on the PDFium actor`.

### Task 4 — Frontend : logique pure (Vitest) + `api.ts`

**Files:** Create `app/src/lib/logic/{geometry.ts, zoom.ts, selection.ts, virtualization.ts, shortcuts.ts, recent.ts, links.ts}` + `*.test.ts` ; Modify `api.ts`, `strings.ts`.

**Interfaces (Produces):**
- `geometry.ts` : `pdfRectToCss(rect: Rect, page: PageSize, scale: number, rotation: Rotation): CssRect` (origine bas-gauche → haut-gauche, rotation 0/90/180/270), `rotatedSize(page, rotation)`.
- `zoom.ts` : `ZOOM_STEPS`, `nextZoom(current, dir)`, `fitWidthScale(page, containerPx, gutter)`, `fitPageScale(page, containerW, containerH, gutter)`, `clampScale`.
- `selection.ts` : `charIndexAt(boxes: TextBox[], x, y)` (plus proche en points), `selectionRects(boxes, from, to)` (fusion par ligne), `selectedText(boxes, from, to)` (insertion d'espaces/sauts selon écarts).
- `virtualization.ts` : `pageLayout(pages, scale, rotation, gap)` → offsets/hauteurs ; `visiblePages(layout, scrollTop, viewportH, overscan)` ; `pageAtScroll(layout, scrollTop)`.
- `shortcuts.ts` : `actionFor(event: KeyLike): Action | null` (table unique), `SHORTCUT_HELP` (dérivée de la table).
- `recent.ts` : `pushRecent(list, path, max=10)`.
- `links.ts` : `externalLinkPolicy(uri): 'open' | 'blocked'` (http/https/mailto seulement).
- `api.ts` : miroirs typés de toutes les commandes de Task 3 + `pageUrl(id, index, scale, rotation, generation, priority)`.

- [ ] Tests RED pour chaque fonction (valeurs calculées à la main : A4, scale 2, rotation 90 ; fit width 800 px → ~1.344) ; implémenter ; `pnpm test` vert ; commit `feat(app): viewer logic modules with unit tests`.

### Task 5 — Frontend : état, onglets, ouverture, mot de passe, récents, glisser-déposer

**Files:** Create `app/src/lib/state/{documents.svelte.ts, settings.svelte.ts}`, `components/{Tabs.svelte, PasswordDialog.svelte, Toolbar.svelte}` ; Modify `+page.svelte`.

- `documents.svelte.ts` : `openDocs: OpenDoc[]` (`{ summary, scale, zoomMode, rotation, viewMode, page, generation, scrollTop }`), `activeId`, actions `openPath(path, password?)`, `close(id)`, `activate(id)`, `setZoom`, `setRotation`… (chaque changement de zoom/rotation appelle `bump_generation`).
- `settings.svelte.ts` : chargement/sauvegarde via `@tauri-apps/plugin-store` (`settings.json`) : `theme`, `defaultZoomMode`, `viewMode`, `recent`, `positions` (par chemin : page + zoom).
- Mot de passe : dialogue sur `password_required` / `wrong_password`, réessai, annulation.
- Glisser-déposer : `getCurrentWebview().onDragDropEvent` ; `drop` → ouvrir chaque `.pdf`.
- Récents : menu dans la barre (10 max), supprimés s'ils n'existent plus (`io` → retirer).
- [ ] Vitest sur la logique extraite (`recent.ts` déjà) ; `pnpm check/lint` ; CDP : deux fichiers → deux onglets, fermeture, mot de passe ; commit.

### Task 6 — Frontend : visionneuse (zoom, modes, rotation, navigation, virtualisation)

**Files:** Create `components/{Viewer.svelte, PageView.svelte, PageNav.svelte, ZoomControls.svelte}`.

- Défilement continu virtualisé (placeholders dimensionnés, `<img>` chargées pour visibles ± 1 avec `prio=visible`, ± 3 `prefetch`), mode page unique, zoom %, ajuster largeur/page (recalcul au redimensionnement via `ResizeObserver`), Ctrl+molette autour du curseur, rotation, saisie de page, Home/End/PageUp/PageDown, position mémorisée à la fermeture (`positions`).
- Rerendu net : à chaque changement d'échelle, nouvelle `generation` ; l'ancienne image reste affichée (mise à l'échelle CSS) jusqu'à l'arrivée de la nouvelle.
- [ ] CDP : captures à 50 %, 200 %, largeur, page, rotation, page unique ; commit.

### Task 7 — Frontend : vignettes, signets, liens

**Files:** Create `components/{Sidebar.svelte, Thumbnails.svelte, Outline.svelte, LinkLayer.svelte}`.

- Vignettes (`prio=thumbnail`, échelle 0,2, virtualisées), page courante surlignée, clic → page.
- Signets : arbre repliable, clic → page. Liens : calque de rectangles ; interne → page ; URI → `confirm` puis `openUrl` si `externalLinkPolicy === 'open'`.
- [ ] CDP sur `outlined.pdf` ; commit.

### Task 8 — Frontend : calque de texte, sélection, copie, recherche

**Files:** Create `components/{TextLayer.svelte, SearchBar.svelte}`.

- Sélection par glisser (plage d'index via `charIndexAt`), rectangles surlignés, Ctrl+C / menu → `navigator.clipboard.writeText(selectedText)`, double-clic = mot.
- Recherche : Ctrl+F ouvre la barre, requête → `search_document`, surbrillances sur toutes les pages, F3/Shift+F3, compteur « n / total », option casse, Échap ferme.
- [ ] CDP : sélection de « Page 2 » + lecture du presse-papiers via `navigator.clipboard.readText()` ; recherche 3 résultats ; commit.

### Task 9 — Frontend : propriétés, thème, raccourcis, aide

**Files:** Create `components/{PropertiesDialog.svelte, ShortcutsHelp.svelte}`, `app/src/lib/theme.ts`, `app/src/app.css` (variables de thème).

- Thème clair/sombre/système (variables CSS, `prefers-color-scheme`, persistant) ; dialogue propriétés (`document_properties` + `file_size`) ; F1 → aide générée depuis `SHORTCUT_HELP`.
- [ ] CDP captures ; commit.

### Task 10 — Vérification, documentation, relecture

- `scripts/ui-check.mjs` (version du script CDP du scratchpad, documentée) + `scripts/ui-check.ps1`.
- Checklist de la spec exécutée (captures dans la PR) ; `/verif` ; CI.
- ADR-0006 « acteur PDFium et cache » ; `docs/apprentissage/lot-1.md` ; `ROADMAP.md`, `FONCTIONNALITES.md` cochés ; README.
- Relecture indépendante (fable) + passe de correction ; PR ; CI 3 OS ; fusion.
