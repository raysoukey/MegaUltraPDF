# Lot 6 — Export, impression, sécurité : spécification

Statut : approuvée par mandat (exécution autonome). Référence : conception §5 Lot 6, ADR-0003
(double moteur), ADR-0007 (commandes), ADR-0010 (passes `lopdf`, refus explicites).

## Intention

Faire sortir le document autrement qu'en PDF (images, texte, papier), le décrire
(métadonnées), le protéger ou le déprotéger (mot de passe, permissions), l'alléger
(optimisation), voir et extraire ses pièces jointes, et comparer deux versions. Tout ce qui
écrit dans le fichier passe par `lopdf` ou PDFium selon la règle ADR-0003 ; tout ce qui est
une décision sur le document est une `EditCommand`.

## Ce que les moteurs permettent

- **pdfium-render** : rendu à toute échelle (déjà), extraction du texte d'une page
  (`page.text().all()`), pièces jointes (`attachments()` : nom, octets ; création depuis des
  octets, suppression). Pas d'écriture des métadonnées, pas de chiffrement.
- **lopdf 0.46** : dictionnaire `/Info` (chaînes PDFDoc / UTF-16BE+BOM), `encrypt(&EncryptionState)`
  (V5 = AES-256, R6, permissions), `decrypt(password)`, `prune_objects`, `Stream::compress`
  (Flate), accès aux XObjects image (dictionnaire + flux) pour les recomprimer.
- **image 0.25** : décodage/encodage PNG et JPEG (qualité), redimensionnement.
- **WebView** : `window.print()` ouvre le dialogue d'impression du système (WebView2, WebKit) ;
  une vue d'impression HTML (une image par page, `@page`) donne un rendu fidèle sans code natif.

## Périmètre

Entre :
1. **Export en images** : pages (toutes, courante, sélection, plage), format PNG ou JPEG
   (qualité), résolution en DPI (72–600), dossier et préfixe ; un fichier `<préfixe>-N.png`.
2. **Export du texte** : pages choisies, un fichier `.txt` UTF-8 (une ligne « ===== Page N »
   entre les pages, option), ordre de lecture de PDFium.
3. **Impression** : vue d'impression (toutes les pages ou une plage, 150 ou 300 dpi, avec ou
   sans annotations), `window.print()` ; aperçu avant d'imprimer ; Ctrl+P.
4. **Métadonnées** : titre, auteur, sujet, mots-clés modifiables (dialogue Propriétés) ;
   `/ModDate` mis à jour ; commande annulable. XMP non touché (documenté).
5. **Protection** : mot de passe d'ouverture (utilisateur), mot de passe propriétaire,
   permissions (imprimer, copier, modifier, annoter) ; AES-256 (R6). **Retrait** de la
   protection d'un document ouvert avec son mot de passe. Après protection, le document reste
   ouvert (rechargé avec son mot de passe) ; les passes `lopdf` ultérieures le refusent, donc
   l'interface propose la protection comme **dernière étape** et le dit.
6. **Optimisation** : élagage des objets inutilisés, compression Flate des flux non
   compressés, recompression des images (JPEG, qualité choisie) et réduction des images
   au-delà d'une dimension maximale ; rapport avant / après (octets). Images traitées :
   DeviceRGB / DeviceGray 8 bits sans masque ni `/Decode`, filtre absent, Flate ou DCT ; les
   autres sont laissées telles quelles (documenté).
7. **Pièces jointes** : liste (nom, taille), extraction vers un fichier, ajout d'un fichier,
   suppression (commandes annulables) ; onglet « Pièces jointes » dans le panneau latéral.
8. **Comparaison** : deux documents ouverts (ou un fichier choisi), page par page : rendu des
   différences (pixels différents en rouge sur les pages estompées) et compteur de pixels
   différents ; navigation page suivante / précédente ; comparaison « rapide » du nombre de
   pages et des tailles.
9. CLI : `mega export-images`, `mega export-text`, `mega meta` (lecture / `--set`),
   `mega protect`, `mega unprotect`, `mega optimize`, `mega attachments` (`--extract`),
   `mega compare`.

N'entre pas : export PDF/A, impression sans dialogue (silencieuse), signature numérique,
OCR, édition XMP, comparaison textuelle mot à mot (seulement visuelle), recompression des
images CMYK / indexées / avec masque, chiffrement RC4 (lecture seulement).

## API publiques ajoutées

### `mega-core`

```rust
// export.rs
pub enum ImageFormat { Png, Jpeg { quality: u8 } }
pub struct ImageExport { pub pages: Option<Vec<u16>>, pub format: ImageFormat, pub dpi: u16, pub with_annotations: bool }
impl Document {
    pub fn export_images(&self, spec: &ImageExport, dir: &Path, prefix: &str, force: bool) -> CoreResult<Vec<PathBuf>>;
    pub fn page_text(&self, page: u16) -> CoreResult<String>;
    pub fn export_text(&self, pages: Option<&[u16]>, separators: bool) -> CoreResult<String>;
}
// properties.rs
pub struct MetadataPatch { pub title: Option<String>, pub author: Option<String>, pub subject: Option<String>, pub keywords: Option<String> }
impl Document { pub fn set_metadata(&mut self, patch: &MetadataPatch) -> CoreResult<()>; }
// security.rs
pub struct Permissions { pub print: bool, pub copy: bool, pub modify: bool, pub annotate: bool }
pub struct Protection { pub user_password: String, pub owner_password: String, pub permissions: Permissions }
pub struct SecurityInfo { pub encrypted: bool, pub user_password_required: bool, pub permissions: Permissions }
impl Document {
    pub fn security(&self) -> CoreResult<SecurityInfo>;
    pub fn protect(&mut self, p: &Protection) -> CoreResult<()>;     // lopdf V5, puis rechargement avec le mot de passe
    pub fn unprotect(&mut self) -> CoreResult<()>;                     // lopdf decrypt(mot de passe connu), rechargement
}
// optimize.rs
pub struct OptimizeOptions { pub image_quality: Option<u8>, pub max_image_px: Option<u32>, pub compress_streams: bool }
pub struct OptimizeReport { pub bytes_before: u64, pub bytes_after: u64, pub images_recompressed: u16, pub objects_removed: u32 }
impl Document { pub fn optimize(&mut self, opts: &OptimizeOptions) -> CoreResult<OptimizeReport>; }
// attachments.rs
pub struct Attachment { pub index: u16, pub name: String, pub size: u64 }
impl Document {
    pub fn attachments(&self) -> CoreResult<Vec<Attachment>>;
    pub fn save_attachment(&self, index: u16, path: &Path) -> CoreResult<()>;
    pub fn add_attachment(&mut self, path: &Path) -> CoreResult<()>;
    pub fn delete_attachment(&mut self, index: u16) -> CoreResult<()>;
}
// compare.rs
pub struct PageDiff { pub image: RenderedPage, pub differing_pixels: u64, pub total_pixels: u64 }
pub fn compare_pages(a: &Document, page_a: u16, b: &Document, page_b: u16, scale: f32) -> CoreResult<PageDiff>;
// edit.rs
EditCommand::{SetMetadata { patch }, Protect { protection }, Unprotect, Optimize { options }, AddAttachment { path }, DeleteAttachment { index }}
```

Limites : `max_image_px` ∈ [256, 8192], qualité ∈ [10, 100], DPI ∈ [36, 600] et image rendue
≤ 50 Mpx ; pièce jointe ≤ 100 Mo ; mots de passe ≤ 127 octets UTF-8 (SASLprep hors périmètre :
ASCII et Latin-1 acceptés, le reste refusé avec message) ; `export_images` refuse d'écraser sans
`force`.

### Acteur, Tauri, `api.ts`

Requêtes : `PageText`, `ExportImages`, `ExportText`, `Security`, `Attachments`,
`SaveAttachment`, `ComparePages` (PNG en octets bruts, `tauri::ipc::Response`). Commandes
Tauri homonymes ; `api.ts` : `ImageExport`, `MetadataPatch`, `Protection`, `Permissions`,
`SecurityInfo`, `OptimizeOptions`, `OptimizeReport`, `Attachment`, `PageDiff` (sans l'image :
l'image arrive par `comparePages` en `Uint8Array` → `blob:` URL), `EditCommand` étendu.

### Interface

- Menu « Fichier » (nouveau regroupement dans la barre : Ouvrir, récents, Exporter…,
  Imprimer…, Propriétés…, Protéger…, Optimiser…, Comparer…).
- `ExportDialog` (images / texte, portée, format, qualité, DPI, dossier, préfixe, séparateurs).
- `PrintView` (plein écran, images `mega://` à la résolution choisie, `@media print`, bouton
  Imprimer → `window.print()`, Échap pour fermer) ; raccourci Ctrl+P.
- `PropertiesDialog` : champs modifiables + `SecurityInfo` (lecture) + bouton Enregistrer.
- `ProtectDialog` (mots de passe, confirmation, permissions ; retrait si protégé).
- `OptimizeDialog` (options, lancer, rapport).
- `AttachmentsPanel` (onglet latéral : liste, extraire, ajouter, supprimer).
- `CompareDialog` (choix de l'autre document ouvert ou d'un fichier, page, image des
  différences, compteur, navigation).

## Concepts Rust travaillés

Traits `std::io::Read` / `Write` et `Cursor` (encodage d'images en mémoire), features Cargo
(`image/jpeg` déjà là ; `lopdf` avec chiffrement), `bitflags` par l'exemple (`Permissions` de
lopdf), génération d'un secret (`rand`/`getrandom` via lopdf : d'où vient l'aléa), chaînes
vs octets pour les mots de passe (UTF-8 → octets), `Option<&[u16]>`, erreurs typées
composées (`CoreError::WrongPassword` réutilisée), comparaison d'images (itérateurs `zip`,
`chunks_exact`).

## Fixtures

`fixtures/encrypted.pdf` (utilisateur) et `fixtures/owner-only.pdf` (propriétaire seul)
existants ; `testing::attachments_pdf()` (deux fichiers embarqués `/EmbeddedFiles`, écrit à la
main) → `fixtures/attachments.pdf` ; `testing::heavy_images_pdf()` (image RGB 800 × 600 Flate
non optimisée + image Gray DCT) ; `sample_pdf` pour les exports ; deux variantes
`sample_pdf(2, …)` dont une modifiée pour la comparaison.

## Checklist CDP (`export-securite.json`, sur une copie de `simple.pdf`)

1. Propriétés : modifier titre et auteur → onglet renommé, libellé « Modifier les
   métadonnées », `mega meta` lit les valeurs après enregistrement.
2. Optimiser → rapport affiché (avant/après), libellé « Optimiser ».
3. Exporter : dialogue ouvert, options visibles (le dossier passe par un dialogue natif : la
   production des fichiers est vérifiée par la CLI).
4. Imprimer : vue d'impression affichée avec 3 images, `window.print` (remplacé par un
   espion) appelé ; Échap ferme.
5. Protéger (mot de passe « secret », impression interdite) → `SecurityInfo` affiché
   « protégé » ; enregistrer ; `mega info` demande le mot de passe ; `mega unprotect` le retire.
6. Pièces jointes (copie d'`attachments.pdf`, scénario `pieces-jointes.json`) : liste de 2,
   suppression annulable, ajout refusé sans dialogue natif (couvert par la CLI).
7. Comparer avec un autre onglet : image des différences et compteur > 0.
