# Itérateurs et fermetures

## L'idée en trois phrases

Un **itérateur** produit des valeurs une à une (`next()`), et la bibliothèque standard fournit
des dizaines d'**adaptateurs** (`map`, `filter`, `find`, `skip`…) qui se chaînent sans rien
calculer tant qu'un **consommateur** (`collect`, `for`, `count`, `sum`) ne tire pas dessus.
Une **fermeture** (`|x| …`) est une petite fonction anonyme qui peut capturer les variables
autour d'elle ; c'est l'argument naturel des adaptateurs. Le résultat se lit comme une phrase
(« les arguments, sauf le premier, rangés dans un vecteur ») et compile vers une boucle aussi
rapide qu'à la main.

## Dans ce projet

### 1. La phrase la plus courante — `xtask/src/main.rs:27-28`

```rust
let args: Vec<String> = std::env::args().skip(1).collect(); // tous les arguments sauf le nom du programme
match args.first().map(String::as_str) {                    // Option<&String> → Option<&str>
```

- `skip(1)` n'avance pas tout de suite : rien ne se passe avant `collect()`.
- `collect()` a besoin de connaître le type cible : d'où l'annotation `Vec<String>`.
- `.map(String::as_str)` : une fonction nommée passée là où on attend une fermeture ; les deux
  sont interchangeables quand la signature correspond.

### 2. Chercher et transformer d'un coup — `xtask/src/main.rs:54-56`

```rust
text.lines()
    .find_map(|l| l.strip_prefix("host: "))  // la première ligne qui commence par "host: ", déjà découpée
    .map(str::to_string)                     // on possède la chaîne au lieu d'emprunter `text`
```

`find_map` arrête la recherche au premier `Some` : les lignes suivantes ne sont jamais lues.

### 3. Une fermeture qui capture — `xtask/src/pdfium.rs:65`

```rust
.find(|a| a.targets.iter().any(|t| t == target))
```

La fermeture interne compare à `target`, une variable **de la fonction englobante** : c'est la
capture. Rust décide seul si la fermeture emprunte (`&target`, ici) ou prend la propriété ;
on peut forcer la seconde option avec `move` (nécessaire quand la fermeture part dans un
thread, comme dans le test de concurrence de `tests/document.rs`).

### 4. Collecter des `Result` — `crates/mega-core/src/document.rs:124`

```rust
(0..self.page_count()).map(|i| self.page_size(i)).collect()
```

Un intervalle `0..n` est un itérateur. Chaque `page_size` rend un `CoreResult<PageSize>`.
`collect()` dans un `CoreResult<Vec<PageSize>>` s'arrête au premier `Err` et le renvoie :
tout-ou-rien, sans `?` ni boucle visible. Même mécanisme que la fiche `result-et-operateur-point-d-interrogation`, point 5.

### 5. Transformer caractère par caractère — `crates/mega-cli/src/commands/info.rs:31-41`

```rust
s.chars()
    .map(|c| if is_display_dangerous(c) { '\u{FFFD}' } else { c })
    .collect()   // le type de retour (String) dit à collect de reconstruire une chaîne
```

`chars()` itère sur des caractères Unicode, pas des octets : « é » ou « — » restent entiers.

### 6. Filtrer en convertissant — `app/src-tauri/src/commands.rs:95-98`

```rust
std::env::args_os().skip(1).filter_map(|a| a.into_string().ok())
```

`filter_map` garde les `Some` et jette les `None` : un argument qui n'est pas de l'Unicode
valide disparaît au lieu de faire paniquer `env::args()`. Aucune allocation intermédiaire :
l'itérateur est consommé directement par `startup_file_from_args`.

### 7. `Option` se manipule comme un itérateur d'au plus un élément — `document.rs:97-100`

```rust
.get(PdfDocumentMetadataTagType::Title)
.map(|tag| tag.value().trim().to_string())  // Some(tag) → Some(String)
.filter(|s| !s.is_empty())                  // Some("") → None
```

## Ce qui se passerait sans

- Sans paresse : `lines().find_map(...)` lirait toutes les lignes avant de chercher. Avec des
  itérateurs, on écrit la version lisible et on obtient la version économe.
- Sans fermetures : chaque `map` exigerait une fonction nommée à part, loin de son usage, et
  passer `target` demanderait un paramètre supplémentaire ou une structure.
- En écrivant les boucles à la main : plus de lignes, des index à gérer (`i < len`), et les
  bugs qui vont avec. Les itérateurs n'ont pas d'index hors bornes.

## Erreurs fréquentes

- `type annotations needed` sur `collect()` : dire dans quoi collecter (`Vec<_>`, `String`,
  `Result<Vec<_>, _>`), soit sur la variable, soit avec `collect::<Vec<_>>()`.
- « Rien ne se passe » : un `map` sans consommateur n'exécute rien (avertissement
  `unused Iterator that must be used`). Terminer par `collect`, `for`, `count`, `for_each`.
- `closure may outlive the current function, but it borrows x` : la fermeture part ailleurs
  (thread, stockage) en empruntant une variable locale. Solution : `move`, souvent avec un
  `Arc::clone` avant (voir `tests/document.rs`, test de concurrence).
- Appeler `.iter()` et modifier la collection dans la même boucle : refusé par l'emprunteur ;
  collecter d'abord, modifier ensuite.

## Pour aller plus loin

- Rust Book, chapitre 13 : https://doc.rust-lang.org/book/ch13-00-functional-features.html
- Les adaptateurs : https://doc.rust-lang.org/std/iter/trait.Iterator.html

## Exercice de lecture

`crates/mega-core/src/error.rs:110-114`, fonction `format_paths` : la chaîne
`iter().map(…).collect::<Vec<_>>().join(", ")` construit un `Vec<String>` intermédiaire avant
de joindre. Pourquoi ce vecteur est-il nécessaire ici (que demande `join`) ? Réécrivez
mentalement la même fonction avec une boucle `for` et un `String` : laquelle des deux versions
préférez-vous lire, et pourquoi ?
