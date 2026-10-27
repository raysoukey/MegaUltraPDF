# Lot 8 — OCR et signature numérique : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rendre cherchables les PDF numérisés (OCR par `tesseract`, couche de texte invisible) et signer / vérifier des PDF avec un certificat PKCS#12, en options de compilation.

**Architecture:** `mega-core` gagne deux modules derrière des features (`ocr`, `signature`), un mécanisme d'« octets intacts » dans `Document`, et une couche de texte invisible dans `overlay.rs` ; le service, Tauri et le CLI exposent trois opérations ; le frontend ajoute deux dialogues et un onglet.

**Tech Stack:** Rust (pdfium-render, lopdf 0.46, `cms` 0.2, `x509-cert` 0.2, `rsa` 0.9, `p256` 0.13, `sha2` 0.10, `p12-keystore`), commande `tesseract` 5, Svelte 5, Vitest, pyHanko (vérification croisée en développement).

**Spec:** docs/superpowers/specs/2026-10-27-lot-8-ocr-signature.md

## Global Constraints

- CLAUDE.md : `write_atomically` pour toute écriture de PDF, `EditCommand` pour toute édition
  (la signature est un enregistrement, voir spec), pas d'`unwrap` hors tests, docs `///` en
  français, deux dictionnaires `fr`/`en`, aucun appel réseau à l'exécution.
- `mega-core` : features `ocr` et `signature` désactivées par défaut ; `mega-cli` et
  `mega-app` les activent par défaut.
- Toute coordonnée publique en espace affiché (`PageFrame`), pages tournées comprises.
- Documents chiffrés : OCR et signature refusés explicitement (ADR-0010 §7).

## Review Focus

1. PDF déjà signé, enregistré **sans modification** : la signature reste intègre (Task 1).
2. Signer un PDF qui a déjà une signature : la première reste intègre (révision antérieure),
   la seconde couvre tout le document (Task 5).
3. OCR d'une page tournée (`/Rotate 90`) : les mots retrouvés par la recherche tombent sur
   l'image (Task 2).
4. `tesseract` absent ou langue absente : message clair, aucune modification (Task 3).
5. Mauvais mot de passe `.p12` ou fichier qui n'est pas un PKCS#12 : erreur typée
   « mot de passe incorrect » / « certificat illisible », sans panique (Task 4).

---

### Task 1 — Features et octets intacts

**Files:** Modify `crates/mega-core/Cargo.toml` (features `ocr`, `signature`, vides au
départ), `crates/mega-cli/Cargo.toml`, `app/src-tauri/Cargo.toml` (features par défaut),
`crates/mega-core/src/document.rs` (`pristine: Option<Vec<u8>>`), `edit.rs` (`apply` vide
`pristine` ; `save_as` écrit `pristine` s'il existe), `.github/workflows/ci.yml`
(`cargo check -p mega-core --no-default-features`).

- [ ] Tests (RED) : `untouched_save_writes_the_loaded_bytes` (octets identiques au fichier
  ouvert) ; `save_after_an_edit_rewrites_the_file` ; `undo_to_the_start_still_rewrites`
  (après une édition, `pristine` ne revient pas : comportement documenté).
- [ ] Implémenter ; commit `feat(core): keep the loaded bytes of an untouched document`.

### Task 2 — Couche de texte invisible

**Files:** Modify `crates/mega-core/src/overlay.rs` (`pub struct InvisibleWord { text, rect }`
en espace affiché ; `Document::add_invisible_words(page, &[InvisibleWord])`, `3 Tr`, `Tz`,
mesure PDFium avant la phase `lopdf`, mots non WinAnsi : caractères remplacés par `?`).

- [ ] Tests (RED) : `invisible_words_are_extracted_and_searchable` (page vide, deux mots,
  `page_text` les contient, `search` les trouve, boîte du premier caractère dans le rectangle
  demandé à 1 pt près) ; `invisible_words_are_not_rendered` (rendu identique avant/après) ;
  `invisible_words_follow_a_rotated_page` (fixture `rotated.pdf`, Review Focus 3) ;
  `non_win_ansi_letters_become_question_marks`.
- [ ] Implémenter ; commit `feat(core): invisible text layer`.

### Task 3 — OCR par `tesseract` (feature `ocr`)

**Files:** Create `crates/mega-core/src/ocr.rs` ; Modify `edit.rs` (`EditCommand::OcrPage`),
`lib.rs`, `error.rs` si besoin (`CoreError::ToolNotFound`?), `xtask/src/main.rs`
(`gen-fixtures` produit `fixtures/scanned.pdf`).

**Produces:** `search_candidates(app, env, path_dirs) -> Vec<PathBuf>` (pur),
`find_tesseract(app: Option<&Path>) -> Option<PathBuf>`, `set_tesseract_path`,
`tesseract_languages(exe) -> CoreResult<Vec<String>>`, `parse_tsv(&str) -> Vec<OcrWord>`,
`Document::page_has_text(page) -> CoreResult<bool>`.

- [ ] Tests (RED) : `parse_tsv_keeps_words_with_their_boxes` (extrait réel de sortie TSV,
  niveau 5 seulement, mots vides et confiance −1 ignorés) ; `search_candidates_order` ;
  `ocr_page_refuses_when_tesseract_is_missing` (chemin réglé inexistant, Review Focus 4) ;
  `ocr_page_refuses_an_unknown_language` ; `ocr_makes_a_scanned_page_searchable` (exécuté
  seulement si `tesseract` est trouvé : sinon le test l'écrit et passe) ;
  `ocr_page_is_undoable`.
- [ ] Implémenter ; commit `feat(core): OCR pages with tesseract into an invisible text layer`.

### Task 4 — Identité de signature et CMS (feature `signature`)

**Files:** Create `crates/mega-core/src/signature/mod.rs`, `signature/cms.rs` ;
`fixtures/test-signer.p12`, `fixtures/test-signer-ec.p12` (openssl, commandes dans
`xtask/src/main.rs` doc de `gen-fixtures`).

**Produces:** `SigningIdentity::from_pkcs12(bytes, password)`, `cms::sign_digest(&identity,
digest: [u8; 32], signing_time) -> CoreResult<Vec<u8>>` (DER `ContentInfo`),
`cms::verify(der, digest) -> CmsCheck { signer, signed_at, status }`.

- [ ] Tests (RED) : `pkcs12_rsa_and_ec_identities_load` ; `wrong_pkcs12_password_is_wrong_password`
  et `garbage_is_not_a_certificate` (Review Focus 5) ; `cms_round_trip_verifies` (RSA, EC) ;
  `cms_with_another_digest_is_invalid` ; `cms_signer_name_is_the_certificate_common_name`.
- [ ] Implémenter ; commit `feat(core): PKCS#12 identities and detached CMS signatures`.

### Task 5 — Signer et vérifier un PDF

**Files:** Create `signature/pdf.rs` (mise à jour incrémentale, `/ByteRange`, `/Contents`) ;
`fixtures/signed-external.pdf` (pyHanko).

**Produces:** `Document::sign_to(path, &SigningIdentity, &SignOptions) -> CoreResult<()>`,
`signatures(bytes) -> CoreResult<Vec<SignatureInfo>>`, `Document::signatures()`.

- [ ] Tests (RED) : `signed_file_verifies_and_covers_the_whole_document` ;
  `original_bytes_are_a_prefix_of_the_signed_file` (incrémental) ; `flipping_a_byte_invalidates`;
  `second_signature_keeps_the_first_valid` (Review Focus 2) ; `untouched_save_keeps_the_signature`
  (Review Focus 1) ; `edited_and_saved_signed_file_is_reported_invalid` ;
  `signing_keeps_existing_form_fields` (`form.pdf`) ; `signing_an_encrypted_pdf_is_refused` ;
  `third_party_signature_verifies` (`signed-external.pdf`).
- [ ] Vérification croisée : pyHanko valide un fichier signé par nous (résultat noté dans le ledger).
- [ ] Implémenter ; commit `feat(core): sign PDFs by incremental update and verify signatures`.

### Task 6 — Service, Tauri, CLI

**Files:** Modify `service/mod.rs` (requêtes `OcrStatus`, `Signatures`, `SignDocument`),
`app/src-tauri/src/commands.rs` (+ `lib.rs`), `crates/mega-cli` (`ocr`, `sign`, `signatures`).

- [ ] Tests (RED) : service `ocr_status_reports_missing_tool`, `signatures_of_an_unsigned_document_is_empty`,
  `sign_document_reloads_the_signed_bytes` ; CLI `sign_then_signatures_reports_valid`,
  `sign_reads_the_password_from_the_environment`, `ocr_without_tesseract_fails_cleanly` ;
  Tauri : sérialisation de `SignatureInfo`.
- [ ] Implémenter ; commit `feat(cli,app): OCR, signing and signature verification commands`.

### Task 7 — Interface

**Files:** Create `OcrDialog.svelte`, `SignDialog.svelte`, `SignaturesPanel.svelte`,
`logic/ocr.ts` (`ocrPages(scope, current, pagesWithText)`), `logic/signatures.ts`
(`signatureStatusText(info)`) ; Modify `api.ts`, `strings.ts` (fr/en), `ContentToolbar.svelte`,
`FileMenu.svelte`, `Sidebar.svelte`, `+page.svelte` (garde modale, bandeau).

- [ ] Tests (RED, Vitest) : `ocrPages` (sans texte / toutes / courante) ; `signatureStatusText`
  (intègre et couvrante, intègre non couvrante, invalide, non prise en charge).
- [ ] Implémenter ; commit `feat(app): OCR dialog, signing dialog and signatures panel`.

### Task 8 — Vérification, documentation, relecture

- CI : `tesseract-ocr` installé sur Ubuntu (le test OCR réel y tourne) ; `cargo check -p
  mega-core --no-default-features`.
- Scénario CDP `ocr-signature.json` (checklist de la spec) ; ADR-0013 ; `lot-8.md` ;
  `/concept features-et-compilation-conditionnelle` ou complément ; ROADMAP, FONCTIONNALITES,
  README, CLAUDE.md.
- `/verif`, relecture (code + `pdf-expert`), passe de correction, PR, CI, fusion.
