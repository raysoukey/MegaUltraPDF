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
