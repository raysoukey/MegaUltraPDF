# MegaUltraPDF — Document de conception

> Statut : **approuvé le 2026-10-26**. Document de référence du projet ; le plan
> d'implémentation de chaque lot se trouve dans `docs/superpowers/plans/`.

## 1. Contexte et intention

L'utilisateur veut un lecteur et éditeur PDF **libre, sans publicité, sans télémétrie, sans
dépendances superflues**, écrit en **Rust**. Le projet a une seconde finalité tout aussi
importante : **apprendre Rust** (niveau actuel : quelques tutoriels suivis). Claude Code écrit
l'intégralité du code ; l'utilisateur lit, questionne, et apprend à travers des explications
intégrées. Les fonctionnalités sont livrées **par lots progressifs**, sur une fondation conçue
dès le départ pour toutes les accueillir.

La demande initiale est de « créer un environnement de développement Claude Code » pour ce
projet : ce document définit donc à la fois **le produit** (ce qu'on construit, dans quel ordre)
et **l'environnement** (comment Claude Code et l'utilisateur travaillent dessus).

Le dossier `C:\Users\labre\Desktop\MegaUltraPDF` est vide (hors `.remember/`). Git et VS Code
sont installés. **Rust n'est pas installé** (ni rustup, ni cargo, ni les Build Tools C++).

## 2. Objectifs, non-objectifs, anti-objectifs

**Objectifs**
- Application de bureau complète : lecture confortable, puis édition (pages, annotations,
  formulaires, contenu, export/sécurité), livrée en lots utilisables individuellement.
- Code lisible et pédagogique : chaque lot consolide des concepts Rust identifiés à l'avance.
- Environnement Claude Code reproductible : instructions, compétences, agents, hooks, vérifs.
- Fonctionne d'abord sous **Windows 11** ; conçue multiplateforme (Linux/macOS compilés en CI).

**Non-objectifs (pour l'instant)**
- Version web/mobile. Synchronisation cloud. Travail collaboratif.
- OCR et signature cryptographique : prévus seulement comme options désactivées par défaut
  (Lot 7), car ils tirent de grosses dépendances.

**Anti-objectifs (ce qui serait un échec même si « ça marche »)**
- Toute collecte de données, mise à jour silencieuse, appel réseau non demandé.
- Embarquer un navigateur complet (Electron) ou des frameworks lourds : on utilise le WebView2
  déjà présent dans Windows, et toute la logique PDF reste en Rust.
- Un code que l'utilisateur ne peut pas relire : abstractions gratuites, macros opaques,
  `unsafe` non justifié.

**Note** : l'utilisateur a explicitement demandé de ne pas exclure une pile web par principe et
d'intégrer ce qui optimise le projet ; d'où le choix de Tauri 2 (§3).

## 3. Choix techniques (choix de Claude, avec raisons)

| Domaine | Choix | Pourquoi | Alternative écartée |
|---|---|---|---|
| Moteur PDF (rendu, texte, pages, annotations, formulaires, sauvegarde) | **PDFium** via la crate `pdfium-render` (0.9.x), binaires précompilés de bblanchon (licence BSD-3) | Un seul moteur mature couvre lecture **et** édition ; rendu fidèle (celui de Chrome) ; licence permissive ; API Rust idiomatique et sûre | MuPDF (AGPL, contraignant) ; `hayro` (Rust pur mais encore jeune, lecture seule, pas de PDF chiffrés) — à réévaluer plus tard comme moteur alternatif |
| Interface | **Tauri 2** (backend Rust) + **Svelte 5 / TypeScript** (frontend, Vite) — **décision de l'utilisateur** | UI riche naturelle pour un PDF : calques de sélection de texte et d'annotations en SVG, formulaires mappés sur des champs HTML, impression via le WebView, accessibilité et i18n matures. Binaire léger (WebView2 du système, pas de Chromium embarqué), zéro télémétrie. Environ 60 % du code reste en Rust (toute la logique PDF) | egui (tout Rust, UI moins riche), Dioxus (tout Rust, écosystème plus jeune), Electron (bloat) |
| Transfert des pages rendues | **Protocole personnalisé Tauri** `mega://page/<doc>/<n>?scale=…` servant des PNG produits par PDFium ; le frontend les affiche dans des `<img>`/`<canvas>` | Évite d'encoder des images en base64 dans l'IPC ; le navigateur met en cache ; simple à comprendre | IPC `invoke` avec tableau d'octets (lent, lourd) |
| Structure | **Workspace Cargo** : `mega-core` (logique, sans UI), `mega-cli` (ligne de commande), `app/src-tauri` (backend Tauri : commandes exposées au frontend) ; **`app/src`** (Svelte/TS) | Sépare la logique testable de l'interface ; le CLI sert de banc d'essai et d'outil réel (fusion, scission, export) ; le backend Tauri n'est qu'une fine couche de commandes | Monocrate ; logique PDF côté JS (PDF.js) — écartée pour garder un seul moteur, en Rust |
| Erreurs | `thiserror` dans `mega-core`, `anyhow` dans app/cli | Convention idiomatique, bon support pédagogique pour `Result`/`?` | `Box<dyn Error>` partout |
| Annuler/Rétablir | Patron **Command** (trait `Command { apply, undo }`) dans `mega-core` | Concept Rust clé (traits, boîtes dynamiques) et prérequis de tout éditeur | Snapshots complets du document (mémoire) |
| Concurrence | **Patron acteur** : un thread travailleur *possède* tout l'état PDFium (documents, pages, texte, recherche) ; les commandes Tauri (asynchrones, exécutées par le runtime `tokio` de Tauri) lui envoient des messages par canal et attendent la réponse (`oneshot`). Cache de pages rendues (LRU) côté Rust. | PDFium n'est pas thread-safe et les types de `pdfium-render` ne sont pas `Send` : on ne peut pas partager le document entre threads. Le patron acteur est la solution correcte et un très bon support pédagogique (ownership exclusif, messages, premier contact maîtrisé avec `async`) | `Arc<Mutex<Document>>` (impossible avec PDFium) |
| Multi-document | **Plusieurs documents ouverts en onglets**, dès la structure d'état du Lot 0 (registre `DocId → OpenDocument` côté Rust, onglet actif côté frontend) ; l'UI des onglets arrive au Lot 1 | Évite de refondre tout l'état plus tard ; rend naturelle la fusion de documents déjà ouverts au Lot 2 | Mono-document (plus simple au départ, coûteux à faire évoluer) |
| Dialogues fichiers | Plugin officiel `tauri-plugin-dialog` | Natif, léger | `rfd` direct |
| Outillage frontend | **Node.js LTS + pnpm**, Vite, Svelte 5, TypeScript strict, Prettier + ESLint, Vitest | Standard, rapide, bien documenté | npm/yarn, React (plus verbeux pour un apprenant) |
| Licence | **GPL-3.0-or-later** (décision de l'utilisateur) | Cohérent avec l'esprit « libre sans spyware » : protège contre les forks propriétaires | MIT/Apache-2.0 |
| Hébergement | **GitHub** (décision de l'utilisateur) + GitHub Actions | CI multiplateforme gratuite, `gh` pour les PR | Local seul |
| Langues | Identifiants (Rust et TS) et messages de commit **en anglais** (convention écosystème) ; commentaires, docs et notes d'apprentissage **en français** ; interface en français puis FR/EN (Lot 7) | L'utilisateur apprend en français ; le code reste lisible par la communauté | Tout anglais |
| Chaînes d'interface | Dès le Lot 1, **toutes les chaînes visibles sont centralisées** dans `app/src/lib/strings.ts` (un objet typé, instance française) ; aucune chaîne en dur dans les composants. Le Lot 7 branche une vraie bibliothèque i18n sans toucher aux composants | Prépare FR/EN sans imposer une abstraction de traduction dès le début | Bibliothèque i18n dès le Lot 1 (trop tôt) ; chaînes en dur (réécriture totale au Lot 7) |
| Pédagogie et TypeScript | L'objectif d'apprentissage reste **Rust**. Le frontend est écrit simplement et commenté, mais les notes d'apprentissage et les commentaires `[Apprentissage]` portent sur le Rust ; le TS est expliqué à la demande (`/explique`) | Ne pas diluer l'apprentissage ; le frontend est un moyen, pas le sujet | Double cursus Rust + web (trop lourd) |

**Double moteur, décidé dès la fondation.** PDFium couvre rendu, texte, pages, annotations,
formulaires et sauvegarde, mais **pas** : le chiffrement à l'enregistrement, le caviardage fin
(il ne supprime que des objets entiers), l'optimisation (recompression, objets inutilisés), ni
certaines retouches de texte. Pour que la fondation accueille vraiment tous les lots :
- `mega-core::Document` **possède les octets du PDF en mémoire** (`Vec<u8>`) et les charge dans
  PDFium ; chaque enregistrement repasse par ces octets.
- `lopdf` (Rust pur, 0.44.x) est introduit au **Lot 5** comme second moteur pour les **passes
  structurelles** : caviardage, optimisation, chiffrement, métadonnées. Une passe `lopdf` prend
  les octets, produit de nouveaux octets, et le document est rechargé dans PDFium (cache
  invalidé). Les deux moteurs ne travaillent jamais simultanément sur la même représentation.
- Périmètre assumé : la modification du texte existant reste « au mieux » (limites du format) ;
  l'application le dit clairement à l'utilisateur.

**Distribution de PDFium** : la DLL (~6 Mo) est téléchargée par une tâche `cargo xtask fetch-pdfium`
(somme de contrôle vérifiée, version épinglée), jamais commitée. L'exécutable charge la DLL
placée à côté de lui. C'est le seul composant non-Rust ; il est documenté comme tel.

## 4. Architecture cible

```
MegaUltraPDF/
├── Cargo.toml                 # workspace (crates/* + app/src-tauri + xtask)
├── crates/
│   ├── mega-core/             # Document, Page, Renderer, Commands, Annotations, Forms…
│   └── mega-cli/              # mega merge/split/render/info…
├── app/                       # application de bureau (Tauri 2)
│   ├── src-tauri/             # backend : commandes Tauri, acteur PDFium, protocole mega://
│   ├── src/                   # frontend Svelte 5 / TypeScript (Vite)
│   │   ├── lib/strings.ts     # toutes les chaînes visibles
│   │   ├── lib/api.ts         # wrappers typés des commandes Tauri
│   │   └── routes|components/ # vues : viewer, vignettes, panneaux, outils
│   ├── package.json, vite.config.ts, tsconfig.json
│   └── tauri.conf.json
├── xtask/                     # tâches de dev (fetch-pdfium, dist)
├── fixtures/                  # petits PDF de test (texte, images, formulaire, chiffré…)
├── docs/
│   ├── ROADMAP.md             # les lots, leur état
│   ├── apprentissage/         # une note par lot : concepts Rust rencontrés
│   ├── adr/                   # décisions d'architecture (ADR courts)
│   └── superpowers/{specs,plans}/
├── .claude/                   # environnement Claude Code (§6)
├── .github/workflows/ci.yml   # fmt, clippy, test, pnpm check/test, build Win/Linux/macOS
├── CLAUDE.md
├── README.md, LICENSE, CONTRIBUTING.md
└── rust-toolchain.toml        # stable épinglé
```

**Frontières** :
- `app/src-tauri` et `mega-cli` ne parlent **jamais** à `pdfium-render` directement ; tout passe
  par l'API de `mega-core` (`Document`, `PageRenderer`, `Command`…). `mega-core` est testable
  sans fenêtre et sans WebView.
- Le backend Tauri est une **fine couche** : il traduit des commandes `invoke` en messages pour
  l'acteur PDFium et sert les pages rendues via `mega://`. Pas de logique métier dedans.
- Le frontend ne contient que de la **logique de présentation** (zoom, défilement, outils,
  géométrie écran ↔ page). Toute décision sur le document (éditer, sauvegarder, annuler) est une
  commande vers Rust. Les types échangés sont définis en Rust (`serde`) et reflétés en TS dans
  `api.ts`.

## 5. Les lots

Chaque lot : objectif → fonctionnalités → prérequis techniques → concepts Rust travaillés →
critère de « fini ». Un lot est fini quand `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test` passent, que la note d'apprentissage est écrite et que l'utilisateur a validé à
l'usage.

### Lot 0 — Fondations et environnement (prérequis de tout le reste)
- Installer : Visual Studio Build Tools 2022 (charge « Développement Desktop C++ »), `rustup`
  (stable, cible `x86_64-pc-windows-msvc`), Node.js LTS + pnpm, `tauri-cli`, GitHub CLI `gh`,
  extensions VS Code (rust-analyzer, CodeLLDB, Even Better TOML, Svelte, Prettier, ESLint).
  Vérifier la présence du runtime WebView2 (livré avec Windows 11).
- Workspace Cargo (`mega-core`, `mega-cli`, `app/src-tauri`, `xtask`), projet Svelte dans
  `app/`, `xtask fetch-pdfium`, `rust-toolchain.toml`.
- Preuve de vie : `mega-cli info fichier.pdf` (nb pages, titre) et `pnpm tauri dev` qui ouvre une
  fenêtre affichant la page 1 d'un PDF choisi via le dialogue (chaîne complète : commande Tauri →
  acteur PDFium → `mega://` → `<img>`).
- `git init`, `.gitignore`, dépôt GitHub, CI GitHub Actions, fixtures initiales.
- Tout l'environnement Claude Code du §6.
- Concepts : cargo/workspace, modules, `Result` et `?`, ownership de base, `struct`/`impl`,
  `serde` (types partagés Rust ↔ TS), un premier thread et un canal.

### Lot 1 — Lecteur
- Ouvrir (dialogue, glisser-déposer, argument CLI, fichiers récents), PDF protégés par mot de passe.
- Rendu paginé net à tout zoom (rendu par l'acteur PDFium, cache LRU, pages visibles en priorité,
  placeholder pendant le rendu, re-rendu à la bonne résolution après zoom).
- Modes : page unique / défilement continu ; zoom (%, ajuster largeur, ajuster page) ; rotation d'affichage.
- Navigation : clavier, saisie de numéro, vignettes, signets (outline), liens internes.
- Recherche texte (surbrillance, suivant/précédent) ; **calque de texte** (boîtes de caractères
  fournies par PDFium) pour sélection et copie.
- Thème clair/sombre, raccourcis, propriétés du document, paramètres persistants (`tauri-plugin-store`).
- Onglets (plusieurs documents ouverts), chaînes centralisées dans `strings.ts`.
- Concepts : threads, canaux et patron acteur (ownership exclusif de PDFium par le travailleur),
  `async`/`await` côté commandes Tauri, `Arc`/`Mutex` illustrés sur le cache de pages et l'état
  partagé Tauri (`State`), traits `Iterator`, enums riches sérialisées vers TS.

### Lot 2 — Pages et documents
- Pivoter, supprimer, réordonner (glisser des vignettes), extraire, insérer page vide ou depuis un
  autre PDF, fusionner plusieurs fichiers, scinder (par plages / toutes les N pages).
- Enregistrer / enregistrer sous (écriture atomique via fichier temporaire + renommage),
  indicateur « modifié », confirmation à la fermeture.
- **Annuler / Rétablir** générique (patron Command) — toute édition ultérieure s'y branche.
- Équivalents CLI : `merge`, `split`, `rotate`, `extract`.
- Concepts : traits objets (`Box<dyn Command>`), généricité, gestion d'erreurs riche, tests
  d'intégration sur fixtures.

### Lot 3 — Annotations
- Sur texte : surligner, souligner, barrer. Libres : note, texte libre, encre (dessin), formes
  (rectangle, ellipse, ligne, flèche), tampon prédéfini.
- Propriétés : couleur, opacité, épaisseur, auteur, date ; déplacer, redimensionner, supprimer.
- Panneau liste des annotations ; enregistrement conforme (lisible par d'autres lecteurs).
- Côté frontend : calque SVG d'édition superposé à la page ; côté Rust : création/modification via
  PDFium, chaque action = une `Command` annulable.
- Concepts : machines à états pour les outils, géométrie (coordonnées PDF vs écran), `enum` +
  `match` exhaustif, pattern builder.

### Lot 4 — Formulaires et signature visuelle
- Affichage et remplissage des champs AcroForm (texte, cases, radios, listes, dates) : les champs
  sont exposés par Rust (type, position, valeur) et rendus comme de vrais champs HTML superposés ;
  navigation Tab, enregistrement, **aplatir** le formulaire.
- Signature visuelle : insérer une image de signature ou la dessiner, la placer et la sauvegarder.
- Concepts : interop avec l'environnement de formulaire PDFium (gestion de ressources, `Drop`),
  conversions de types, validation d'entrée.

### Lot 5 — Édition de contenu
- Insérer du texte (police, taille, couleur) et des images ; déplacer/supprimer des objets.
- Modifier le texte existant quand c'est possible (limites intrinsèques du format PDF expliquées
  à l'utilisateur : polices sous-ensemble, texte positionné).
- Caviardage réel (suppression du contenu, pas un rectangle noir) : **première passe `lopdf`**
  (introduction du second moteur), filigrane, numérotation de pages / en-têtes / pieds de page,
  recadrage.
- Concepts : modèle objet du PDF, transformations matricielles, gestion des polices, travailler
  avec deux représentations d'une même donnée.

### Lot 6 — Export, impression, sécurité
- Export en images (PNG/JPEG, résolution choisie), export texte, **impression** via le dialogue
  d'impression du WebView (pages rendues en haute résolution dans une vue d'impression) :
  multiplateforme sans code natif.
- Métadonnées (titre, auteur, mots-clés), chiffrement / mot de passe (via `lopdf`), suppression
  du mot de passe, optimisation (recompression d'images, suppression d'objets inutilisés).
- Concepts : FFI/bindings sous le capot (lecture, pas écriture), traits `Read`/`Write`, features Cargo.

### Lot 7 — Finition et distribution
- i18n FR/EN (bibliothèque i18n branchée sur `strings.ts`), accessibilité (sémantique HTML,
  ARIA, navigation clavier), préférences complètes, aide intégrée.
- Installeurs via le **bundler Tauri** (NSIS/MSI Windows, AppImage/deb Linux, dmg macOS) +
  version portable ; builds en CI. Pas d'auto-updater (anti-objectif : pas d'appel réseau).
- Options désactivées par défaut (features Cargo) : OCR (`tesseract`), signature numérique
  cryptographique. Elles n'alourdissent le binaire que si on les active.
- Concepts : features conditionnelles, `cfg`, packaging, profils de release.

## 6. Environnement Claude Code

Objectif : que Claude Code produise du code juste, vérifié et **explicable**, et que l'utilisateur
puisse apprendre à chaque étape.

**`CLAUDE.md`** (racine) — règles du projet :
- Architecture et frontières (§4), conventions de langue, interdictions (anti-objectifs).
- Pédagogie : tout élément public a une doc `///` en français qui dit *pourquoi* ; les passages
  qui introduisent un concept Rust portent un commentaire `// [Apprentissage] …` renvoyant à
  `docs/apprentissage/` ; chaque tâche se termine par un court résumé « ce que vous venez de voir ».
- Processus : TDD strict (`superpowers:test-driven-development`) pour `mega-core`, `mega-cli` et
  les commandes du backend Tauri ; côté frontend, TDD (Vitest) pour la logique pure TS (géométrie,
  état du viewer, `api.ts`), et les composants Svelte sont validés par la checklist manuelle de
  fin de lot. Vérification avant de déclarer fini ; un lot = spec → plan → exécution → note
  d'apprentissage.
- Commandes canoniques : `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
  `pnpm check` (svelte-check + tsc), `pnpm test` (Vitest), `pnpm lint`, `pnpm tauri dev`.

**`.claude/settings.json`**
- Permissions pré-accordées : `cargo *`, `rustup *`, `pnpm *`, `git status/diff/log/add/commit`,
  `gh pr *`.
- Hooks : après modification d'un `.rs` → `cargo fmt` sur le fichier ; après modification d'un
  `.ts`/`.svelte` → Prettier ; à la fin d'une tâche (`Stop`) → `cargo clippy` + `cargo test` du
  crate touché, ou `pnpm check` + `pnpm test` si le frontend a changé, résultat injecté en contexte.

**Compétences (`.claude/skills/`)**
- `/lot <n>` : démarre un lot — relit `ROADMAP.md`, rédige la spec du lot puis le plan
  d'implémentation, propose les points de validation.
- `/explique <chemin>` : explication pédagogique d'un module ou d'une fonction au niveau de
  l'utilisateur (schéma des types, flux des données, pourquoi ces choix).
- `/concept <nom>` : mini-leçon sur un concept Rust (ex. lifetimes) illustrée **avec le code du
  projet**, ajoutée à `docs/apprentissage/concepts/`.
- `/note-apprentissage` : à la fin d'un lot, rédige `docs/apprentissage/lot-N.md`.
- `/verif` : fmt + clippy + test + `cargo doc` + `pnpm check/test/lint`, rapport synthétique.
- `/pdf-spec <sujet>` : rappel de la norme PDF (ISO 32000) sur un point précis (annotations,
  AcroForm, chiffrement) avant d'implémenter.

**Agents (`.claude/agents/`)**
- `rust-reviewer` : relecture idiomatique (ownership, erreurs, API publique, `unsafe`) + vérifie
  que la pédagogie est respectée (docs `///`, commentaires `[Apprentissage]`).
- `pdf-expert` : vérifie la conformité PDF d'une fonctionnalité (structure produite lisible par
  d'autres lecteurs) et propose les fixtures de test.
- `ui-reviewer` : relecture du frontend (Svelte/TS) : pas de logique métier côté JS, chaînes
  centralisées, accessibilité, types alignés sur `api.ts`.

**Éditeur** : `.vscode/extensions.json` (rust-analyzer, CodeLLDB, Even Better TOML, Svelte for
VS Code, Prettier, ESLint) et `settings.json` (clippy à l'enregistrement, format on save).

## 7. Requis

**Machine** : Windows 11 (présent), ~8 Go d'espace disque (Build Tools ≈ 6 Go), connexion
Internet pour rustup, crates.io et la DLL PDFium.

**Logiciels à installer au Lot 0** (décision de l'utilisateur : **Claude installe via `winget`**,
l'utilisateur valide les invites UAC) : Visual Studio Build Tools 2022 avec la charge
« Développement Desktop C++ » ; rustup + toolchain stable MSVC ; Node.js LTS + pnpm ;
`tauri-cli` ; GitHub CLI `gh` ; extensions VS Code via `code --install-extension`. Vérification
du runtime WebView2. Après chaque installation, Claude vérifie (`rustc --version`,
`node --version`, `cargo build` d'un hello-world) avant de continuer. Si un installeur exige une
interaction, Claude s'arrête et donne la marche à suivre.

**Connaissances** : le niveau actuel suffit pour le Lot 0. Chaque lot annonce ses concepts ; la
note d'apprentissage du lot précédent est le prérequis du suivant.

## 8. Décisions prises après relecture

Quatre décisions de l'utilisateur : licence **GPL-3.0-or-later**, hébergement **GitHub**,
installation de l'outillage **par Claude via winget**, interface **Tauri 2 + Svelte/TypeScript**
(ne pas exclure une pile web si elle optimise le projet).

Six choix de Claude, à contester si besoin :
1. **Multi-document en onglets**, prévu dans l'état dès le Lot 0, visible au Lot 1.
2. **Double moteur** : PDFium principal ; `lopdf` pour les passes structurelles à partir du Lot 5 ;
   `Document` possède les octets du PDF.
3. **Concurrence en patron acteur** (PDFium n'est pas partageable entre threads).
4. **Chaînes d'interface centralisées** dès le Lot 1, bibliothèque i18n seulement au Lot 7.
5. **TDD strict côté Rust et logique TS pure ; composants Svelte validés manuellement.**
6. **Svelte 5 + pnpm + protocole `mega://`** pour le frontend et le transfert des pages (plutôt
   que React, npm et IPC d'octets).

## 9. Ce qui est laissé au constructeur

- Détail des API de `mega-core` (noms, signatures), découpage interne des modules.
- Choix exacts des raccourcis clavier et de la disposition des panneaux (à valider à l'usage).
- Format des paramètres persistants (TOML via `serde`, emplacement via `directories`).
- Stratégie précise de cache de rendu et seuils de résolution.
- Ordre interne des tâches dans chaque lot (défini dans le plan d'implémentation de ce lot).

## 10. Vérification de bout en bout

- Lot 0 : `cargo build --workspace` et `pnpm check` passent ; `cargo run -p mega-cli -- info
  fixtures/simple.pdf` affiche le nombre de pages ; `pnpm tauri dev` ouvre une fenêtre, le
  dialogue permet de choisir un PDF et la page 1 s'affiche ; CI verte sur GitHub ; `/verif`
  exécutable ; hooks déclenchés sur une modification test.
- Lots suivants : tests unitaires dans `mega-core`, tests d'intégration CLI sur `fixtures/`,
  tests de rendu par comparaison d'images tolérante, Vitest sur la logique TS, et validation
  manuelle guidée par une checklist en fin de lot.

## 11. Prochaines étapes après approbation

1. Enregistrer ce document dans `docs/superpowers/specs/` (premier commit du dépôt).
2. Rédiger le plan d'implémentation détaillé du **Lot 0** avec `superpowers:writing-plans`.
3. Exécuter le Lot 0, puis `/lot 1`.
