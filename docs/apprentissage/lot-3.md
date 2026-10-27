# Lot 3 — Annotations : note d'apprentissage

## Ce que le lot a construit

Annoter un PDF : surligner, souligner, barrer un passage sélectionné ; poser une note, un
texte libre, un tampon ; dessiner à main levée, tracer rectangle, ellipse, ligne, flèche ;
sélectionner, déplacer, redimensionner, modifier (contenu, couleur, opacité, épaisseur,
taille, auteur) et supprimer ; panneau « Annotations » par page ; rendu des annotations
existantes dans l'image de la page ; CLI `mega annots`. Chaque action est une `EditCommand`
(annuler/rétablir, « • », enregistrement atomique). Décisions : ADR-0008.

## Concepts Rust rencontrés

### 1. `enum` riches et `match` exhaustif

`AnnotationKind`, `Geometry`, `EditCommand` : des types somme dont chaque variante porte
ses données. Le `match` sans `_` de `kind_of` fait du compilateur un gardien : un nouveau
sous-type PDFium sera un **refus de compiler**, pas un silence. Une garde `if` ne couvre
pas une variante (leçon apprise sur `validate`).

- Fiche : [`enums-et-match-exhaustif`](concepts/enums-et-match-exhaustif.md).

### 2. Pattern builder

Les objets chemin et texte de pdfium-render se construisent étape par étape (`new`,
`line_to`, `translate`, puis `add_path_object` qui consomme l'objet) ; `PdfRenderConfig`
s'enchaîne par valeur ; `NewAnnotation` est validé une fois en tête d'`add_annotation`.

- Fiche : [`pattern-builder`](concepts/pattern-builder.md).

### 3. Conversions `From` entre nos types et ceux de la bibliothèque

`impl From<PdfColor> for Color` (`annotations/mod.rs`) : `c.into()` partout où une couleur
PDFium devient la nôtre. Dans l'autre sens, `Color::with_opacity` fabrique un `PdfColor`
avec l'alpha. Les types de la bibliothèque ne sortent **jamais** de `mega-core` (frontière
de `CLAUDE.md`) : le frontend ne voit que `Color { r, g, b }`.

### 4. Lire des données partielles : `Option`, `filter`, `filter_map`

Un PDF tiers peut omettre l'auteur, la date, la couleur. `truncate(ann.creator())` garde
`Option<String>` et borne la longueur ; `doc.outline()?.iter().filter_map(|n| n.page)` (Lot 2)
ignore les signets sans page. Le modèle `Annotation` assume ces `Option` plutôt que
d'inventer des valeurs.

### 5. Un trait avec méthode par défaut

`Command::annotation_target()` retourne `None` par défaut : seules les commandes qui
visent une annotation existante le surchargent. `Document::apply` s'en sert pour nommer le
genre réel dans l'historique (« Supprimer : Note ») **avant** d'appliquer la commande —
lire l'état, puis agir.

### 6. Lifetimes explicites, une seule fois

`draw_shape<'a>(doc: &PdfDocument<'a>, objects: &mut PdfPageAnnotationObjects<'a>, …)` :
le compilateur a refusé `'_` séparés (« lifetime may not live long enough ») car les objets
créés à partir du document doivent vivre aussi longtemps que la collection qui les reçoit.
Nommer `'a` des deux côtés dit exactement cela.

### 7. Côté TypeScript (pour mémoire)

Machine à états pure des outils (`tools.ts`), géométrie d'interface (`annotations.ts`) :
toutes deux sans DOM et testées. Le calque SVG ne fait que traduire des événements en
points PDF et des points PDF en pixels ; les règles (validation, apparence, historique)
restent en Rust.

## Décisions de conception du lot

ADR-0008. Consignées pendant l'exécution : `/C` jamais lu directement (voir le piège
ci-dessous) ; déplacer une forme = recréer ; les marquages se sélectionnent
depuis le panneau (leur zone recouvrirait le texte) ; saisie du texte dans un champ flottant
du calque ; choix du tampon par liste déroulante.

## Pièges rencontrés

- **Plantage dans pdfium-render** : `stroke_color()` sur une annotation munie d'un `/AP`
  retombe sur `FPDFPageObj_GetStrokeColor` avec le handle de l'annotation transtypé en objet
  de page → `STATUS_ACCESS_VIOLATION` (ou, en comportement indéfini, du noir renvoyé sans
  plantage : vu dans l'application pour une note). Trouvé en instrumentant `read_annotation`
  ligne par ligne (`eprintln!` + `--test-threads=1 --no-capture`), puis en décompressant le
  flux d'apparence écrit par PDFium (`/PDFIUM_HasGeneratedAP true`). Contournement : couleur
  par les objets d'apparence uniquement, jamais `/C`.
- **`FPDFAnnot_SetColor` échoue après l'apparence** : ordre imposé dans `add_annotation`.
- **`FPDFAnnot_AppendObject` n'accepte qu'`Ink` et `Stamp`** : d'où le choix des sous-types
  (ADR-0008).
- **Déplacer par `/Rect` seul** ne suit pas l'apparence : recréation avec mise à l'échelle
  entre emprises intérieures (le `/Rect` d'une forme déborde du trait de `épaisseur/2 + 1`).
- **Revue de sécurité automatique** : bornes de lecture ajoutées (annotations par page,
  quadrilatères, longueur des chaînes) ; un PDF reste une donnée non fiable.

## Vérification sans souris

`scripts/ui-check.ps1 -Pdf <copie de simple.pdf> -Scenario scripts/scenarios/annotation.json`
(surlignage depuis la sélection, note avec saisie, rectangle + poignées + Suppr + Ctrl+Z,
encre, flèche, texte libre, tampon, éditeur, Ctrl+S) puis `mega annots` sur la copie ;
`annotation-tierce.json` sur `fixtures/annotated.pdf` (lecture d'annotations d'un autre
producteur). Le glisser à coordonnées du runner (`drag`) déclenche les `pointer*` du calque.

## Exercices de lecture

1. `annotations/mod.rs`, `add_annotation` : pourquoi le jeton de police (`font`) est-il
   créé **avant** `let doc = self.inner()` ? Que dirait le compilateur si on inversait ?
2. `annotations/appearance.rs`, `rescale` : écrivez à la main le résultat pour un
   `Geometry::Line { from: [0, 0], to: [10, 10] }` transformé de `{0,0,10,10}` vers `{100,100,20,5}`.
3. `app/src/lib/components/AnnotationLayer.svelte` : suivez un clic avec l'outil
   Sélectionner sur une note. Quelles fonctions pures sont appelées, et dans quel ordre ?

## Prérequis pour le Lot 4

Les formulaires : environnement de formulaire de PDFium (`FPDFDOC_InitFormFillEnvironment`
via pdfium-render : `PdfForm`, champs, valeurs), le trait `Drop` pour les ressources
natives, et la correspondance champs PDF ↔ `<input>` HTML superposés (même géométrie que ce lot).
