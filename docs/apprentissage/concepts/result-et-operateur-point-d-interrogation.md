# `Result` et l'opérateur `?`

## L'idée en trois phrases

En Rust, une fonction qui peut échouer ne lève pas d'exception : elle **retourne** une valeur
`Result<T, E>` qui vaut soit `Ok(valeur)`, soit `Err(erreur)`. Le compilateur oblige à traiter
les deux cas : impossible d'oublier une erreur par inadvertance. L'opérateur `?` est le
raccourci qui rend cela supportable : « si c'est `Ok`, donne-moi la valeur ; si c'est `Err`,
sors immédiatement de la fonction en renvoyant cette erreur (convertie si besoin) ».

## Dans ce projet

### 1. Deux erreurs différentes, un seul `?` — `crates/mega-core/src/document.rs:58-61`

```rust
pub fn open(path: impl AsRef<Path>, password: Option<&str>) -> CoreResult<Document> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;          // Err(std::io::Error)  → Err(CoreError::Io(..))
    let mut doc = Self::from_bytes(bytes, password)?; // Err(CoreError)  → inchangée
```

- `CoreResult<Document>` est un simple alias de `Result<Document, CoreError>`
  (`crates/mega-core/src/error.rs:82`).
- Ligne 60 : `std::fs::read` renvoie `Result<Vec<u8>, std::io::Error>`. Le type d'erreur ne
  correspond pas à `CoreError`… et pourtant `?` compile. Parce que `?` appelle `From::from`
  sur l'erreur avant de la renvoyer, et que `CoreError` sait se construire depuis une
  `io::Error` : c'est ce que génère `#[from]` sur la variante `Io` (`error.rs:62`).
- Ligne 61 : même mécanisme, conversion identité.

### 2. `?` entre deux bibliothèques d'erreurs — `crates/mega-cli/src/commands/info.rs:9-11`

```rust
pub fn run(file: &Path, password: Option<&str>) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;   // CoreError → anyhow::Error
```

`anyhow::Error` accepte **n'importe quelle** erreur qui implémente `std::error::Error`. C'est
pour cela que les applications (`mega-cli`, backend Tauri) utilisent `anyhow`, et que la
bibliothèque (`mega-core`) définit sa propre enum : l'application n'a pas besoin de distinguer
les cas, la bibliothèque doit permettre de le faire.

### 3. Ajouter du contexte en passant — `crates/mega-cli/src/commands/render.rs:24-25`

```rust
std::fs::write(out, rendered.to_png()?)
    .with_context(|| format!("écriture de {}", out.display()))?;
```

Deux `?` sur une ligne : le premier déballe le PNG, le second renvoie l'erreur d'écriture
enrichie d'un message. C'est ce message que `main.rs:33` affiche avec `{e:#}` : « Erreur :
écriture de x.png : Accès refusé ».

### 4. Le bout de la chaîne — `crates/mega-cli/src/main.rs:32-35`

```rust
if let Err(e) = result {
    eprintln!("Erreur : {e:#}");
    std::process::exit(1);
}
```

`?` fait remonter ; quelqu'un doit finir par **décider**. Ici, `main` affiche et sort avec le
code 1. Dans le backend Tauri, c'est la conversion en `AppError` qui décide
(`app/src-tauri/src/error.rs`).

### 5. Collecter des `Result` — `crates/mega-core/src/document.rs:123-125`

```rust
pub fn page_sizes(&self) -> CoreResult<Vec<PageSize>> {
    (0..self.page_count()).map(|i| self.page_size(i)).collect()
}
```

Un itérateur de `Result<PageSize, _>` collecté dans un `Result<Vec<PageSize>, _>` : le premier
`Err` arrête tout et devient le résultat. Pas de `?` visible, même idée.

## Ce qui se passerait sans

Sans `?`, la ligne 60 de `document.rs` s'écrirait :

```rust
let bytes = match std::fs::read(path) {        // version longue, équivalente
    Ok(b) => b,
    Err(e) => return Err(CoreError::from(e)),
};
```

Et sans `Result` du tout (comme en C, où `read` renverrait -1), rien n'obligerait à vérifier :
le programme continuerait avec un tampon vide et planterait plus loin, loin de la cause.

## Erreurs fréquentes

- **`?` dans une fonction qui ne renvoie pas de `Result`** :
  `the `?` operator can only be used in a function that returns Result or Option`. Soit la
  fonction doit renvoyer un `Result`, soit il faut décider sur place (`match`, `unwrap_or`).
  C'est pourquoi `main.rs` ne contient aucun `?` : `main` renvoie `()`.
- **`?: the trait From<X> is not implemented for Y`** : les deux types d'erreur ne se
  connaissent pas. Trois issues : `#[from]` dans l'enum (comme `Io`), un `impl From` manuel
  (comme `From<PdfiumError>` à `error.rs:117`), ou `.map_err(|e| ...)` sur place.
- **Avaler l'erreur** avec `.unwrap()` ou `let _ =` : compile, mais panique (ou se tait) au
  lieu de remonter. Dans ce projet, `unwrap` n'apparaît que dans les tests et pour des
  invariants commentés.

## Pour aller plus loin

Rust Book, chapitre 9.2 : https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html

## Exercice de lecture

`crates/mega-core/src/document.rs:69-75` : `from_bytes` utilise un `match` complet au lieu
d'un simple `?` sur `engine::load_bytes(...)`. Pourquoi ? Que vaut la dernière branche
`other => other?` quand `other` est `Ok(...)`, et quand c'est une autre erreur que
`PasswordRequired` ?
