# `Mutex` et état partagé

## L'idée en trois phrases

Quand plusieurs threads doivent toucher la même valeur, Rust exige une preuve qu'ils ne le
feront jamais en même temps : `Mutex<T>` est cette preuve, la valeur n'est accessible qu'à
travers `lock()`, qui rend un **jeton** (`MutexGuard`) et libère le verrou quand le jeton est
détruit. Le compilateur refuse de partager entre threads ce qui n'est pas `Send + Sync`, et
`Mutex<T>` rend `Sync` n'importe quel `T: Send`. Reste ce que le compilateur ne peut pas voir :
combien de temps on tient le jeton, et ce qui se passe **entre** deux verrouillages.

## Dans ce projet

### 1. Un verrou qui ne protège rien… sauf l'ordre — `crates/mega-core/src/engine.rs:104-107`

```rust
pub(crate) fn guard() -> PdfiumGuard {                      // PdfiumGuard = MutexGuard<'static, ()>
    static PDFIUM_LOCK: Mutex<()> = Mutex::new(());         // un Mutex autour de… rien
    PDFIUM_LOCK.lock().unwrap_or_else(|e| e.into_inner())   // on reprend même un verrou « empoisonné »
}
```

- `Mutex<()>` : il n'y a pas de donnée à protéger, seulement une règle : **une seule opération
  PDFium complète à la fois** dans le processus.
- `lock()` renvoie `Err` si un thread a paniqué en tenant le verrou (« empoisonné »). Comme il
  n'y a aucune donnée à moitié modifiée, on récupère le jeton (`into_inner`) au lieu de
  propager une erreur.

### 2. Tenir le jeton toute l'opération — `crates/mega-core/src/document.rs:95-131`

Chaque méthode publique commence par `let _guard = engine::guard();` et le jeton vit
jusqu'à l'accolade fermante. Le nom `_guard` (et non `_`) est volontaire : `let _ = guard()`
détruirait le jeton **immédiatement**, et le verrou serait relâché avant la première ligne
utile. Piège classique.

### 3. Pas réentrant : les variantes `_unlocked` — `document.rs:137-149`

`page_size` prend le jeton puis appelle `checked_index_unlocked`, qui appelle
`count_unlocked`. Si `page_size` appelait `page_count` (qui prend le jeton), le même thread
attendrait un verrou qu'il tient déjà : **interblocage**, pas d'erreur de compilation. D'où la
règle écrite dans `engine.rs` : jamais de `guard()` imbriqué ; les fonctions internes
supposent le jeton déjà pris et le disent dans leur nom.

### 4. L'état Tauri — `app/src-tauri/src/commands.rs:37-47`, `lib.rs:39`

```rust
pub type SharedRegistry = Mutex<DocumentRegistry>;          // enregistré avec .manage(...)
registry.lock().map_err(|_| AppError::new(ErrorKind::Internal, "registre … indisponible"))
```

Ici le `Mutex` protège une vraie donnée (la `HashMap` des documents). Tauri impose
`Send + Sync` à tout état géré ; un verrou empoisonné devient une erreur renvoyée au
frontend, pas une panique. Dans `protocol.rs:114-122`, `guard.get(page.id)` emprunte le jeton :
le verrou du registre reste tenu pendant tout le rendu et l'encodage PNG, parce que la
référence au document ne peut pas survivre au jeton (voir la fiche `ownership-et-emprunts`).

### 5. Les trois bugs du Lot 0, et pourquoi « verrouillé » ne suffisait pas

| Bug (CI) | Cause | Correction |
|---|---|---|
| SIGTRAP macOS | deux threads initialisaient PDFium en même temps | `OnceLock::get_or_init` (fiche `oncelock`) |
| « fichier invalide » devenu « erreur inconnue » (Linux/macOS) | PDFium stocke le code d'erreur du dernier chargement dans une variable globale, lu par un **second** appel ; un chargement réussi d'un autre thread s'intercalait | verrou autour de la **séquence** (`guard()` dans `from_bytes`) |
| SIGSEGV (Linux/macOS) sous charge | utilisations concurrentes entrelacées malgré le verrou **par appel** de `pdfium-render` (`thread_safe.rs:64`, un `_lock` par fonction) | `guard()` autour de **chaque opération complète**, destruction comprise |

La leçon tient en une phrase : un verrou garantit l'exclusion **pendant qu'on le tient** ;
si une bibliothèque garde de l'état entre deux appels, il faut tenir le verrou sur toute la
séquence, pas sur chaque appel.

## Ce qui se passerait sans

- Sans `Mutex` : le compilateur refuse de mettre `DocumentRegistry` dans `State` (pas `Sync`).
  C'est l'erreur qu'on **veut** voir : elle remplace un plantage aléatoire en production.
- Avec `RefCell` à la place : même refus (`RefCell` n'est pas `Sync`). `RefCell` est le
  `Mutex` d'un seul thread.
- Avec `Arc<Mutex<Document>>` partagé entre threads au lieu de l'acteur prévu au Lot 1 : ça
  compilerait, mais chaque rendu bloquerait l'interface pendant qu'il tient le verrou.

## Erreurs fréquentes

- `let _ = m.lock()` : jeton détruit sur-le-champ, verrou inutile. Nommer `_guard`.
- Garder le jeton pendant un appel long ou une attente (réseau, autre verrou) : tout le monde
  attend. Ici c'est assumé pour PDFium (une seule opération à la fois) et corrigé au Lot 1.
- Prendre deux verrous dans des ordres différents selon les fonctions : interblocage possible.
  Dans ce projet l'ordre est fixe : registre **puis** PDFium (`respond` → `render_page`).
- `lock().unwrap()` : panique en cascade si un autre thread a paniqué. Décider : `into_inner`
  quand il n'y a pas de donnée à moitié modifiée, erreur propagée sinon.

## Pour aller plus loin

- Rust Book, 16.3 : https://doc.rust-lang.org/book/ch16-03-shared-state.html
- Documentation : https://doc.rust-lang.org/std/sync/struct.Mutex.html

## Exercice de lecture

`crates/mega-core/src/document.rs:123-125` : `page_sizes` appelle `page_size(i)` dans une
boucle, et chaque `page_size` prend puis relâche le jeton PDFium. Un autre thread peut donc
s'intercaler entre deux pages. Est-ce un problème de **correction** (résultat faux possible) ou
seulement de **performance** ? Que faudrait-il changer pour tenir le jeton une seule fois sur
toute la boucle, sans interblocage ?
