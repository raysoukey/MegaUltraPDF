// SPDX-License-Identifier: GPL-3.0-or-later
//! Protection par mot de passe (AES-256 via `lopdf`) et retrait de la protection.

use mega_core::edit::EditCommand;
use mega_core::security::{Permissions, Protection};
use mega_core::testing::sample_pdf;
use mega_core::{CoreError, Document};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn all() -> Permissions {
    Permissions {
        print: true,
        copy: true,
        modify: true,
        annotate: true,
    }
}

fn protection(user: &str, owner: &str, permissions: Permissions) -> Protection {
    Protection {
        user_password: user.to_string(),
        owner_password: owner.to_string(),
        permissions,
    }
}

#[test]
fn unprotected_document_reports_no_security() {
    let doc = Document::from_bytes(sample_pdf(1, "libre"), None).unwrap();
    let info = doc.security().unwrap();
    assert!(!info.encrypted && !info.user_password_required);
    assert_eq!(info.permissions, all());
}

#[test]
fn protect_then_reopen_requires_password_and_keeps_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("protege.pdf");
    std::fs::write(&path, sample_pdf(2, "secret")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    let before = doc.render_page(0, 0.5).unwrap();
    doc.protect(&protection(
        "secret",
        "patron",
        Permissions {
            print: false,
            copy: false,
            modify: true,
            annotate: true,
        },
    ))
    .unwrap();
    // Le document reste ouvert et navigable.
    assert_eq!(doc.page_count(), 2);
    assert_eq!(doc.render_page(0, 0.5).unwrap().rgba, before.rgba);
    let info = doc.security().unwrap();
    assert!(info.encrypted && info.user_password_required);
    assert!(!info.permissions.print && !info.permissions.copy);
    doc.save().unwrap();
    assert!(matches!(
        Document::open(&path, None),
        Err(CoreError::PasswordRequired)
    ));
    assert!(matches!(
        Document::open(&path, Some("faux")),
        Err(CoreError::WrongPassword)
    ));
    let reopened = Document::open(&path, Some("secret")).unwrap();
    assert_eq!(reopened.page_count(), 2);
    let info = reopened.security().unwrap();
    assert!(info.encrypted && !info.permissions.print && info.permissions.modify);
    assert!(String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains("/AESV3"));
}

#[test]
fn protect_refuses_bad_passwords_and_double_protection() {
    let mut doc = Document::from_bytes(sample_pdf(1, "p"), None).unwrap();
    for (user, owner) in [("", ""), (&"x".repeat(200), "o"), ("🙂", "o")] {
        assert!(
            matches!(
                doc.protect(&protection(user, owner, all())),
                Err(CoreError::InvalidOperation(_))
            ),
            "{user:?}"
        );
    }
    assert!(!doc.security().unwrap().encrypted, "rien n'a changé");
    doc.protect(&protection("", "proprio", all())).unwrap();
    assert!(doc.security().unwrap().encrypted);
    assert!(
        !doc.security().unwrap().user_password_required,
        "mot de passe propriétaire seul : s'ouvre sans mot de passe"
    );
    assert!(matches!(
        doc.protect(&protection("a", "b", all())),
        Err(CoreError::InvalidOperation(_))
    ));
}

#[test]
fn protect_is_undoable() {
    let mut doc = Document::from_bytes(sample_pdf(1, "undo"), None).unwrap();
    doc.apply(&EditCommand::Protect {
        protection: protection("secret", "patron", all()),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Protéger le document"));
    assert!(doc.security().unwrap().encrypted);
    doc.undo().unwrap();
    assert!(!doc.security().unwrap().encrypted);
    doc.redo().unwrap();
    assert!(doc.security().unwrap().encrypted);
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn unprotect_owner_only_needs_the_owner_password() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owner.pdf");
    std::fs::write(&path, fixture("owner-only.pdf")).unwrap();
    // Ouvert sans mot de passe : on ne peut pas prouver qu'on est le propriétaire.
    let mut doc = Document::open(&path, None).unwrap();
    let info = doc.security().unwrap();
    assert!(info.encrypted && !info.user_password_required);
    assert!(matches!(doc.unprotect(), Err(CoreError::WrongPassword)));
    assert!(doc.security().unwrap().encrypted);
    // Ouvert avec le mot de passe propriétaire : la protection tombe.
    let mut doc = Document::open(&path, Some("secret")).unwrap();
    doc.apply(&EditCommand::Unprotect { password: None })
        .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Retirer la protection"));
    assert!(!doc.security().unwrap().encrypted);
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&saved).contains("/Encrypt"));
    assert_eq!(Document::open(&path, None).unwrap().page_count(), 3);
}

#[test]
fn unprotect_user_password_document() {
    let mut doc = Document::from_bytes(fixture("encrypted.pdf"), Some("secret")).unwrap();
    assert!(doc.security().unwrap().user_password_required);
    doc.unprotect().unwrap();
    let info = doc.security().unwrap();
    assert!(!info.encrypted && !info.user_password_required);
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(reopened.page_count(), 3);
    assert!(matches!(
        reopened.security(),
        Ok(i) if !i.encrypted
    ));
    let mut plain = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    assert!(matches!(
        plain.unprotect(),
        Err(CoreError::InvalidOperation(_))
    ));
}

/// Lot 10 : après « Protéger », les passes `lopdf` (filigrane…) rechiffrent avec le même mot
/// de passe au lieu de refuser.
#[test]
fn edits_after_protect_keep_the_protection() {
    let mut doc = Document::from_bytes(sample_pdf(1, "après"), None).unwrap();
    doc.protect(&protection("secret", "patron", all())).unwrap();
    let spec = mega_core::overlay::Watermark {
        text: "X".to_string(),
        size: 40.0,
        opacity: 0.5,
        angle_deg: 0.0,
        color: mega_core::Color { r: 0, g: 0, b: 0 },
        pages: None,
    };
    doc.add_watermark(&spec).unwrap();
    assert!(doc.page_text(0).unwrap().contains('X'));
    doc.rotate_pages(&[0], 1).unwrap();
    let bytes = doc.to_bytes().unwrap();
    assert!(matches!(
        Document::from_bytes(bytes.clone(), None),
        Err(CoreError::PasswordRequired)
    ));
    let reopened = Document::from_bytes(bytes, Some("secret")).unwrap();
    assert!(reopened.page_text(0).unwrap().contains('X'));
}

/// I3 (relecture) : un fichier AES-256 à mot de passe propriétaire seul (`/P -4`, tout
/// permis) était déclaré « tout interdit » : lopdf le déchiffre en silence et retire
/// `/Encrypt`, il faut lire les permissions dans l'état de chiffrement.
#[test]
fn owner_only_permissions_are_read_from_the_encryption_state() {
    let doc = Document::from_bytes(fixture("owner-only.pdf"), None).unwrap();
    let info = doc.security().unwrap();
    assert!(info.encrypted);
    assert_eq!(
        info.permissions,
        all(),
        "owner-only.pdf a /P -4 : tout permis"
    );
    // Fichier produit par nous, propriétaire seul, impression interdite.
    let mut ours = Document::from_bytes(sample_pdf(1, "p"), None).unwrap();
    ours.protect(&protection(
        "",
        "patron",
        Permissions {
            print: false,
            copy: true,
            modify: true,
            annotate: true,
        },
    ))
    .unwrap();
    let reopened = Document::from_bytes(ours.to_bytes().unwrap(), None).unwrap();
    let p = reopened.security().unwrap().permissions;
    assert!(!p.print && p.copy && p.modify && p.annotate, "{p:?}");
}

/// I4 (relecture) : un fichier à mot de passe propriétaire seul s'ouvre sans mot de passe ;
/// l'application doit pouvoir le déprotéger en saisissant le mot de passe propriétaire.
#[test]
fn unprotect_accepts_a_typed_owner_password() {
    let mut doc = Document::from_bytes(fixture("owner-only.pdf"), None).unwrap();
    assert!(matches!(
        doc.apply(&EditCommand::Unprotect {
            password: Some("faux".to_string())
        }),
        Err(CoreError::WrongPassword)
    ));
    assert!(doc.security().unwrap().encrypted && !doc.is_modified());
    doc.apply(&EditCommand::Unprotect {
        password: Some("secret".to_string()),
    })
    .unwrap();
    assert!(!doc.security().unwrap().encrypted);
    let saved = doc.to_bytes().unwrap();
    assert!(!String::from_utf8_lossy(&saved).contains("/Encrypt"));
    doc.undo().unwrap();
    assert!(doc.security().unwrap().encrypted, "annulable");
}

/// Le dictionnaire `/Encrypt` tel qu'écrit dans le fichier (il n'est jamais chiffré lui-même ;
/// lopdf le retire dès qu'il déchiffre, d'où la lecture brute) : `(P, /Length 256 présent)`
/// (le filtre imbriqué `/StdCF` a son propre `/Length 32`).
fn encrypt_entries(bytes: &[u8]) -> (i64, bool) {
    let text = String::from_utf8_lossy(bytes);
    let at = text
        .find("/Filter/Standard")
        .or_else(|| text.find("/Filter /Standard"))
        .expect("dictionnaire /Encrypt");
    let start = text[..at].rfind(" obj").expect("début d'objet");
    let end = at + text[at..].find("endobj").expect("fin d'objet");
    let dict = &text[start..end];
    let number_after = |key: &str| -> Option<i64> {
        let i = dict.find(key)? + key.len();
        let rest = dict[i..].trim_start();
        let n: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        n.parse().ok()
    };
    (
        number_after("/P").expect("/P"),
        dict.contains("/Length 256"),
    )
}

/// B3 + A1 (relecture pdf-expert) : le bit 10 (extraction pour l'accessibilité) reste à 1
/// même sans « copier » (lecteurs d'écran), et `/Length 256` figure dans `/Encrypt`
/// (`qpdf --check` le réclame).
#[test]
fn protect_keeps_accessibility_bit_and_declares_key_length() {
    let mut doc = Document::from_bytes(sample_pdf(1, "a11y"), None).unwrap();
    doc.protect(&protection(
        "secret",
        "patron",
        Permissions {
            print: true,
            copy: false,
            modify: false,
            annotate: true,
        },
    ))
    .unwrap();
    let bytes = doc.to_bytes().unwrap();
    let (p, length) = encrypt_entries(&bytes);
    let p = p as u32;
    assert_ne!(p & (1 << 9), 0, "bit 10 (accessibilité) : /P = {p:#x}");
    assert_eq!(p & (1 << 4), 0, "bit 5 (copie) refusé");
    assert_eq!(p & (1 << 3), 0, "bit 4 (modification) refusé");
    assert!(length, "/Length 256 dans /Encrypt");
}

/// B1 (relecture pdf-expert) : protéger chiffrait des octets PDFium non nettoyés ; la passe
/// d'enregistrement (`/InkList`, clé privée) doit s'appliquer avant le chiffrement.
#[test]
fn protected_file_keeps_the_save_cleanup() {
    use mega_core::{AnnotationKind, Color, Geometry, NewAnnotation, Style};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("propre.pdf");
    std::fs::write(&path, sample_pdf(1, "propre")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Ink,
            geometry: Geometry::Strokes {
                strokes: vec![vec![[100.0, 100.0], [150.0, 160.0], [200.0, 120.0]]],
            },
            style: Style {
                color: Color { r: 200, g: 0, b: 0 },
                fill: None,
                opacity: 1.0,
                stroke_width: 2.0,
                font_size: 12.0,
            },
            contents: String::new(),
            author: String::new(),
        },
    )
    .unwrap();
    doc.render_page(0, 0.5).unwrap();
    doc.protect(&protection("secret", "patron", all())).unwrap();
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    let lo =
        lopdf::Document::load_mem_with_options(&saved, lopdf::LoadOptions::with_password("secret"))
            .unwrap();
    let mut out = Vec::new();
    let mut lo = lo;
    lo.save_to(&mut out).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("/InkList"), "/InkList perdue au chiffrement");
    assert!(
        !text.contains("PDFIUM_HasGeneratedAP"),
        "clé privée chiffrée avec le reste"
    );
}

/// A3 (relecture pdf-expert) : la révision 6 applique SASLprep au mot de passe (lopdf), pas
/// PDFium : un caractère que SASLprep transforme rendrait le fichier impossible à rouvrir.
#[test]
fn protect_refuses_passwords_that_saslprep_would_change() {
    let mut doc = Document::from_bytes(sample_pdf(1, "sasl"), None).unwrap();
    for bad in [
        "a\u{00A0}b",
        "x\u{00B2}",
        "a\tb",
        "a\u{00AD}b",
        "\u{0085}x",
        "\u{00B5}m",
    ] {
        match doc.protect(&protection(bad, "patron", all())) {
            Err(CoreError::InvalidOperation(_)) => {}
            other => panic!("{bad:?} : {other:?}"),
        }
    }
    // Les lettres accentuées du français restent permises.
    doc.protect(&protection("élève été", "patron", all()))
        .unwrap();
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), Some("élève été")).unwrap();
    assert_eq!(reopened.page_count(), 1);
}

/// Audit E8 : l'AES-256 R6 n'existe pas en PDF 1.7 sans l'extension d'Adobe (niveau 8) ;
/// l'en-tête doit annoncer au moins 1.7 et le catalogue déclarer `/Extensions`.
#[test]
fn protect_declares_the_adobe_extension_level_and_a_1_7_header() {
    let protect_and_read = |version: &str| -> (Vec<u8>, lopdf::Document) {
        let mut lo = lopdf::Document::load_mem(&sample_pdf(1, "ext")).unwrap();
        lo.version = version.to_string();
        let mut input = Vec::new();
        lo.save_to(&mut input).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("e.pdf");
        let mut doc = Document::from_bytes(input, None).unwrap();
        doc.protect(&protection("u", "o", all())).unwrap();
        doc.save_as(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let options = lopdf::LoadOptions {
            password: Some("u".to_string()),
            ..Default::default()
        };
        let out = lopdf::Document::load_mem_with_options(&bytes, options).unwrap();
        (bytes, out)
    };
    let (bytes, out) = protect_and_read("1.4");
    assert!(bytes.starts_with(b"%PDF-1.7"), "{:?}", &bytes[..8]);
    let catalog = out.catalog().unwrap();
    let adbe = catalog
        .get(b"Extensions")
        .and_then(|e| e.as_dict())
        .and_then(|e| e.get(b"ADBE"))
        .and_then(|a| a.as_dict())
        .unwrap();
    assert_eq!(adbe.get(b"ExtensionLevel").unwrap().as_i64().unwrap(), 8);
    assert_eq!(adbe.get(b"BaseVersion").unwrap().as_name().unwrap(), b"1.7");
    let (bytes, _) = protect_and_read("2.0");
    assert!(
        bytes.starts_with(b"%PDF-2.0"),
        "une version plus haute reste"
    );
}

/// Le paquet XMP d'un document lu dans `bytes` (déchiffré avec `password`), en texte.
fn xmp_text(bytes: &[u8], password: &str) -> String {
    let options = lopdf::LoadOptions {
        password: Some(password.to_string()),
        ..lopdf::LoadOptions::default()
    };
    let doc = lopdf::Document::load_mem_with_options(bytes, options).unwrap();
    let id = doc
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = doc.get_object(id).unwrap().as_stream().unwrap();
    String::from_utf8_lossy(
        &stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()),
    )
    .into_owned()
}

/// Tâche 21 du Lot 14 (constat de la relecture) : PDFium écrit le flux `/Metadata` **en clair**
/// même quand `/EncryptMetadata` vaut `true`, mais le déchiffre à la relecture. Chaque
/// enregistrement (PDFium relit sa propre sortie) changeait alors le XMP d'un document que nous
/// venions de protéger en octets aléatoires. Depuis la tâche 21b, `Document::to_bytes` rechiffre
/// ce flux : le XMP survit à la protection, à l'enregistrement et à une édition suivie d'un
/// second enregistrement.
#[test]
fn protecting_keeps_the_xmp_readable_through_saves() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("protege.pdf");
    let mut doc =
        Document::from_bytes(mega_core::testing::rich_catalog_pdf("1.7", None), None).unwrap();
    doc.protect(&protection("u", "o", all())).unwrap();
    doc.save_as(&path).unwrap();
    let first = std::fs::read(&path).unwrap();
    let xmp = xmp_text(&first, "u");
    assert!(
        xmp.starts_with("<?xpacket") && xmp.contains("</x:xmpmeta>"),
        "XMP illisible après protection : {:?}",
        xmp.chars().take(60).collect::<String>()
    );
    doc.apply(&EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    doc.save_as(&path).unwrap();
    let xmp = xmp_text(&std::fs::read(&path).unwrap(), "u");
    assert!(
        xmp.starts_with("<?xpacket") && xmp.contains("</x:xmpmeta>"),
        "XMP illisible après un second enregistrement : {:?}",
        xmp.chars().take(60).collect::<String>()
    );
}

/// Un `protect` refusé (document déjà protégé, mots de passe vides) ne marque pas le document
/// modifié (CLAUDE.md : `mark_changed` après les gardes de refus) : enregistrer recopie alors
/// les octets d'origine tels quels.
#[test]
fn refused_protect_leaves_the_document_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let source = fixture("encrypted.pdf");
    let mut doc = Document::from_bytes(source.clone(), Some("secret")).unwrap();
    assert!(matches!(
        doc.protect(&protection("a", "b", all())),
        Err(CoreError::InvalidOperation(_))
    ));
    let path = dir.path().join("deja.pdf");
    doc.save_as(&path).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        source,
        "déjà protégé : intact"
    );

    let plain = sample_pdf(1, "intact");
    let mut doc = Document::from_bytes(plain.clone(), None).unwrap();
    assert!(matches!(
        doc.protect(&protection("", "", all())),
        Err(CoreError::InvalidOperation(_))
    ));
    let path = dir.path().join("vide.pdf");
    doc.save_as(&path).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        plain,
        "mots de passe vides : intact"
    );
}
