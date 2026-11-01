# Lot 14 — Compatibilité mesurée : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** corriger les 28 constats mesurés par l'audit `docs/audit/2026-10-31-compatibilite-pdf.md`
pour qu'un fichier enregistré par MegaUltraPDF soit exactement ce que l'utilisateur croit avoir
enregistré, et que le texte lu soit celui que lisent poppler, MuPDF et pdf.js.

**Architecture :** PDFium reste le moteur d'affichage et d'édition d'objets ; chaque perte qu'il
cause à l'enregistrement est rattrapée par une passe `lopdf` pure (fonctions sur
`lopdf::Document`, testées sans PDFium) branchée dans `cleanup::before_save` ou juste après une
copie de pages ; les opérations que PDFium fait mal (déplacement de pages, insertion d'image)
passent entièrement par `lopdf` puis `reload_from_bytes`. Le banc `tests/audit_compat.rs` fournit
les fixtures et les verdicts attendus ; chaque tâche convertit son expérience en test.

**Tech Stack :** Rust 2024, pdfium-render 0.9.4, lopdf 0.46, subsetter 0.2, Svelte 5 / TypeScript
strict (deux champs et deux codes d'erreur côté interface).

**Spec :** `docs/superpowers/specs/2026-10-31-lot-14-compatibilite-mesuree.md`

## Global Constraints

- `CLAUDE.md` : `#![forbid(unsafe_code)]`, aucun `unwrap`/`expect`/`panic!` hors tests, doc `///`
  française avec le **pourquoi** sur tout `pub`, `// [Apprentissage]` sur chaque concept nouveau.
- Toute lecture lopdf par `crypt::load_limited` / `LopdfSession`, tout flux par
  `crypt::stream_bytes` ; aucune récursion sur une structure venue du fichier (pile explicite,
  profondeur ≤ 64, ensemble des visités).
- Toute méthode publique qui modifie le document appelle `self.mark_changed()` en tête.
- Toute passe lopdf qui réécrit un document passe par `crypt::LopdfSession` (rechiffrement à
  l'identique) ; PDFium seulement pour mesurer, jamais sous un verrou lopdf.
- Chaînes de l'interface dans `app/src/lib/strings.ts`, dictionnaires `fr` **et** `en` ; types
  partagés dans `app/src/lib/api.ts` miroirs de `commands.rs` / `error.rs`.
- Nouvelle variante d'`EditCommand` ou nouveau chemin d'écriture ⇒ entrée dans
  `tests/conformance.rs` (`qpdf --check`, 0 WARNING).
- Commits Conventional Commits en anglais, `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.
- Une PR par partie (A à E), `/verif` 8/8 avec `MEGA_QPDF` posé avant chaque PR.

## Review Focus

1. Un document chiffré par mot de passe (AES-256) sur lequel on déplace une page : la passe lopdf
   doit rechiffrer à l'identique et PDFium rouvrir avec le même mot de passe → test dans la tâche A5.
2. Fusion de deux documents dont les champs de formulaire portent le même nom qualifié : le second
   doit être renommé, pas fusionné silencieusement → tâche A4.
3. Page déplacée dont le parent intermédiaire porte `/Rotate` ou `/MediaBox` hérités : la page doit
   garder sa rotation et sa taille une fois sous un autre parent → tâche A5.
4. Texte où une moitié haute de substitution n'est pas suivie d'une moitié basse (police cassée) :
   aucune panique, le caractère isolé est écarté → tâche B1.
5. Pièce jointe de page (`/FileAttachment`) sur un document chiffré par mot de passe : l'extraction
   doit déchiffrer le flux via `crypt::stream_bytes` → tâche B4.

---

## Partie A — Enregistrement sûr et fidèle

### Task 1 (A1): `cleanup::can_clean` et refus des suppressions non nettoyables

**Files:**
- Modify: `crates/mega-core/src/cleanup.rs` (après `before_save`, ligne 61)
- Modify: `crates/mega-core/src/annotations/mod.rs:429,641,664` (`update_annotation`, `delete_annotation`, `set_annotation_rect`)
- Modify: `crates/mega-core/src/pages.rs:49` (`delete_pages`), `crates/mega-core/src/content/mod.rs:296,314,340` (`delete_object`, `move_object`, `resize_object`)
- Modify: `crates/mega-core/src/testing.rs` (migrer `identity_encrypted_note_pdf` depuis `tests/audit_compat.rs`)
- Test: `crates/mega-core/tests/cleanup.rs`

**Interfaces:**
- Produces: `pub(crate) fn can_clean(bytes: &[u8], password: Option<&str>) -> bool` dans `cleanup.rs` ; `pub fn identity_encrypted_note_pdf(secret: &str) -> Vec<u8>` dans `testing.rs` ; `pub(crate) fn ensure_cleanable(&self) -> CoreResult<()>` sur `Document` (`document.rs`), qui appelle `can_clean(&self.to_bytes()?, self.password())` et renvoie `CoreError::InvalidOperation("ce fichier ne peut pas être nettoyé après une suppression (chiffrement ou structure que lopdf ne relit pas) : la suppression est refusée, l'ajout reste possible")`.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/cleanup.rs
#[test]
fn deleting_an_annotation_on_an_uncleanable_file_is_refused() {
    let mut doc = Document::from_bytes(testing::identity_encrypted_note_pdf("SECRET-ZX9"), None).unwrap();
    let err = doc.delete_annotation(0, 0).unwrap_err();
    assert!(matches!(err, CoreError::InvalidOperation(m) if m.contains("suppression est refusée")));
}
#[test]
fn adding_an_annotation_on_an_uncleanable_file_still_works() {
    let mut doc = Document::from_bytes(testing::identity_encrypted_note_pdf("x"), None).unwrap();
    assert!(doc.add_annotation(0, &testing::note_at(10.0, 10.0)).is_ok()); // helper existant ou NewAnnotation::note
}
#[test]
fn can_clean_distinguishes_identity_filters_from_plain_files() {
    assert!(!mega_core::cleanup::can_clean(&testing::identity_encrypted_note_pdf("x"), None)); // avant A2 ; devient vrai après A2 (passe-plat) : le test est alors inversé dans A2
    assert!(mega_core::cleanup::can_clean(&testing::sample_pdf(1, "t"), None));
}
```
(`can_clean` est exposée `pub` derrière `#[doc(hidden)]` uniquement pour ce test, comme `before_save`.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mega-core --test cleanup uncleanable can_clean`
Expected: FAIL (`can_clean` introuvable ; `delete_annotation` renvoie `Ok`)

- [ ] **Step 3: Implement**

`can_clean` = `crate::crypt::LopdfSession::load(bytes, password).is_ok()`. `ensure_cleanable` en tête (après `mark_changed`) de `delete_annotation`, `update_annotation`, `set_annotation_rect`, `delete_pages`, `delete_object`, `move_object`, `resize_object`. Doc `///` : pourquoi (audit E1 : `FPDF_SaveAsCopy` recopie tout, seul l'élagage lopdf retire ce qui est supprimé).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mega-core --test cleanup` et `cargo test -p mega-core` (aucune régression : les fixtures existantes sont toutes nettoyables)
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/mega-core/src/cleanup.rs crates/mega-core/src/document.rs crates/mega-core/src/annotations/mod.rs crates/mega-core/src/pages.rs crates/mega-core/src/content/mod.rs crates/mega-core/src/testing.rs crates/mega-core/tests/cleanup.rs
git commit -m "fix(core): refuse deletions when the save-time cleanup cannot run (audit E1)"
```

### Task 2 (A2): session lopdf passe-plat pour `/StmF /StrF Identity`

**Files:**
- Modify: `crates/mega-core/src/crypt.rs:104-205` (`check_crypt_filters`, `LopdfSession::from_loaded`, `finish`)
- Test: `crates/mega-core/tests/encrypted.rs`, `crates/mega-core/tests/cleanup.rs`

**Interfaces:**
- Produces: `LopdfSession` gagne un champ privé `passthrough: Option<(Object, Object)>` (`/Encrypt` d'origine et `/ID` d'origine) ; `finish` les remet dans `doc.trailer` avant `save_to` et n'appelle pas `encrypt` ; `check_crypt_filters` renvoie `Ok(Passthrough)` au lieu de refuser quand `/StmF` **et** `/StrF` valent `Identity` (ou sont absents) et que `/V` ≥ 4.
- Consumes: `identity_encrypted_note_pdf`, `testing::attachments_only_encrypted_pdf` (`testing.rs:1360`).

- [ ] **Step 1: Write the failing tests**

```rust
// tests/cleanup.rs
#[test]
fn deleted_note_does_not_survive_in_an_identity_encrypted_file() {
    let mut doc = Document::from_bytes(testing::identity_encrypted_note_pdf("SECRET-ZX9"), None).unwrap();
    doc.delete_annotation(0, 0).unwrap();
    let dir = tempfile::tempdir().unwrap(); let out = dir.path().join("o.pdf");
    doc.save_as(&out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(!contains(&bytes, b"SECRET-ZX9"));
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    assert!(lo.trailer.has(b"Encrypt"), "/Encrypt conservé");
}
// tests/encrypted.rs
#[test]
fn identity_filtered_file_keeps_its_encrypt_dictionary_and_attachment_bytes_after_watermark() {
    let input = testing::attachments_only_encrypted_pdf();
    let ef_before = embedded_file_stream_bytes(&input); // helper local : flux /EF /F brut via lopdf
    let mut doc = Document::from_bytes(input, None).unwrap();
    doc.add_watermark(&Watermark::text("x")).unwrap(); // constructeur existant de tests/overlay.rs
    let out = doc.to_bytes().unwrap(); let out = mega_core::cleanup::before_save(&out, None, None).unwrap();
    assert_eq!(embedded_file_stream_bytes(&out), ef_before);
    let lo = lopdf::Document::load_mem(&out).unwrap();
    assert_eq!(lo.trailer.get(b"ID").unwrap(), lopdf::Document::load_mem(&testing::attachments_only_encrypted_pdf()).unwrap().trailer.get(b"ID").unwrap());
}
```
Mettre à jour `can_clean_distinguishes…` (A1) : `identity_encrypted_note_pdf` devient nettoyable. Le test `tests/encrypted.rs:263` (« Err ou Ok ») devient `Ok` strict.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mega-core --test cleanup identity --test encrypted identity`
Expected: FAIL (`before_save` prend deux arguments ; chaîne présente ; filigrane refusé)

- [ ] **Step 3: Implement** le mode passe-plat dans `crypt.rs` : enum privée `FilterCheck { Standard, Passthrough }` renvoyée par `check_crypt_filters` ; en `Passthrough`, `from_loaded` garde `doc.trailer["Encrypt"]` déréférencé et `/ID` ; `finish` : si `passthrough`, réinsère l'objet `/Encrypt` (nouvel id) et `/ID` **avant** `save_to`, sans `encrypt`. `prune` ne doit pas élaguer `/Encrypt` : il est dans la remorque, donc atteignable. Signature de `before_save` : voir A3 (troisième paramètre) — faire A3 d'abord si plus simple, sinon ajouter `None` ici.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mega-core`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(core): rewrite Identity-filtered encrypted files through a pass-through lopdf session (audit E1, L16)"
```

### Task 3 (A3): version d'en-tête d'origine conservée

**Files:**
- Modify: `crates/mega-core/src/crypt.rs` (nouvelle fonction pure), `crates/mega-core/src/cleanup.rs:28` (`before_save`), `crates/mega-core/src/document.rs` (champ `file_version`), `crates/mega-core/src/edit.rs:823` (`rewritten_bytes`), `crates/mega-core/src/pages.rs:142,212` (`extract_pages`, `merge`), `crates/mega-core/src/security.rs:256`, `crates/mega-core/src/digital_signature/pdf.rs` (réécriture avant signature), `crates/mega-cli/src` (appels de `before_save` s'il y en a), `crates/mega-core/src/testing/fuzz.rs:116`
- Test: `crates/mega-core/src/crypt.rs` (`#[cfg(test)]`), `crates/mega-core/tests/cleanup.rs`, `crates/mega-core/tests/pages.rs`

**Interfaces:**
- Produces: `pub(crate) fn header_version(bytes: &[u8]) -> Option<(u8, u8)>` (`crypt.rs`) ; `pub fn before_save(bytes: &[u8], password: Option<&str>, min_version: Option<(u8, u8)>) -> CoreResult<Vec<u8>>` ; `pub(crate) fn file_version(&self) -> Option<(u8, u8)>` sur `Document` (mis à jour par `from_bytes`, `save_as`, `reload_from_bytes` **non** : la version du fichier est celle des `file_bytes`).
- Règle : la version écrite est `max(version lue dans bytes, min_version)` ; le repli brut recopie les octets 5..8 de `%PDF-M.n` quand `min_version` est supérieure.

- [ ] **Step 1: Write the failing tests**

```rust
// crypt.rs #[cfg(test)]
#[test] fn header_version_reads_major_minor_after_a_prefix() {
    assert_eq!(header_version(b"%PDF-2.0\n%\xE2\xE3"), Some((2, 0)));
    assert_eq!(header_version(b"junk\n%PDF-1.4\n"), Some((1, 4)));
    assert_eq!(header_version(b"nope"), None);
}
// tests/cleanup.rs
#[test] fn before_save_keeps_a_2_0_header() {
    let input = testing::rich_catalog_pdf("2.0", None); // migré en A4 ; ici : testing::sample_pdf réécrit en 2.0 via lopdf `doc.version = "2.0"`
    let doc = Document::from_bytes(input, None).unwrap();
    let out = mega_core::cleanup::before_save(&doc.to_bytes().unwrap(), None, doc_file_version(&doc)).unwrap(); // via save_as en pratique
    assert!(out.starts_with(b"%PDF-2.0"));
}
// tests/pages.rs
#[test] fn rotate_merge_extract_and_protect_keep_the_2_0_header() { /* save_as après rotate → %PDF-2.0 ; merge(2.0, 1.5) → 2.0 ; extract_pages → 2.0 ; protect → 2.0 */ }
#[test] fn catalog_version_entry_survives_a_rewrite() { /* 1.4 + /Version /1.7 → sortie : en-tête 1.4, /Version 1.7 */ }
```

- [ ] **Step 2: Run** `cargo test -p mega-core header_version 2_0_header catalog_version` → FAIL

- [ ] **Step 3: Implement** : `header_version` (recherche de `%PDF-` dans les 1024 premiers octets, deux chiffres) ; `before_save` pose `session.doc.version = format!("{M}.{n}")` ; `Document::file_version` lit `header_version(&self.file_bytes)` ; `rewritten_bytes` passe `self.file_version()` ; `merge` passe `docs.iter().filter_map(file_version).max()` ; `extract_pages`/`split` passent `self.file_version()` ; `protect` et la réécriture de `sign_to` idem. `// [Apprentissage]` : comparaison lexicographique dérivée sur les tuples.

- [ ] **Step 4: Run** `cargo test --workspace` → PASS (mettre à jour les appels CLI/fuzz).

- [ ] **Step 5: Commit** `fix(core): keep the original PDF header version on save (audit E2)`

### Task 4 (A4): report du catalogue après `merge`, `extract_pages`, `split`

**Files:**
- Create: `crates/mega-core/src/pages/carry.rs` ; `pages.rs` devient `pages/mod.rs` (déplacement sans changement)
- Modify: `crates/mega-core/src/pages/mod.rs:128-143,201-213`
- Modify: `crates/mega-core/src/testing.rs` (migrer `rich_catalog_pdf(header, catalog_version)` du banc)
- Test: `crates/mega-core/src/pages/carry.rs` (`#[cfg(test)]`, pur lopdf), `crates/mega-core/tests/pages.rs`, `crates/mega-core/tests/conformance.rs`

**Interfaces:**
- Produces (toutes `pub(crate)`, dans `carry.rs`) :
  - `struct PageMap { pairs: Vec<(ObjectId, ObjectId)> }` avec `fn by_index(src: &LoDocument, dst: &LoDocument, src_pages: &[u16]) -> PageMap` (la i-ème page de `dst` ↔ la page `src_pages[i]` de `src`) et `fn target(&self, src: ObjectId) -> Option<ObjectId>`.
  - `fn clone_object(src: &LoDocument, dst: &mut LoDocument, obj: &Object, map: &PageMap, cache: &mut HashMap<ObjectId, ObjectId>) -> Object` : clone récursif à pile explicite ; une référence vers une page source devient la page cible ou `Null` si elle n'est pas copiée.
  - `fn carry_simple(src, dst, map, cache)` : clés `Lang`, `ViewerPreferences`, `PageMode`, `PageLayout`, `Metadata`, `OutputIntents`, `MarkInfo`, et `Names/EmbeddedFiles` (jamais `Names/JavaScript`, `OpenAction`, `AA`).
  - `fn carry_acroform(src, dst, map, cache, suffix: Option<&str>)` : `/DA`, `/DR`, `/NeedAppearances`, `/Fields` filtré aux champs dont un widget (`/Kids` ou lui-même) a une page dans `map` ; chaque widget de `dst` dont le `/Parent` d'origine est connu reçoit la référence clonée ; `suffix` ajouté à `/T` des champs de premier niveau en cas de collision avec un nom déjà présent dans `dst`.
  - `fn carry_outlines(src, dst, map, cache)`, `fn carry_named_dests(src, dst, map, cache)`, `fn carry_page_labels(src, dst, src_pages: &[u16])`, `fn carry_oc_properties(src, dst, cache)`, `fn strip_struct_parents(dst)`.
  - `pub(crate) fn carry_catalog(src: &LoDocument, dst: &mut LoDocument, src_pages: &[u16], field_suffix: Option<&str>)` : enchaîne tout.
- Consumes: `crypt::load_limited`, `cleanup::before_save(…, min_version)` (A3).

- [ ] **Step 1: Write the failing tests** (pur lopdf, `carry.rs`) :

```rust
#[test] fn carry_catalog_keeps_the_seventeen_structures_of_rich_catalog() { /* src = rich_catalog_pdf("1.7", None) chargé ; dst = copie PDFium des 3 pages (fixture `rich_catalog_copied_by_pdfium` produite par Document::extract_pages(&[0,1,2]) AVANT la passe, écrite une fois par gen-fixtures) ; après carry_catalog : has_key_path pour Names/EmbeddedFiles, Outlines, AcroForm, Dests, Names/Dests, PageLabels, Lang, ViewerPreferences, PageMode, Metadata, OutputIntents, OCProperties, MarkInfo ; absents : Names/JavaScript, OpenAction, AA, StructTreeRoot ; aucune page ne porte /StructParents */ }
#[test] fn widgets_parent_points_to_a_field_after_merge() { /* form_parent_kid_pdf + simple ; chaque /Widget : /Parent → dict avec /T ou /FT ; /AcroForm /Fields non vide */ }
#[test] fn outlines_to_dropped_pages_are_removed_and_the_rest_renumbered() { /* extract [1] : signet « Vers chap2 » → page 0, les deux autres retirés, /Count = 1 */ }
#[test] fn duplicate_field_names_get_a_suffix_on_merge() { /* merge(form_pdf, form_pdf) : "nom" et "nom (2)" */ }
#[test] fn clone_object_handles_cycles_without_overflow() { /* dict qui se référence ; profondeur bornée */ }
```
Intégration `tests/pages.rs` : `merge` de deux `rich_catalog_pdf` → `Document::outline()` 6 nœuds, `attachments()` 2, `form_fields` 2 ; `extract_pages(&[1])` → pièce jointe conservée ; `tests/conformance.rs` : `merge`, `split`, `extract` ajoutés à `assert_conforms`.

- [ ] **Step 2: Run** `cargo test -p mega-core carry` → FAIL (module absent)

- [ ] **Step 3: Implement** `carry.rs` ; dans `pages/mod.rs`, `merge` : pour chaque source dans l'ordre, `carry_catalog(src_lo, &mut dst_lo, pages_of_src, suffix_if_collision)` après `append`, sur `load_limited(target.save_to_bytes())`, puis `before_save` ; `extract_pages` : `carry_catalog(self_lo, dst_lo, pages, None)`. Les sources chiffrées sont lues avec leur mot de passe (`load_limited(self.file_bytes(), self.password())`). `// [Apprentissage]` : clone avec table de correspondance et ensemble des visités.

- [ ] **Step 4: Run** `cargo test --workspace` ; `MEGA_QPDF=… cargo test -p mega-core --test conformance` → PASS

- [ ] **Step 5: Commit** `feat(core): carry outlines, forms, attachments, labels, layers and metadata through merge, extract and split (audit E3)`

### Task 5 (A5): `move_pages` par réordonnancement `/Kids` et destinations orphelines après `delete_pages`

**Files:**
- Modify: `crates/mega-core/src/pages/mod.rs:106-122` ; Create: `crates/mega-core/src/pages/reorder.rs`
- Modify: `crates/mega-core/src/cleanup.rs` (nouvelle passe), `docs/adr/0007-edit-commands-memento-atomic-save.md` (limite levée)
- Test: `reorder.rs` (`#[cfg(test)]`), `crates/mega-core/tests/pages.rs`, `crates/mega-core/tests/cleanup.rs`

**Interfaces:**
- Produces: `pub(crate) fn reorder_kids(doc: &mut LoDocument, pages: &[u16], to: u16) -> CoreResult<()>` (pure) ; `pub(crate) fn neutralize_dead_destinations(doc: &mut LoDocument)` dans `cleanup.rs`, appelée par `before_save` : signets, liens (`/Dest`, `/A /S /GoTo /D`), `/Names /Dests`, `/Dests` dont la page n'est pas dans `get_pages()` perdent `/Dest`/`/A` (signet conservé, lien retiré de `/Annots`).
- `Document::move_pages` : `mark_changed` ; `LopdfSession::load(&self.to_bytes()?, self.password())` ; `reorder_kids` ; `finish` ; `reload_from_bytes`.

- [ ] **Step 1: Write the failing tests**

```rust
// reorder.rs
#[test] fn reorder_kids_moves_a_page_and_keeps_counts() { /* deep_pages_pdf-like à deux niveaux ; pages [0] → 2 : get_pages() ordre, /Count racine et intermédiaires */ }
#[test] fn reorder_kids_copies_inherited_attributes_before_reparenting() { /* /Rotate 90 et /MediaBox sur l'ancien parent → présents sur la page après déplacement sous un parent sans */ }
// tests/pages.rs
#[test] fn moving_a_page_keeps_its_links_tagging_and_the_bookmarks_to_it() {
    // rich_catalog_pdf ; move_pages(&[0], 2) ; save_as ; rouvrir :
    // links(2).len() == 3 ; annotations(2).len() == 4 ; outline() : "Vers chap2" → Some(0), "Vers chap3" → Some(1), "Direct p3" → Some(1)
    // lopdf : page 2 a /StructParents 0 et son contenu contient "BDC" ; qpdf --check 0 warning (conformance)
}
#[test] fn moving_a_page_in_an_aes256_document_keeps_the_password() { /* encrypted.pdf, "secret" ; move_pages ; save_as ; rouvre avec "secret", refuse sans */ }
// tests/cleanup.rs
#[test] fn bookmarks_to_a_deleted_page_lose_their_destination_but_remain() { /* rich ; delete_pages(&[1]) ; outline()[0].page == None, 3 signets ; aucun /Type /Page hors arbre */ }
```

- [ ] **Step 2: Run** → FAIL

- [ ] **Step 3: Implement** ; retirer `copy_pages`+snapshot de `move_pages` ; ADR-0007 : paragraphe « signets perdus » remplacé par « déplacement par lopdf depuis le Lot 14 ».

- [ ] **Step 4: Run** `cargo test --workspace` → PASS (`tests/pages.rs` existants sur `move_pages` inchangés)

- [ ] **Step 5: Commit** `fix(core): move pages by reordering /Kids so bookmarks, links and tagging survive; neutralize destinations to deleted pages (audit E4)`

### Task 6 (A6): MCID orphelins et `/Info` sur PDF/A-4

**Files:**
- Modify: `crates/mega-core/src/cleanup.rs` (nouvelle passe), `crates/mega-core/src/properties.rs:327` (`stamp_mod_date`)
- Modify: `crates/mega-core/src/testing.rs` (migrer `two_marked_objects_pdf`, avec `/StructTreeRoot` + `/ParentTree` ajoutés)
- Test: `crates/mega-core/tests/cleanup.rs`, `crates/mega-core/tests/metadata.rs`

**Interfaces:**
- Produces: `pub(crate) fn prune_orphan_mcids(doc: &mut LoDocument)` (appelée par `before_save` seulement si `/StructTreeRoot` existe) ; `stamp_mod_date(doc)` : si `pdfa_claim_of(doc)` commence par `"PDF/A-4"` et que `/Info` est absent, ne rien créer.

- [ ] **Step 1: Tests** : `deleting_one_of_two_marked_objects_prunes_its_mcid_from_the_structure_tree` (après `delete_object(0,0)` + `save_as` : `/K` de l'élément ne cite plus `0`, cite encore `1`) ; `pdfa_4_documents_do_not_receive_an_info_dictionary` (`testing::pdfa_pdf` modifié en partie 4 sans `/Info` → sortie sans `/Info`, `xmp:ModifyDate` mis à jour).
- [ ] **Step 2: Run** → FAIL
- [ ] **Step 3: Implement** (pile explicite sur `/K`, profondeur ≤ 64, ensemble des visités).
- [ ] **Step 4: Run** → PASS
- [ ] **Step 5: Commit** `fix(core): prune orphan MCIDs after content edits; no /Info on PDF/A-4 (audit E5, E6)`

### Task 7 (A7): PR A — vérification, relecture, fusion

- [ ] `/verif` 8/8 (`MEGA_QPDF` posé) ; `bash scripts/audit/write-bench.sh fixtures/corpus/pdfa fixtures/corpus/formulaires` : veraPDF `PASS` conservé sur `pdfa-4-*` après rotation et métadonnées.
- [ ] Agent `rust-reviewer` sur `crates/mega-core/src/{cleanup.rs,crypt.rs,pages/}` ; corrections test d'abord.
- [ ] Résumé « Ce que vous venez de voir » ; PR « Lot 14 (A): safe and faithful saving » ; CI 3 OS ; fusion.

## Partie B — Texte extrait et annotations tierces

### Task 8 (B1): paires de substitution

**Files:**
- Modify: `crates/mega-core/src/text.rs:110-135` (`drawn_chars`)
- Modify: `crates/mega-core/src/testing.rs` (nouveau `astral_text_pdf()` : Type0/Identity-H sur DejaVu via `fonts::unicode::Type0Writer` **ne suffit pas** (DejaVu n'a pas U+1D70B) → police Type0 synthétique avec `/ToUnicode` mappant CID 1 → `<D835DF0B>` et glyphe 0, texte invisible `3 Tr` ; PDFium renvoie les deux moitiés)
- Test: `crates/mega-core/tests/document.rs`

**Interfaces:**
- Produces: `drawn_chars` apparie `unicode_value()` ∈ `D800..=DBFF` suivi de `DC00..=DFFF` en un `DrawnChar` dont `rect` est l'union ; moitié isolée écartée. Fonction pure `pub(crate) fn pair_surrogates(units: &[(u32, Rect)]) -> Vec<(char, Rect)>` dans `text.rs`, testée en `#[cfg(test)]`.

- [ ] **Step 1: Tests** : `pair_surrogates_recomposes_a_pair_and_drops_a_lone_half` (unitaire : `[(0xD835,r1),(0xDF0B,r2)]` → `[('𝜋', r1∪r2)]` ; `[(0xD835,r1),(0x41,r2)]` → `[('A', r2)]`) ; `tests/document.rs::astral_characters_are_extracted_searchable_and_copied` (`page_text` contient `𝜋`, `search_page("𝜋")` = 1, aucun `TextBox.ch` entre U+D800 et U+DFFF sur `edge-article.pdf` du corpus si `MEGA_CORPUS`).
- [ ] **Step 2: Run** → FAIL
- [ ] **Step 3: Implement** ; `// [Apprentissage]` : pourquoi `char::from_u32(0xD835)` est `None`.
- [ ] **Step 4: Run** `cargo test -p mega-core` → PASS (ordre de lecture inchangé : `tests/document.rs` existants)
- [ ] **Step 5: Commit** `fix(core): recompose UTF-16 surrogate pairs in extracted text (audit L1)`

### Task 9 (B2): espaces de mots en écriture de droite à gauche

**Files:**
- Modify: `crates/mega-core/src/reading_order.rs` (nouvelle fonction pure), `crates/mega-core/src/text.rs:103` (`reading_chars`)
- Modify: `crates/mega-core/src/testing.rs` (`rtl_gaps_pdf()` : Type0 DejaVu, deux mots arabes espacés par un `Td`, sans glyphe espace)
- Test: `reading_order.rs` (`#[cfg(test)]`), `crates/mega-core/tests/document.rs`

**Interfaces:**
- Produces: `pub fn word_gaps(line: &[usize], rects: &[Rect], chars: &[char]) -> Vec<usize>` : positions (dans `line`) après lesquelles insérer un espace, quand la ligne n'a aucun `' '` et que l'écart horizontal entre deux voisins dépasse `0.25 × hauteur médiane` ; `reading_chars` insère un `DrawnChar { ch: ' ', rect: blanc }` à ces positions.

- [ ] **Step 1: Tests** : unitaire `word_gaps_finds_a_gap_wider_than_a_quarter_em_only_when_the_line_has_no_space` (deux cas) ; `tests/document.rs::arabic_words_are_separated_by_spaces` (`page_text` contient un espace entre les deux mots ; `search_page` du second mot = 1) ; corpus `ArabicCIDTrueType.pdf` sous `MEGA_CORPUS` : ≥ 10 espaces.
- [ ] **Step 2–5** : FAIL → implement → PASS → `fix(core): synthesize word spaces on right-to-left lines (audit L3)`

### Task 10 (B3): police CFF CID OpenType sans texte (diagnostic puis correction)

**Files:**
- Modify: `crates/mega-core/src/testing.rs` (`opentype_cff_cid_pdf()` : un programme CFF minimal d'un glyphe enveloppé OpenType, `/FontFile3 /Subtype /OpenType`, `/ToUnicode` CID 1 → « 除 »)
- Modify selon le diagnostic : `crates/mega-core/src/text.rs` (repli) ou note seulement
- Test: `crates/mega-core/tests/document.rs`

- [ ] **Step 1: Test** `opentype_cff_cid_text_is_extracted` : `page_text(0) == "除"`.
- [ ] **Step 2: Run** → FAIL attendu (reproduit L2) ; **si PASS** : la cause est ailleurs (le fichier du corpus) → mesurer `Embedded_font.pdf` par `[L19]` avec `loose_bounds` ; consigner.
- [ ] **Step 3: Implement** le repli `pub(crate) fn tounicode_fallback(&self, index: u16) -> CoreResult<Vec<DrawnChar>>` (lopdf : `/ToUnicode` de chaque police de la page, chaînes `Tj`/`TJ` décodées par le CMap, position = matrice de texte courante × `Tm`, largeur par `/W` ou `/DW`), utilisé par `drawn_chars` **seulement** quand PDFium ne renvoie aucun caractère alors que le contenu a un `BT`. Borné : 20 000 caractères, profondeur de XObjects 8.
- [ ] **Step 4: Run** → PASS ; `cargo test -p mega-core` sans régression.
- [ ] **Step 5: Commit** `fix(core): fall back to /ToUnicode when PDFium exposes no text for an OpenType CFF CID font (audit L2)`

### Task 11 (B4): annotations tierces et pièces jointes de page

**Files:**
- Modify: `crates/mega-core/src/annotations/mod.rs:388-420,926-960`, `crates/mega-core/src/attachments.rs:26-155`
- Modify: `app/src/lib/api.ts:176` (`Attachment.page: number | null`), `app/src/lib/strings.ts` (`attachments.onPage`: fr « (page {n}) », en « (page {n}) »), composant de la liste des pièces jointes
- Modify: `crates/mega-core/src/testing.rs` (migrer `third_party_annots_pdf()`)
- Test: `crates/mega-core/tests/annotations.rs`, `crates/mega-core/tests/attachments.rs`, `app/src/lib/logic/*.test.ts` si une fonction pure de formatage est ajoutée

**Interfaces:**
- Produces: `Attachment { index, name, size, page: Option<u16> }` ; `attachments()` liste d'abord `/EmbeddedFiles` (index 0..n) puis les `/FileAttachment` de chaque page (index suivants, `page: Some(p)`) ; `save_attachment` et `delete_attachment` acceptent les deux ; `read_annotation` : `editable = annotation.to_new().is_some() && kind != FreeText source` (type PDFium `FreeText` → `kind: Text`, `editable: false`) ; `Popup` exclues de `annotations()`.

- [ ] **Step 1: Tests** : `third_party_free_text_is_shown_but_not_editable` (+ `update_annotation` → `InvalidOperation`) ; `popups_are_not_listed` ; `file_attachment_annotations_are_listed_with_their_page_and_extracted` (`attachments()` contient `page: Some(0)`, `save_attachment` écrit les octets attendus) ; `file_attachment_in_a_password_protected_document_is_extracted` (fixture chiffrée par qpdf dans `gen-fixtures` ou `testing` + `crypt::stream_bytes`) ; Vitest si formatage.
- [ ] **Step 2–5** : FAIL → implement → PASS → `feat(core): list page file attachments, keep third-party FreeText read-only, hide popups (audit L11)`

### Task 12 (B5): titre depuis le XMP et `/UserUnit`

**Files:**
- Modify: `crates/mega-core/src/properties.rs:45-69,175-200,225` ; `app/src/lib/api.ts:72` (`user_unit: number | null`) ; `app/src/lib/strings.ts` (`properties.physicalSize` fr « Taille physique » / en « Physical size », `properties.userUnit`) ; boîte Propriétés
- Modify: `crates/mega-core/src/testing.rs` (migrer `metadata_variant_pdf`, `user_unit_pdf`)
- Test: `crates/mega-core/tests/metadata.rs`, Vitest de la fonction pure `physicalSize(widthPt, heightPt, userUnit)` → « 2 100 × 2 970 mm »

- [ ] **Step 1: Tests** : `title_falls_back_to_xmp_dc_title_when_info_is_absent` (`"Ancien"` de `xmp_pdf`), idem `author`, `subject`, `keywords`, `creation_date` ; `user_unit_is_reported` (`Some(10.0)`, `None` sur `simple.pdf`) ; Vitest `physicalSize`.
- [ ] **Step 2–5** : FAIL → implement (`LopdfInfo` gagne `xmp_fallback: XmpFields`, `user_unit`) → PASS → `feat(core,ui): metadata fallback on XMP, /UserUnit reported in properties (audit L12, L13)`

### Task 13 (B6): PR B — vérification, relecture, fusion

- [ ] `/verif` 8/8 ; `bash scripts/audit/read-bench.sh fixtures/corpus/polices fixtures/corpus/producteurs` : similarité de texte ≥ 80 % avec deux moteurs sur `bug1937438_from_word`, `ArabicCIDTrueType`, `Embedded_font`.
- [ ] `rust-reviewer` + `ui-reviewer` ; résumé « Ce que vous venez de voir » ; PR « Lot 14 (B): extracted text and third-party annotations » ; fusion.

## Partie C — Ajouts respectueux de PDF/A et PDF/UA

### Task 14 (C1): artefacts, `/CIDSet`, groupe de transparence

**Files:**
- Modify: `crates/mega-core/src/overlay.rs:201-300,381-465,707-750` (`append_content`), `crates/mega-core/src/fonts/unicode.rs:182-282` (`Type0Writer::write`)
- Test: `crates/mega-core/tests/overlay.rs`, `crates/mega-core/tests/conformance.rs`

**Interfaces:**
- `append_content` enveloppe le flux ajouté dans `/Artifact BMC … EMC` (filigrane, numéros, OCR) ; `Type0Writer` écrit `/CIDSet` (flux bitmap des CID utilisés) dans le `/FontDescriptor` ; quand un ExtGState avec `/ca` < 1 est posé, la page reçoit `/Group << /S /Transparency /CS /DeviceRGB /I true >>` si absent.

- [ ] **Step 1: Tests** : `watermark_content_is_marked_as_artifact` (contenu contient `/Artifact BMC` et `EMC`) ; `unicode_subset_has_a_cidset` ; `translucent_watermark_adds_a_page_transparency_group` ; `conformance.rs` : avec `MEGA_VERAPDF` et `MEGA_CORPUS`, `pdfua-1-7-1-t01-pass-a.pdf` + filigrane opaque → `PASS` ua1 (sauté sinon).
- [ ] **Step 2–5** : FAIL → implement → PASS → `fix(core): mark overlays as artifacts, write /CIDSet, add a transparency group (audit E6)`

### Task 15 (C2): XMP écrit même absent, `/NeedAppearances` retiré, `/AP` des marquages

**Files:**
- Modify: `crates/mega-core/src/properties.rs:352-452` (`update_xmp` crée les propriétés manquantes dans `rdf:Description`), `crates/mega-core/src/cleanup.rs` (`drop_need_appearances`), `crates/mega-core/src/edit.rs:823` (`rewritten_bytes` : rendu 8 px des pages à marquage sans apparence avant `save_to_bytes`)
- Test: `crates/mega-core/tests/metadata.rs`, `crates/mega-core/tests/forms.rs`, `crates/mega-core/tests/annotations.rs`

- [ ] **Step 1: Tests** : `set_metadata_creates_dc_title_in_an_xmp_without_it` (XMP de `pdfa_pdf` sans `dc:title` → après : `dc:title` x-default = titre, et `/Info /Title` égal) ; `need_appearances_is_dropped_when_every_widget_has_an_appearance` (4 variantes du banc E3 → `/NeedAppearances` absent) ; `highlight_added_then_saved_has_an_appearance_stream` (`/AP /N` présent, contient `/Multiply` ou l'ExtGState `/BM`).
- [ ] **Step 2–5** : FAIL → implement → PASS → `fix(core): sync XMP on metadata edits, drop /NeedAppearances when appearances exist, write highlight appearances (audit E6, E7, E11)`

### Task 16 (C3): `/Extensions` au chiffrement AES-256, verdicts DocMDP et horodatage

**Files:**
- Modify: `crates/mega-core/src/security.rs:237-290`, `crates/mega-core/src/digital_signature/mod.rs:35-125`, `crates/mega-core/src/digital_signature/pdf.rs:319-322,449-471`
- Modify: `app/src/lib/api.ts:401` (`SigIssue` + `certified_then_modified`, `document_timestamp`), `app/src/lib/strings.ts` `sigIssues` fr/en (« Document certifié puis modifié » / « Certified document modified afterwards » ; « Horodatage de document (non vérifié) » / « Document timestamp (not verified) »)
- Modify: `crates/mega-core/src/testing.rs` (migrer `sig_subfilter_pdf`, ajouter `certified_then_modified_pdf()` = `certified_pdf` + révision incrémentale)
- Test: `crates/mega-core/tests/security.rs`, `crates/mega-core/tests/digital_signature.rs`, test Vitest existant sur la forme des dictionnaires (`strings.test.ts`)

- [ ] **Step 1: Tests** : `protect_declares_the_adobe_extension_level_and_a_1_7_header` (entrée 1.4 → en-tête `%PDF-1.7`, `/Extensions /ADBE /ExtensionLevel 8`) ; `a_certified_document_modified_afterwards_is_reported` (`Invalid(CertifiedThenModified)`) ; `rfc3161_subfilter_is_reported_as_a_document_timestamp` (`Unsupported(DocumentTimestamp)`).
- [ ] **Step 2–5** : FAIL → implement → PASS (`pnpm --dir app test` pour la symétrie fr/en) → `feat(core,ui): AES-256 extension declaration, DocMDP and timestamp verdicts (audit E8, E14)`

### Task 17 (C4): PR C — vérification, relecture, fusion

- [ ] `/verif` 8/8 ; `bash scripts/audit/write-bench.sh fixtures/corpus/pdfa fixtures/corpus/pdfua` puis veraPDF avant/après : `PASS` conservé sur 1b/2b/2u/3b/ua1 après filigrane opaque et métadonnées (le filigrane translucide sur PDF/A-1 reste `FAIL` : limite documentée).
- [ ] `rust-reviewer`, `ui-reviewer` ; résumé ; PR « Lot 14 (C): PDF/A and PDF/UA friendly additions » ; fusion.

## Partie D — Taille des fichiers

### Task 18 (D1): image insérée par lopdf (`/DCTDecode` tel quel, `/SMask` réel seulement)

**Files:**
- Modify: `crates/mega-core/src/content/image.rs:78-97` (`place`), `crates/mega-core/src/content/mod.rs:548-563` (`insert_image`)
- Test: `crates/mega-core/tests/content.rs`, `crates/mega-core/tests/conformance.rs` (JPEG déjà couvert ; ajouter PNG alpha)

**Interfaces:**
- `pub(crate) fn place(doc: &mut LoDocument, page: ObjectId, frame: &PageFrame, rect: Rect, image: &ImageData) -> CoreResult<()>` : `ImageData::Jpeg { bytes, width, height, gray: bool }` → XObject `/Filter /DCTDecode` octets d'origine ; `ImageData::Rgba(DynamicImage)` → Flate RGB + `/SMask` si un pixel a alpha < 255 ; flux `q cm /ImN Do Q` avec ressources propres à la page (recette `overlay::ensure_resources`), rectangle converti par `frame.to_user_space`. `insert_image` : `mark_changed`, `LopdfSession`, `place`, `finish`, `reload_from_bytes`. CMJN JPEG refusé comme aujourd'hui.

- [ ] **Step 1: Tests** : `inserted_jpeg_keeps_dct_encoding_and_has_no_smask` (taille sortie < 1,2 × JPEG + 3 Ko ; `/Filter /DCTDecode`) ; `inserted_png_with_alpha_gets_a_real_smask` ; `inserted_opaque_png_has_no_smask` ; `inserted_image_on_a_rotated_page_lands_at_the_displayed_rectangle` (rendu : pixels sombres dans `rect`, `tests/rotation.rs` pattern) ; conformance PNG alpha.
- [ ] **Step 2–5** : FAIL → implement → PASS → `perf(core): insert images through lopdf, JPEG bytes kept as DCT (audit E9)`

### Task 19 (D2): sous-ensemble des polices DejaVu incorporées par PDFium

**Files:**
- Modify: `crates/mega-core/src/cleanup.rs` (`subset_embedded_fonts`), `crates/mega-core/src/fonts/unicode.rs` (exposer `subset_tag`, `DEJAVU_SANS` empreinte)
- Test: `crates/mega-core/tests/content.rs`, `crates/mega-core/tests/annotations.rs`

**Interfaces:**
- `pub(crate) fn subset_embedded_fonts(doc: &mut LoDocument) -> CoreResult<()>` appelée par `before_save` après `dedupe_font_files` : pour chaque `/FontDescriptor` dont `/FontFile2` a la taille de `DEJAVU_SANS`, GID utilisés collectés dans les contenus de page et les `/AP` (`Tj`/`TJ` hexadécimaux des polices `Type0` qui référencent ce descripteur, 2 octets par CID, Identity-H sans `/CIDToGIDMap`) ; `subsetter::subset` avec remappage identité (GID conservés) ; `/FontFile2`, `/Length1`, `/BaseFont` et `/FontName` = `XXXXXX+DejaVuSans`.

- [ ] **Step 1: Tests** : `inserted_unicode_text_embeds_a_subset_font` (`/FontFile2` < 40 Ko, `/BaseFont` commence par 6 majuscules + `+`, `page_text` = « Ω », rendu identique avant/après par `compare`) ; `two_unicode_annotations_share_one_subset` ; conformance `insert-text` Unicode déjà couvert.
- [ ] **Step 2–5** : FAIL → implement → PASS → `perf(core): subset the Unicode font embedded by PDFium at save time (audit E10)`

### Task 20 (D3): PR D — vérification, relecture, fusion

- [ ] `/verif` 8/8 ; `bash scripts/audit/write-bench.sh fixtures/corpus/images` : aucune régression qpdf/MuPDF/pdf.js.
- [ ] `rust-reviewer` ; résumé ; PR « Lot 14 (D): file size » ; fusion.

## Partie E — Conformance et documentation

### Task 21 (E1): `conformance.rs` étendu, avertissement qpdf, tests corpus

**Files:**
- Modify: `crates/mega-core/tests/conformance.rs`, `CONTRIBUTING.md`
- Test: lui-même

- [ ] Ajouter à `assert_conforms` les commandes du §5 du rapport : `unprotect` ; `set_field_value` case/radio/combo ; `add_annotation` ellipse, flèche, souligné, barré, ondulé, tampon ASCII et Unicode ; `watermark` et `page_numbers` Unicode ; `add_invisible_words` Unicode ; `insert_text` Helvetica ; `set_object_text` → DejaVu ; `insert_image` JPEG/PNG ; `insert_signature` image ; `optimize` avec `heavy_images_pdf` ; rotation+filigrane+note sur `encrypted.pdf` et `owner-only.pdf` (qpdf `--password`) ; signature visible ; `merge`, `split`, `extract_pages`.
- [ ] Test `every_edit_command_keeps_the_rich_catalog` : pour chaque variante JSON d'`EditCommand` sur `rich_catalog_pdf` : les 17 structures présentes après `save_as` (liste des commandes = celle de `conformance.rs`).
- [ ] Relecture optionnelle : `fn mutool() -> Option<PathBuf>` (`MEGA_MUTOOL`), `fn pdfjs_check() -> Option<PathBuf>` (`MEGA_PDFJS_CHECK`, lancé par `node`) ; chaque sortie : `mutool info` code 0 sans « error », pdf.js `"ok":true` ; sautées si absentes.
- [ ] Tests corpus sous `MEGA_CORPUS` (chemin du dossier) : ouverture de tous les fichiers sans panique, rotation + `qpdf --check`.
- [ ] Quand qpdf manque sans `MEGA_REQUIRE_QPDF` : `eprintln!("AVERTISSEMENT : qpdf introuvable, contrôle de conformité SAUTÉ (poser MEGA_QPDF)")` en tête de chaque test, et `CONTRIBUTING.md` documente `MEGA_QPDF`, `MEGA_MUTOOL`, `MEGA_PDFJS_CHECK`, `MEGA_VERAPDF`, `MEGA_CORPUS`.
- [ ] `MEGA_QPDF=… MEGA_REQUIRE_QPDF=1 cargo test -p mega-core --test conformance` → PASS ; commit `test(core): conformance covers every write path, catalogue preservation and optional cross-readers`

### Task 22 (E2): documentation et ADR-0019

**Files:**
- Create: `docs/adr/0019-save-fidelity.md` (statut accepté : report du catalogue, déplacement par lopdf, `can_clean`, version d'en-tête, passe-plat `Identity`)
- Modify: `docs/adr/0007…` (déjà en A5), `0008` (`/InkList` et `prune_objects` : paragraphes retirés ; `/AP` des marquages écrit), `0009` (« chiffrés non modifiables » retiré), `0010` (passe lopdf sur chiffré, E5), `0011` (mots de passe Latin-1 retiré ; `/Extensions`), `0013` (OCR WinAnsi retiré ; DocMDP, horodatage), `0018` (§7 : mesures veraPDF, ce qui conserve PDF/A), `docs/FONCTIONNALITES.md` (L7, L9, L10, L13, E12, E13, E15 ; « vérifié par pdf.js et MuPDF »), `README.md` (ligne « compatibilité mesurée »), `docs/ROADMAP.md`
- [ ] Rédiger, relire les liens, commit `docs: ADR-0019 save fidelity, ADRs refreshed after the compatibility audit`

### Task 23 (E3): migration des générateurs et retrait du banc

- [ ] Vérifier que chaque expérience E1–E20, L11–L19 a son test (grep des identifiants dans `tests/`) ; supprimer `crates/mega-core/tests/audit_compat.rs` ; `scripts/audit/` reste (mesure de non-régression) ; `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Commit `test(core): measurement bench retired, every experiment is now an asserting test`

### Task 24 (E4): fin de lot

- [ ] Checklist manuelle de la spec (8 points) sur l'application lancée (`pnpm --dir app tauri dev -- -- fixtures/corpus/formulaires/160F-2019.pdf`).
- [ ] `bash scripts/audit/read-bench.sh` et `write-bench.sh` complets ; comparer à `target/audit/*.csv` de l'audit (archivés dans `docs/audit/mesures-2026-10-31/` sous forme de CSV commités, taille < 200 Ko) : aucune régression, gains L1, L2, L3, E3, E6 visibles.
- [ ] `/note-apprentissage` (lot 14) ; `/verif` ; version 0.14.0 (`Cargo.toml`, `app/package.json`, `tauri.conf.json`) ; PR finale ; release avec contrôle du paquet installé.
