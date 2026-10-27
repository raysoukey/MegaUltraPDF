# Lot 7 — Finition et distribution : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Interface bilingue FR/EN, accessible au clavier et aux lecteurs d'écran, préférences, aide et « À propos » avec licences, et paquets installables sur les trois OS produits par la CI.

**Architecture:** Le frontend porte l'essentiel (dictionnaires, actions d'accessibilité, nouveaux dialogues) avec sa logique pure sous Vitest ; `mega-core` gagne un seul réglage (`engine::set_library_dir`) ; `xtask` génère les licences et prépare PDFium pour le bundler ; un workflow `release.yml` construit et publie des paquets non signés.

**Tech Stack:** Rust (pdfium-render, serde_json dans xtask), Tauri 2 bundler (NSIS, MSI, deb, AppImage, dmg), Svelte 5, Vitest, GitHub Actions.

**Spec:** docs/superpowers/specs/2026-10-27-lot-7-finition-distribution.md

## Global Constraints

- CLAUDE.md : aucune chaîne visible en dur (les deux dictionnaires dans `strings.ts`), docs
  `///` en français, pas d'`unwrap` hors tests, aucun appel réseau à l'exécution.
- Le dictionnaire `en` a exactement la forme de `fr` (vérifié par `tsc` et par un test).
- Version 0.7.0 partout. Paquets non signés.

## Review Focus

1. Changer de langue avec deux documents ouverts : les onglets, la page courante et l'historique
   sont conservés (Task 2).
2. Échap dans un menu ouvert ferme le menu **sans** fermer le dialogue ou la recherche en
   dessous, et rend le focus au bouton (Task 3).
3. Application installée (hors du dépôt) : PDFium est trouvé dans les ressources, aucune
   dépendance au dossier `vendor/` (Task 6).
4. `cargo xtask licenses` sur une crate sans champ `license` (seulement `license-file`) :
   listée avec « voir fichier », pas d'échec (Task 5).
5. Piège de focus dans un dialogue dont un bouton devient désactivé : Tab saute les éléments
   désactivés et cachés (Task 3).

---

### Task 1 — `engine::set_library_dir`

**Files:** Modify `crates/mega-core/src/engine.rs`, tests unitaires du module.

**Produces:** `pub fn set_library_dir(dir: PathBuf) -> bool` (OnceLock<PathBuf> ; vrai si
retenu, faux si déjà réglé ou si PDFium est déjà initialisé) ; `library_search_paths()` place
ce dossier en tête.

- [ ] Tests (RED) : `library_dir_set_by_the_app_comes_first` (fonction pure
  `search_paths_with(app_dir: Option<&Path>, env: Option<OsString>, exe: Option<PathBuf>)`),
  `set_library_dir_is_write_once`.
- [ ] Implémenter ; commit `feat(core): let the application set the PDFium library directory`.

### Task 2 — Dictionnaire anglais et choix de la langue

**Files:** Modify `app/src/lib/strings.ts` (`fr` sans `as const`, `en: typeof fr`, `t` mutable,
`setLanguage`) ; Create `app/src/lib/logic/i18n.ts` (+ test) ; Modify `state/settings.svelte.ts`
(`language: 'system' | 'fr' | 'en'`, `restorePositions: boolean`, miroir `localStorage`),
`+page.svelte` (`{#key}` sur la langue), `logic/shortcuts.ts` (`shortcutHelp()` calculé à
l'appel), composants avec texte en dur (ex. « Ctrl+molette »).

- [ ] Tests (RED, Vitest) : `resolveLanguage` (système fr-CA → fr, en-US → en, de → en, choix
  explicite prioritaire) ; `en` a les mêmes clés que `fr`, récursivement, et les fonctions la
  même arité ; `shortcutHelp()` suit la langue courante.
- [ ] Implémenter ; commit `feat(app): English interface and language preference`.

### Task 3 — Accessibilité : menus au clavier, piège de focus, noms accessibles

**Files:** Create `logic/menu.ts`, `logic/focus.ts`, `logic/contrast.ts` (+ tests),
`lib/actions/trapFocus.ts` ; Modify `FileMenu.svelte`, `ContentToolbar.svelte`, tous les
dialogues (`role="dialog"` sur une enveloppe des `<form>`), boutons à glyphe (`aria-label`),
`app.css` (`:focus-visible`, jetons si contraste insuffisant).

- [ ] Tests (RED) : `menuKey` (↓ ↑ bouclent, Début/Fin, Échap → fermer, Entrée → activer),
  `nextFocus` (Tab / Maj+Tab bouclent, éléments désactivés ignorés — Review Focus 5),
  `contrastRatio` (noir/blanc = 21) et jetons `--muted` / `--bg` des deux thèmes ≥ 4,5.
- [ ] Implémenter ; commit `feat(app): keyboard menus, focus trap and accessible names`.

### Task 4 — Préférences et aide

**Files:** Create `PreferencesDialog.svelte`, `HelpDialog.svelte` (remplace `ShortcutsHelp`) ;
Modify `FileMenu.svelte` (Préférences…), `logic/shortcuts.ts` (Ctrl+, → `preferences`),
`state/documents.svelte.ts` (`restorePositions` respecté par `initialScaleFor`), `strings.ts`.

- [ ] Tests (RED, Vitest) : `initialPosition(settings, path)` (logique pure extraite) ignore la
  position mémorisée si `restorePositions` est faux ; `actionFor` Ctrl+, → `preferences`.
- [ ] Implémenter ; commit `feat(app): preferences dialog and integrated help`.

### Task 5 — Licences tierces et « À propos »

**Files:** Create `xtask/src/licenses.rs` ; Modify `xtask/src/main.rs`, `xtask/src/pdfium.rs`
(extraire aussi `LICENSE` de l'archive) ; generated `app/src/lib/generated/licenses.json`,
`THIRD_PARTY_LICENSES.md` ; `HelpDialog` onglet « À propos ».

- [ ] Tests (RED, `xtask`) : `crate_entries_from_metadata` sur un extrait JSON (licence absente
  → « voir fichier » — Review Focus 4 ; crates du workspace exclues), `npm_entries` sur un extrait
  de `pnpm licenses list --json`, rendu Markdown trié et dédoublonné.
- [ ] Implémenter, générer ; commit `feat(xtask): third-party licenses and PDFium notice`.

### Task 6 — Paquets et version portable

**Files:** Create `xtask/src/stage.rs` (+ test du nom de fichier par cible) ; Modify
`app/src-tauri/tauri.conf.json` (cibles, `resources`, `beforeBuildCommand`), `app/src-tauri/src/lib.rs`
(`setup` : `set_library_dir(resource_dir/pdfium)`), `.gitignore`, `Cargo.toml` (`[profile.release]`,
version 0.7.0), `app/package.json`, `.github/workflows/release.yml`, `scripts/ui-check.ps1`
(paramètre `-Exe` pour lancer un exécutable déjà construit).

- [ ] Tests (RED) : `staged_library_name` (Windows `pdfium.dll`, Linux `libpdfium.so`, macOS
  `libpdfium.dylib`) ; `mega-app` : test du chemin calculé à partir d'un dossier de ressources.
- [ ] Construire l'installeur NSIS localement, l'installer en silence dans un dossier
  temporaire, lancer l'application installée avec `-Exe` et dérouler `lecteur-simple.json`
  (Review Focus 3) ; commit `build: installers, portable zip and release workflow`.

### Task 7 — Vérification, documentation, relecture

- Scénario CDP `finition.json` (checklist de la spec) ; scénarios existants verts en anglais
  pour au moins un (lecteur-simple adapté si besoin).
- ADR-0012 (i18n par remontage, accessibilité, distribution, Lot 8), `docs/apprentissage/lot-7.md`,
  `/concept profils-cargo-et-distribution`, ROADMAP (Lot 8 ajouté), FONCTIONNALITES, README
  (installation), CLAUDE.md.
- `/verif`, relecture (code + `ui-reviewer`), passe de correction, PR, CI, fusion.
