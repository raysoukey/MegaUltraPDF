# `thiserror` : la cause (`source`) n'est pas le message (`Display`)

## L'idée en trois phrases

Une erreur Rust a deux faces : son **message**, produit par le trait `Display`, et sa
**cause**, renvoyée par `Error::source()`, qui est elle-même une erreur avec ses deux faces.
`thiserror` remplit les deux à partir d'attributs : `#[error("…")]` écrit le message,
`#[source]` ou `#[from]` désigne la cause. Si le message recopie la cause, tout afficheur
qui remonte la chaîne des causes la montre deux fois : c'est le bogue réel de ce projet.

## Le bogue réel (commit `b2ecc88`)

Avant, dans `crates/mega-core/src/error.rs` :

```rust
#[error("Erreur d'accès au fichier : {0}")]   // {0} recopie la cause dans le message…
Io(#[from] std::io::Error),                    // …et #[from] la déclare aussi comme source
```

Sortie observée de `mega info fixtures` (un dossier, pas un fichier) :

```
Erreur : Erreur d'accès au fichier : Accès refusé. (os error 5): Accès refusé. (os error 5)
```

`anyhow`, avec `{e:#}` (`crates/mega-cli/src/main.rs:44`), affiche le message **puis** chaque
cause. Le message contenait déjà la cause : elle est sortie deux fois. Après correction
(`error.rs:61-62`) :

```rust
#[error("Erreur d'accès au fichier")]          // le message ne dit que ce que cette couche sait
Io(#[from] std::io::Error),                    // la cause reste accessible par source()
```

## Dans ce projet

### 1. `#[from]` fait deux choses — `crates/mega-core/src/error.rs:56-62`

`#[from]` génère `impl From<io::Error> for CoreError` (ce qui permet `?`, voir la fiche
`result-et-operateur-point-d-interrogation`) **et** implémente `source()` pour renvoyer
l'`io::Error` enveloppée. Les deux sont liés : si l'on veut `?` sans source, il faut écrire
`From` à la main.

### 2. Remonter la chaîne soi-même — `error.rs:95-103`

```rust
pub fn display_chain(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();   // le message de ce maillon
    let mut cause = err.source();     // la cause, s'il y en a une
    while let Some(c) = cause {       // tant qu'il y a un maillon suivant…
        text.push_str(" : ");
        text.push_str(&c.to_string()); // …on ajoute son message
        cause = c.source();            // …et on passe au suivant
    }
    text
}
```

`&dyn std::error::Error` : la fonction accepte n'importe quel type d'erreur, pas seulement
`CoreError`. C'est le même algorithme que `{e:#}` d'`anyhow`.

### 3. Deux extrémités, un seul format — `crates/mega-cli/src/main.rs:43-45` et
`app/src-tauri/src/error.rs:60-61`

Le CLI laisse `anyhow` assembler la chaîne ; le backend Tauri appelle `display_chain(&e)` pour
remplir `AppError.message`, parce que le frontend ne verra jamais la cause autrement : la
sérialisation JSON ne transporte que du texte.

### 4. Les tests qui gardent la porte

- `crates/mega-core/tests/document.rs:91-96` : `io_error_message_mentions_the_cause_exactly_once`
  compte les occurrences de `os error` dans la chaîne complète : exactement une.
- `crates/mega-cli/tests/cli.rs:33` : même compteur sur la sortie réelle du binaire.

## Ce qui se passerait sans

- **Message qui recopie la cause** : le doublon ci-dessus, et chaque couche supplémentaire
  ajouterait une copie de plus.
- **Le choix inverse**, recopier la cause mais ne pas la marquer `#[source]` : le texte est
  correct, mais la chaîne est coupée. Un outil qui inspecte les causes (journalisation,
  `anyhow::Error::chain()`, un futur « Détails » dans l'interface) ne verrait plus l'`io::Error`
  d'origine ni son `kind()` (fichier introuvable, droits insuffisants…).

## Erreurs fréquentes

- `#[error("… {0}")]` **et** `#[from]`/`#[source]` sur le même champ : le doublon. Règle :
  le message décrit ce que cette couche sait ; la cause parle d'elle-même.
- Un champ de type erreur sans `#[source]` : la chaîne s'arrête là sans prévenir. `thiserror`
  ne devine `#[source]` automatiquement que pour un champ nommé `source`.
- `map_err(|e| e.to_string())` ou `CoreError::Pdfium(format!("{e:?}"))` : aplatit l'erreur en
  texte, plus de `source()`, plus de `kind()`. Acceptable comme filet (`Pdfium(String)`),
  mauvaise habitude ailleurs.

## Pour aller plus loin

- Rust Book, 9.2 : https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html
- `std::error::Error::source` : https://doc.rust-lang.org/std/error/trait.Error.html#method.source

## Exercice de lecture

`app/src-tauri/src/error.rs:49-62` : pourquoi `AppError::from` utilise-t-il
`display_chain(&e)` plutôt que `e.to_string()` ? Pour un fichier en lecture seule qu'on ne
peut pas ouvrir, qu'afficherait l'application dans chaque cas ? Et quel champ d'`AppError`
sert au frontend à **décider** (afficher la saisie de mot de passe, par exemple) plutôt qu'à
afficher ?
