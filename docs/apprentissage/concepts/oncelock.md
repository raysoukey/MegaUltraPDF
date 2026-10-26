# `OnceLock` : initialiser une fois, lire partout

## L'idée en trois phrases

Certaines valeurs doivent exister **une seule fois** par programme et être lisibles depuis
n'importe quel thread : une bibliothèque native chargée, une configuration lue au démarrage.
`static` seul ne suffit pas, car un `static` doit être construit à la compilation et ne peut
pas être modifié sans `unsafe`. `OnceLock<T>` est une case vide à la compilation, remplie
**exactement une fois** à l'exécution, puis en lecture seule : la bonne réponse de la
bibliothèque standard, sans `unsafe` et sans dépendance.

## Dans ce projet

### 1. Le cas simple — `xtask/src/pdfium.rs:50-53`

```rust
fn lock() -> &'static Lock {
    static LOCK: OnceLock<Lock> = OnceLock::new();                 // vide à la compilation
    LOCK.get_or_init(|| toml::from_str(LOCK_TOML).expect("pdfium.lock.toml est invalide"))
}
```

- `static … = OnceLock::new()` est autorisé parce que `new()` est `const`.
- `get_or_init(f)` : si la case est pleine, renvoie la valeur ; sinon exécute `f`, stocke le
  résultat, le renvoie. Toujours une référence `&'static` : la valeur vit jusqu'à la fin du
  programme, on peut la passer partout sans se soucier de durée de vie.
- Le `static` est déclaré **dans** la fonction : personne d'autre ne peut y toucher.

### 2. Le cas qui a planté la CI — `crates/mega-core/src/engine.rs:72-79`

Version d'origine (commit précédant `4fbaad6`), qui semblait raisonnable :

```rust
if let Some(p) = PDFIUM.get() { return Ok(p); }   // 1. déjà chargé ? on rend
let fresh = load()?;                                // 2. sinon on charge…
let _ = PDFIUM.set(fresh);                          // 3. …et on tente d'écrire (le 2e perd)
```

Le commentaire disait : « deux threads peuvent charger en même temps, le second résultat est
jeté, pas grave ». Faux : entre 1 et 3, deux threads exécutaient `load()` en parallèle, donc
**deux initialisations simultanées de PDFium**, que la bibliothèque ne tolère pas. Linux et
Windows passaient par chance de timing ; macOS arm64 plantait (SIGTRAP) dès que les tests
tournaient en parallèle. Version actuelle :

```rust
static PDFIUM: OnceLock<Result<Pdfium, Vec<PathBuf>>> = OnceLock::new();
match PDFIUM.get_or_init(load) {                     // une seule exécution de load(), garantie
    Ok(p) => Ok(p),
    Err(searched) => Err(CoreError::LibraryNotFound { searched: searched.clone() }),
}
```

`get_or_init` prend un verrou interne pendant l'initialisation : les autres threads
**attendent** le premier au lieu de recommencer. C'est toute la différence entre « écrit une
fois » (`set`, qui ignore les perdants) et « initialisé une fois » (`get_or_init`).

### 3. Mémoriser aussi l'échec — même extrait

La case contient un `Result`, pas un `Pdfium`. Pourquoi ? `get_or_init` ne sait pas « réessayer
plus tard » : si `load()` échoue, il faut stocker quelque chose. On stocke l'échec (la liste des
dossiers essayés) et on le retraduit en `CoreError` à chaque appel. Conséquence assumée :
après `cargo xtask fetch-pdfium`, il faut relancer le programme.

### 4. Le test qui garde le cas — `crates/mega-core/tests/document.rs:113-140`

Huit threads attendent derrière une `Barrier`, puis chargent, rendent et détruisent des
documents en boucle. Sans la correction, ce test plante sous macOS ; avec, il passe quatre
fois de suite sur les trois systèmes (vérifié en CI).

## Ce qui se passerait sans

- Avec un `static mut` et une initialisation manuelle : `unsafe`, et la même course.
- Avec un `Mutex<Option<Pdfium>>` : correct, mais chaque lecture prend un verrou et l'on
  manipule une `Option` à chaque appel. `OnceLock` ne verrouille **que** pendant
  l'initialisation ; ensuite `get()` est une lecture sans verrou.
- Avec un paramètre `pdfium: &Pdfium` passé partout : `Document` devrait porter une durée de
  vie (`PdfDocument<'a>` emprunte `Pdfium`) et ne pourrait plus vivre dans un `HashMap`
  longue durée. Le `'static` du `OnceLock` est ce qui rend `Document` simple.

## Erreurs fréquentes

- `get()` + `set()` au lieu de `get_or_init()` : la course ci-dessus. Règle : si le calcul
  d'initialisation a des effets de bord ou coûte cher, c'est `get_or_init`, toujours.
- Appeler `get_or_init` **depuis** la fonction d'initialisation (réentrance) : interblocage
  garanti. Ici `load()` n'appelle jamais `pdfium()`.
- Vouloir une valeur mutable après coup : `OnceLock` est en lecture seule ; pour un état qui
  change, c'est `Mutex`/`RwLock` (fiche `mutex-et-etat-partage`).
- Confondre avec `thread_local!` : une valeur **par thread**, pas une valeur partagée.

## Pour aller plus loin

- Documentation : https://doc.rust-lang.org/std/sync/struct.OnceLock.html
- Rust Book, 16.3 (état partagé) : https://doc.rust-lang.org/book/ch16-03-shared-state.html

## Exercice de lecture

`crates/mega-core/src/engine.rs:120-133`, fonction `load()` : elle renvoie
`Result<Pdfium, Vec<PathBuf>>` et non `CoreResult<Pdfium>`. Pourquoi ce type d'erreur
« pauvre » ? Indice : que faut-il pouvoir faire de la valeur stockée dans le `OnceLock` à
**chaque** appel suivant, et `CoreError` le permet-il ?
