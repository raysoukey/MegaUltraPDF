# ADR-0004 — Instance PDFium globale (`OnceLock`) ; patron acteur reporté au Lot 1

Date : 2026-10-26 · Statut : accepté

## Contexte

Dans `pdfium-render`, `PdfDocument<'a>` emprunte l'instance `Pdfium` dont il vient. Un type
`Document` qui posséderait à la fois son `Pdfium` et son `PdfDocument` serait auto-référencé,
ce que Rust interdit sans artifices. Par ailleurs PDFium n'est pas thread-safe, et une
application Tauri exécute ses commandes sur plusieurs threads.

## Décision

- Une **seule** instance `Pdfium` par processus, dans `static PDFIUM: OnceLock<Pdfium>`
  (`crates/mega-core/src/engine.rs`). `Document` contient donc un `PdfDocument<'static>`.
- La feature `thread_safe` de `pdfium-render` (activée par défaut) sérialise les appels à
  PDFium derrière un verrou interne ; en 0.9 tous les objets sont `Send + Sync`.
- Côté Tauri, le registre des documents vit dans `State<Mutex<DocumentRegistry>>` ; chaque
  commande et chaque requête `mega://` le verrouille pendant toute sa durée (une requête
  `mega://` tient le verrou du registre pendant le rendu et l'encodage PNG, car la référence
  au document emprunte le guard). Les commandes non-`async` s'exécutent sur le thread
  principal ; le protocole répond sur un autre thread : le `Mutex` est nécessaire.
- Le **patron acteur** (un thread travailleur qui possède tout l'état PDFium, dialogue par
  canaux, file de priorité des rendus, cache LRU) est prévu au **Lot 1** avec le rendu
  asynchrone. Le Lot 0 reste volontairement simple pour l'apprentissage.

## Recherche de la bibliothèque (ordre)

1. variable `MEGA_PDFIUM_DIR` ; 2. dossier de l'exécutable ; 3. `vendor/pdfium/<triple>/`
   à la racine du workspace (triple fourni par `build.rs` via `MEGA_TARGET_TRIPLE`) ;
4. bibliothèque système. Sinon `CoreError::LibraryNotFound` avec les chemins essayés.

## Conséquences

- L'initialisation est **strictement unique** (`OnceLock::get_or_init`) : les threads
  concurrents attendent le premier. Une version initiale tolérait un double chargement
  (« la seconde instance est jetée ») ; la CI macOS arm64 a montré que PDFium plante
  (SIGTRAP) quand deux threads l'initialisent en même temps. Test de régression :
  `concurrent_first_use_is_safe`.
- Un échec de chargement est mémorisé pour la durée du processus : après
  `cargo xtask fetch-pdfium`, relancer le programme.
- **Toute opération complète** sur un document (chargement, lecture, rendu, destruction,
  génération de fixtures) se fait sous un verrou de processus de `mega-core`
  (`engine::guard`). `thread_safe` ne verrouille que chaque appel natif ; PDFium garde de
  l'état global entre deux appels (code d'erreur du dernier chargement, lu par un second
  appel : « invalide » devenait « inconnu »), et des usages concurrents soutenus allaient
  jusqu'au SIGSEGV en CI Linux/macOS. Le patron acteur du Lot 1 (un seul thread PDFium)
  remplacera ce verrou. Règle : jamais de `guard()` imbriqué (interblocage).
- Tout accès à PDFium est sérialisé : le rendu de plusieurs pages ne sera pas parallèle. Le
  Lot 1 mesurera et, si nécessaire, rendra par lots côté acteur.
