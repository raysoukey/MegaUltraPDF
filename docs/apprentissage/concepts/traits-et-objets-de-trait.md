# Traits et objets de trait (`dyn`)

## L'idée en trois phrases

Un **trait** décrit un comportement (« sait se décrire », « sait s'appliquer à un document »)
sans dire qui l'implémente ; c'est la façon de Rust d'écrire du code qui marche pour plusieurs
types. Quand le type concret est connu à la compilation, le compilateur génère une version par
type (**génériques**, `impl Trait`) ; quand il ne l'est pas, on passe par un **objet de trait**
(`&dyn Trait`, `Box<dyn Trait>`) et l'appel est résolu à l'exécution par une petite table de
fonctions. Les deux formes s'écrivent presque pareil ; la différence est *où* la décision est prise.

## Dans ce projet

### 1. Déclarer le contrat — `crates/mega-core/src/edit.rs`, `trait Command`

```rust
pub trait Command {
    fn label(&self) -> String;                              // libellé pour l'interface
    fn apply(&self, doc: &mut Document) -> CoreResult<()>;  // l'édition elle-même
}
```

- Deux méthodes, pas de champ : un trait ne décrit que ce qu'on peut **faire**.
- `&self` pour `label` (lecture), `&self` aussi pour `apply` : c'est le **document** qui est
  emprunté mutablement, pas la commande. Une commande est une valeur immuable que l'on peut
  rejouer.

### 2. L'implémenter — `impl Command for EditCommand` (même fichier)

```rust
impl Command for EditCommand {
    fn label(&self) -> String {
        match self {
            EditCommand::RotatePages { pages, .. } => format!("Pivoter {}", plural(pages.len(), "page", "pages")),
            EditCommand::DeletePages { pages } => format!("Supprimer {}", plural(pages.len(), "page", "pages")),
            // …
        }
    }
    fn apply(&self, doc: &mut Document) -> CoreResult<()> {
        match self {
            EditCommand::RotatePages { pages, quarter_turns } => doc.rotate_pages(pages, *quarter_turns),
            // …
        }
    }
}
```

- `impl Trait for Type` : on « branche » le comportement sur un type existant. On pourrait
  aussi l'implémenter pour un type d'une autre crate (avec la règle de l'orphelin : le trait
  **ou** le type doit venir de chez nous).
- Le `match` est exhaustif : ajouter une variante à `EditCommand` fait échouer la compilation
  ici tant que `label` et `apply` ne la traitent pas. Le compilateur tient la liste à jour.

### 3. L'utiliser sans connaître le type — `Document::apply(&mut self, command: &dyn Command)`

```rust
pub fn apply(&mut self, command: &dyn Command) -> CoreResult<()> {
    let snapshot = Snapshot { bytes: self.to_bytes()?, label: command.label() };
    command.apply(self)?;      // appel « dynamique » : résolu à l'exécution
    self.history.push_undo(snapshot);
    // …
}
```

- `&dyn Command` : « une référence vers *quelque chose* qui implémente `Command` ». La
  référence est **large** (deux mots) : l'adresse de la valeur et l'adresse de sa table de
  méthodes (*vtable*). C'est ce qui permet à l'historique d'ignorer complètement `EditCommand`.
- Pourquoi `dyn` ici plutôt qu'un générique `fn apply<C: Command>(…)` ? Parce que l'acteur
  reçoit des commandes désérialisées depuis l'interface et qu'un futur Lot pourra ajouter
  d'autres types de commandes (annotations) : un seul `Document::apply` pour tous, un seul
  code compilé.
- `pdfium-render` fait la même chose en interne : `PdfiumLibraryBindings` est un trait, et la
  bibliothèque charge une implémentation différente selon la plateforme.

### 4. Les traits que vous utilisez déjà sans y penser

- `impl From<PdfiumError> for CoreError` (`error.rs`) : c'est `From` qui donne l'opérateur `?`
  sa conversion automatique (fiche [`result-et-operateur-point-d-interrogation`](result-et-operateur-point-d-interrogation.md)).
- `impl Ord for Queued` (`service/queue.rs`) : `BinaryHeap` ne sait pas ce qu'est une
  priorité ; il sait seulement comparer des `Ord`.
- `impl Drop for Document` (`document.rs`) : le trait appelé automatiquement à la fin de vie.
- `#[derive(Debug, Clone, Serialize)]` : des `impl` écrits par une macro.

## Ce qui se passerait sans

Sans trait, `History` devrait connaître `EditCommand` et chaque futur type de commande : un
`match` géant au mauvais endroit, à modifier à chaque Lot. Sans `dyn`, il faudrait un
générique, donc connaître le type concret à l'appel, ce qui n'est pas le cas pour une valeur
qui arrive par IPC.

```rust
// Ne compile pas : un trait n'est pas un type « de taille connue ».
fn apply(&mut self, command: Command) -> CoreResult<()> { … }
// error[E0782]: expected a type, found a trait  →  écrire `&dyn Command` ou `impl Command`.
```

## Erreurs fréquentes

- Oublier `dyn` (message E0782 ci-dessus) ou vouloir stocker un `dyn Trait` **par valeur**
  dans un `Vec` : il faut `Vec<Box<dyn Trait>>`, la boîte donne une taille connue.
- « the trait `X` is not implemented for `Y` » quand on appelle une méthode de trait sur un
  type qui ne l'implémente pas : vérifier le `impl`, et que le trait est **importé** (`use`)
  dans le module appelant ; un trait non importé rend ses méthodes invisibles.
- Mettre une méthode générique dans un trait que l'on veut utiliser en `dyn` : le trait
  n'est alors plus « *object safe* » (E0038). Garder les traits destinés à `dyn` simples.

## Pour aller plus loin

Rust Book, chapitre 10.2 « Traits » et 18.2 « Using Trait Objects » :
<https://doc.rust-lang.org/book/ch10-02-traits.html>, <https://doc.rust-lang.org/book/ch18-02-trait-objects.html>.

## Exercice de lecture

`crates/mega-core/src/edit.rs`, `Document::apply` : l'instantané est pris **avant**
`command.apply(self)`, mais n'est empilé qu'**après** son succès. Que se passerait-il pour
« annuler » si l'ordre était inversé et que l'édition échouait à mi-chemin ? Relisez la doc
du trait : quelle promesse `apply` doit-elle tenir pour que l'historique reste cohérent ?
