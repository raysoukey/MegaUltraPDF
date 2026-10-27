# Flux de contenu PDF : lire et écrire le « programme » d'une page

## L'idée en 3 phrases

Le dessin d'une page PDF est un petit programme à pile : `72 700 Td` déplace le curseur,
`(Bonjour) Tj` écrit, `q … Q` sauvegarde et restaure l'état graphique, `re f` remplit un
rectangle. PDFium exécute ce programme pour afficher ; `lopdf` nous laisse le **lire comme
une liste d'opérations** et en **ajouter**. Au Lot 5 on s'en sert dans les deux sens : relire
l'apparence d'une forme pour en déduire son `/InkList`, et ajouter filigranes et numéros sans
dépendre de PDFium.

## Dans ce projet

### 1. Lire : `Content::decode` (`crates/mega-core/src/cleanup.rs`, `ink_points`)

```rust
let data = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
let content = Content::decode(&data).ok()?;
for op in content.operations.iter().take(100_000) {
    let v: Vec<f32> = op.operands.iter().filter_map(num).collect();
    match (op.operator.as_str(), v.as_slice()) {
        ("m", [x, y]) => { /* nouveau trait */ }
        ("l", [x, y]) => current.extend([*x, *y]),
        ("c", [.., x, y]) => current.extend([*x, *y]),
        ("re", [x, y, w, h]) => { /* quatre coins + retour */ }
        _ => {}
    }
}
```

Ligne par ligne : le flux est d'abord **décompressé** (presque toujours `FlateDecode`), puis
découpé en opérations (`operator` + `operands`). Le `match` sur un tuple `(opérateur,
tranche d'opérandes)` est une forme très lisible de « si c'est `m` avec deux nombres… » ;
`[.., x, y]` prend les deux derniers opérandes d'une courbe de Bézier (son point d'arrivée).
La borne `take(100_000)` rappelle qu'un fichier PDF est une donnée non fiable.

### 2. Écrire : un flux ajouté après les autres (`overlay.rs`, `append_content`)

```rust
let existing = doc.get_page_contents(page_id);
if wrapped.insert(page_id) && !existing.is_empty() {
    list.push(Object::Reference(stream(doc, b"q\n")));        // avant tout le contenu existant
    list.extend(existing.iter().map(|id| Object::Reference(*id)));
    list.push(Object::Reference(stream(doc, b"\nQ\n")));      // après
}
list.push(Object::Reference(stream(doc, &content)));          // puis le nôtre
page.set("Contents", Object::Array(list));
```

`/Contents` peut être un tableau de flux : le lecteur les concatène. On encadre le contenu
existant par `q` et `Q` pour qu'une transformation ou une couleur laissée « ouverte » par le
document ne déteigne pas sur notre texte. Le flux ajouté commence lui aussi par `q` et finit
par `Q` : il ne laisse rien derrière lui.

### 3. Le texte du flux : `BT … Tj … ET` (`overlay.rs`, `apply_placements`)

```text
q
0 1 -1 0 400 0 cm          ← matrice du repère (page tournée seulement)
/MegaGS1 gs                 ← état graphique : opacité
0.5 0.5 0.5 rg              ← couleur de remplissage
BT /MegaF1 60 Tf            ← police et taille
0.707 0.707 -0.707 0.707 180 300 Tm   ← matrice texte : rotation + position
<42524F55494C4C4F4E> Tj     ← « BROUILLON » en WinAnsi, hexadécimal
ET
Q
```

Les noms `/MegaF1` et `/MegaGS1` doivent exister dans `/Resources` de la page :
`ensure_resources` les y ajoute, en **copiant** les ressources héritées d'un ancêtre `/Pages`
si la page n'en a pas (sinon la page perdrait les polices de son propre contenu). La chaîne
est écrite en hexadécimal `<…>` : pas d'échappement de parenthèses à gérer, et chaque octet
vient de `win_ansi_byte` (`content/mod.rs`).

## Ce qui se passerait sans

Un filigrane posé par PDFium (objet texte) n'existe que si PDFium accepte de l'écrire, avec
ses polices et son nettoyage ; un filigrane écrit **dans le flux** est du PDF ordinaire,
identique dans Acrobat, PDF.js, MuPDF. Et sans `q … Q` autour du contenu existant, un
document terminant par `2 0 0 2 0 0 cm` afficherait notre numéro de page deux fois trop grand.

## Erreurs fréquentes

- Oublier que les flux concaténés doivent se couper entre deux **jetons** : nos flux
  commencent et finissent par un saut de ligne.
- Écrire un nombre en notation exponentielle (`1e-5`) : interdit dans un flux ; `num()` formate
  en décimal.
- Modifier un `/Font` partagé par référence entre plusieurs pages : on le copie d'abord.

## Pour aller plus loin

ISO 32000-2 §7.8 (flux de contenu), §8.4 (état graphique), §9.4 (opérateurs de texte).
Documentation de `lopdf::content`.

## Exercice de lecture

`cleanup.rs`, branche `("h", _) if current.len() >= 4` : pourquoi un `h` (fermer le chemin)
ajoute-t-il le **premier** point à la fin du trait, et qu'afficherait un lecteur qui lit
`/InkList` sans ce point ?
