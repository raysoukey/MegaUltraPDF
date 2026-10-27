# ADR-0012 — Interface bilingue par remontage, accessibilité en logique pure, paquets avec PDFium embarqué

Date : 2026-10-27 · Statut : accepté (Lot 7)

## Contexte

Le Lot 7 rend l'application utilisable par d'autres que son auteur : en français et en
anglais, au clavier et avec un lecteur d'écran, réglable, documentée, et installable sur les
trois OS. Jusqu'ici toutes les chaînes vivaient dans un objet `t` constant, les menus ne
s'utilisaient qu'à la souris, et PDFium n'était trouvé qu'à côté de l'exécutable ou dans
`vendor/` (profil de développement).

## Décisions

1. **Deux dictionnaires de même forme.** `strings.ts` exporte `fr` (référence) et
   `en: Strings` où `Strings = typeof fr` : une clé manquante ou une fonction d'arité
   différente est une erreur `tsc`, doublée d'un test Vitest qui compare les deux formes
   récursivement. `t` est un objet mutable rempli par `setLanguage` (`Object.assign`).
2. **Changer de langue = remonter l'interface.** `+page.svelte` enveloppe `<main>` dans
   `{#key ui.language}` : chaque composant relit `t` à sa création. L'état des documents
   (onglets, page, historique) vit dans des modules (`documents.svelte.ts`) et survit ; seuls
   les états locaux des composants (un champ en cours de saisie) sont perdus. Plus simple
   qu'un `t` réactif lu à environ 450 endroits des composants. La langue est connue **de façon synchrone** au
   démarrage grâce à un miroir `localStorage` de la préférence (le store Tauri est
   asynchrone) : aucun affichage dans la mauvaise langue. Limite assumée : les messages
   d'erreur détaillés du cœur Rust restent en français.
3. **Accessibilité : la décision en logique pure, le DOM dans une action.**
   `logic/menu.ts` (`menuKey`), `logic/focus.ts` (`nextFocus`) et `logic/contrast.ts`
   (`contrastRatio`, jetons lus dans `app.css`) sont testés sous Vitest ; les actions
   `trapFocus` et `menuNav` ne font que les relier au DOM. Tous les dialogues sont annoncés
   `role="dialog" aria-modal="true"` (les `<form>` sont enveloppés d'un `div`, Svelte
   refusant ce rôle sur un formulaire ; `.dialog-form { display: contents }`) ; Échap dans
   un menu ouvert est arrêté (`stopPropagation`) pour ne pas fermer le dialogue ou la
   recherche en dessous. La garde modale de `+page.svelte` couvre désormais aussi les
   dialogues mot de passe, propriétés, préférences et aide. Le scénario CDP `finition.json`
   vérifie le tout dans l'application réelle.
4. **Préférences appliquées immédiatement**, sans bouton « Valider » ; l'aide (F1) génère sa
   table depuis `shortcuts.ts` ; « À propos » lit la version par `getVersion` (Tauri).
5. **Licences générées et commitées.** `cargo xtask licenses` lit `cargo metadata` et
   `pnpm licenses list --json` : toutes les crates (toutes plateformes, dev compris) et tous
   les paquets npm (les runtimes Svelte sont des devDependencies compilées dans le bundle).
   Sur-ensemble annoncé plutôt qu'oubli. La notice PDFium livrée combine le `LICENSE` de
   l'archive (empaqueteur pdfium-binaries, MIT) et `licenses/*` (PDFium BSD-3 et les 15
   bibliothèques qu'il inclut), avec repli Latin-1 (`freetype.txt` n'est pas UTF-8).
6. **PDFium dans les ressources du paquet.** `cargo xtask stage-pdfium` copie la
   bibliothèque et sa notice dans `app/src-tauri/resources/pdfium/` (ignoré par git).
   Les ressources sont déclarées dans une **surcouche** `tauri.release.conf.json`
   (`pnpm --dir app release` = `tauri build --config …`) et non dans la configuration de
   base : `tauri-build` vérifie l'existence des ressources à **chaque** compilation, ce qui
   casserait `cargo test` et `clippy` tant que PDFium n'est pas préparé. Au démarrage,
   `run()` calcule le dossier des ressources (`tauri::utils::platform::resource_dir`, sans
   attendre `setup`) et appelle `engine::set_library_dir` **avant** de démarrer l'acteur
   PDFium. Ordre de recherche : dossier réglé par l'application, `MEGA_PDFIUM_DIR`, dossier
   de l'exécutable (version portable), `vendor/` en développement.
7. **Paquets non signés, publiés en brouillon.** `release.yml` (étiquette `v*` ou à la main)
   construit NSIS + MSI + zip portable (Windows), deb + AppImage (Ubuntu 22.04, glibc plus
   ancienne donc plus compatible), dmg (macOS Apple Silicon) et les joint par `gh` à une
   release brouillon ; pas d'action tierce de publication. Le profil `release` du workspace
   (LTO « thin », `codegen-units = 1`, `strip`) date du Lot 0 et reste tel quel.
8. **OCR et signature numérique cryptographique forment un Lot 8** (fonctionnalités
   optionnelles) : dépendances natives ou cryptographiques qui méritent leur propre relecture.

## Conséquences

- Ajouter une chaîne = l'ajouter dans les deux dictionnaires, sinon `pnpm check` échoue.
- Un nouveau dialogue doit porter `role="dialog" aria-modal="true" use:trapFocus` et entrer
  dans la garde modale de `+page.svelte` ; un nouveau menu, `use:menuNav`.
- `pnpm tauri build` sans la surcouche produit un paquet **sans** PDFium : toujours passer
  par `pnpm --dir app release`.
- macOS Intel n'a pas de paquet (pas de runner dédié) ; la version portable Windows cherche
  `pdfium.dll` à côté de l'exécutable.
