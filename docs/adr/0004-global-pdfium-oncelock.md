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
  commande et chaque requête `mega://` verrouille le temps strict de son travail.
- Le **patron acteur** (un thread travailleur qui possède tout l'état PDFium, dialogue par
  canaux, file de priorité des rendus, cache LRU) est prévu au **Lot 1** avec le rendu
  asynchrone. Le Lot 0 reste volontairement simple pour l'apprentissage.

## Recherche de la bibliothèque (ordre)

1. variable `MEGA_PDFIUM_DIR` ; 2. dossier de l'exécutable ; 3. `vendor/pdfium/<triple>/`
   à la racine du workspace (triple fourni par `build.rs` via `MEGA_TARGET_TRIPLE`) ;
4. bibliothèque système. Sinon `CoreError::LibraryNotFound` avec les chemins essayés.

## Conséquences

- Deux threads qui initialisent en même temps peuvent charger la DLL deux fois ; la seconde
  instance est abandonnée (sans effet). Acceptable.
- Tout accès à PDFium est sérialisé : le rendu de plusieurs pages ne sera pas parallèle. Le
  Lot 1 mesurera et, si nécessaire, rendra par lots côté acteur.
