# MegaUltraPDF — règles du projet pour Claude Code

Lecteur et éditeur PDF libre (GPL-3.0-or-later), sans publicité ni télémétrie, en Rust.
Double finalité : outil utilisable au quotidien **et** support d'apprentissage de Rust pour
le propriétaire du projet (niveau : quelques tutoriels suivis). Claude écrit tout le code ;
l'utilisateur apprend en le lisant.

- Conception de référence : `docs/superpowers/specs/2026-10-26-megaultrapdf-design.md`
- Feuille de route par lots : `docs/ROADMAP.md`
- Notes d'apprentissage : `docs/apprentissage/`
- Décisions d'architecture : `docs/adr/`

## Architecture et frontières

- `crates/mega-core` : **toute** la logique PDF (PDFium via `pdfium-render`). Seul crate
  autorisé à importer `pdfium_render`.
- `crates/mega-cli` : binaire `mega`, banc d'essai et outil réel. N'appelle que `mega-core`.
- `app/src-tauri` (crate `mega-app`) : fine couche Tauri : commandes `invoke`, protocole
  `mega://` servant des PNG, registre des documents. **Aucune logique métier.**
- `app/src` : frontend SvelteKit (SPA, Svelte 5, TypeScript strict). Présentation seulement ;
  toute décision sur un document est une commande vers Rust. Types partagés : `app/src/lib/api.ts`
  reflète `commands.rs` / `error.rs`.
- `xtask` : tâches de dev (`cargo xtask fetch-pdfium`, `cargo xtask gen-fixtures`).
- Une seule instance PDFium globale (`OnceLock`, feature `thread_safe`) ; patron acteur prévu
  au Lot 1. `Document` possède ses octets (`from_bytes` / `to_bytes`) ; `lopdf` (ADR-0003) sert
  depuis le Lot 4 à écrire ce que pdfium-render écrit mal (`forms/write.rs`) : passer par lui
  pour toute écriture de dictionnaire PDF, puis recharger dans PDFium.

## Conventions

- Identifiants (Rust, TS) et messages de commit en **anglais** (Conventional Commits) ;
  commentaires, docs `///`, notes, interface en **français**.
- Première ligne de chaque `.rs`, `.ts`, `.js` : `// SPDX-License-Identifier: GPL-3.0-or-later`
  (`<!-- … -->` dans un `.svelte`). `#![forbid(unsafe_code)]` dans chaque crate.
- Aucune chaîne visible en dur dans un composant : tout passe par `app/src/lib/strings.ts`,
  **dans les deux dictionnaires** `fr` et `en` (même forme, vérifiée par `tsc` et un test).
- Tout dialogue modal : `role="dialog" aria-modal="true" use:trapFocus` (un `<form>` est
  enveloppé d'un `div`) et entrée dans la garde modale de `+page.svelte` ; tout menu
  déroulant : `use:menuNav`. Boutons à glyphe : `aria-label` (ADR-0012).
- Aucun appel réseau à l'exécution. Seul `cargo xtask fetch-pdfium` télécharge (en dev).
- Pas de nouvelle dépendance sans raison écrite dans le message de commit.
- Données venant d'un PDF = données non fiables (voir `display_safe` dans `mega-cli`).
- **Toute écriture de fichier passe par `mega_core::edit::write_atomically`** (temporaire dans
  le même dossier, `sync_all`, renommage) : jamais de `fs::write` direct sur un PDF de
  l'utilisateur. Toute édition est une variante de `EditCommand` appliquée via
  `Document::apply` (historique, « modifié ») ; jamais d'appel direct aux opérations de pages
  depuis le backend Tauri (ADR-0007).
- Gestion des erreurs : ADR-0005 (une enum typée par bibliothèque, conversion aux frontières,
  décision aux extrémités, cause conservée sans répétition). `unwrap`/`expect`/`panic!` sont
  **refusés par lint** hors tests ; un `expect` justifié porte `#[allow(clippy::expect_used)]`
  et un commentaire expliquant pourquoi il ne peut pas se déclencher.

## Pédagogie (non négociable)

- Tout élément `pub` porte une doc `///` en français qui dit **pourquoi** il existe.
- Un concept Rust nouveau pour l'apprenant est signalé par `// [Apprentissage] …`
  (dans une doc `///` ou `//!`, écrire `\[Apprentissage\]` pour que rustdoc ne le prenne pas
  pour un lien) et renvoie si possible à `docs/apprentissage/`.
- Chaque tâche se termine par un court résumé « **Ce que vous venez de voir** » : les concepts
  mobilisés et où les lire (`fichier:ligne`).
- Préférer le code simple et explicite aux abstractions malines. Pas de macro maison.

## Processus

- **TDD strict** pour `mega-core`, `mega-cli`, `app/src-tauri` et la logique TS pure (Vitest) :
  test écrit, vu échouer, puis implémentation. Les composants Svelte sont validés par la
  checklist manuelle de fin de lot (ou par pilotage CDP, voir `docs/apprentissage/lot-0.md`).
- Un lot = `/lot N` → spec du lot → plan → exécution → `/note-apprentissage` → `/verif`.
- Ne jamais déclarer « fini » sans `/verif` tout vert.
- Compétences : `/lot`, `/explique`, `/concept`, `/note-apprentissage`, `/verif`, `/pdf-spec`.
  Agents : `rust-reviewer`, `pdf-expert`, `ui-reviewer`.

## Commandes canoniques

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc -p mega-core --no-deps
pnpm --dir app check      # svelte-check + tsc
pnpm --dir app lint       # eslint
pnpm --dir app test       # vitest
pnpm --dir app format     # prettier
pnpm --dir app tauri dev -- -- fixtures/simple.pdf   # lance l'app sur un fichier
pnpm --dir app release    # paquets avec PDFium embarqué (surcouche tauri.release.conf.json)
cargo xtask licenses      # regénère licenses.json et THIRD_PARTY_LICENSES.md
cargo run -p mega-cli -- info fixtures/simple.pdf
```

## Pièges connus

- PDFium n'est pas thread-safe : ne jamais partager un `PdfDocument` entre threads sans la
  feature `thread_safe` ; ne pas réintroduire `Arc<Mutex<Document>>` hors de l'état Tauri.
- `thread_safe` verrouille **chaque appel**, pas une séquence : l'initialisation passe par
  `OnceLock::get_or_init` et toute opération complète sur un `Document` par `engine::guard()`
  (verrou de processus, jamais imbriqué ; helpers `_unlocked` quand le jeton est déjà pris).
  Trois bugs réels de la CI du Lot 0 (SIGTRAP macOS, erreur de chargement corrompue, SIGSEGV).
- Sous Windows, le protocole `mega://` est vu par le WebView comme `http://mega.localhost/…` ;
  la CSP (`tauri.conf.json`) autorise les deux formes. `convertFileSrc` percent-encode le chemin.
- La DLL PDFium vient de `cargo xtask fetch-pdfium` (`vendor/pdfium/<triple>/`, non commitée).
  Sans elle : `CoreError::LibraryNotFound`. **En profil `release`**, ce dossier n'est plus
  cherché (`cfg!(debug_assertions)`) : l'application installée trouve PDFium dans ses
  ressources (`resources/pdfium/`, réglé par `engine::set_library_dir` dans `run()` avant
  l'acteur), la version portable à côté de l'exécutable, sinon `MEGA_PDFIUM_DIR`.
- Octets intacts (Lot 8) : `Document` garde les octets chargés (`pristine`) et `save_as` les
  écrit tels quels tant que rien n'a changé (signatures préservées). **Toute nouvelle
  méthode publique qui modifie le document appelle `self.mark_changed()` en tête** (en plus
  de `apply`), sinon une modification directe (chemin du CLI) serait perdue à l'enregistrement.
- Paquets : **toujours** `pnpm --dir app release` (jamais `pnpm tauri build` nu, qui
  produirait un paquet sans PDFium). Les ressources sont dans la surcouche
  `tauri.release.conf.json` car `tauri-build` les exige à chaque compilation.
- `pdfium-render` 0.9 : `PdfPageIndex` est `i32`, pas de setter de métadonnées
  (voir `mega-core/src/testing.rs` pour le squelette PDF écrit à la main).
- Les métadonnées d'un PDF sont du texte PDFDocEncoding ou UTF-16BE+BOM, jamais de l'UTF-8 brut.
- PDFium rapporte la taille d'une page **après** `/Rotate` : ne pas réappliquer la rotation
  enregistrée côté interface (seule la rotation d'affichage `doc.rotation` est appliquée).
- `pdfium-render` 0.9 n'expose pas `FPDF_MovePages` : `move_pages` copie puis supprime.
- Annotations (ADR-0008) : **jamais `stroke_color()`/`fill_color()` sur une annotation**
  (pdfium-render 0.9.4 transtype le handle dès qu'un `/AP` existe → comportement indéfini,
  et PDFium génère un `/AP` au premier rendu) : la couleur se lit dans les objets ; couleurs
  posées **avant** toute apparence (`FPDFAnnot_SetColor` refuse ensuite) ; apparences
  personnalisées seulement sur `Ink` et `Stamp` ; aucun binding brut (`forbid(unsafe_code)`).
- Formulaires (ADR-0009) : **ne jamais écrire une valeur de champ avec pdfium-render**
  (`set_checked` écrit `/AS (/Yes)` en chaîne, `set_value` vise le widget et non le champ) :
  `forms/write.rs` (`lopdf`) écrit `/V`, `/AS`, `/NeedAppearances`, puis `reload_from_bytes` ;
  charger une page dans l'environnement de formulaire (rendu minuscule) avant de lire ou
  d'aplatir ; `PdfPage::flatten` aplatit aussi les annotations.
- Contenu et pages tournées (ADR-0010) : **toute l'API publique parle en espace affiché**
  (`geometry::PageFrame`, `frame_of(&page)`) ; convertir vers l'espace utilisateur au seul
  endroit où une coordonnée entre dans PDFium, et inversement à la lecture ; un texte ou une
  image dessinés sur une page tournée se placent en espace affiché puis `apply_matrix` /
  `cm`. `scaled_font_size()` est nul pour un texte tourné : `content::effective_font_size`.
  Après `set_crop`, recharger (`reload_from_bytes`) : PDFium garde les dimensions en cache.
  Filigranes et numéros : flux `lopdf` avec ressources **propres à la page** (copier l'hérité) ;
  PDFium seulement pour mesurer, avant la phase `lopdf` (verrou non réentrant). Une annotation
  fraîche est écrite **inline** dans `/Annots` par PDFium : les passes `lopdf` traitent les
  deux formes.
- Sécurité et métadonnées (ADR-0011) : pdfium-render 0.9.4 ne connaît que les révisions de
  sécurité 2 à 4 (`security_handler_revision()` échoue en AES-256 : inconnu = chiffré, lire
  `/Encrypt /P` soi-même) ; sa balise « ModificationDate » n'existe pas (`/ModDate` lu par
  lopdf) ; `lopdf` déchiffre en silence un mot de passe utilisateur vide (`was_encrypted()`
  avant toute réécriture) ; chiffrer = `EncryptionVersion::V5` + `Aes256CryptFilter` + clé
  `rand` ; les instantanés d'historique portent le mot de passe ; pièces jointes listées dans
  l'ordre de l'arbre de noms (clés triées) ; `cargo clippy --all-targets` échoue tant qu'un
  fichier de test ne compile pas : ne jamais enchaîner `task-done` après un gate qui a échoué.
