# Lot 4 — Formulaires et signature visuelle : spécification

Statut : approuvée par mandat (exécution autonome). Référence : conception §5 Lot 4,
ADR-0007 (commandes), ADR-0008 (annotations ; les champs de formulaire sont des annotations
`Widget`).

## Intention

Remplir un formulaire PDF (AcroForm) comme dans un navigateur : les champs apparaissent
comme de vrais `<input>` superposés à la page, on tape, on coche, on passe au suivant avec
Tab ; on enregistre (les valeurs sont écrites dans le fichier) ; on peut **aplatir** le
formulaire (les champs deviennent du contenu fixe). Et apposer une **signature visuelle** :
une image (PNG/JPEG) ou un tracé à la souris, placé sur la page et enregistré comme contenu
de page (donc visible partout, non modifiable comme un champ).

## Ce que `pdfium-render` 0.9.4 permet sans `unsafe`

- Lecture : `document.form()` (`form_type` : aucun / AcroForm / XFA), par page les annotations
  `Widget` → `form_field()` : `PdfFormField::{Text, Checkbox, RadioButton, ComboBox, ListBox,
  PushButton, Signature, Unknown}` ; commun : `name`, `is_read_only`, `is_required` ; texte :
  `value`, `is_multiline`, `is_password` ; case/radio : `is_checked`, `group_value`,
  `index_in_group` ; combo/liste : `options` (libellé, `is_set`), `value`.
- Écriture : texte `set_value` (écrit `/V` et `/M`) ; case et radio `set_checked` (écrit `/AS`
  et `/V`, l'apparence change tout de suite). **Pas de réglage** pour combo et liste.
- Apparence d'un champ texte après `set_value` : PDFium ne la régénère pas en place ; il la
  régénère **au chargement** du document si `/NeedAppearances true` est dans l'`/AcroForm`.
  D'où la règle : après chaque écriture de valeur, le document est **rechargé** depuis ses
  octets (même mécanisme que l'historique) ; les formulaires sans `NeedAppearances` gardent
  une apparence périmée dans PDFium et Acrobat (valeur correcte dans `/V`, affichée par
  PDF.js et par notre calque HTML). Poser `NeedAppearances` exige `lopdf` → Lot 5.
- Aplatir : `PdfPage::flatten()` (annotations et champs → contenu).
- Image : `PdfPageImageObject::new(document, &DynamicImage)` + `translate`/`scale`
  (crate `image`, déjà dépendance ; ajouter la feature `jpeg`).

## Décisions (mes choix, consignées ; ADR-0009)

1. **Les champs sont des widgets HTML natifs** superposés (calque `FormLayer` par page) :
   `<input type=text|password>`, `<textarea>`, `<input type=checkbox|radio>`, `<select>`
   (combo, liste : **affichage seul**, désactivé, avec un titre expliquant la limite).
   Tab/Maj+Tab natifs. L'image rendue par PDFium reste en dessous (apparences PDFium) ; le
   calque masque le champ sous-jacent par un fond opaque pour éviter le doublon visuel.
2. **Chaque changement de valeur = une `EditCommand::SetFieldValue`** (annulable, « • »).
   Côté Rust : `set_value`/`set_checked` puis **rechargement** du document pour que l'apparence
   suive (`NeedAppearances`). Les radios d'un même groupe : cocher l'un décoche les autres
   (PDFium le gère par `/V` du groupe ; vérifié par test).
3. **Aplatir** = `EditCommand::FlattenForm` (toutes les pages) ; annulable ; après quoi la page
   n'a plus de champs ni d'annotations (PDFium aplatit aussi les annotations : documenté).
4. **Signature visuelle** = `EditCommand::InsertSignature { page, rect, source }` avec
   `source = Image { path } | Strokes { strokes }` : objets de contenu de page (image
   redimensionnée dans `rect` en gardant les proportions ; traits = chemins arrondis de
   2 pt, noirs), puis `regenerate_content`. Pas un champ `/Sig` (signature cryptographique
   = Lot 7, option désactivée).
5. **Modèle** : `FormField { page, index, name, kind, value, checked, options, rect,
   read_only, required, multiline, password }` (lecture, `serde`) ; `FieldValue = Text(String)
   | Checked(bool)` (écriture). `Document::form_type()`, `form_fields(page)`,
   `set_field_value(page, index, &FieldValue)`, `flatten_all()`, `insert_signature(...)`.
6. Hors périmètre : XFA (détecté et signalé : « formulaire XFA non pris en charge »),
   JavaScript de formulaire, calculs, soumission, `/NeedAppearances` (Lot 5), listes et combos
   modifiables (Lot 5), signature cryptographique (Lot 7), import/export FDF.

## Périmètre (ce qui entre) — `docs/FONCTIONNALITES.md`, section « Formulaires et signature »

- Afficher et remplir : texte (mono/multi-ligne, mot de passe), cases, radios ; afficher
  combos et listes ; champs lecture seule et requis signalés ; navigation Tab ;
  enregistrement des valeurs ; annuler/rétablir ; aplatir.
- Signature visuelle : image ou dessin, placement par glisser (rectangle), enregistrement.
- Panneau « Formulaire » (liste des champs avec valeurs, clic = aller au champ, compteur
  des champs requis vides) ; bouton Aplatir avec confirmation.
- CLI : `mega fields <fichier>` (liste), `mega flatten <fichier> -o <sortie>`.

## Architecture ajoutée

- `mega-core::forms` : modèle, lecture (`PdfFormField` → `FormField`), écriture, aplatir,
  signature (`signature.rs` : image via `image::load_from_memory`, traits via objets chemin).
- `edit.rs` : `SetFieldValue { page, index, value }`, `FlattenForm`, `InsertSignature`.
- `service` : `Request::FormFields { id, page }`, `FormInfo { id }` (type, nombre de champs).
- Tauri : `form_fields(id, page)`, `form_info(id)` ; `api.ts` : `FormField`, `FieldValue`,
  `FormInfo`, `SignatureSource`.
- Frontend : `components/FormLayer.svelte` (champs HTML, `pdfRectToCss`), `FormPanel.svelte`
  (onglet « Formulaire »), `SignatureDialog.svelte` (fichier image ou canvas de dessin →
  traits en points), outil `signature` dans la barre d'annotation (placement par glisser),
  `logic/forms.ts` (pure : champs requis vides, conversion des traits canvas → PDF),
  chaînes dans `strings.ts`.

## Fixtures

- `testing::form_pdf()` écrit à la main : `/AcroForm` avec `/NeedAppearances true`, `/DR`
  Helvetica, champs : texte « nom » (vide), texte multiligne « adresse », case « ok »
  (`/AP` `/Yes` et `/Off`), groupe radio « choix » (2 boutons, états `/A` et `/B`), combo
  « ville » (`/Opt`, `/V (Paris)`), texte lecture seule « ref » (`/Ff 1`), texte requis
  « email » (`/Ff 2`). `fixtures/form.pdf` via `gen-fixtures`.
- `testing::xfa_pdf()` minimal (catalogue avec `/AcroForm << /XFA … >>`) pour la détection.

## Concepts Rust travaillés

`Drop` et ressources natives (le `form handle` de PDFium vit avec le document), conversions
`From`/`match` sur une `enum` de la bibliothèque vers la nôtre, features Cargo (`image/jpeg`),
données d'entrée non fiables (image : taille bornée, décodage faillible), rechargement d'état
(`reload_from_bytes` réutilisé), `Option` et valeurs par défaut.

## Checklist de validation (CDP, `formulaire.json`, sur une copie de `form.pdf`)

1. Ouvrir : panneau « Formulaire » liste 7 champs ; calque : 1 case, 2 radios, 1 combo désactivé.
2. Taper « Dupont » dans « nom », Tab → « • », panneau affiche « Dupont », Ctrl+Z vide.
3. Cocher « ok », choisir le radio B → image rerendue (case cochée visible), Ctrl+Z.
4. Champ « ref » non modifiable ; « email » signalé requis (compteur « 1 requis vide »).
5. Signature : dessiner trois traits dans le dialogue, placer sur la page → visible dans l'image.
6. Ctrl+S → `mega fields copie.pdf` montre `nom = Dupont`, `ok = Yes`.
7. Aplatir → plus de champs ; `mega fields` : 0 ; Ctrl+Z → 7 champs.
8. Ouvrir `xfa.pdf` → bandeau « formulaire XFA non pris en charge ».
