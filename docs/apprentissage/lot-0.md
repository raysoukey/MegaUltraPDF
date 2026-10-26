# Lot 0 — Fondations et environnement : note d'apprentissage

## Ce que le lot a construit

Un dépôt complet qui compile de bout en bout : un cœur Rust (`mega-core`) capable d'ouvrir
un PDF, lire son titre et ses pages, et rendre une page en image ; un outil en ligne de
commande (`mega info`, `mega render`) ; une application de bureau (Tauri 2 + Svelte) qui ouvre
un PDF depuis un dialogue ou un argument et affiche sa première page ; l'outillage de
développement (`cargo xtask fetch-pdfium`, hooks, compétences Claude Code, CI).

## Concepts Rust rencontrés (du plus simple au plus avancé)

### 1. Workspace, crates, modules

Un *workspace* Cargo regroupe plusieurs *crates* qui partagent un `Cargo.lock` et un dossier
`target/`. `Cargo.toml` à la racine liste les membres ; chaque crate a son propre manifeste.
Dans un crate, `mod error;` dans `lib.rs` déclare un module dont le code vit dans `error.rs` ;
`pub use error::CoreError;` le réexporte pour que l'utilisateur écrive `mega_core::CoreError`.

- À lire : `Cargo.toml` (racine), `crates/mega-core/src/lib.rs` (déclarations `mod` et `pub use`).

### 2. `Result`, `?` et un type d'erreur par crate

Une fonction qui peut échouer retourne `Result<T, E>`. L'opérateur `?` « déballe » un `Ok` ou
retourne immédiatement l'`Err` en le convertissant via `From`. `mega-core` définit **une** enum
d'erreurs (`CoreError`) avec la crate `thiserror`, qui écrit `Display` et `Error` pour nous.
Les applications (`mega-cli`) utilisent `anyhow`, qui accepte n'importe quelle erreur et garde
la chaîne des causes (`{e:#}`).

- À lire : `crates/mega-core/src/error.rs` (enum, `#[from]`, `impl From<PdfiumError>`),
  `crates/mega-cli/src/main.rs` (affichage de l'erreur et code de sortie 1).

### 3. `struct`, `impl`, ownership et emprunts

`Document` possède (`owns`) son `PdfDocument` interne et un `Option<PathBuf>`. Les méthodes
qui lisent prennent `&self` (emprunt partagé) ; `from_bytes` prend `bytes: Vec<u8>` **par
valeur** : la propriété des octets passe à PDFium, plus besoin de les copier. Quand un
`Document` sort de portée, `Drop` libère tout.

- À lire : `crates/mega-core/src/document.rs` (`Document`, `open`, `from_bytes`, `page_size`),
  `app/src-tauri/src/registry.rs` (le registre possède les documents dans une `HashMap`).

### 4. Initialisation paresseuse et unique : `OnceLock`

PDFium doit être chargée une seule fois par processus. `static PDFIUM: OnceLock<Pdfium>` est
une case « écrite une fois » lisible depuis n'importe quel thread, qui donne une référence
`'static`. C'est ce qui permet à `Document` de contenir un `PdfDocument<'static>` sans
structure auto-référencée.

- À lire : `crates/mega-core/src/engine.rs` (`pdfium()`), `xtask/src/pdfium.rs` (`lock()`).

### 5. Traits dérivés et sérialisation avec `serde`

`#[derive(Serialize)]` sur `PageSize` et `DocumentInfo` génère le code qui les transforme en
JSON ; Tauri s'en sert pour livrer les valeurs au frontend. `AppError` implémente `Serialize`
**à la main** pour fixer la forme `{ kind, message }` que `api.ts` attend.

- À lire : `crates/mega-core/src/document.rs` (`PageSize`), `app/src-tauri/src/error.rs`
  (`impl Serialize for AppError`), `app/src/lib/api.ts` (les mêmes types côté TypeScript).

### 6. État partagé entre threads : `Mutex` et `State` Tauri

Les commandes Tauri peuvent s'exécuter en parallèle. Le registre est donc dans un
`Mutex<DocumentRegistry>` enregistré avec `.manage(...)` ; chaque commande le verrouille le
temps strict de son travail. Un verrou « empoisonné » est transformé en erreur plutôt qu'en
panique.

- À lire : `app/src-tauri/src/commands.rs` (`lock_registry`, `open_document`),
  `app/src-tauri/src/lib.rs` (`.manage`).

### 7. Itérateurs et fermetures

`std::env::args().skip(1).collect()`, `paths.iter().map(...).collect::<Vec<_>>().join(", ")`,
`(0..count).map(|i| self.page_size(i)).collect()` : des chaînes d'itérateurs avec des
fermetures (`|x| …`). Le dernier exemple collecte des `Result` en un seul `Result<Vec<_>>` :
le premier `Err` arrête tout.

- À lire : `xtask/src/main.rs` (`main`, `host_target`), `crates/mega-core/src/document.rs`
  (`page_sizes`), `crates/mega-cli/src/commands/info.rs` (`display_safe`).

### 8. Tests : unitaires, d'intégration, de bout en bout

- Unitaires : `#[cfg(test)] mod tests` dans le fichier (`xtask/src/pdfium.rs`, `protocol.rs`).
- D'intégration : `crates/mega-core/tests/document.rs` n'a accès qu'à l'API publique.
- De bout en bout : `crates/mega-cli/tests/cli.rs` lance le vrai binaire avec `assert_cmd`.
- La feature Cargo `testing` expose `sample_pdf` (générateur de PDF) sans l'embarquer dans
  l'application finale.

## Décisions de conception du lot

Voir `docs/adr/` : 0001 (PDFium), 0002 (Tauri + Svelte), 0003 (double moteur, `Document`
sérialisable), 0004 (`Pdfium` global en `OnceLock`, acteur reporté au Lot 1).

## Pièges rencontrés pendant le développement

- **Encodage des chaînes PDF** : un titre écrit en UTF-8 brut dans `(…)` est lu en
  PDFDocEncoding → « — » devenait « â•ﬂ ». Correction : hexadécimal UTF-16BE avec BOM `FEFF`
  (`crates/mega-core/src/testing.rs`, `pdf_text_string`). Test : `title_with_non_ascii_characters_roundtrips`.
- **Format A4 de PDFium** : 595,28 × 841,89 pt, pas 595 × 842 ; le test de rendu dérive la
  taille attendue de `page_size()` au lieu de coder des nombres.
- **Pas de setter de métadonnées** dans `pdfium-render` : le générateur de tests écrit un
  squelette PDF minimal à la main (objets, `xref` avec positions calculées, `trailer`).
- **Données non fiables** : un titre de PDF peut contenir des séquences d'échappement ANSI ou
  des caractères bidirectionnels ; `display_safe` les neutralise avant affichage terminal.
- **`PdfPageIndex` est `i32`** : l'API publique garde `u16` et convertit après vérification.
- **Protocole personnalisé sous Windows** : `mega://` devient `http://mega.localhost/…`, et
  `convertFileSrc` percent-encode le chemin (`page%2F1%2F0`) : le backend décode.
- **PowerShell 5.1 et l'UTF-8** : un `.ps1` sans BOM est lu en ANSI ; les hooks sont en ASCII.
- **Vérifier l'interface sans cliquer** : WebView2 accepte
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` ; un script Node parle
  alors le protocole DevTools (évaluer du JS, capturer l'écran). Lancer l'app sur un fichier :
  `pnpm --dir app tauri dev -- -- fixtures/simple.pdf`.

## Exercices de lecture

1. Dans `crates/mega-core/src/document.rs`, pourquoi `open` appelle-t-il `from_bytes` puis
   modifie `doc.path` ensuite, plutôt que de construire le `Document` directement ?
2. Dans `app/src-tauri/src/protocol.rs`, que se passe-t-il si deux requêtes `mega://` arrivent
   en même temps ? Où est le verrou, et combien de temps est-il tenu ?
3. Dans `crates/mega-core/src/error.rs`, retirez mentalement `#[from]` sur `Io` : quelles lignes
   du projet ne compileraient plus, et pourquoi ?

## Prérequis pour le Lot 1

Être à l'aise avec `Result`/`?`, les emprunts `&self`/`&mut self`, et l'idée qu'un `Mutex`
protège une valeur partagée. Le Lot 1 introduit les threads, les canaux (`mpsc`) et le patron
acteur pour le rendu asynchrone, puis un cache LRU.
