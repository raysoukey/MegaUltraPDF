# ADR-0019 — Fidélité à l'enregistrement : rien de supprimé ne survit, rien de non demandé ne se perd

Date : 2026-10-31 · Statut : accepté (Lot 14)

## Contexte

L'audit `docs/audit/2026-10-31-compatibilite-pdf.md` a confronté la v0.13.0 à 137 PDF (132 réels
et 5 générés sur ce poste) et à quatre moteurs indépendants (poppler, MuPDF, pdf.js, veraPDF). En
écriture, il a montré qu'un fichier enregistré par MegaUltraPDF n'était pas toujours celui que
l'utilisateur croyait avoir enregistré :

- **E1** : quand `lopdf` ne relisait pas la sortie de PDFium, la passe de nettoyage
  (`cleanup::before_save`) était sautée sans rien dire. Une annotation supprimée restait alors
  dans le fichier, son texte en clair.
- **E2** : PDFium n'écrit que des en-têtes `%PDF-1.0` à `%PDF-1.7`. Un fichier `%PDF-2.0`
  ressortait en `%PDF-1.0`, et tout PDF/A-4 ou PDF/UA-2 modifié devenait non conforme.
- **E3** : fusionner, extraire ou scinder assemble un document **neuf** avec PDFium, qui ne
  copie que les pages. Tout le reste du catalogue disparaissait : formulaire, signets, pièces
  jointes, destinations, étiquettes de pages, calques, langue, XMP… Pire, le `/Parent` d'un
  widget copié désignait un objet quelconque du nouveau document : le formulaire était corrompu.
- **E4** : déplacer une page la remplaçait par une copie. Signets, liens et champ de formulaire
  qui visaient l'ancienne page ne menaient plus nulle part.
- **E5** : supprimer un objet balisé laissait l'arbre de structure citer un contenu absent.
- **E9, E10** : un JPEG inséré était réencodé (15 Ko devenaient 85 Ko) ; un texte Unicode
  inséré par PDFium incorporait la police DejaVu entière (un fichier de 2 Ko passait à 420 Ko).

Pendant les corrections, les tests de conformité étendus ont trouvé deux pertes silencieuses du
XMP (le flux `/Metadata`, qui porte titre, auteur et déclaration PDF/A), notées F21-4 et F21-5
(la liste des constats « F21 » est dans les limites assumées) :

- **F21-4** : dans un document chiffré, PDFium écrit le XMP **en clair**, puis le « déchiffre »
  en le relisant. Le XMP devenait des octets aléatoires dès la première édition, sur tout
  fichier chiffré par Acrobat ou qpdf (ils chiffrent le XMP par défaut).
- **F21-5** : PDFium retire le `/Filter` d'un XMP compressé mais en garde les octets
  compressés. Le XMP devenait illisible dès la première édition (titre, auteur et
  déclaration PDF/A perdus), même sans chiffrement.

L'intention du lot (`docs/superpowers/specs/2026-10-31-lot-14-compatibilite-mesuree.md`) : rien
de supprimé ne survit dans le fichier, rien n'est perdu que l'utilisateur n'ait pas demandé, la
version et les déclarations de conformité sont respectées.

Quelques mots employés plus bas :

- **remorque** (*trailer*) : le petit dictionnaire de fin de fichier qui désigne le catalogue
  (`/Root`), les métadonnées `/Info`, le chiffrement (`/Encrypt`) et l'identifiant `/ID` ;
- **table de références** (*xref*) : la table qui donne la position de chaque objet dans le
  fichier ; depuis PDF 1.5, elle peut être écrite **en flux** (compressée dans un objet) au
  lieu d'une table de texte suivie du mot-clé `trailer` ;
- **jeton PDFium** : le verrou de processus que rend `engine::guard()` ; tant qu'on le tient,
  aucun autre fil ne touche à PDFium (`docs/apprentissage/concepts/mutex-et-etat-partage.md`) ;
- **XObject formulaire** : un sous-dessin réutilisable, décrit dans son propre flux et appelé
  depuis le contenu d'une page par l'opérateur `Do` ;
- **CID** : dans une police composite (`Type0`), le numéro de caractère écrit dans le contenu
  de page ; une table le relie au glyphe à dessiner (`docs/apprentissage/lot-10.md`) ;
- **empreinte** : un hachage des octets, qui reconnaît un fichier sans le comparer en entier.

## Décisions

1. **Pas de repli silencieux après une suppression** (E1).
   - Toute opération qui **retire** une information appelle d'abord
     `Document::ensure_cleanable` : `delete_annotation`, `update_annotation`,
     `set_annotation_rect`, `delete_pages`, `delete_object`, `set_object_text`,
     `delete_attachment`, le caviardage et, depuis la relecture finale, `flatten_all` (les
     annotations aplaties quittent `/Annots`, leur texte devient orphelin) et
     `set_field_value` (l'ancienne apparence du champ devient orpheline). La garde vérifie
     que `lopdf` relit la sortie actuelle de PDFium (`cleanup::check_cleanable`). Sinon : refus `InvalidOperation`
     (« la suppression est refusée, l'ajout reste possible »), document intact.
   - La garde passe **avant** `mark_changed()` : une opération refusée ne rend pas le document
     « modifié » (un fichier signé ne serait pas réécrit pour rien). La même règle vaut pour
     toute méthode qui modifie : le lot a remis dans cet ordre `protect`, `unprotect`,
     `add_invisible_words`, `add_watermark`, `add_page_numbers` et `crop_pages`, qui valide
     maintenant toutes les pages avant d'en recadrer une seule.
   - La garde lève aussi le drapeau `needs_cleanup`. `before_save` rend un `Saved { bytes,
     cleaned }` marqué `#[must_use]`, et le code de production ne lit ses octets que par
     `Saved::into_checked(needs_cleanup, …)`. Si une suppression a eu lieu et que le nettoyage
     n'a pas pu s'appliquer, l'écriture est refusée (« enregistrement refusé », « extraction
     refusée », « protection refusée ») et le fichier d'origine reste tel quel. Une fusion
     refuse de même quand une de ses sources porte une suppression et que le nettoyage du
     document neuf n'a pas pu s'appliquer.

     [Apprentissage] `#[must_use]` sur un type fait avertir le compilateur quand une valeur
     de ce type est calculée puis jetée sans être lue : un nettoyage dont on oublierait de
     vérifier le résultat ne passerait pas `cargo clippy -D warnings`. Faire passer les octets
     par une seule méthode (`into_checked`) rend la vérification impossible à contourner par
     mégarde (`crates/mega-core/src/cleanup/mod.rs`).
   - Drapeau **prudent** : annuler ne le baisse pas (l'instantané rechargé peut lui aussi
     porter des orphelins) ; seul un enregistrement réussi le remet à faux.
   - Déplacer ou redimensionner un objet n'est pas gardé : rien de visible n'est retiré.
2. **Passe-plat pour les filtres `Identity`** (E1, L16). Quand `/StmF` et `/StrF` valent
   `Identity`, chaînes et flux sont en clair : seules les pièces jointes sont chiffrées (`/EFF`).
   `crypt::LopdfSession` lit alors le fichier **sans déchiffrer**, élague les orphelins, remet
   `/Encrypt` et `/ID` tels quels et **ne renumérote pas** les objets : le numéro d'un objet
   entre dans la clé de ses octets chiffrés. Un tel fichier sans remorque classique (table de
   références en flux) reste refusé, comme avant ; les gardes et l'enregistrement lisent
   toutefois la sortie de PDFium, qui en a toujours une.
   - **Aucun mot de passe n'est nécessaire** (relecture finale, constat F24-1). Rien n'étant
     chiffré hors des pièces jointes, PDFium ouvre ces fichiers sans vérifier de mot de passe,
     même quand le mot de passe d'ouverture n'est pas vide ; `lopdf`, lui, ne s'y authentifie
     pas et laisse le document chiffré. `LopdfSession` lit alors le dictionnaire `/Encrypt`
     (jamais chiffré) et, pour le gestionnaire standard à filtres `Identity` en version 4 ou
     plus, ouvre la session passe-plat, qui ne lit aucune clé. Les pièces jointes, seules
     chiffrées, ne sont ni déchiffrées ni rechiffrées : leurs octets sont recopiés.
3. **Version d'en-tête d'origine** (E2, E8). PDFium réécrit toujours l'en-tête en 1.x ; on
   reprend donc la version du fichier d'origine **après** PDFium, au moment où `lopdf` réécrit
   le fichier, plutôt que d'essayer de la faire écrire à PDFium, qui ne le sait pas.
   - `Document::file_version` lit `%PDF-M.n` dans les octets du fichier (`file_bytes`, tels
     que chargés ou enregistrés) par `crypt::header_version`, après un éventuel préfixe.
   - `before_save(bytes, password, min_version)` écrit la plus grande des deux versions : celle
     des octets de PDFium et celle d'origine. On ne baisse jamais une version : une édition
     peut ajouter une structure plus récente, jamais en retirer l'exigence.
   - Le repli sans nettoyage relève l'en-tête sur place (`crypt::raise_header_version` : même
     longueur, aucune position décalée).
   - La fusion prend la plus haute version des sources. L'extraction, la scission et la
     protection passent celle du document ; la signature garde la version déclarée du fichier
     qu'elle complète.
   - Protéger en AES-256 (révision 6) relève l'en-tête à 1.7 au minimum et pose
     `/Extensions << /ADBE << /BaseVersion /1.7 /ExtensionLevel 8 >> >>` : ce chiffrement
     n'existe pas en PDF 1.4, et un lecteur strict doit savoir qu'il le rencontrera.
4. **Report du catalogue** après fusion, extraction et scission (E3). PDFium ne copie que les
   pages ; plutôt que de lui faire copier autre chose (il ne le sait pas), on relit après coup
   la source et la copie avec `lopdf` et on reporte, structure par structure, ce qui concerne
   les pages copiées. C'est le module `pages/carry.rs` (et `pages/carry/` : `acroform.rs`,
   `dests.rs`, `labels.rs`, `layers.rs`).

   [Apprentissage] **Fonction pure sur un `lopdf::Document`** : chaque étape prend deux
   documents en mémoire et rend le second modifié, sans fichier ni PDFium. Ses tests
   construisent un petit document à la main et vérifient le résultat directement : rapide,
   sans moteur de rendu, et l'on voit exactement quel objet a changé. La copie d'un objet
   avec tout ce qu'il cite utilise une table de correspondance (numéro source → numéro cible)
   qui sert aussi d'ensemble des visités
   (`docs/apprentissage/concepts/complexite-et-collections.md`).
   - Entrées simples : `/Lang`, `/ViewerPreferences`, `/PageMode`, `/PageLayout`, `/Metadata`,
     `/OutputIntents`. Pièces jointes (`/Names /EmbeddedFiles`), renommées si le nom est déjà
     pris (`notes (2).txt`). À la fusion, la première source qui porte une entrée l'emporte.
   - Formulaire : `/DA`, `/DR`, `/NeedAppearances`, `/Fields` réduit aux champs dont un widget a
     survécu, **`/Parent` des widgets réassocié** au champ copié. À la fusion, un nom de champ
     déjà pris reçoit un suffixe (` (2)`).
   - Signets réduits à ceux dont la cible est copiée (destination vers une page copiée, ou lien
     web) ; destinations nommées remappées ; `/PageLabels` recalculées ; `/OCProperties`
     reconstruit à partir des calques que citent les pages copiées.
   - **Jamais reporté** : les scripts (`/Names /JavaScript`, `/OpenAction`, `/AA`, y compris
     ceux des champs et widgets copiés), car on n'importe pas de code ; l'arbre de structure
     (`/StructTreeRoot`), qui ne se découpe pas page par page (les `/StructParents` qui le
     citaient sont retirés) ; `/MarkInfo`, car `/Marked true` sans arbre déclarerait balisé un
     document qui ne l'est plus (faux en PDF/UA et en PDF/A-1a).
5. **Déplacer sans copier** (E4). `pages/reorder.rs` réordonne les `/Kids` de l'arbre des pages
   avec `lopdf` (session chiffrée comprise) et met `/Count` à jour jusqu'à la racine. Une page
   qui change de parent fige d'abord les attributs qu'elle héritait de l'ancien
   (`/Resources`, `/MediaBox`, `/CropBox`, `/Rotate`), et reçoit une valeur neutre (`/Rotate 0`,
   `/Resources` vide, `/CropBox` égale à sa `/MediaBox`) là où le nouveau parent lui en
   imposerait une à tort (`reorder::pin_inherited`). PDFium recharge ensuite le document. La
   page reste **le même objet** : tout ce qui la vise reste juste. Si `lopdf` ne lit pas le
   fichier ou n'y voit pas les mêmes pages que PDFium, le déplacement est refusé, document
   intact. On ne revient pas à l'ancienne copie, qui perdait signets et liens sans le dire.
   Après une suppression de pages, `cleanup::neutralize_dead_destinations` traite ce qui visait
   une page disparue : le signet reste sans cible, le lien et `/OpenAction` sont retirés.
6. **MCID orphelins** (E5). `cleanup/mcid.rs` retire de l'arbre de structure les références à
   un contenu balisé (MCID) qui n'existe plus dans la page. On ne retire que ce qui est
   **prouvé** absent. Rien n'est retiré pour un `/MCR` qui désigne un autre flux (`/Stm`), ni
   sur une page qui dessine un XObject formulaire (ou un XObject introuvable), car le contenu
   balisé pourrait s'y trouver. Une image ne bloque pas la passe.
7. **Une lecture `lopdf` gardée en cache** (`Document::lopdf_view`). La couche de texte de
   secours (`text/tounicode.rs`), les pièces jointes de page et les propriétés lisent le
   document avec `lopdf`. Sans cache, chaque page d'un long document relirait tout le fichier.
   La lecture est gardée jusqu'à la prochaine modification (`mark_changed` et `replace_inner`
   la vident).
   - Coût : de l'ordre de la taille du fichier en mémoire, par document ouvert.
   - Elle n'est pas construite à l'ouverture, seulement au premier besoin.
   - Elle prend le jeton PDFium (par `to_bytes`) : jamais sous `engine::guard()`, verrou non
     réentrant.

   [Apprentissage] Le cache est un `OnceLock<Option<lopdf::Document>>` : il se remplit une
   seule fois à travers une référence partagée (`&self`), et se vide par `take`, qui demande
   `&mut self`, ce qui garantit que personne ne le lit à ce moment-là
   (`docs/apprentissage/concepts/oncelock.md`).
8. **Le XMP survit à PDFium** (F21-4, F21-5). En résumé : PDFium abîme le XMP de deux façons,
   en l'écrivant en clair dans un document chiffré, et en lui retirant son filtre de
   compression sans le décompresser. Dans les deux cas, on corrige au seul endroit où les
   octets passent entre PDFium et nous : à la **sortie** de PDFium pour le chiffrement (on
   rechiffre ce flux, et lui seul), à l'**entrée** de PDFium pour la compression (on lui
   donne le XMP déjà décompressé). On ne touche jamais aux octets d'origine du fichier, et un
   échec ne bloque jamais un enregistrement : au pire, le XMP est perdu comme avant le lot.
   - **Document chiffré** : `crypt::seal_metadata` passe sur **chaque** sortie de PDFium
     (`Document::to_bytes`, seconde sortie de `rewritten_bytes`).
     - Tout flux `/Type /Metadata` resté en clair dans un document qui le déclare chiffré est
       chiffré, et lui seul. On le reconnaît à `<?xpacket`, `<x:xmpmeta` ou `<rdf:RDF` dans
       son premier Kio, après une éventuelle marque d'ordre UTF-8.
     - Le chiffrement suit le filtre de flux déclaré (`/StmF` ; RC4 avant la version 4), avec
       la clé propre à l'objet.
     - Le fichier est lu sans être déchiffré : aucun autre objet n'est touché ni renuméroté,
       et un flux n'est jamais chiffré deux fois.
     - Un filtre inconnu (`/StmF` absent de `/CF`, `/CFM /None`) est une erreur : se rabattre
       sur RC4, comme le fait `lopdf`, rendrait le XMP illisible partout.
     - Un échec laisse les octets de PDFium tels quels et est noté au journal de diagnostic.
     - `protect` chiffre le XMP comme Acrobat et qpdf ; jamais `encrypt_metadata: false`.
   - **XMP compressé** : `document::load_file_bytes` est le seul point d'entrée des octets de
     fichier dans PDFium (ouverture, annuler et rétablir, rechargement après une signature).
     - PDFium y reçoit une copie où chaque flux `/Type /Metadata` filtré est stocké décodé
       (`crypt::unfiltered_metadata_copy`) ; il l'écrit ensuite tel quel.
     - `file_bytes` n'est pas touché : un fichier ouvert puis enregistré sans modification
       garde ses octets exacts, signatures comprises.
     - La copie n'est faite que si `lopdf` lit le fichier **exactement** comme PDFium le
       lirait : lecture stricte (`crypt::load_exact`), table de références intacte, `%PDF-` en
       tout début, toute entrée en usage de la table présente, pas de `/Encrypt` (cas
       précédent), même nombre de pages après réécriture. Sur un fichier abîmé, les deux
       bibliothèques pourraient réparer différemment : PDFium verrait alors un autre document.
     - Sinon, PDFium reçoit les octets d'origine et la renonciation est notée au journal (sauf
       pour `/Encrypt`, chemin normal). Si PDFium refuse la copie, l'original est chargé, et
       c'est noté aussi.
     - Une simple lecture des octets (`may_have_filtered_metadata`, en temps linéaire) écarte
       d'abord le cas courant : aucun XMP compressé, aucune lecture `lopdf`.
   - **Coûts mesurés** :
     - à l'ouverture, +0,5 ms par Mo pour la lecture des octets, puis +6 ms par Mo une seule
       fois si un XMP compressé est trouvé ;
     - pour un document chiffré qui a un XMP, +7 ms par Mo à chaque instantané d'historique
       (environ 1,8 ms de dérivation de clé en révision 6, et 5 ms par Mo de lecture et
       réécriture `lopdf`) ;
     - rien pour un fichier non chiffré sans XMP compressé.
   - **Écrire les métadonnées** : `set_metadata` reporte titre, auteur, sujet et mots-clés dans
     le XMP existant (`dc:title`, `dc:creator`, `dc:description`, `pdf:Keywords`). Un élément
     manquant est créé, son préfixe déclaré au besoin ; un paquet XMP absent n'est pas créé.
     Sur un PDF/A-4, seul le XMP est écrit : aucun `/Info` n'est créé, et `/ModDate` n'est mis
     à jour que s'il existait (PDF/A-4 interdit `/Info`). À la lecture, `properties()` se
     replie sur le XMP quand `/Info` ne donne pas la valeur (L12).
9. **Déclarations de conformité respectées** (E6, E7, E11). Un document qui déclare PDF/A ou
   PDF/UA promet à ses lecteurs des règles précises (polices incorporées, pas de transparence
   en PDF/A-1, contenu ajouté balisé, apparence pour chaque annotation…). Ce que nous ajoutons
   doit les suivre, sinon une simple édition rend le fichier non conforme sans que
   l'utilisateur le sache. On adapte donc ce qu'on écrit à la déclaration lue dans le XMP,
   plutôt que de retirer la déclaration : filigrane, numéros et couche OCR sont des artefacts
   balisés, et choisissent police, couleur et transparence selon la norme déclarée ;
   `/NeedAppearances` est retiré après régénération des champs ; les marquages reçoivent leur
   `/AP`. Le détail et les mesures veraPDF sont dans ADR-0018 (§7), ADR-0008 et ADR-0009.
10. **Taille des fichiers** (E9, E10).
    - **Sous-ensemble de la DejaVu de PDFium** (E10, `cleanup/subset.rs`, dans `before_save`) :
      - la police n'est réduite que dans les octets **écrits** ; le document PDFium en
        mémoire garde la police entière ;
      - candidat = un programme `/FontFile2` **exactement** égal à la DejaVu embarquée
        (taille et empreinte) ;
      - les glyphes utilisés sont relevés dans les contenus de page, les XObjects formulaires
        et les apparences d'annotation ; le glyphe de l'espace est toujours gardé ;
      - `subsetter` renumérote les glyphes gardés : on réécrit `/CIDToGIDMap` et on garde les
        CID, si bien que `/W`, `/ToUnicode` et les flux de contenu restent justes sans y
        toucher ;
      - **tout doute laisse la police entière** : police citée hors des ressources parcourues
        (`/DR` d'un formulaire, police Type 3…), ExtGState à `/Font` ou `/SMask`,
        dictionnaire de ressources indirect partagé, contenu illisible, trop de glyphes,
        formulaires trop imbriqués. Jamais une police cassée, au pire une police trop
        grosse ;
      - mesuré : le programme de police écrit tombe sous 40 Kio (386 Ko avant).
    - **Image insérée par `lopdf`** (E9, `content/image.rs`) :
      - un JPEG gris ou RVB entre avec ses octets d'origine (`/DCTDecode`) : le fichier
        grossit de moins de 1,2 fois la taille du JPEG (plus 3 Kio de structure) ;
      - une autre image (PNG, JPEG CMJN) entre en pixels Flate, avec un `/SMask` seulement si
        un pixel est translucide ;
      - ressources propres à la page, placement de l'espace affiché vers l'espace du fichier
        par matrice (ADR-0010).
11. **Conformité vérifiée sur tous les chemins d'écriture** (§5 de l'audit,
    `crates/mega-core/tests/conformance/`). Nos propres tests relisent nos fichiers avec
    PDFium et `lopdf`, les deux bibliothèques qui les ont écrits : ils ne verraient pas un
    défaut que ces deux-là tolèrent. Chaque sortie passe donc aussi par des lecteurs
    indépendants, et une variante d'édition ajoutée plus tard ne peut pas échapper au
    contrôle.
    - Chaque variante d'`EditCommand`, écrite en JSON comme l'interface l'envoie, est appliquée
      puis enregistrée, et sa sortie passe `qpdf --check` sans erreur **ni** avertissement.
      La fonction `variant` fait un `match` sur `EditCommand` : une variante ajoutée sans cas
      l'empêche de compiler. La liste `ALL_VARIANTS` est ensuite vérifiée à l'exécution
      (`missing_variants`) : chaque variante doit être exercée par au moins une commande.
      `ocr_page` en est exclue (voir les limites).

      [Apprentissage] Le `match` exhaustif transforme un oubli en erreur de compilation
      (`docs/apprentissage/concepts/enums-et-match-exhaustif.md`).
    - Les fichiers chiffrés par qpdf (RC4 40 et 128 bits, AES 128 et 256 bits) gardent leur XMP
      au fil des éditions. Sont aussi couverts : la signature numérique visible, la fusion, la
      scission et l'extraction.
    - Un catalogue riche (17 structures) est contrôlé après chaque variante, et après fusion,
      extraction et scission.
    - Relectures facultatives par MuPDF (`MEGA_MUTOOL`) et pdf.js (`MEGA_PDFJS_CHECK`).
      Corpus réel (`MEGA_CORPUS`) : tout fichier s'ouvre sans panique, et chaque fichier tourné
      puis réenregistré passe qpdf s'il le passait avant. Avec veraPDF (`MEGA_VERAPDF`) :
      filigrane et numéros gardent la conformité des 23 fichiers PDF/A et PDF/UA du corpus, et
      changer le titre d'un PDF/A-4 aussi.
    - Sans qpdf, chaque test affiche un avertissement visible et passe ; la CI pose
      `MEGA_REQUIRE_QPDF=1`. Les variables sont décrites dans `CONTRIBUTING.md`.

## Conséquences

- Une suppression peut être refusée sur un fichier que `lopdf` ne lit pas (rare) ; le message
  dit que l'ajout reste possible. Chaque suppression coûte un `to_bytes` et une lecture
  `lopdf` de plus : mesuré au CLI en fin de lot sur `producteurs/tracemonkey.pdf` (1 Mo, 14 pages,
  profil `debug`, médiane de 7 passes), une extraction de 13 pages prend 186 ms contre 170 ms pour
  une rotation sans contrôle, soit au plus 15 ms par Mo ; le profil `release` divise ce coût.
- Après un enregistrement qui réécrit le fichier, PDFium est **rechargé depuis les octets
  écrits** (relecture finale, I1). Sans cela, la lecture `lopdf` (qui relit le fichier écrit,
  nettoyé) et PDFium (resté sur l'état d'avant le nettoyage) pouvaient donner des index
  `/Annots` différents : retirer une pièce jointe de page après avoir supprimé une note et
  enregistré supprimait la mauvaise. Coût mesuré au CLI sur `producteurs/tracemonkey.pdf`
  (1 Mo, rotation d'une page puis enregistrement, profil `debug`, médianes de deux séries de
  7 passes) : 163 ms avant, 165 à 166 ms après, soit environ 3 ms par Mo. Un enregistrement
  sans modification (octets recopiés) ne recharge rien.
- Un déplacement de pages sans effet (la sélection est déjà à sa place) ne touche à rien :
  pas d'étape d'historique, document non modifié, octets du fichier recopiés tels quels à
  l'enregistrement (une signature reste intacte).
- Un document fusionné, extrait ou scindé garde son formulaire (remplissable), ses signets,
  ses pièces jointes, ses calques et son XMP ; il perd son arbre de structure et ses scripts.
- L'identifiant `/ID` reste conforme : pour un fichier existant, le premier est conservé et le
  second renouvelé ; pour un document neuf (fusion, extraction, scission), les deux sont égaux
  (E12, aucune correction).
- Les flux écrits par `lopdf` (filigrane, OCR) sont recompressés par PDFium au rechargement : la
  sortie finale n'a aucun flux sans filtre (E13, aucune correction).
- Un fichier à XMP compressé est vu par PDFium à travers une réécriture `lopdf` (la forme de sa
  table de références change), jamais dans ses octets exacts ; ses octets d'origine restent
  ceux de `file_bytes`.
- Le fichier à police Unicode reste lourd en mémoire pendant la session (police entière dans
  PDFium) et léger sur le disque. Enregistrer, insérer un nouveau caractère, puis réenregistrer
  donne deux programmes de police : PDFium recharge la DejaVu entière quand le sous-ensemble
  relu n'a pas le glyphe. Les deux restent sous 40 Kio au total.

## Limites assumées

Les constats « F21 » sont ceux que les tests de conformité étendus ont trouvés pendant le lot,
sur le corpus de l'audit :

- **F21-1** : une page sans `/Resources` ;
- **F21-2** : un arbre de noms au nœud vide ou aux clés mal triées ;
- **F21-3** : un flux Flate suivi de données parasites ;
- **F21-4** : le XMP d'un document chiffré écrit en clair par PDFium (corrigé, décision 8) ;
- **F21-5** : le XMP compressé dont PDFium retire le filtre (corrigé, décision 8, sauf pour les
  fichiers ci-dessous).
- **F24-1** (checklist de fin de lot, corrigé par la relecture finale) : un fichier dont seules
  les pièces jointes sont chiffrées (`navigation/encrypted-attachment.pdf`, cas L10 de l'audit ;
  `chiffrement/enc-R6-encrypted-attachments.pdf`, observation O24-1) s'ouvre sans mot de passe,
  mais toute suppression y était **refusée**. La cause n'était pas le mot de passe manquant :
  `lopdf` ne s'authentifiait pas, et la session refusait le document resté chiffré **avant**
  d'examiner ses filtres. Avec des filtres `Identity`, la session passe-plat s'applique
  désormais sans mot de passe (décision 2) : la note supprimée disparaît du fichier, qui reste
  chiffré, pièces jointes intactes (tests de `tests/conformance/encrypted.rs` sur le fichier
  du corpus et de `tests/cleanup.rs` sur un fichier généré à mot de passe d'ouverture).
  L'interface n'a donc pas à demander ce mot de passe : l'ancienne dette O24-1 est sans objet.

Ce qui reste :

- **Entrées abîmées réécrites telles quelles** (F21-1 à F21-3). Le lot promet de ne pas
  **ajouter** de défaut, pas de réparer ceux de l'entrée : qpdf signale sur la sortie ce qu'il
  signalait déjà sur l'original. Le test `rewriting_repairs_damaged_corpus_files` garde cette
  exigence plus forte, ignoré (`#[ignore]`) avec les identifiants des constats.
- **XMP en UTF-16 ou UTF-32** (permis par ISO 16684-1, jamais rencontré dans un PDF) : non
  reconnu, laissé tel quel, comme avant le lot.
- **`/Type` indirect sur un flux de métadonnées** (`/Type 5 0 R`), ou dictionnaire de flux de
  plus de 4 Kio : la lecture des octets ne le repère pas, le XMP compressé est perdu à la
  première édition (comportement d'avant le lot).
- **Octets avant `%PDF-`**, et plus généralement tout fichier que `lopdf` ne lit pas exactement
  comme PDFium (table de références réparée, objet sauté) : pas de copie aux XMP décodés ; un
  XMP compressé y est perdu à la première édition, et c'est noté au journal.
- **JPEG au corps corrompu** : le décodeur l'accepte (zone complétée de gris), comme les lecteurs
  PDF ; il est inséré tel quel, sans erreur.
- **`ocr_page` hors de la boucle de conformité** : la commande lance Tesseract, absent de la CI.
  La couche de mots invisibles qu'elle écrit est contrôlée directement
  (`edits::ocr_text_layers_produce_conforming_files`).
- **Report du catalogue** : les pièces jointes d'une source chiffrée à filtres `Identity` ne
  sont pas reportées (chiffrées avec leur numéro d'objet) ; une destination nommée présente
  dans deux sources garde celle de la première ; une action de signet autre que « aller à » ou
  « lien web » est abandonnée ; le XMP reporté peut déclarer une conformité (`pdfaid`,
  `pdfuaid`) que la sortie n'a plus.
- **MCID** : un élément de structure écrit en dictionnaire direct dans le `/K` d'un autre, et
  les nœuds `/Kids` directs du `/ParentTree`, ne sont pas parcourus.
- **Apparences de champs non latins** régénérées par PDFium : il y cite des polices non
  incorporées (Arial, SimSun), que le lecteur doit trouver sur le poste.
- `neutralize_dead_destinations` suppose que `lopdf` voit les mêmes pages que PDFium ; un
  désaccord n'est pas mesuré à cet endroit.

## Alternatives écartées

- **Garder le repli silencieux, ou seulement avertir** : l'utilisateur croirait sa note
  supprimée alors qu'elle reste lisible dans le fichier. Refuser l'écriture est le seul choix
  sûr ; l'original reste intact.
- **Déplacer par copie PDFium puis remapper chaque référence** (signets, liens, champs,
  structure) : bien plus de code, et chaque structure oubliée serait une perte silencieuse.
  Réordonner `/Kids` ne laisse rien à remapper.
- **Reporter `/StructTreeRoot` à la fusion** : l'arbre ne se découpe pas page par page ; hors
  de portée raisonnable. Reporter `/MarkInfo` : faux, voir la décision 4.
- **`encrypt_metadata: false` à la protection** (premier contournement de F21-4) : annulé.
  Il ne réglait que nos propres fichiers, pas ceux qu'on reçoit déjà chiffrés, et s'écartait
  de ce que font Acrobat et qpdf.
- **Réparer le XMP compressé dans `to_bytes`** (lire chaque sortie de PDFium, puis une passe
  `lopdf`) : le coût serait payé à chaque édition (quelques ms par Mo) au lieu d'une fois à
  l'ouverture. La piste reste ouverte pour les fichiers abîmés, que l'ouverture écarte.
- **Greffer le XMP en place** (remplacer le seul flux et recalculer la table de références,
  sans réécriture `lopdf` complète) : éviterait les 5 ms par Mo de réécriture sur un document
  chiffré, mais demande un code délicat (positions, table en flux). Reporté (voir les dettes).
- **Router les textes Unicode de PDFium par `Type0Writer`** au lieu de réduire la police après
  coup : il aurait fallu changer l'insertion, la retouche, le texte libre et le tampon ; le
  sous-ensemble à l'enregistrement ne touche qu'un endroit.
- **Laisser PDFium créer l'image puis remplacer ses octets** : PDFium pose un `/SMask` dès que
  le bitmap a un canal alpha ; écrire l'objet image nous-mêmes (recette de `content/raster.rs`)
  est plus simple.
- **Refuser un JPEG sans marqueur de fin** : non fait ; les lecteurs PDF l'affichent.

## Dettes reportées

- Découper `cleanup/subset.rs` (près de 1 500 lignes) et `cleanup/mod.rs` en sous-modules.
- Partager les aides `lopdf` de page (quatre copies de `inherited_resources`) ; remplacer par
  une pile explicite les fonctions récursives antérieures au lot : `refs_of` de
  `cleanup::reachable`, et `walk` de `signature_fields` (`digital_signature/pdf.rs`, bornée à
  32 niveaux, avec ensemble des nœuds visités). Écart à l'ADR-0017 : pas de récursion sur une
  structure venue du fichier.
- Tailler `/W` et `/ToUnicode` aux seuls caractères utilisés (quelques Ko à gagner par police).
- Faire passer l'image d'une signature visuelle par `content::image::place` (elle passe encore
  par PDFium).
- Greffer le XMP en place au lieu d'une réécriture `lopdf` complète (documents chiffrés).
- Corriger la déclaration `pdfaid` / `pdfuaid` du XMP reporté par fusion et extraction.
- Passer le nombre de pages de PDFium à `before_save`, pour que
  `neutralize_dead_destinations` se retienne quand `lopdf` voit un autre arbre.
- Mémoriser `ensure_cleanable` (une lecture `lopdf` par édition gardée).
- Signatures certifiées : `/Perms`, `/DocMDP` à `P = 2` ou `3`, révisions de type DSS (avis
  `pdf-expert` à prendre).
- Trois gardes encore posées avant leur validation : `add_attachment`, `optimize` et
  `delete_annotation` appellent `mark_changed()` avant de refuser (sans effet par l'interface,
  où `apply` rétablit le drapeau ; seul un appel direct du CLI qui échoue en est touché, et il
  n'enregistre pas ensuite).
- Tests : le générateur `identity_encrypted_note_with_user_password_pdf` n'écrit ni `/ID` dans
  la remorque ni `/Length` dans `/Encrypt` (qpdf avertit, R6 n'en dépend pas) ; le test du corpus
  sur `navigation/encrypted-attachment.pdf` vérifie le nom de la pièce jointe conservée, pas que
  ses octets chiffrés sont recopiés à l'identique (vérifié à la main avec qpdf).
