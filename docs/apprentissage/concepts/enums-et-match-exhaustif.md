# `enum` riches et `match` exhaustif

## L'idée en trois phrases

Une `enum` Rust n'est pas une liste d'entiers nommés : chaque variante peut porter ses
propres données (`Rect { rect }`, `Line { from, to }`), ce qui en fait un **type somme** :
« une valeur est *soit* ceci, *soit* cela ». Le `match` oblige à traiter **toutes** les
variantes, sinon le programme ne compile pas. Résultat : quand on ajoute un cas, le
compilateur liste lui-même tous les endroits à mettre à jour.

## Dans ce projet

### 1. Une géométrie par genre — `crates/mega-core/src/annotations/mod.rs`, `enum Geometry`

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Geometry {
    Rect { rect: Rect },                       // note, texte, tampon, rectangle, ellipse
    Quads { rects: Vec<Rect> },                // marquages de texte
    Line { from: [f32; 2], to: [f32; 2] },     // ligne, flèche
    Strokes { strokes: Vec<Vec<[f32; 2]>> },   // encre
}
```

- Chaque variante porte exactement les données dont elle a besoin : pas de champ `to`
  inutile dans un rectangle, pas de `rect` vide dans une ligne.
- L'attribut `serde(tag = "kind")` sérialise la variante en objet JSON `{ "kind": "line",
  "from": …, "to": … }` : le frontend TypeScript déclare la même union (`api.ts`,
  `type Geometry = { kind: 'rect'; … } | { kind: 'line'; … }`). Un seul vocabulaire des
  deux côtés de l'IPC.

### 2. Le `match` qui ne laisse rien passer — même fichier, `fn kind_of`

```rust
match ann.annotation_type() {
    PdfPageAnnotationType::Text => AnnotationKind::Note,
    PdfPageAnnotationType::Highlight => AnnotationKind::Highlight,
    // …
    PdfPageAnnotationType::Unknown
    | PdfPageAnnotationType::Polygon
    | PdfPageAnnotationType::Polyline
    | /* … toutes les autres, nommées une à une … */
    | PdfPageAnnotationType::Redacted => AnnotationKind::Other,
}
```

- Pas de `_ =>` fourre-tout : si pdfium-render ajoute un sous-type dans une prochaine
  version, la compilation échoue ici avec `error[E0004]: non-exhaustive patterns`, et l'on
  décide alors consciemment où le ranger. C'est un choix : `_` aurait été plus court, mais
  silencieux.

### 3. Les gardes `if` ne comptent pas pour l'exhaustivité — `fn validate`

```rust
match new.kind {
    AnnotationKind::Highlight | AnnotationKind::Underline
    | AnnotationKind::StrikeOut | AnnotationKind::Squiggly => { /* exige des quads */ }
    // …
}
```

Première version écrite : `k if k.is_text_markup() => …`. Le compilateur a refusé :
« patterns `Highlight`, `Underline`… not covered ». Un bras avec **garde** (`if`) peut
échouer à l'exécution, donc Rust ne le compte pas comme couvrant ses variantes. Il faut
les nommer (ou garder un `_` final, ce qu'on voulait justement éviter).

### 4. Déconstruire avec `let … else` — `Document::add_annotation`

```rust
let Geometry::Rect { rect } = &new.geometry else {
    unreachable!("validé plus haut")
};
```

Quand on *sait* qu'une seule variante est possible (la validation vient de le garantir),
`let … else` extrait ses champs sans écrire un `match` à deux bras. `unreachable!` est
une panique volontaire pour un cas impossible ; le lint `clippy::panic` l'autorise car ce
n'est pas `panic!`, mais l'invariant doit être **vrai**, d'où le commentaire.

## Ce qui se passerait sans

Avec un `struct Geometry { rect: Option<Rect>, from: Option<[f32;2]>, … }`, rien n'empêcherait
une ligne sans `to` ou un rectangle avec des `strokes` : chaque fonction devrait vérifier
des combinaisons absurdes. Avec un `match` muni d'un `_`, un nouveau sous-type PDF serait
classé « autre » sans que personne ne le remarque.

## Erreurs fréquentes

- `error[E0004]: non-exhaustive patterns` : il manque une variante ; lire la liste proposée
  par le compilateur, ne pas ajouter `_` par réflexe.
- Oublier `&` : `match new.geometry { Geometry::Rect { rect } => … }` **déplace** la valeur
  hors de `new` (erreur E0507 si `new` est emprunté) ; écrire `match &new.geometry`.
- Comparer deux enums avec `==` exige `#[derive(PartialEq)]` ; tester une variante sans
  comparer ses champs : `matches!(g, Geometry::Rect { .. })`.

## Pour aller plus loin

Rust Book, chapitre 6 « Enums and Pattern Matching » : <https://doc.rust-lang.org/book/ch06-00-enums.html>.

## Exercice de lecture

`crates/mega-core/src/annotations/appearance.rs`, `fn inspect` : le `match paths.as_slice()`
distingue `[single]`, `[shaft, head] if …` et `_`. Pourquoi le bras de la flèche a-t-il
besoin d'une garde, et que se passe-t-il pour une annotation `Ink` à trois objets chemin ?
