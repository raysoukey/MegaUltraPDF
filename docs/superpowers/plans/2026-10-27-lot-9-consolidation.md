# Lot 9 — Consolidation : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Corriger les constats mineurs reportés des Lots 0 à 8 encore ouverts (effort S/M), chacun prouvé par un test vu échouer, et consigner ceux qui relèvent d'une nouvelle fonctionnalité.

**Architecture:** Corrections locales réparties par domaine (sécurité/CLI, annotations, formulaires, contenu/polices, métadonnées, optimisation/export, signatures, OCR, service, interface, accessibilité, distribution). Deux ajouts transverses : une table de largeurs Helvetica (`mega-core/src/fonts.rs`) et un code stable pour les verdicts de signature (`SigIssue`).

**Tech Stack:** Rust (pdfium-render 0.9.4, lopdf 0.46, crates RustCrypto du Lot 8, `rpassword` nouveau), Tauri 2, Svelte 5, Vitest, scénarios CDP.

**Spec:** docs/superpowers/specs/2026-10-27-lot-9-consolidation.md

## Global Constraints

- CLAUDE.md : `write_atomically`, `EditCommand` via `apply`, `mark_changed()` en tête de toute
  méthode publique qui modifie le document, pas d'`unwrap` hors tests, docs `///` en
  français, deux dictionnaires `fr`/`en`, aucun appel réseau, `SPDX` en première ligne.
- Tout parcours d'un graphe PDF garde un ensemble des nœuds visités.
- Aucune nouvelle dépendance sauf `rpassword` (raison dans le commit).
- Les identifiants A/B/C des items sont ceux de la spec ; chaque commit les cite.
- Version 0.9.0 (Task 13).

## Review Focus

1. Note caviardée dont le popup est hors zone : le texte de la note n'est plus dans le fichier
   enregistré, ni dans la réponse `/IRT` (Task 1, `redacted_note_does_not_survive_through_its_popup`).
2. CLI : `--out` égal à l'entrée sans `--force` : refus, fichier intact (Task 2,
   `out_equal_to_input_is_refused_without_force`).
3. Signature sur un fichier dont l'en-tête est `%PDF-1.4` : catalogue `/Version /1.6`, pyHanko
   valide toujours (Task 7, `old_header_is_upgraded_in_the_signed_revision`).
4. Changement de langue pendant qu'un message est affiché : le message disparaît, pas de
   mélange de langues (Task 11, étape CDP `langue`).
5. Élagage maison sur un fichier à références cycliques : même ensemble d'objets que lopdf,
   sans boucle (Task 6, `prune_matches_lopdf_on_cycles`).

---

### Task 1 — Caviardage : popups et réponses orphelins (B20)

**Files:** Modify `crates/mega-core/src/cleanup.rs` (`drop_orphan_popups(doc, page_id)` :
retire de `/Annots` tout `/Subtype /Popup` dont `/Parent` n'est plus dans `/Annots` ; retire
la clé `/IRT` qui vise une annotation absente), `annotations/mod.rs` (`delete_annotation`
supprime aussi le popup de l'annotation, via son `/Popup`), `testing.rs`
(`note_with_popup_pdf(secret)` : note `/Text` dans la zone, popup hors zone, réponse `/IRT`).

- [ ] Tests (RED) : `redacted_note_does_not_survive_through_its_popup` (caviarder la zone de
  la note, `save_as`, aucune chaîne de `lopdf::Document::load_mem` du résultat ne contient
  le secret) ; `deleting_a_note_deletes_its_popup` (nombre d'annotations après suppression
  = avant − 2) ; `reply_to_a_removed_note_loses_its_irt`.
- [ ] Implémenter ; commit `fix(core): orphan popups and replies no longer keep redacted notes (B20)`.

### Task 2 — CLI (B16, B30, B23, B32, C10, C42)

**Files:** Modify `crates/mega-cli/src/cli.rs` (`force: bool` sur `rotate`, `flatten`,
`extract`, `redact`, `merge`, `content`, `ocr`, `sign` et toute commande à `--out` ;
`redact --pages <spec>` remplace `--page` ; `compare --password-b`), `src/commands/mod.rs`
(`fn ensure_writable(out: &Path, force: bool) -> anyhow::Result<()>` ; `fn warn_if_signed(doc)`
sur stderr), `rotate.rs`/`flatten.rs` (`doc.save_as`), `extract.rs` (`cleanup::before_save`),
`sign.rs` (`rpassword::prompt_password` si `stdin` est un terminal), `Cargo.toml` (`rpassword`).

- [ ] Tests (RED, `tests/cli.rs`) : `existing_out_is_refused_without_force` (code 1, message
  « --force ») ; `out_equal_to_input_is_refused_without_force` (octets de l'entrée inchangés) ;
  `force_allows_overwrite` ; `rotate_output_is_cleaned` (pas de `PDFIUM_HasGeneratedAP` ni
  d'objet orphelin : nombre d'objets = celui de `save_as`) ; `rewriting_a_signed_file_warns`
  (stderr contient « signature ») ; `redact_accepts_a_page_range` (`--pages 1-2`, rapport
  cumulé) ; `compare_accepts_two_passwords`.
- [ ] Implémenter ; README (section CLI : `--force`) ; commit
  `feat(cli): refuse overwriting without --force; redact --pages; compare --password-b (B16 B30 B23 B32 C10 C42)`
  avec la raison de `rpassword` (saisie sans écho, multiplateforme, sans dépendance native).

### Task 3 — Polices et contenu (B24, C46, B26, B22, B29, B31)

**Files:** Create `crates/mega-core/src/fonts.rs` (`pub(crate) const HELVETICA_WIDTHS: [u16; 224]`
pour les octets 32..=255 WinAnsi, AFM Adobe ; `pub(crate) fn helvetica_width(byte: u8) -> u16`
(0 hors table) ; `pub(crate) fn text_width(bytes: &[u8], size: f32) -> f32`). Modify
`overlay.rs` (`/FirstChar 32 /LastChar 255 /Widths` dans les deux dictionnaires de police ;
`measure_width` remplacé par `fonts::text_width` pour les mots OCR, plus de police PDFium
par appel), `content/mod.rs` (`set_object_text` refuse un caractère absent du texte
d'origine si la police est incorporée ou sous-ensemble `ABCDEF+`), `overlay.rs`
(`/MediaBox` lue avec héritage via `lopdf`), `edit.rs` (`EditCommand::ResizeObject { page,
index, rect: Rect }` en espace affiché, matrice échelle + translation), `content/mod.rs`
(`Document::resize_object`), Tauri `commands.rs` + `api.ts` (variante sérialisée),
`ObjectLayer.svelte` (poignée de coin).

- [ ] Tests (RED) : `helvetica_widths_match_the_afm` (« A » 667, espace 278, « i » 222,
  « W » 944, 0xE9 « é » 556) ; `overlay_fonts_declare_widths` ; `ocr_word_box_matches_advance_width`
  (un mot « I » : largeur `Tz` × avance = largeur de la boîte à 0,5 pt près) ;
  `editing_text_with_a_new_glyph_in_a_subset_font_is_refused` ; `inherited_media_box_is_read`
  (fixture `/MediaBox` sur `/Pages` seulement, filigrane centré) ; `resize_object_scales_an_image`
  (bornes = rect demandé à 0,5 pt, page tournée comprise) ; `resize_object_is_undoable`.
- [ ] Étape CDP `contenu.json` : glisser la poignée d'une image insérée, sa taille change.
- [ ] Implémenter ; commit `feat(core): Helvetica widths, OCR advance widths, object resize (B24 C46 B26 B22 B29 B31)`.

### Task 4 — Annotations (B1, B3, B5, B9, B2, B25)

**Files:** Modify `annotations/mod.rs` (`NewAnnotation.created: Option<String>` porté par
`to_new` ; `set_meta` écrit `/M` = maintenant et garde `/CreationDate` ; `validate` :
`content::win_ansi_byte` pour `Text`/`Stamp`/`FreeText`, `is_finite` partout), `appearance.rs`
(`wrap_lines(text, width, size) -> Vec<String>` mesuré par `fonts::helvetica_width` de la
Task 3), `cleanup.rs` (`/NM` = `mega-<page>-<index>-<hash>` et `/P`
si absents ; `/InkList` : Bézier en 8 segments, `/Matrix` appliquée).

- [ ] Tests (RED) : `modifying_keeps_the_creation_date` ; `saved_annotations_have_nm_and_p` ;
  `non_win_ansi_note_text_is_refused` (« Ω ») ; `nan_and_infinite_values_are_refused` ;
  `stamp_text_wraps_to_the_frame` (chaque ligne mesurée ≤ largeur) ; `ink_curves_are_sampled`
  (`testing::bezier_ink_pdf` : `/InkList` > 2 points par courbe, points au milieu de la courbe
  à 1 pt près) ; `ink_matrix_is_applied`.
- [ ] Implémenter ; commit `fix(core): annotation dates, ids, validation, wrapping, ink curves (B1 B3 B5 B9 B2 B25)`.

### Task 5 — Formulaires (B17, B18, B15)

**Files:** Modify `forms/mod.rs` (`checked_state(dict) -> bool` lu par lopdf : `/AS` sinon `/V`,
différent de `/Off` ; remplace `is_checked()`), `forms/write.rs` (`set_field_value` refuse un
texte hors WinAnsi si la police de `/DA` est simple et non incorporée),
`forms/signature.rs` (`last_image_bounds` sous `#[cfg(feature = "testing")]`), `testing.rs`
(`checkbox_without_ap_pdf()`, case « On »).

- [ ] Tests (RED) : `checkbox_without_appearance_reads_its_value` ; `on_state_other_than_yes_reads_checked` ;
  `non_win_ansi_field_text_is_refused_with_a_simple_font`.
- [ ] Implémenter ; commit `fix(core): checkbox state, field text encoding (B17 B18 B15)`.

### Task 6 — Métadonnées, sécurité, pièces jointes, optimisation, export (C5-C23, C44)

**Files:** Modify `properties.rs` (XMP : `update_xmp(stream, &Metadata)` remplace le texte des
éléments présents ; `/Info` direct déplacé en objet indirect ; contrôles C0 refusés ;
`/ModDate` lu sur `file_bytes` si `!changed`, avec le mot de passe sinon ;
`decode_text_string` : table PDFDocEncoding 0x18-0x1F, 0x80-0x9F, 0xAD, et UTF-8 + BOM),
`cleanup.rs` (`/ModDate` = maintenant dans `before_save` hors chiffrement ;
`reachable_objects(doc) -> BTreeSet<ObjectId>` + `prune(doc)` remplacent `prune_objects`),
`security.rs` (`user_password_required` = l'ouverture sans mot de passe échoue ; `/EFF` ≠
`/StmF` → `CoreError::InvalidOperation` « chiffrement des seules pièces jointes : non pris en
charge » avant toute réécriture lopdf ; `/Encrypt` lu sur `file_bytes`), `attachments.rs`
(`File::open` + `take(MAX + 1)`), `optimize.rs` (image réduite gardée si plus petite ; Flate
sans recompression ; rapport « estimation »), `export.rs` (JFIF `set_pixel_density`, PNG
`pHYs`), `strings.ts` (« environ »).

- [ ] Tests (RED) : `set_metadata_updates_xmp_title` ; `direct_info_keeps_its_keys` ;
  `control_characters_in_metadata_are_refused` ; `save_sets_mod_date` ;
  `mod_date_of_a_user_password_file_is_read` ; `owner_only_file_does_not_require_a_password`
  (`owner-only.pdf` → faux ; `encrypted.pdf` → vrai) ; `attachment_only_encryption_is_refused` ;
  `pdfdoc_encoding_bullet_and_euro` (0x80 → « • », 0xA0 → « € ») ; `utf8_bom_text_is_decoded` ;
  `oversized_attachment_is_refused_after_one_read` ; `bigger_resized_image_is_not_kept` ;
  `uncompressed_image_is_flated_without_recompression` ; `prune_matches_lopdf_on_cycles` ;
  `cmyk_pages_compare` ; `exported_png_and_jpeg_carry_the_dpi` (300 dpi → pHYs 11811 px/m,
  JFIF densité 300).
- [ ] Implémenter ; commit `fix(core): XMP, ModDate, encodings, EFF, optimisation, export dpi (C5-C23 C44)`.

### Task 7 — Signatures (C34-C38, C43, C45)

**Files:** Modify `digital_signature/pdf.rs` (`/ByteRange` : exactement 4 entiers ; `/Perms
/UR3` → entrée `Unsupported` ; `sign_to` refuse quand lopdf échoue et que `/ByteRange` est
présent ; widget `/AP << /N 0 0 R >>` (XObject vide `/BBox [0 0 0 0]`) ; `/NeedAppearances
false` dans l'AcroForm recopié ; second `/ID` nouveau (SHA-256 de l'heure et de la taille) ;
`/Version /1.6` dans le catalogue si l'en-tête est antérieur ; remorque sans `DecodeParms`,
`Filter`, `W`, `Index`, `Length`, `Type`), `mod.rs` (`SignatureStatus::Invalid { reason:
SigIssue, detail }` / `Unsupported { … }`, `SigIssue` en `snake_case`), `cms.rs`
(`id-ecPublicKey` traité comme ECDSA-SHA256 ; `signing-certificate-v2` : SHA-256 du
certificat signataire comparé, différent → `Invalid`), `logic/signatures.ts` + `strings.ts`
(traduction de `SigIssue`), `SignaturesPanel.svelte` (clé = index).

- [ ] Tests (RED) : `byte_range_with_extra_entries_is_invalid` ; `ur3_is_reported_unsupported` ;
  `unreadable_signed_file_is_refused` ; `signed_widget_has_an_empty_appearance` ;
  `signed_revision_turns_need_appearances_off` ; `signed_revision_renews_the_second_id` ;
  `old_header_is_upgraded_in_the_signed_revision` ; `xref_stream_trailer_is_clean`
  (`simple-objstm.pdf`) ; `cades_signing_certificate_mismatch_is_invalid` ;
  `ec_public_key_algorithm_is_accepted` ; Vitest `signature verdict uses the issue code`.
- [ ] Revérifier `fixtures/signed.pdf` régénéré avec pyHanko et `qpdf --check`.
- [ ] Implémenter ; commit `fix(core): stricter signature reading and cleaner signed revisions (C34-C38 C43 C45)`.

### Task 8 — OCR (C39, C40, C41)

**Files:** Modify `ocr.rs` (`ocr_page_with` retourne `OcrOutcome { words: usize }` ; aucun
appel à `add_invisible_words` si 0 ; rendu sans annotations ; `search_candidates` ignore les
entrées vides ou relatives), `edit.rs` (`OcrPage` sans mot → `CoreError::NothingToDo`
traité par `apply` comme « aucune étape »), `service/mod.rs` (`languages_checked_once` : la
liste des langues est lue une fois par requête `OcrPages { pages }` côté frontend : appel
`ocr_status` avant la boucle), Tauri `set_tesseract_path(path: Option<String>)`,
`settings.svelte.ts` (`tesseractPath`), `PreferencesDialog.svelte` (champ + parcourir),
`OcrDialog.svelte` (erreur de `pagesWithText` affichée ; pages vides comptées à part).

- [ ] Tests (RED) : `page_without_words_adds_no_history_step` ; `ocr_ignores_annotations`
  (note FreeText « SECRET » : pas dans le texte reconnu ; ignoré sans Tesseract) ;
  `empty_path_entries_are_skipped` ; `ocr_on_a_rotated_scan_finds_the_words` (ignoré sans
  Tesseract) ; Vitest `ocrPages counts empty pages separately`.
- [ ] Implémenter ; commit `fix(ocr): empty pages, annotations, PATH entries, Tesseract path preference (C39-C41)`.

### Task 9 — Service, historique, cache (A4, A17, A18, A19, A24, A25, B11, B12)

**Files:** Modify `pages.rs` (`ensure_room(count: u16, added: usize) -> CoreResult<()>` dans
les deux insertions), `service/queue.rs` (`PartialEq`/`Eq` d'`Entry` sur (priorité,
séquence)), `service/mod.rs` (supprimer `impl PartialEq for Request` ; requêtes
`AllAnnotations`, `AllFormFields`), `service/cache.rs` (`BTreeMap<u64, PageKey>` d'usage),
`edit.rs` (redo borné en octets), `testing.rs` (`/ID` fixe dans `sample_pdf`), Tauri
`all_annotations` / `all_form_fields`, `documents.svelte.ts` (une requête au lieu d'une par page).

- [ ] Tests (RED) : `inserting_beyond_65535_pages_is_refused` ; `entries_compare_on_priority_and_sequence`
  (`a == a`) ; `touch_keeps_lru_order` (10 000 clés, ordre d'éviction) ;
  `sample_pdf_is_deterministic` (deux générations identiques) ; `redo_stack_is_bounded_in_bytes` ;
  `default_history_keeps_20_steps` (21 éditions → 20 annulations) ;
  `visible_requests_served_before_thumbnails` réécrit avec une requête de blocage de test
  (`Request::Hold(Receiver)` sous `cfg(feature = "testing")`) ; `all_annotations_lists_every_page`.
- [ ] Implémenter ; commit `fix(core): service ordering, cache, history bounds, batch reads (A4 A17-A19 A24 A25 B11 B12)`.

### Task 10 — Interface : logique pure (A13-A16, A20, A22, A23, B6, B10, B14, B27, C8)

**Files:** Create `app/src/lib/logic/zoom.ts` (`anchoredScroll(scroll, cursor, ratio) -> number`),
Modify `logic/virtualization.ts` (`slots(visible, prefetch) -> {index, priority, interactive}[]`),
`logic/pageselection.ts` (`slotAtY(y, tops, heights) -> number`), `logic/position.ts`
(`boundPositions(map, keep = 100)`), `logic/shortcuts.ts` (`shortcutLabel(id)`),
`state/documents.svelte.ts` (file d'éditions : `apply`/`undo`/`redo` enchaînés et dans
`inFlight` ; pas de mise en cache d'une liste vide sur erreur ; `exportImagesTo` renvoie
l'erreur), `Viewer.svelte` (un seul `{#each}`, zoom sous le curseur, molette aux bords en
page unique), `+page.svelte` (`aria-busy` + indicateur), `Thumbnails.svelte`, `AnnotationLayer.svelte`
(`rectFromCorners`), `AnnotationToolbar.svelte`, `ExportDialog.svelte`, `strings.ts` (`keyCtrl`).

- [ ] Tests Vitest (RED) : `anchoredScroll keeps the point under the cursor` ; `slots has one entry per page` ;
  `slotAtY picks the nearest gap` ; `boundPositions keeps the 100 most recent` ;
  `undo twice quickly undoes twice` (api simulée) ; `failed ensureObjects is not cached` ;
  `shortcut labels come from the shortcut table` ; `export error is returned`.
- [ ] Étapes CDP (`lecteur-simple.json`, `edition-pages.json`) : Ctrl+molette sur un point, le
  point reste sous le curseur ; molette en page unique → page suivante ; dépôt dans un
  interstice → position correcte ; double Ctrl+Z → deux annulations, aucun bandeau d'erreur.
- [ ] Implémenter ; commit `fix(app): zoom anchoring, page slots, drop gaps, edit queue, cached errors (A13-A23 B6 B10 B14 B27 C8)`.

### Task 11 — Accessibilité et langues (C24-C30 S, C32, B7, B8, B13)

**Files:** Modify `actions/trapFocus.ts` (`focusable(el)` pur : `:disabled`, un radio par
groupe), `state/settings.svelte.ts` (`lang` posé à l'initialisation ; `applyLanguage` efface
`notice` et `error`), `strings.ts` (`formatSize` localisé, `{title}`/`{titre}`, chaînes
restantes, clés inutilisées supprimées), `overlay.rs` + `logic/content.ts` (`{title}` accepté),
`logic/menu.ts` (Tab sans `preventDefault`, ArrowUp ouvre à la fin, saisie d'une lettre),
`FileMenu.svelte`/`ContentToolbar.svelte` (`aria-disabled`, `role="group"`, ▾ `aria-hidden`),
Create `actions/tabNav.ts` + `logic/tabs.ts` (`tabKey(key, index, n) -> number | null`)
appliqués à Sidebar, ExportDialog, SignatureDialog, Tabs, HelpDialog ; `position.ts`
(`ZoomMode` importé) ; `app.css` (règles `:focus-visible` fusionnées) ; état local de
Sidebar/OutlineItem/SearchBar déplacé hors du `{#key}` ; `AnnotationPanel.svelte` (liste de
boutons, `role="region"`), `FormPanel.svelte` (`aria-required`, focus du champ, focus de la
confirmation), `AnnotationLayer.svelte` (`z-index: 3` en sélection).

- [ ] Tests Vitest (RED) : `focusable skips disabled fieldsets and extra radios` ;
  `tabKey handles arrows, Home and End` ; `menuKey type-ahead` ; `ArrowUp opens on the last item` ;
  `formatSize is localised` ; `every string key is used` (balayage des `.svelte`/`.ts`) ;
  `numbering accepts {title}` (+ test Rust `numbering_accepts_title_placeholder`).
- [ ] Étapes CDP : changement de langue pendant une notice (la notice disparaît) ; flèches
  dans les onglets de la barre latérale ; l'onglet latéral survit au changement de langue.
- [ ] Implémenter ; commit `fix(app): focus trap, tabs and menus keyboard, language switch, panels a11y (C24-C32 B7 B8 B13)`.

### Task 12 — Distribution (C31, C33, A10)

**Files:** Modify `xtask/src/licenses.rs` (`--prod`), `Cargo.toml` racine (`serde_json` en
`[workspace.dependencies]`), `app/src-tauri/src/lib.rs` (`.build()?.run(|app, event|)`,
`RunEvent::Opened { urls }` sous `#[cfg(target_os = "macos")]` → événement `open-paths`
vers le frontend, `+page.svelte` l'écoute), `engine.rs` (`#[cfg(debug_assertions)]`),
`docs/FONCTIONNALITES.md`.

- [ ] Tests (RED) : `licenses_command_uses_prod` (xtask, argument construit) ;
  `opened_urls_become_paths` (fonction pure `file_paths(urls) -> Vec<PathBuf>`, URL `file://`
  avec espaces encodés).
- [ ] Implémenter ; commit `fix: production licences only, macOS Finder open, cfg build path (C31 C33 A10)`.

### Task 13 — Documentation et version

**Files:** `docs/apprentissage/lot-9.md`, fiche `concepts/complexite-et-collections.md`,
`docs/ROADMAP.md` (ligne Lot 9), `README.md` (CLI `--force`), `CLAUDE.md` (pièges nouveaux),
version 0.9.0 (Cargo, `package.json`, `tauri.conf.json`), `cargo xtask licenses`.

- [ ] `/verif` 8/8, 16 scénarios CDP ; commit `docs: Lot 9 learning note; version 0.9.0`.
