# Lot 6 — Export, impression, sécurité : note d'apprentissage

## Ce que le lot a construit

Le document sort autrement qu'en PDF : images PNG/JPEG à la résolution voulue, texte brut,
impression par le dialogue du système (vue d'impression HTML). Il se décrit (titre, auteur,
sujet, mots-clés modifiables), se protège par mot de passe AES-256 avec permissions, se
déprotège, s'allège (optimisation conservatrice avec rapport), montre et extrait ses pièces
jointes, et se compare visuellement à une autre version. Un menu « Fichier » regroupe tout ;
CLI : `export-images`, `export-text`, `meta`, `protect`, `unprotect`, `optimize`,
`attachments`, `compare`. Décisions : ADR-0011.

## Concepts Rust rencontrés

### 1. `Read` / `Write` / `Cursor` : encoder en mémoire, écrire ensuite

Export JPEG, chargement borné d'images, `save_to` de lopdf, tampon brut de la comparaison :
partout la même discipline, « j'ai tous les octets avant de toucher au disque », qui rend
possible l'écriture atomique et la validation d'une réécriture.

- Fiche : [`read-write-et-cursor`](concepts/read-write-et-cursor.md).

### 2. Features Cargo et dépendances explicites

`image` sans ses décodeurs inutiles, `chrono` réduit à l'horloge, `rand` déclaré bien que déjà
présent, notre feature `testing`. Une dépendance est une décision écrite.

- Fiche : [`features-cargo-et-dependances`](concepts/features-cargo-et-dependances.md).

### 3. `Option` comme filtre d'éligibilité (`optimize.rs`, `eligible`)

Une fonction qui renvoie `Option<Candidate>` et enchaîne les `?` : chaque condition manquante
fait sortir avec `None`, l'appelant écrit `let Some(c) = eligible(dict) else { continue }`.
Plus lisible qu'une cascade de `if`, et impossible d'oublier un cas : tout ce qui n'est pas
prouvé éligible est laissé intact.

### 4. Un invariant qui traverse l'historique (`edit.rs`, `Snapshot.password`)

Protéger change le mot de passe nécessaire pour relire les octets ; annuler doit donc
restaurer **et les octets et le mot de passe**. L'instantané porte les deux : c'est le test
`protect_is_undoable` qui l'a imposé. Même leçon qu'au Lot 2 (patron memento) : ce qui est
nécessaire pour rejouer un état fait partie de l'état.

### 5. Bits de permissions et `bitflags` par l'exemple

`lopdf::Permissions` est un ensemble de drapeaux (`PRINTABLE | COPYABLE …`) ; nos quatre
booléens y sont traduits dans `security::to_lopdf`, et relus à l'inverse dans `/Encrypt /P`
quand PDFium ne connaît pas la révision. Un entier comme sac de booléens : vieux comme le
PDF, et toujours là.

### 6. Vérifier ce que dit une bibliothèque

Trois surprises trouvées par les tests : pdfium-render demande une balise « ModificationDate »
qui n'existe pas (date toujours vide depuis le Lot 1), ne connaît pas les révisions de sécurité
5 et 6, et liste les pièces jointes dans l'ordre de l'arbre de noms. Dans chaque cas la
réponse est la même : un test qui asserte l'effet, une note dans CLAUDE.md, un contournement
documenté.

## Côté interface

Le menu « Fichier » (`FileMenu.svelte`) remplace la liste déroulante des récents ; la vue
d'impression est un composant plein écran qui réutilise le protocole `mega://` et
`@media print` ; la comparaison reçoit un tampon binaire décodé par `parseCounters`
(testé). Les dialogues partagent `PageScopeSelect` du Lot 5.

## Exercices (plus tard, à votre rythme)

1. `security.rs`, `to_lopdf` : quels bits de `/P` restent à zéro si l'utilisateur refuse
   « modifier » ? Vérifiez avec `mega meta --password` sur un fichier produit par
   `mega protect --no-modify`.
2. `optimize.rs` : ajoutez (sur le papier) l'éligibilité des images `/ColorSpace /Indexed`.
   Quelles clés faut-il lire, et pourquoi le JPEG n'est-il plus le bon format de sortie ?
3. `edit.rs` : tracez à la main `apply(Protect)` puis `undo()` puis `redo()` : quelle valeur a
   `self.password` à chaque étape, et d'où vient-elle ?
4. `export.rs` : pourquoi vérifier le budget de pixels **avant** de rendre la première page,
   alors que `render_page_with` le vérifie déjà ?

## Ce qui reste ouvert

Export PDF/A, comparaison textuelle, impression sans dialogue, recompression des images
CMYK / indexées / masquées, filigrane image, édition XMP, SASLprep des mots de passe.

## Prérequis pour le Lot 7 (finition, distribution)

- `cfg(feature)` et profils de release : `features-cargo-et-dependances`.
- Bundler Tauri (NSIS/MSI, AppImage/deb, dmg), chemin de la DLL PDFium en release
  (`MEGA_PDFIUM_DIR`, Lot 0).
- i18n branchée sur `strings.ts` (toutes les chaînes y sont déjà), ARIA et navigation clavier.
