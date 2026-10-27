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
- **Un changement = une `EditCommand::SetFieldValue`** (annulable). Côté Rust, l'écriture est
  suivie d'un **rechargement** du document (`reload_from_bytes(to_bytes())`) : PDFium ne
  régénère l'apparence d'un champ texte qu'au chargement, et seulement si le formulaire
  déclare `/NeedAppearances true`. Pour les autres formulaires, `/V` est correct (PDF.js,
  Acrobat après régénération, notre calque HTML le montrent), l'image PDFium reste périmée.
- **Ce qui n'est pas modifiable dans ce lot** et pourquoi : boutons radio (`set_checked`
  n'écrit pas `/AS` des autres boutons du groupe → deux boutons cochés dans le fichier ;
  vérifié par test), combos et listes (pas d'API), `/NeedAppearances` (dictionnaire
  inaccessible). Ils sont **affichés désactivés avec une explication**, et traités au Lot 5
  avec `lopdf`, qui écrira directement `/V`, `/AS` et `/NeedAppearances`.
- **Aplatir** = `EditCommand::FlattenForm` sur toutes les pages (`PdfPage::flatten`, qui
  aplatit aussi les annotations) ; annulable par l'historique.
- **Signature visuelle = contenu de page**, pas annotation ni champ `/Sig` : une image
  (`PdfPageImageObject`, ajustée dans le rectangle en gardant les proportions) ou des traits
  (chemins noirs de 2 pt, bouts ronds), puis `regenerate_content`. Visible partout, non
  « décochable », non liée à une signature cryptographique (Lot 7, option désactivée).
  L'image est bornée (20 Mo, 8192 px) et décodée par la crate `image` (png, jpeg).
- **Valeurs lues** bornées (4096 caractères), valeur d'un champ mot de passe jamais exposée.
- XFA : détecté (`FormType::Xfa`), affiché tel quel avec un bandeau ; non remplissable.

## Alternatives écartées

- Dessiner les champs dans le calque SVG : réinventer les contrôles, l'accessibilité et le
  clavier que le navigateur offre déjà.
- Écrire les radios en forçant `/V` du parent via un détour : impossible sans `unsafe`.
- Signature comme annotation `Stamp` avec image : « supprimable » comme une annotation, et
  les lecteurs la présenteraient comme un tampon ; une signature apposée doit rester.

## Conséquences

- Chaque saisie coûte un `to_bytes` + rechargement (comme un instantané d'historique) ;
  acceptable pour des formulaires de bureau, à mesurer sur de très gros fichiers.
- Après aplatissement, les annotations du Lot 3 disparaissent aussi de la page (comportement
  de PDFium) : documenté dans le panneau (« définitifs, annulable avec Ctrl+Z »).
- Tests : `crates/mega-core/tests/forms.rs`, `tests/edit.rs`, `tests/service.rs`, CLI
  `fields`/`flatten`, scénarios CDP `formulaire.json`, `formulaire-xfa.json`.
