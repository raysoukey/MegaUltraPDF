# ADR-0013 — OCR par Tesseract en couche invisible, signatures CMS par mise à jour incrémentale

Date : 2026-10-27 · Statut : accepté (Lot 8)

## Contexte

Deux capacités restaient dans la liste de référence, prévues « désactivées par défaut » par
la conception : l'OCR des documents numérisés et la signature numérique cryptographique
(avec la vérification des signatures existantes). L'une dépend d'un moteur de reconnaissance,
l'autre d'une pile cryptographique ; aucune ne doit ajouter d'appel réseau.

## Décisions

1. **Features Cargo.** `mega-core` déclare `ocr` et `signature`, désactivées par défaut :
   la bibliothèque n'embarque ni le code OCR ni la pile cryptographique sans les demander.
   `mega-cli` les active par défaut, `mega-app` les active toujours (ses commandes Tauri les
   utilisent). La CI vérifie `cargo check -p mega-core --no-default-features`.
2. **OCR = la commande `tesseract`**, installée par l'utilisateur et lancée comme processus
   (liste d'arguments, jamais de shell ; sans fenêtre de console sous Windows). Recherche :
   réglage de l'application, `MEGA_TESSERACT`, `PATH`, dossiers d'installation usuels.
   Comparée à un moteur lié (compilation C++ pénible), à `ocrs` (Rust pur mais en
   préversion, entraîné sur l'anglais, modèles à télécharger) et aux OCR des systèmes (un
   moteur par OS), c'est la seule option qui couvre le français, les trois systèmes, le
   hors-ligne et une construction simple. Sa faiblesse, l'installation séparée, est
   compensée par un dialogue qui détecte l'outil et ses langues.
3. **Couche de texte invisible** (`Document::add_invisible_words`) : flux `lopdf` ajouté
   après le contenu, ressources propres à la page, Helvetica WinAnsi, mode `3 Tr`, un mot par
   boîte en espace affiché (pages tournées comprises), étiré par `Tz` à la largeur mesurée par
   PDFium ; les mots d'une ligne partagent taille et ligne de base, une espace explicite les
   sépare (sinon « Page 2 » ne serait pas trouvé). Caractères hors WinAnsi : `?`.
4. **OCR page par page** (`EditCommand::OcrPage`) : historique, progression et arrêt entre
   deux pages ; une page qui a déjà du texte est refusée (relancer l'OCR est sans effet).
5. **Octets du fichier.** `Document` garde les octets chargés ou enregistrés (`file_bytes`)
   et un drapeau `changed` ; `save_as` réécrit ces octets **tels quels** tant que rien n'a
   changé (une réécriture PDFium casserait toute signature), et annuler / rétablir jusqu'à
   l'état enregistré rend le document de nouveau intact. Toute méthode publique qui modifie
   le document appelle `mark_changed()` (le CLI modifie par appels directs). Les signatures
   se vérifient toujours sur `file_bytes`.
6. **Signer = enregistrer**, par **mise à jour incrémentale** (`lopdf::IncrementalDocument`) :
   dictionnaire `/Sig` (`/adbe.pkcs7.detached`), `/ByteRange` et `/Contents` réservés à
   largeur fixe puis complétés en place, widget invisible sur la page 1, `/AcroForm
   /SigFlags 3`. Les signatures existantes restent valides. Le fichier produit est revérifié
   avant l'écriture atomique. Table de références reconstruite : réécriture préalable (refus
   si le fichier est déjà signé). Refus : document chiffré, XFA, certifié sans modification
   (`/DocMDP` `P = 1`).
7. **CMS** : pile RustCrypto stable (`cms` 0.2 avec `builder`, `x509-cert`, `rsa` 0.9,
   `p256`, `sha2`), lecture PKCS#12 par `p12-keystore` (qui apporte la génération « der
   0.8 » : deux générations compilées, seuls des octets DER passent de l'une à l'autre ;
   préféré à des préversions). SHA-256, RSA PKCS#1 v1.5 ou ECDSA P-256, attributs signés
   `content-type`, `message-digest`, `signing-time`.
8. **Vérifier ≠ certifier.** Le statut `Intact` dit : l'empreinte correspond, la signature
   est juste avec la clé du certificat joint, `content-type` vaut `id-data`, aucun attribut
   obligatoire n'est en double, la date déclarée tombe dans la validité du certificat, une
   clé RSA fait au moins 2048 bits. Il ne dit **jamais** que l'identité est certifiée :
   aucune chaîne de confiance ni révocation (il faudrait le réseau). L'interface le rappelle.
   Plusieurs signataires dans un même CMS, SHA-1, horodatages de document : « non pris en
   charge ».
9. **Vérification croisée** : pyHanko 0.38 juge nos signatures RSA et ECDSA valides et
   couvrant tout le fichier ; `openssl cms -verify` accepte notre CMS ; nos tests vérifient
   des fichiers signés par pyHanko (pkcs7 et PAdES).

## Conséquences et limites

- La signature est invisible (pas d'image) ; pas d'horodatage ni de LTV (réseau).
- `rsa` 0.9 porte l'avis RUSTSEC-2023-0071 (canal auxiliaire temporel) ; sans portée pratique
  pour une signature faite localement par l'utilisateur.
- Enregistrer un document signé **modifié** invalide ses signatures (bandeau d'avertissement) ;
  la préservation par mise à jour incrémentale de toute édition est hors périmètre.
- L'OCR exige Tesseract et ses fichiers de langue ; la couche se limite à l'alphabet WinAnsi.
