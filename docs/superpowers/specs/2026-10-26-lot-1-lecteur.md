# Lot 1 — Lecteur : spécification

Statut : approuvée par mandat (exécution autonome, 2026-10-26). Référence : conception §5 Lot 1.

## Intention

Transformer la coquille du Lot 0 en **lecteur PDF complet et confortable** : plusieurs
documents en onglets, rendu net à tout zoom sans bloquer l'interface, navigation par
vignettes et signets, sélection et recherche de texte, thème, raccourcis, préférences.
C'est le lot qui introduit le **patron acteur** (un seul thread propriétaire de PDFium) qui
remplace le verrou global du Lot 0, et le **cache de rendu**.

## Périmètre (ce qui entre)

Lecture (voir `docs/FONCTIONNALITES.md`, section Lecture, toutes les lignes marquées [1]) :
ouverture par dialogue / argument / glisser-déposer / fichiers récents ; mot de passe
(saisie, absent vs incorrect) ; onglets ; rendu asynchrone priorisé avec cache LRU et
placeholder ; zoom (%, largeur, page, Ctrl+molette) ; modes page unique / continu ;
rotation d'affichage ; navigation complète avec position mémorisée ; vignettes ; signets ;
liens internes et externes (confirmation) ; calque de texte (sélection, copie) ; recherche
(surbrillance, suivant/précédent, compteur, casse) ; propriétés ; thème ; raccourcis et aide ;
préférences persistantes.

## Hors périmètre

Toute écriture dans le PDF (Lot 2+), impression et export (Lot 6), i18n (Lot 7, les chaînes
restent centralisées), pièces jointes (Lot 6).

## Architecture ajoutée

- `mega-core::service` : **acteur PDFium**. `PdfService::spawn() -> ServiceHandle` (clonable,
  `Send`). Le thread possède `DocumentRegistry`-équivalent (`HashMap<DocId, Document>`), un
  cache LRU de PNG (budget mémoire, clé `(doc, page, échelle quantifiée, rotation)`), une file
  de requêtes **priorisée** (Visible > Préchargement > Vignette) et un **numéro de génération**
  par document : les rendus d'une génération périmée (zoom changé) sont répondus `Cancelled`
  sans travail. L'API du handle est **bloquante** (canaux `std::sync::mpsc`) ; le backend Tauri
  l'appelle depuis `tauri::async_runtime::spawn_blocking` dans des commandes `async`.
- `mega-core::Document` gagne : `render_page_with(page, scale, rotation)`, `text_boxes(page)`
  (caractères avec rectangles en points, origine bas-gauche), `search_page(page, query, opts)`,
  `outline()` (arbre de signets avec page cible), `links(page)` (rectangle + destination
  interne ou URI), `properties()` (métadonnées, version PDF).
- Backend Tauri : `State<ServiceHandle>` remplace `Mutex<DocumentRegistry>` ; commandes
  `async` ; protocole `mega://page/<id>/<n>?scale=&rot=&gen=&prio=` **asynchrone**
  (`register_asynchronous_uri_scheme_protocol`, réponse depuis `spawn_blocking`). Plugins :
  `store` (préférences, récents, positions), `opener` (liens externes), `dialog` (confirmations).
- Frontend : état en runes Svelte 5 dans `app/src/lib/state/` ; logique pure testée (Vitest)
  dans `app/src/lib/logic/` (géométrie points↔pixels avec rotation, calcul des zooms « ajuster »,
  sélection de texte par plage de caractères, virtualisation du défilement, table des
  raccourcis, liste des récents) ; composants dans `app/src/lib/components/`.

## Décisions (mes choix, consignées)

1. L'acteur vit dans `mega-core` (pas dans le backend) : testable sans Tauri, réutilisable par
   le CLI plus tard ; le CLI continue d'utiliser `Document` directement.
2. Le verrou global `engine::guard()` est **conservé** (ceinture et bretelles) : avec un seul
   thread PDFium il n'est plus contendu.
3. Sélection de texte **maison** (rectangles de caractères, plage d'index) plutôt que des
   `<span>` transparents : fidèle à l'ordre de lecture de PDFium et sans dépendre de la mise en
   page HTML.
4. Recherche exécutée par l'acteur sur tout le document (page par page) ; les résultats sont
   renvoyés par page avec leurs rectangles.
5. Vignettes = même protocole `mega://` à petite échelle, priorité basse.
6. Copie : `navigator.clipboard.writeText` (contexte sécurisé du WebView) ; pas de plugin.
7. Positions de lecture et récents : fichier `settings.json` du plugin store, clé = chemin.

## Concepts Rust travaillés

Threads et `std::thread::spawn`, canaux `mpsc` (requête) et `sync_channel(1)` (réponse unique),
patron acteur et `enum` de messages, `BinaryHeap` et `Ord` pour la priorité, `Arc` pour le
handle partagé, `spawn_blocking`, cache LRU (`VecDeque`/`HashMap`, budget), générations pour
l'annulation, `impl Ord` dérivé vs manuel, traits `Iterator` sur arbres (signets).

## Fixtures

`fixtures/outlined.pdf` (3 pages, 2 signets dont un enfant, 1 lien interne page 1 → 3, 1 lien
URI) générée à la main dans `testing.rs` (`outlined_pdf()`), `fixtures/simple.pdf` et
`fixtures/encrypted.pdf` existantes.

## Checklist de validation (fin de lot, pilotage CDP + captures)

1. Lancer sur `simple.pdf` : 3 pages en défilement continu, vignettes à gauche, titre dans l'onglet.
2. Ouvrir `encrypted.pdf` : dialogue mot de passe ; mauvais → « incorrect » ; `secret` → ouvert.
3. Ouvrir un second fichier : deuxième onglet ; Ctrl+W le ferme ; récents listent les deux.
4. Zoom 50 % → 200 % → ajuster largeur → ajuster page ; rotation 90° ; mode page unique.
5. Saisir « 3 » dans la zone page → page 3 ; Home/End ; clic sur un signet d'`outlined.pdf`.
6. Sélectionner « Page 2 » à la souris, Ctrl+C → presse-papiers contient « Page 2 ».
7. Rechercher « Page » : 3 résultats, F3 passe au suivant, surbrillance visible.
8. Propriétés : titre, nb pages, version ; thème sombre ; F1 affiche les raccourcis.
9. Glisser-déposer un PDF sur la fenêtre → ouvert dans un nouvel onglet.
10. Relancer l'app : thème, zoom et dernière position restaurés.
