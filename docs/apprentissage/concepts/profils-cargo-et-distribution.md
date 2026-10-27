# Profils Cargo et distribution : du `cargo run` au paquet installé

## L'idée en 3 phrases

Cargo compile le même code selon des **profils** : `dev` (rapide à compiler, lent à
l'exécution, plein d'informations de débogage) et `release` (optimisé, plus long à
compiler). Le code peut savoir dans quel profil il est compilé (`cfg!(debug_assertions)`)
ou pour quel système (`cfg!(windows)`, `#[cfg(target_os = …)]`), et se comporter
différemment. Attention à la nuance : `#[cfg(…)]` retire l'élément **avant** la compilation
(il n'existe pas dans le binaire), alors que `cfg!(…)` n'est qu'un booléen constant — les
deux branches du `if` sont compilées et vérifiées, et c'est l'optimiseur qui supprime
ensuite la branche morte.
Distribuer, c'est enfin livrer à côté du binaire ce qu'il ne contient pas (ici la
bibliothèque PDFium) et lui dire où le trouver.

## Dans ce projet

### 1. Le profil `release` (`Cargo.toml:16-19`)

```toml
[profile.release]
lto = "thin"        # optimisation à travers les crates, à l'édition des liens
codegen-units = 1   # une seule unité de compilation : plus lent, mais mieux optimisé
strip = true        # retire les symboles de débogage : binaire plus petit
```

Ces réglages ne touchent que `cargo build --release` (et `tauri build`, qui l'utilise).
`cargo test` et `cargo run` restent en `dev`, donc rapides à compiler.

### 2. Un chemin qui n'existe qu'en développement (`crates/mega-core/src/engine.rs:83-86`)

```rust
if cfg!(debug_assertions) {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
```

`env!("CARGO_MANIFEST_DIR")` est le chemin du dépôt **sur la machine qui compile** : utile
pour trouver `vendor/pdfium/` pendant `cargo test`, absurde chez l'utilisateur. En `release`,
`cfg!(debug_assertions)` vaut `false` : ce chemin n'est jamais utilisé (la branche est
compilée mais morte, et l'optimiseur la retire du binaire).

### 3. Pas de console en `release` (`app/src-tauri/src/main.rs:3`)

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

`cfg_attr(condition, attribut)` n'applique l'attribut que si la condition est vraie : en
`release`, Windows lance l'application sans fenêtre de console noire.

### 4. Un comportement par système (`xtask/src/licenses.rs:164-165`)

```rust
fn pnpm() -> Command {
    if cfg!(windows) {
```

Sous Windows, `pnpm` est un script `.cmd` que `Command::new("pnpm")` ne sait pas lancer : on
passe par `cmd /C pnpm`. Ailleurs, l'appel direct suffit.

### 5. Dire au cœur où est PDFium (`app/src-tauri/src/lib.rs:48-55`)

```rust
let context = tauri::generate_context!();
if let Ok(resources) =
    tauri::utils::platform::resource_dir(context.package_info(), &tauri::Env::default())
    && let Some(dir) = pdfium_resource_dir(&resources)
{
    mega_core::engine::set_library_dir(dir);
}
```

Le dossier des ressources dépend du système : dossier de l'exécutable sous Windows,
`/usr/lib/<app>` pour un paquet deb, `…/Contents/Resources` dans un `.app` macOS. Tauri le
calcule ; on vérifie qu'il contient `pdfium/` (`pdfium_resource_dir`, ligne 36), puis on règle
le dossier **avant** que l'acteur PDFium ne démarre (`OnceLock`, fiche `oncelock`).

### 6. Préparer les fichiers du paquet (`xtask/src/stage.rs:18-23`)

`staged_library_name` choisit `pdfium.dll`, `libpdfium.so` ou `libpdfium.dylib` selon la
cible ; `stage` les copie dans `app/src-tauri/resources/pdfium/`, que la surcouche
`tauri.release.conf.json` embarque (ADR-0012 §6).

## Ce qui se passerait sans

- Sans `cfg!(debug_assertions)` : le message « bibliothèque introuvable » d'un utilisateur
  afficherait `C:\Users\<développeur>\…\vendor\pdfium`, chemin qui n'existe pas chez lui.
- Sans `set_library_dir` : l'application installée ne chercherait PDFium qu'à côté de
  l'exécutable. Sous Windows cela marcherait par chance ; sous Linux (`/usr/bin/` contre
  `/usr/lib/<app>/`) et macOS, échec au premier PDF ouvert.
- Avec les ressources dans la configuration de base : `tauri-build` exige qu'elles existent à
  **chaque** compilation, et `cargo test` échouerait sur un dépôt fraîchement cloné.

## Erreurs fréquentes

- Confondre `cfg!(…)` (une expression `bool`, les deux branches doivent compiler) et
  `#[cfg(…)]` (l'élément disparaît purement et simplement, il peut utiliser du code qui
  n'existe que sur ce système).
- Croire que `cargo test` teste le profil `release` : il faut `cargo test --release`.
- Oublier que `strip = true` retire aussi les noms de fonctions des traces d'erreur.

## Pour aller plus loin

- Le livre Cargo, « Profiles » : <https://doc.rust-lang.org/cargo/reference/profiles.html>
- Rust Reference, « Conditional compilation » :
  <https://doc.rust-lang.org/reference/conditional-compilation.html>

## Exercice de lecture

Relisez `crates/mega-core/src/engine.rs`, tests `#[cfg(debug_assertions)]` et
`#[cfg(not(debug_assertions))]` vers la ligne 216. Pourquoi deux tests au lieu d'un seul
avec un `if cfg!(…)` ? Que lance `cargo test` et que lance `cargo test --release` ?
