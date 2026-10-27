# Lot 5 — Édition de contenu : note d'apprentissage

## Ce que le lot a construit

Le contenu des pages devient modifiable : objets de page listés, déplacés, supprimés,
texte retouché « au mieux » ; insertion de texte (Standard 14, gras/italique, taille, couleur)
et d'images ; caviardage réel (le texte disparaît du fichier) ; filigrane, numérotation
(modèle `{n} / {total} — {titre}`, six positions) et recadrage, par page, page courante ou
vignettes sélectionnées. Les dettes du Lot 3 sont soldées par une passe `lopdf` à
l'enregistrement (`/InkList`, `/AP` des notes, élagage), et toute l'API parle désormais en
**espace affiché** : une page `/Rotate 90` se manipule comme les autres. CLI : `mega objects`,
`redact`, `watermark`, `number`, `crop`. Décisions : ADR-0010 ; ADR-0003 mis à jour.

## Concepts Rust rencontrés

### 1. Un changement de repère écrit à la main

`geometry::PageFrame` : quatre cas de rotation, deux fonctions inverses, une matrice, et un
test qui vérifie que les trois disent la même chose. C'est le cœur du lot, et une bonne leçon
de méthode : écrire le test « le coin (0,0) finit en haut à gauche » **avant** la formule.

- Fiche : [`matrices-et-transformations`](concepts/matrices-et-transformations.md).

### 2. Lire et écrire un flux de contenu

`cleanup::ink_points` décode un flux d'apparence en opérations (`m`, `l`, `c`, `re`, `h`) ;
`overlay::apply_placements` écrit un flux `q … BT … Tj … ET Q` et le branche sur la page avec
ses ressources. Deux directions, un même type `lopdf::content::Content`.

- Fiche : [`flux-de-contenu-pdf`](concepts/flux-de-contenu-pdf.md).

### 3. Calculer d'abord, écrire ensuite (emprunts)

`cleanup::cleaned` renvoie une **copie** corrigée du dictionnaire au lieu de le modifier
pendant qu'on lit l'apparence : lecture et écriture de `doc` ne peuvent pas se chevaucher.
Même idée dans `overlay.rs` : les repères, le titre et les largeurs sont calculés avec PDFium
(chaque appel prend et rend le jeton), puis `lopdf` travaille seul, puis le document est
rechargé. Le verrou n'est pas réentrant : imbriquer bloquerait (`engine::guard`).

- Fiches : [`ownership-et-emprunts`](concepts/ownership-et-emprunts.md),
  [`mutex-et-etat-partage`](concepts/mutex-et-etat-partage.md).

### 4. `let … else` et le `match` sur tuple

`let Ok(bounds) = obj.bounds() else { continue; };` (`redact.rs`) lit « si ça échoue, on
passe au suivant » sans imbrication. `match (op.operator.as_str(), v.as_slice())` avec des
motifs de tranche `[x, y]`, `[.., x, y]` (`cleanup.rs`) remplace une cascade de `if`.

### 5. Parcourir à l'envers pour supprimer

`redact` retire des objets par index : en partant de la fin, les positions encore à visiter
ne bougent pas (`for i in (0..count).rev()`). Même règle que `delete_pages` au Lot 2.

### 6. Vérifier l'effet, pas la donnée

Comme aux lots 3 et 4 : les tests asservissent le **pixel** (rouge là où l'annotation a été
tracée, blanc là où l'ancien code l'aurait mise), l'**extraction de texte** (plus de
« Secret » après caviardage) et les **octets** (`/Rect` utilisateur lu avec `lopdf`), jamais la
valeur que l'on vient d'écrire. Deux bogues ont été trouvés ainsi : `scaled_font_size()` nul
pour un texte tourné (coefficient `d` seul), et une annotation fraîche écrite **inline** dans
`/Annots` que la passe de nettoyage ne voyait pas.

## Côté interface (TypeScript)

`logic/content.ts` ne contient que de la logique pure et testée (désignation d'un objet sous
le pointeur, rectangle à partir de deux coins, aperçu du modèle, portée des pages, validation
des marges) ; `ObjectLayer.svelte` ne décide rien : il traduit des gestes en commandes. Les
dialogues partagent `PageScopeSelect`. Aucune chaîne en dur.

## Exercices (à faire plus tard, à votre rythme)

1. Dans `geometry.rs`, ajoutez un test qui place un point au **centre** d'une page tournée de
   180° et vérifiez qu'il reste au centre. Puis cassez volontairement un signe dans
   `point_from_user_space` et lisez quels tests tombent.
2. `content/mod.rs`, `insert_text` : pourquoi la police est-elle obtenue dans un bloc
   `{ let fonts = self.inner_mut().fonts_mut(); … }` **avant** `let doc = self.inner();` ?
   Que dit le compilateur si vous inversez les deux ?
3. `overlay.rs`, `ensure_resources` : dessinez l'arbre `/Pages → /Page` d'un document dont les
   polices sont héritées, et expliquez ce qui se passerait si l'on posait un `/Resources` vide
   sur la page au lieu de copier l'hérité.
4. `redact.rs` : proposez (sans coder) comment on caviarderait **par caractère** un objet texte
   `Tj` avec `lopdf`, et listez ce qui peut mal tourner (positionnement, polices, `TJ`).

### 7. Ce que les relectures ont apporté

Deux relectures indépendantes (code, conformité PDF) ont trouvé ce que les tests « heureux »
ne voyaient pas : `lopdf` **déchiffre en silence** un PDF protégé par mot de passe
propriétaire seul (`is_encrypted()` ment, `was_encrypted()` dit vrai) ; retirer un widget
laisse sa valeur dans `/AcroForm` ; PDFium régénère un flux en perdant les polices Type 3, les
images en ligne et les ombrages. La réponse commune : **refuser avec un message précis** plutôt
que produire un fichier qui ment (ADR-0010 §7), et une fixture par cas
(`owner-only.pdf`, `fragile_content_pdf`, `xobject_text_pdf`).

## Ce qui reste ouvert

Police embarquée pour l'Unicode hors WinAnsi, caviardage par caractère et caviardage raster
des images, recadrage par rectangle tracé, objets de page dans un panneau latéral, popups
orphelins, avertissement de signature invalidée. Voir le ledger du lot et ADR-0010.

## Prérequis pour le Lot 6 (export, impression, sécurité)

- `lopdf` pour les métadonnées et le chiffrement (ADR-0003) : dictionnaire `/Info`, chaînes
  PDFDoc/UTF-16, `/Encrypt`.
- Encodage d'images (`image` : PNG/JPEG, qualité, résolution) et flux binaires.
- Impression via le WebView : événements Tauri, `window.print()`, mise en page CSS.
