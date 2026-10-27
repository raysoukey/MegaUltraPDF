# Pattern builder : construire pas à pas, valider une fois

## L'idée en trois phrases

Quand un objet a beaucoup de paramètres optionnels ou se construit en plusieurs étapes, on
ne lui donne pas un constructeur à douze arguments : on **accumule** les réglages par des
appels successifs, puis une méthode finale produit la valeur (ou l'erreur). Rust s'y prête
par les méthodes qui prennent `self` ou `&mut self` et retournent `Self`, et par `?` qui
interrompt la chaîne au premier échec. Le résultat est lisible et impossible à utiliser à
moitié construit.

## Dans ce projet

### 1. Un chemin PDF, segment par segment — `crates/mega-core/src/annotations/appearance.rs`, `draw_shape`

```rust
let mut head = PdfPagePathObject::new(doc, pt(to[0]), pt(to[1]), Some(stroke), Some(width), None)?;
head.line_to(pt(to[0] + len * a1.cos()), pt(to[1] + len * a1.sin()))?;   // premier trait
head.move_to(pt(to[0]), pt(to[1]))?;                                      // retour à la pointe
head.line_to(pt(to[0] + len * a2.cos()), pt(to[1] + len * a2.sin()))?;   // second trait
objects.add_path_object(head)?;                                            // et on l'ajoute
```

- `new` fixe le point de départ et le style ; chaque `line_to`/`move_to` **ajoute** une
  étape au même objet (`&mut self`) ; `add_path_object` consomme l'objet (`self` par valeur)
  pour le donner à l'annotation. Après ce dernier appel, `head` n'existe plus : on ne peut
  pas continuer à le modifier par erreur.
- Chaque appel retourne `Result` : un `?` par ligne, et la pointe de flèche est entière ou
  l'erreur remonte avant que l'annotation ne soit à moitié dessinée.

### 2. La configuration de rendu — `crates/mega-core/src/render.rs`

```rust
let config = PdfRenderConfig::new()
    .set_target_size(width_px, height_px)
    .rotate(rotation.to_pdfium(), false)
    .render_form_data(true)
    .render_annotations(true);
```

Le builder « classique » : chaque méthode prend `self` par valeur et le retourne, d'où
l'enchaînement sans variable intermédiaire. Ajouter `render_annotations(true)` au Lot 3 a
été une ligne, sans toucher aux autres appels.

### 3. Un builder maison, côté données — `NewAnnotation` et `validate`

`NewAnnotation` (`annotations/mod.rs`) n'a pas de méthodes `with_…` : c'est une structure
sérialisée telle quelle par le frontend. Le « builder » est alors la **fonction de
validation** appelée en tête de `add_annotation` : elle refuse opacité nulle, épaisseur
nulle, texte vide, quadrilatères absents, genre non créable. Tout ce qui passe la
validation est constructible ; le reste du code n'a plus à douter.

### 4. Le pendant TypeScript — `app/src/lib/logic/tools.ts`

`beginDraft` → `extendDraft` → `finishDraft` : un brouillon s'accumule point par point
(fonctions pures qui retournent un nouvel objet) et `finishDraft` produit la demande
finale ou `null`. Même forme, autre langage.

## Ce qui se passerait sans

Un constructeur `PdfPagePathObject::new(doc, x, y, stroke, width, fill, segments: Vec<Segment>)`
obligerait à décrire la pointe de flèche comme une liste de données avant de la créer, et
une erreur PDFium au troisième segment laisserait un objet partiel à nettoyer. Côté
données, sans `validate`, chaque fonction de dessin vérifierait ses propres cas limites,
différemment.

## Erreurs fréquentes

- Oublier `mut` sur l'objet construit : `head.line_to(...)` exige `let mut head`.
- Utiliser un objet après l'avoir donné (`add_path_object(head)` puis `head.line_to`) :
  `error[E0382]: borrow of moved value`. C'est le compilateur qui garantit l'ordre.
- Mélanger les deux styles : une méthode `fn x(self) -> Self` s'enchaîne, une méthode
  `fn x(&mut self)` non (elle retourne `()`); lire la signature avant d'écrire `.a().b()`.

## Pour aller plus loin

Rust Book, chapitre 5.3 « Method Syntax » et chapitre 17.3 (« state pattern », une variante) :
<https://doc.rust-lang.org/book/ch05-03-method-syntax.html>.

## Exercice de lecture

`appearance.rs`, `draw_text` (branche `Stamp`) : le texte est créé, **mesuré** (`bounds()`),
puis déplacé (`translate`) avant d'être ajouté. Pourquoi ne peut-on pas calculer la position
avant de créer l'objet, et que vaudrait `b.left()` si `translate` était appelé deux fois ?
