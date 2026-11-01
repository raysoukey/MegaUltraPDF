// SPDX-License-Identifier: GPL-3.0-or-later
//! Enregistrer là où c'est difficile (Lot 13, audit P4 et P6) : fichier en lecture seule,
//! verrouillé par une autre application, dossier interdit en écriture, liens physiques,
//! chemins accentués, avec espaces ou très longs. L'utilisateur reçoit une erreur qui dit quoi
//! faire, et ses fichiers ne sont jamais abîmés.

use mega_core::testing::sample_pdf;
use mega_core::{CoreError, Document};

fn modified_doc() -> Document {
    let mut doc = Document::from_bytes(sample_pdf(2, "enregistrement"), None).unwrap();
    doc.apply(&mega_core::edit::EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    doc
}

/// Un fichier en lecture seule n'est pas remplacé : l'erreur le dit et propose
/// « Enregistrer sous ». (Sous Unix, le renommage passerait outre : on vérifie avant.)
#[test]
fn a_read_only_file_is_refused_with_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lecture-seule.pdf");
    std::fs::write(&path, b"original").unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions.clone()).unwrap();
    let result = modified_doc().save_as(&path);
    assert!(matches!(result, Err(CoreError::FileReadOnly)), "{result:?}");
    assert!(result.unwrap_err().to_string().contains("Enregistrer sous"));
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).unwrap();
}

/// Windows : un fichier ouvert sans partage par une autre application (Acrobat, Word…)
/// donne « fichier utilisé par une autre application ».
#[cfg(windows)]
#[test]
fn a_file_locked_by_another_application_is_reported() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("verrouille.pdf");
    std::fs::write(&path, b"original").unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    let result = modified_doc().save_as(&path);
    drop(lock);
    assert!(matches!(result, Err(CoreError::FileLocked)), "{result:?}");
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
}

/// Unix : un dossier où l'on ne peut pas écrire donne une erreur qui le dit.
#[cfg(unix)]
#[test]
fn a_folder_without_write_permission_is_reported() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("protege");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    // Le superutilisateur passe outre les permissions : le test n'a alors pas d'objet.
    let probe = locked.join("essai");
    if std::fs::write(&probe, b"x").is_ok() {
        let _ = std::fs::remove_file(probe);
        return;
    }
    let result = modified_doc().save_as(locked.join("nouveau.pdf"));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(result, Err(CoreError::FolderNotWritable)),
        "{result:?}"
    );
}

/// Unix : un fichier qui a plusieurs noms (liens physiques) reste un seul fichier après
/// l'enregistrement : les autres noms voient la nouvelle version.
#[cfg(unix)]
#[test]
fn hard_links_keep_pointing_to_the_saved_file() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    std::fs::write(&a, sample_pdf(1, "lien")).unwrap();
    std::fs::hard_link(&a, &b).unwrap();
    let mut doc = Document::open(&a, None).unwrap();
    doc.apply(&mega_core::edit::EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    doc.save_as(&a).unwrap();
    assert_eq!(std::fs::metadata(&a).unwrap().nlink(), 2);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    // Aucune copie de sécurité ne reste dans le dossier.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}

/// P6 : chemins accentués, avec espaces, et (Windows) de plus de 260 caractères.
#[test]
fn unusual_paths_open_and_save() {
    let dir = tempfile::tempdir().unwrap();
    let mut deep = dir.path().join("Dossier accentué é à ü — 日本");
    if cfg!(windows) {
        while deep.as_os_str().len() < 280 {
            deep = deep.join("un sous-dossier assez long pour dépasser MAX_PATH");
        }
    }
    std::fs::create_dir_all(&deep).unwrap();
    let path = deep.join("Fichier d'essai été.pdf");
    std::fs::write(&path, sample_pdf(1, "chemin")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.apply(&mega_core::edit::EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    doc.save().unwrap();
    assert!(Document::open(&path, None).is_ok());
}

/// P6 (Linux) : un nom de fichier qui n'est pas de l'UTF-8 valide s'ouvre et s'enregistre à
/// son propre nom (le chemin reste un `PathBuf`, jamais une chaîne reconstruite).
#[cfg(target_os = "linux")]
#[test]
fn a_non_utf8_file_name_is_saved_in_place() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join(std::ffi::OsStr::from_bytes(b"latin1-\xe9t\xe9.pdf"));
    std::fs::write(&path, sample_pdf(1, "octets")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.apply(&mega_core::edit::EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    doc.save().unwrap();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(Document::open(&path, None).is_ok());
}

// --- Fidélité de l'enregistrement (Lot 14 : expériences E4 et E5 du banc de l'audit) --------

/// Un fichier de `fixtures/`.
fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// Lit `bytes` avec `lopdf` (avec le mot de passe s'il y en a un).
fn read_lopdf(bytes: &[u8], password: Option<&str>) -> lopdf::Document {
    lopdf::Document::load_mem_with_options(
        bytes,
        lopdf::LoadOptions {
            password: password.map(str::to_string),
            ..lopdf::LoadOptions::default()
        },
    )
    .unwrap()
}

/// Les deux identifiants de `/ID` dans la remorque.
fn trailer_id(bytes: &[u8], password: Option<&str>) -> [Vec<u8>; 2] {
    let doc = read_lopdf(bytes, password);
    let ids = doc.trailer.get(b"ID").unwrap().as_array().unwrap();
    assert_eq!(ids.len(), 2, "{ids:?}");
    [0, 1].map(|i| ids[i].as_str().unwrap().to_vec())
}

/// Applique `commands` (JSON, comme l'interface), enregistre et rend les octets écrits.
fn edited_and_saved(
    source: &str,
    password: Option<&str>,
    commands: &[serde_json::Value],
) -> Vec<u8> {
    let mut doc = Document::open(fixture(source), password).unwrap();
    for json in commands {
        let command: mega_core::edit::EditCommand = serde_json::from_value(json.clone()).unwrap();
        doc.apply(&command).unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sortie.pdf");
    doc.save_as(&path).unwrap();
    std::fs::read(&path).unwrap()
}

/// Audit E12 (expérience E4) : enregistrer une modification d'un fichier existant garde le
/// **premier** identifiant de `/ID` (l'identité du document) et renouvelle le **second** (la
/// version), comme le veut ISO 32000-1 §14.4 ; avec ou sans chiffrement.
#[test]
fn an_edited_file_keeps_its_first_id_and_renews_the_second() {
    let rotate = serde_json::json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1});
    for (source, password) in [("simple.pdf", None), ("encrypted.pdf", Some("secret"))] {
        let input = std::fs::read(fixture(source)).unwrap();
        let before = trailer_id(&input, password);
        let out = edited_and_saved(source, password, std::slice::from_ref(&rotate));
        let after = trailer_id(&out, password);
        assert_eq!(after[0], before[0], "{source} : premier identifiant gardé");
        assert_ne!(
            after[1], before[1],
            "{source} : second identifiant renouvelé"
        );
    }
}

/// Flux **sans** `/Filter` d'un fichier, hors `/Metadata` (un XMP se laisse en clair pour être
/// lisible sans décodeur, ISO 32000-1 §14.3.2) : leur numéro d'objet.
fn unfiltered_streams(bytes: &[u8]) -> Vec<lopdf::ObjectId> {
    let doc = read_lopdf(bytes, None);
    doc.objects
        .iter()
        .filter(|(_, object)| {
            object.as_stream().is_ok_and(|s| {
                let metadata = s.dict.get(b"Type").and_then(lopdf::Object::as_name).ok()
                    == Some(b"Metadata".as_slice());
                !metadata && !s.dict.has(b"Filter")
            })
        })
        .map(|(id, _)| *id)
        .collect()
}

/// Audit E13 (expérience E5) : les flux qu'une passe `lopdf` écrit en clair (filigrane,
/// couche OCR, apparence d'un tracé) sont **recompressés** par PDFium au rechargement ; le
/// fichier enregistré n'en garde aucun sans `/Filter`. Sinon chaque édition gonflerait le
/// fichier.
#[test]
fn streams_written_by_edits_are_compressed_in_the_saved_file() {
    let watermark = serde_json::json!({"kind": "watermark", "spec": {"text": "CONFIDENTIEL Ω",
        "size": 60.0, "opacity": 0.3, "angle_deg": 45.0, "color": {"r": 200, "g": 0, "b": 0},
        "pages": null}});
    let ink = serde_json::json!({"kind": "add_annotation", "page": 0, "annotation": {
        "kind": "ink",
        "geometry": {"kind": "strokes", "strokes": [[[100.0, 100.0], [150.0, 160.0], [200.0, 120.0]]]},
        "style": {"color": {"r": 0, "g": 0, "b": 200}, "fill": null, "opacity": 1.0,
                  "stroke_width": 2.0, "font_size": 12.0},
        "contents": "", "author": "Moi"}});
    for (what, command) in [("filigrane", watermark), ("tracé à main levée", ink)] {
        let out = edited_and_saved("simple.pdf", None, &[command]);
        assert_eq!(unfiltered_streams(&out), [], "{what} : flux sans /Filter");
    }
    // La couche OCR s'ajoute directement (la commande d'édition lancerait Tesseract) ; la
    // page numérisée a, en entrée, des flux sans `/Filter`, qui ne doivent pas rester.
    let input = std::fs::read(fixture("scanned.pdf")).unwrap();
    assert!(
        !unfiltered_streams(&input).is_empty(),
        "entrée sans compression"
    );
    let mut doc = Document::from_bytes(input, None).unwrap();
    let word = |text: &str, x: f32| mega_core::overlay::InvisibleWord {
        text: text.to_string(),
        rect: mega_core::Rect {
            x,
            y: 740.0,
            w: 60.0,
            h: 24.0,
        },
    };
    doc.add_invisible_words(0, &[word("Page", 72.0), word("Ωμέγα", 140.0)])
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ocr.pdf");
    doc.save_as(&path).unwrap();
    let out = std::fs::read(&path).unwrap();
    assert_eq!(
        unfiltered_streams(&out),
        [],
        "couche OCR : flux sans /Filter"
    );
}
