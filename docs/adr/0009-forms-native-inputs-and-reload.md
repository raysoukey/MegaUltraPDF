# ADR-0009 — Formulaires : champs HTML natifs, rechargement après écriture, signature en contenu de page

Date : 2026-10-26 · Statut : accepté (Lot 4)

## Contexte

Le Lot 4 remplit des formulaires AcroForm et appose une signature visuelle. PDFium ne voit
les champs qu'à travers son environnement de formulaire ; pdfium-render 0.9.4 en expose la
lecture complète (type, nom, valeur, options, drapeaux) mais une écriture partielle, sans
`unsafe` : `set_value` (texte, écrit `/V`), `set_checked(bool)` (case, écrit `/AS` et `/V`),
`set_checked()` d'un bouton radio (écrit seulement `/V` du bouton visé), aucun réglage pour
combos et listes, `flatten()` par page, et aucune écriture dans le dictionnaire `/AcroForm`
(`/NeedAppearances`).

## Décision

- **Les champs sont des contrôles HTML natifs** (`FormLayer`) positionnés sur l'image de la
  page avec la géométrie du Lot 1 : saisie, accessibilité, Tab et Maj+Tab gratuits. Un fond
  translucide couvre l'apparence PDFium du champ pour éviter le doublon.
- **Un changement = une `EditCommand::SetFieldValue`** (annulable). Côté Rust, l'écriture
  est faite par **`lopdf`** (module `forms/write.rs`) et non par pdfium-render : `/V` sur le
  **champ** (le widget s'il est fusionné, sinon son `/Parent`), `/AS` et `/V` en **noms**
  pour les cases et les radios (`/V` du groupe + `/AS` de chaque bouton), `/V` chaîne pour
  combos et listes (valeur parmi les options), chaînes en PDFDoc/ASCII ou UTF-16BE+BOM, et
  `/NeedAppearances true` posé dans `/AcroForm`. Le document est ensuite **rechargé** dans
  PDFium, qui régénère toutes les apparences au chargement : ce que nous affichons est ce que
  verront Acrobat, PDF.js et MuPDF.
- Pourquoi pas pdfium-render pour écrire : `set_checked` écrit `/AS (/Yes)` et `/V (/Yes)`
  comme des **chaînes** (case décochée partout, PDFium compris), `set_value` écrit `/V` sur le
  widget et non sur le champ parent, et rien n'atteint `/AcroForm`. Trouvé par les relectures
  du lot ; `lopdf` (prévu au Lot 5, ADR-0003) est donc avancé pour cette écriture.
- **Lecture** : une page qui porte des widgets est d'abord chargée dans l'environnement de
  formulaire (rendu minuscule) : les widgets absents de `/AcroForm /Fields` et les apparences
  n'existent qu'après.
- **Aplatir** = `EditCommand::FlattenForm` : chaque page est chargée dans l'environnement de
  formulaire (sinon `flatten` dessinerait des apparences périmées et perdrait les valeurs
  saisies, cas du CLI), aplatie (`PdfPage::flatten`, qui aplatit aussi les annotations), puis
  `/AcroForm` est retiré par `lopdf` (plus de champs orphelins). Annulable par l'historique.
- **Signature visuelle = contenu de page**, pas annotation ni champ `/Sig` : une image
  (`PdfPageImageObject`, ajustée dans le rectangle en gardant les proportions, dimensions lues
  dans l'en-tête avant tout décodage) ou des traits (chemins noirs de 2 pt, bouts ronds, un
  seul flux de contenu), dans les limites de la page. Visible partout, non « décochable »,
  sans lien avec une signature cryptographique (Lot 7, option désactivée).
- **Enregistrer, fermer, Enregistrer sous** valident d'abord le champ en cours de saisie et
  attendent les éditions en vol : une valeur tapée sans quitter le champ n'est pas perdue.
- **Valeurs lues** bornées (4096 caractères, idem à l'écriture), valeur d'un champ mot de
  passe jamais exposée. XFA : détecté, affiché avec un bandeau, non remplissable. Documents
  chiffrés : formulaire non modifiable dans cette version (`lopdf` ne réécrit pas le
  chiffrement ; Lot 6).

## Alternatives écartées

- Dessiner les champs dans le calque SVG : réinventer les contrôles, l'accessibilité et le
  clavier que le navigateur offre déjà.
- Écrire les valeurs avec pdfium-render puis « corriger » : les chaînes `(/Yes)` sont déjà
  dans le fichier ; autant écrire juste du premier coup avec `lopdf`.
- Signature comme annotation `Stamp` avec image : « supprimable » comme une annotation, et
  les lecteurs la présenteraient comme un tampon ; une signature apposée doit rester.

## Conséquences

- Chaque saisie coûte un `to_bytes`, une passe `lopdf` et un rechargement PDFium (comme un
  instantané d'historique) ; acceptable pour des formulaires de bureau, à mesurer sur de très
  gros fichiers.
- Enregistrer sous MegaUltraPDF **normalise les apparences** de tous les champs (effet de
  `/NeedAppearances true` : PDFium réécrit `/AP` de chaque widget) ; un champ sans `/MK`
  perd sa bordure à l'état décoché, comme dans Acrobat.
- Les pages tournées (`/Rotate`) placent encore annotations et signature dans l'espace non
  tourné : à traiter au Lot 5 avec la géométrie des pages.
- Après aplatissement, les annotations du Lot 3 disparaissent aussi de la page (comportement
  de PDFium) : documenté dans le panneau (« définitifs, annulable avec Ctrl+Z »).
- Tests : `crates/mega-core/tests/forms.rs`, `tests/edit.rs`, `tests/service.rs`, CLI
  `fields`/`flatten`, scénarios CDP `formulaire.json`, `formulaire-xfa.json`.
