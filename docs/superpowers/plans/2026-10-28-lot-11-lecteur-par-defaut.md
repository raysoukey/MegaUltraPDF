# Lot 11 — Lecteur PDF par défaut : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** une seule instance (fichiers ouverts en onglets), état et bouton « lecteur par défaut »
dans les préférences, déclarations de paquets ; v0.11.0.

**Architecture:** greffon `tauri-plugin-single-instance` qui relaie les arguments de la seconde
instance vers `PendingPaths` + événement `open-paths` déjà écouté ; module `default_app.rs`
(intégration système, sans logique PDF) appelant `reg`, `xdg-mime` et les paramètres Windows ;
greffon NSIS qui enregistre les Capabilities.

**Spec:** `docs/superpowers/specs/2026-10-28-lot-11-lecteur-par-defaut.md`

## Global Constraints

- Règles de `CLAUDE.md` (SPDX, docs françaises, chaînes fr/en, pas d'`unwrap` hors tests).
- Aucun shell : `std::process::Command` avec liste d'arguments, fenêtre de console masquée sous
  Windows (`CREATE_NO_WINDOW`, comme `ocr::find_tesseract`).
- Nouvelle dépendance justifiée dans le commit.

## Review Focus

1. Seconde instance lancée depuis un autre dossier avec un chemin relatif.
2. Plusieurs fichiers sélectionnés dans l'Explorateur (plusieurs lancements quasi simultanés).
3. Démarrage à froid avec plusieurs PDF en arguments.
4. Version portable (aucune association possible) : message clair, pas d'erreur brute.
5. `reg` / `xdg-mime` absents ou sortie inattendue : état « inconnu », jamais de panne.

---

### Task 1: Arguments → chemins PDF (fonction pure) et démarrage à froid multi-fichiers
- Tests (RED) dans `app/src-tauri/src/commands.rs` : `pdf_paths_from_args_keeps_every_existing_pdf`,
  `relative_paths_resolve_against_the_second_instance_dir`, `options_and_non_pdf_are_ignored`,
  `duplicates_are_dropped`.
- `pub fn pdf_paths_from_args(args: impl IntoIterator<Item = String>, cwd: &Path) -> Vec<String>` ;
  commande `startup_files() -> Vec<String>` remplace `startup_file` ; `api.ts`, `+page.svelte`.

### Task 2: Une seule instance
- Dépendance `tauri-plugin-single-instance = "2"` ; enregistrée en premier dans `run()` ; rappel :
  `pdf_paths_from_args(argv.skip(1), cwd)` → `PendingPaths::push` → `emit("open-paths")` →
  fenêtre principale `unminimize` + `set_focus`. Fonction `relay_second_instance(app, argv, cwd)`.
- Vérification : scénario scripté (lancement dev + seconde instance avec deux PDF).

### Task 3: `default_app.rs` — état
- Types `DefaultAppStatus { kind: DefaultAppKind, handler: Option<String>, can_set: bool }`,
  `DefaultAppKind::{Default, Other, Unknown}` (serde snake_case).
- Fonctions pures testées : `parse_reg_progid(output: &str) -> Option<String>`,
  `parse_xdg_default(output: &str) -> Option<String>`, `classify(handler: Option<&str>, ours: &str) -> DefaultAppKind`,
  `is_portable(exe_dir: &Path) -> bool` (Windows : pas de `uninstall.exe`).
- Commande `default_app_status`.

### Task 4: `default_app.rs` — action et interface
- `DefaultAppAction::{OpenedSettings, Set, Instructions, Unsupported}` ; commande
  `make_default_app` (Windows : opener `ms-settings:defaultapps?registeredAppUser=MegaUltraPDF`,
  repli `ms-settings:defaultapps` ; Linux : `xdg-mime default` ; macOS : `Instructions`).
- `logic/defaultapp.ts` + Vitest ; section dans `PreferencesDialog.svelte` ; chaînes fr/en.

### Task 5: Paquets
- `tauri.conf.json` : rôle `Editor` ; `bundle.windows.nsis.installerHooks` →
  `windows/hooks.nsh` (Capabilities + RegisteredApplications à l'installation, retirés à la
  désinstallation).
- Vérification : paquet NSIS local (`pnpm --dir app release`), installation silencieuse,
  `reg query`, désinstallation.

### Task 6: Documentation, scénario, version
- Scénario CDP `lecteur-par-defaut` ; ADR-0015 ; CLAUDE.md ; ROADMAP ; FONCTIONNALITES ; aide ;
  version 0.11.0 ; `/note-apprentissage` ; `/verif`.
