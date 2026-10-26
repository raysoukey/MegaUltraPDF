# Ownership et emprunts

## L'idée en trois phrases

Chaque valeur Rust a exactement **un propriétaire** ; quand il disparaît, la valeur est
libérée, sans ramasse-miettes et sans `free` à écrire. On peut **prêter** une valeur sans en
céder la propriété : `&T` pour lire (autant de lecteurs qu'on veut), `&mut T` pour modifier
(un seul à la fois, et aucun lecteur pendant ce temps). Le compilateur vérifie ces règles à
la compilation : c'est ce qui élimine les accès après libération et les courses de données.

## Dans ce projet

### 1. Prendre par valeur = prendre la propriété — `crates/mega-core/src/document.rs:67`

```rust
pub fn from_bytes(bytes: Vec<u8>, password: Option<&str>) -> CoreResult<Document>
```

- `bytes: Vec<u8>` : l'appelant **donne** ses octets. Après l'appel, il ne peut plus les
  utiliser (le compilateur refuserait : *value moved*). En échange, aucune copie : PDFium garde
  ce tampon tel quel tant que le document vit.
- `password: Option<&str>` : un simple **prêt en lecture**. L'appelant garde sa chaîne.
- Règle de lecture : paramètre sans `&` → la fonction en devient propriétaire ; `&` → prêt ;
  `&mut` → prêt avec droit de modification.

### 2. Construire, puis compléter — `document.rs:58-63`

```rust
let bytes = std::fs::read(path)?;                 // `open` possède les octets…
let mut doc = Self::from_bytes(bytes, password)?; // …et les donne ; `doc` lui appartient
doc.path = Some(path.to_path_buf());              // modification : `doc` est `mut`
Ok(doc)                                           // la propriété de `doc` passe à l'appelant
```

`to_path_buf()` copie le chemin emprunté (`&Path`) dans un `PathBuf` possédé : le `Document`
ne doit pas dépendre de la durée de vie d'une variable de l'appelant.

### 3. Prêter un morceau de soi — `document.rs:89-90`

```rust
pub fn path(&self) -> Option<&Path> { self.path.as_deref() }
```

`&self` : la méthode emprunte le document ; elle rend une référence **dans** le document.
Tant que l'appelant garde ce `&Path`, il ne peut pas détruire ni modifier le `Document` : le
compilateur s'en assure. À comparer avec `title()` (l. 94) qui rend une `String` **possédée** :
l'appelant peut la garder sans bloquer le document.

### 4. Un conteneur qui possède ses éléments — `app/src-tauri/src/registry.rs:23-42`

```rust
docs: HashMap<DocId, Document>,
pub fn insert(&mut self, doc: Document) -> DocId   // transfert : le registre devient propriétaire
pub fn get(&self, id: DocId) -> Option<&Document>  // prêt en lecture, lié à la durée du registre
pub fn remove(&mut self, id: DocId) -> bool        // le Document retiré est détruit ici : PDFium libère
```

Dans `commands.rs:56-65`, on lit `doc.title()` et `doc.page_sizes()` **avant** `insert(doc)` :
après le transfert, `doc` n'existe plus dans la fonction.

### 5. Libérer sous contrôle — `document.rs:45-50`

```rust
impl Drop for Document {
    fn drop(&mut self) {
        let _guard = engine::guard();
        self.inner.take();   // libère le PdfDocument pendant qu'on tient le verrou PDFium
    }
}
```

`Drop` est appelé automatiquement quand le dernier propriétaire disparaît : fin de fonction,
`remove` du registre, remplacement dans une variable. On n'écrit jamais « fermer le document »
ailleurs. `Option::take()` sort la valeur pour qu'elle soit détruite **à cet endroit précis**.

## Ce qui se passerait sans

- Si `from_bytes` prenait `&[u8]`, PDFium garderait un pointeur vers un tampon que l'appelant
  peut libérer : accès après libération, le classique du C. Rust l'interdit : `PdfDocument`
  devrait alors porter la durée de vie du tampon, et `Document` ne pourrait plus être rangé
  dans un `HashMap` à longue vie.
- Sans `Drop` automatique, chaque chemin de sortie (succès, `?`, panique) devrait fermer le
  document à la main ; un oubli = fuite mémoire côté PDFium.

## Erreurs fréquentes

- `borrow of moved value: bytes` : on a donné `bytes` à `from_bytes` puis on l'utilise encore.
  Soit cloner avant (`bytes.clone()`, coûteux), soit réorganiser pour n'en avoir plus besoin.
- `cannot borrow *self as mutable because it is also borrowed as immutable` : on tient un
  `&` (par ex. le retour de `get`) pendant qu'on appelle une méthode `&mut` (`insert`,
  `remove`). Terminer l'usage de la référence avant.
- `cannot move out of *self which is behind a shared reference` : on essaie de sortir un
  champ possédé depuis `&self`. C'est exactement pourquoi `Drop` utilise `take()` sur une
  `Option`.
- `.clone()` partout pour « faire taire » le compilateur : compile, mais copie des données ;
  se demander d'abord si un emprunt `&` suffit.

## Pour aller plus loin

Rust Book, chapitre 4 : https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html

## Exercice de lecture

`crates/mega-core/src/render.rs:44-68` : `render_page(&self, …)` emprunte le document, obtient
une `page`, un `bitmap`, puis construit `RenderedPage { rgba: bitmap.as_rgba_bytes(), … }`.
Pourquoi copier les octets dans un `Vec<u8>` possédé plutôt que renvoyer une référence vers
le tampon du bitmap ? Que deviendrait la signature si l'on voulait éviter la copie, et qui
en paierait le prix côté `protocol.rs` ?
