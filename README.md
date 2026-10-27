# MegaUltraPDF

Lecteur et éditeur PDF libre, sans publicité ni télémétrie, écrit en Rust.
Le projet est aussi un support d'apprentissage de Rust : le code est commenté en français
et chaque lot de fonctionnalités s'accompagne d'une note d'apprentissage (`docs/apprentissage/`).

- Moteur PDF : PDFium (celui de Chrome) via `pdfium-render`, piloté exclusivement par `mega-core`.
- Interface : Tauri 2 (backend Rust) + Svelte 5 / TypeScript, sur le WebView2 du système.
- Licence : GPL-3.0-or-later.

## État (voir `docs/ROADMAP.md`)

| Lot | Contenu | État |
|---|---|---|
| 0 | Fondations : cœur Rust, CLI `info`/`render`, application qui affiche la page 1, outillage | ✅ |
| 1 | Lecteur complet (onglets, zoom, rotation, vignettes, signets, liens, sélection, recherche, propriétés, thème, raccourcis) | ✅ |
| 2 | Pages et documents (pivoter, supprimer, réordonner, extraire, insérer, fusionner, scinder, enregistrer, annuler/rétablir) | ✅ |
| 3 | Annotations (marquages de texte, notes, texte, encre, formes, tampons ; panneau ; rendu des annotations existantes) | ✅ |
| 4 | Formulaires (champs HTML natifs, remplissage, aplatir, XFA détecté) et signature visuelle | ✅ |
| 5 → 7 | Édition de contenu, export/sécurité, finition | ⬜ |

## Prérequis (développement)

- **Rust stable** via [rustup](https://rustup.rs) (`rustc`, `cargo`, `rustfmt`, `clippy`).
- **Visual Studio 2022** avec la charge « Développement Desktop C++ » (ou les Build Tools 2022
  avec le même composant) : fournit l'éditeur de liens MSVC utilisé par Rust sous Windows.
- **Node.js ≥ 22** et **pnpm ≥ 9** pour l'interface (Tauri 2 + Svelte).
- **WebView2** (inclus dans Windows 11).
- **GitHub CLI** (`gh`), optionnel, pour les pull requests et la CI.
- Sous Linux : les dépendances Tauri 2 (`libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`,
  `librsvg2-dev`, `patchelf`, `build-essential`, `libssl-dev`).

Installation rapide sous Windows (winget) :

```powershell
winget install --id Rustlang.Rustup --exact
winget install --id GitHub.cli --exact
```

## Démarrage rapide

```bash
cargo xtask fetch-pdfium            # bibliothèque PDFium épinglée, somme de contrôle vérifiée
pnpm --dir app install              # dépendances du frontend
cargo test --workspace              # tests Rust
pnpm --dir app tauri dev -- -- fixtures/simple.pdf   # lance l'application sur un fichier
```

## Ligne de commande

```bash
cargo run -p mega-cli -- info fixtures/simple.pdf
cargo run -p mega-cli -- render fixtures/simple.pdf --page 1 --scale 2 --out page1.png
cargo run -p mega-cli -- info fixtures/encrypted.pdf --password secret
cargo run -p mega-cli -- merge a.pdf b.pdf -o fusion.pdf
cargo run -p mega-cli -- split rapport.pdf --every 10 -o parties/      # ou --ranges "1-3,4-" / --bookmarks
cargo run -p mega-cli -- rotate rapport.pdf --pages 1,3-4 --degrees 90 -o tourne.pdf
cargo run -p mega-cli -- extract rapport.pdf --pages 2-3 -o extrait.pdf
cargo run -p mega-cli -- annots rapport.pdf --page 1                      # liste les annotations
cargo run -p mega-cli -- fields formulaire.pdf                             # liste les champs
cargo run -p mega-cli -- flatten formulaire.pdf -o plat.pdf                # aplatit
```

Toutes les écritures sont atomiques (fichier temporaire puis renommage) : un échec ne laisse
jamais un fichier à moitié écrit.

## Vérifier l'interface sans souris

```powershell
scripts/ui-check.ps1 -Pdf fixtures/simple.pdf -Scenario scripts/scenarios/lecteur-simple.json
scripts/ui-check.ps1 -Pdf copie.pdf -Scenario scripts/scenarios/edition-pages.json   # modifie le fichier !
scripts/ui-check.ps1 -Pdf copie.pdf -Scenario scripts/scenarios/annotation.json      # idem
scripts/ui-check.ps1 -Pdf copie-form.pdf -Scenario scripts/scenarios/formulaire.json  # idem
```

Lance l'application avec le port de débogage WebView2, déroule un scénario (clics, touches,
glisser, captures d'écran) et affiche ✔/✘ par assertion. Captures dans `target/ui-check/`.

## Organisation du dépôt

```
crates/mega-core/   logique PDF (seul crate qui parle à PDFium)
crates/mega-cli/    binaire `mega`
app/src-tauri/      backend Tauri (commandes, protocole mega://, registre)
app/src/            frontend SvelteKit (présentation seulement)
xtask/              tâches de dev (fetch-pdfium, gen-fixtures)
fixtures/           PDF de test
docs/               feuille de route, liste des fonctionnalités, ADR, notes d'apprentissage, specs et plans
scripts/            vérification de l'interface par scénarios (CDP)
.claude/            environnement Claude Code (règles, hooks, compétences, agents)
```

Contribuer : voir `CONTRIBUTING.md`. Règles de travail : `CLAUDE.md`.
