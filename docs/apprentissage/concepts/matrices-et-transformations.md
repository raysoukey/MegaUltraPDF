# Matrices et transformations : changer de repère sans se tromper

## L'idée en 3 phrases

Une page PDF a un repère (origine en bas à gauche, en points) ; l'écran en a un autre (origine
en haut à gauche, en pixels) ; une page tournée par `/Rotate` en a un troisième. Une
**transformation affine** `x' = a·x + c·y + e`, `y' = b·x + d·y + f` passe de l'un à l'autre,
et six nombres `[a b c d e f]` suffisent à la décrire (c'est la « matrice » du PDF, §8.3.4).
Rust n'a rien de spécial ici : l'intérêt est d'écrire les quatre cas de rotation **à la main**,
testés un par un, plutôt qu'une formule générale qu'on ne sait plus vérifier.

## Dans ce projet

### 1. Le repère d'une page : `PageFrame` (`crates/mega-core/src/geometry.rs`)

```rust
pub struct PageFrame {
    pub rotate: u16,   // /Rotate : 0, 90, 180 ou 270
    pub bbox: Rect,    // zone visible en espace utilisateur (CropBox ∩ MediaBox)
}

pub fn point_from_user_space(&self, p: [f32; 2]) -> [f32; 2] {
    let (u, v) = (p[0] - self.bbox.x, p[1] - self.bbox.y); // 1. on soustrait l'origine
    let (w, h) = (self.bbox.w, self.bbox.h);
    match self.rotate % 360 {                                // 2. on tourne
        90 => [v, w - u],
        180 => [w - u, h - v],
        270 => [h - v, u],
        _ => [u, v],
    }
}
```

Ligne par ligne : on ramène d'abord le point à l'origine de la boîte (une page peut
commencer en (10, 20)), puis chaque quart de tour **échange** ou **renverse** les axes. Pour
90°, le coin bas-gauche utilisateur `(0, 0)` devient `(0, w)` : le haut-gauche de l'écran,
exactement ce que l'on voit quand on tourne une feuille d'un quart de tour horaire.
`point_to_user_space` fait l'inverse, et le test
`page_frame_round_trips_points_rects_and_matches_its_matrix` vérifie que l'aller-retour
rend le point de départ **et** que la matrice `matrix_to_user_space()` donne le même
résultat que la fonction : deux écritures indépendantes de la même idée, qui se contrôlent.

### 2. Un rectangle tourné reste un rectangle (`geometry.rs`)

```rust
pub fn to_user_space(&self, r: Rect) -> Rect {
    Self::rect_from_corners(
        self.point_to_user_space([r.x, r.y]),
        self.point_to_user_space([r.x + r.w, r.y + r.h]),
    )
}
```

On transforme deux coins opposés, puis `rect_from_corners` reprend le min et le max : après
un quart de tour, le coin « bas-gauche » n'est plus en bas à gauche, et largeur et hauteur
sont échangées. Oublier la normalisation donne des largeurs négatives que PDFium refuse.

### 3. Tourner un objet dessiné : `apply_matrix` (`content/mod.rs`, `insert_text`)

```rust
let matrix = (!frame.is_identity()).then(|| frame.matrix_to_user_space());
// …
t.translate(PdfPoints::new(at[0]), PdfPoints::new(baseline))?;   // placé en espace affiché
if let Some(m) = matrix {
    t.apply_matrix(PdfMatrix::new(m[0], m[1], m[2], m[3], m[4], m[5]))?; // puis tourné
}
```

L'ordre compte : on place le texte **comme si la page n'était pas tournée**, puis on
applique la matrice du repère. PDFium compose les deux (`FPDFPageObj_Transform` multiplie
les matrices) : le texte finit au bon endroit du fichier et droit à l'écran. La même matrice,
écrite en toutes lettres `a b c d e f cm`, ouvre les flux de filigrane (`overlay.rs`).

### 4. Côté interface : `pdfRectToCss` (`app/src/lib/logic/geometry.ts`)

La rotation **d'affichage** choisie par l'utilisateur (`doc.rotation`) est encore une autre
transformation, en TypeScript cette fois, avec les mêmes quatre cas. Elle ne touche pas au
fichier : seul `PageFrame` connaît `/Rotate`.

## Ce qui se passerait sans

Avant le Lot 5, un rectangle tracé à (50, 30) sur une page `/Rotate 90` était écrit tel quel
dans `/Rect` : affiché à (30, 250) après rotation — ailleurs. Le test
`annotations_on_rotated_page_round_trip` (`tests/rotation.rs`) vérifie maintenant le pixel
rouge **à l'endroit tracé** et le pixel blanc **là où l'ancien code l'aurait mis**.

## Erreurs fréquentes

- Confondre rotation horaire (affichage) et trigonométrique (matrices PDF) : `b` et `c` de
  signe opposé selon la convention. Écrire le test d'abord (« le coin (0,0) doit finir en
  haut à gauche ») tranche sans débat.
- Appliquer la rotation **avant** la translation (`translate` puis `apply_matrix` ici, et pas
  l'inverse).
- Lire une taille de police par `matrix.d` : nulle après un quart de tour. On prend la norme
  de la colonne `(b, d)` (`content::effective_font_size`).

## Pour aller plus loin

ISO 32000-2 §8.3 (systèmes de coordonnées) ; Rust Book, chapitre 5 (structures et méthodes),
dont `PageFrame` est un exemple sans surprise.

## Exercice de lecture

`geometry.rs`, `matrix_to_user_space` : vérifiez à la main que, pour `rotate = 270`, la
matrice `[0 −1 1 0 x y+h]` envoie bien le point affiché `(h, 0)` (bas-droite de l'écran) sur
le coin bas-gauche utilisateur `(x, y)`. Puis trouvez le test qui le garantit.
