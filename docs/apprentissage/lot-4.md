# Lot 4 — Formulaires et signature visuelle : note d'apprentissage

## Ce que le lot a construit

Les formulaires AcroForm : champs lus par Rust et affichés comme de vrais contrôles HTML sur
la page (texte, multiligne, mot de passe, cases ; radios, combos et listes affichés),
remplissage annulable, panneau « Formulaire » (valeurs, requis vides, aller au champ),
aplatissement, détection XFA ; signature visuelle par image ou dessin, apposée comme contenu
de page ; CLI `mega fields` et `mega flatten`. Décisions : ADR-0009.

## Concepts Rust rencontrés

### 1. `Drop` et ressources natives

L'environnement de formulaire de PDFium, le jeton `engine::guard()`, les fichiers
temporaires, les objets de page non encore attachés : tous libérés par `Drop`, jamais à la
main. Le bloc `{ let _guard = …; … }` suivi d'un `reload_from_bytes` dans `set_field_value`
montre pourquoi la **portée** d'un guard compte.

- Fiche : [`drop-et-ressources-natives`](concepts/drop-et-ressources-natives.md).

### 2. Traduire l'`enum` d'une bibliothèque

`read_field` (`forms/mod.rs`) fait un `match` exhaustif sur `PdfFormField` (huit variantes,
chacune d'un type différent) et retourne un `FormField` plat. La fonction générique
`common<F: PdfFormFieldCommon>(f: &F)` évite de répéter nom / lecture seule / requis huit
fois : un **trait** comme borne de générique (fiche
[`traits-et-objets-de-trait`](concepts/traits-et-objets-de-trait.md)).

### 3. Vérifier une hypothèse par un test, pas par la doc

La doc de `set_checked` d'un bouton radio laissait croire qu'il « sélectionne » le bouton.
Le test `check_box_and_radio_group` a montré qu'A restait coché après avoir coché B : seul
`/V` du bouton visé est écrit, pas `/AS` des autres. Décision : radios en lecture seule
jusqu'au Lot 5. Même méthode qu'au Lot 3 pour `/QuadPoints`.

### 4. Entrées non fiables, côté fichier

`insert_signature` borne la taille du fichier **avant** de le lire, les dimensions **avant**
de créer l'objet image, et traduit l'erreur de `image::load_from_memory` en
`InvalidOperation`. Une feature Cargo (`image/jpeg`) ajoute un décodeur : une dépendance se
règle au plus juste.

### 5. Rechargement comme outil

`reload_from_bytes(self.to_bytes()?)` (hérité de l'historique du Lot 2) sert ici à forcer
PDFium à régénérer les apparences après une écriture : un détour, mais un seul mécanisme à
comprendre.

### 6. Côté TypeScript (pour mémoire)

`fitRect` existe en Rust (`signature::fit`) et en TS (`logic/forms.ts`) avec la **même
règle**, testée des deux côtés : le TS ne décide rien, il prévisualise. `canvasStrokesToPdf`
inverse l'axe vertical du canvas ; les contrôles HTML envoient leur valeur au `change`, et le
calque se resynchronise depuis Rust après chaque édition.

## Décisions de conception du lot

ADR-0009. Consignées pendant l'exécution : radios, combos, listes et `/NeedAppearances` →
Lot 5 (`lopdf`) ; champ mot de passe : valeur jamais exposée ; signature = contenu de page.

## Pièges rencontrés

- **Radio `set_checked`** n'écrit que `/V` du bouton visé (voir plus haut).
- **`PdfFormFieldOption` n'a pas de paramètre de durée de vie** (E0107) : lire la définition
  avant de copier une signature « par analogie ».
- **Apparence des champs texte** : régénérée seulement au chargement et avec
  `/NeedAppearances` ; d'où le rechargement systématique et la fixture
  `form_pdf_without_need_appearances` qui documente la limite.
- **Revue de sécurité automatique** : chaînes lues bornées, boucle de comptage bornée, valeur
  des champs mot de passe masquée.

## Vérification sans souris

`scripts/ui-check.ps1 -Pdf <copie de form.pdf> -Scenario scripts/scenarios/formulaire.json`
(calque de champs, saisie « Dupont », case cochée puis Ctrl+Z, lecture seule, requis,
signature dessinée, Ctrl+S, aplatissement puis Ctrl+Z) et `formulaire-xfa.json` sur
`fixtures/xfa.pdf` (bandeau). `mega fields` sur la copie montre les valeurs écrites.

## Exercices de lecture

1. `forms/mod.rs`, `set_field_value` : listez les cinq façons dont la fonction peut refuser
   avant de toucher au document. Laquelle n'est pas une erreur de l'utilisateur ?
2. `forms/signature.rs`, `fit` : pour une image 300×100 dans un rectangle 100×100, calculez
   `(x, y, w, h)` à la main puis vérifiez avec le test `signature_image_is_fitted`.
3. `app/src/lib/components/FormLayer.svelte` : pourquoi l'envoi se fait-il au `change` et non à
   chaque frappe (`input`) ? Qu'arriverait-il à l'historique sinon ?

## Prérequis pour le Lot 5

Le modèle objet du PDF (dictionnaires, flux, références) tel que `lopdf` l'expose : c'est
avec lui que les dettes des Lots 3 et 4 (`/InkList`, `/AP` des notes, `/V` et `/AS` des
radios, `/NeedAppearances`, élagage des objets) seront réglées, avant l'édition de contenu.
