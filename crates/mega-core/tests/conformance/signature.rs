// SPDX-License-Identifier: GPL-3.0-or-later
//! Signatures numériques (mise à jour incrémentale), visibles ou non.

use mega_core::Document;
use mega_core::edit::EditCommand;

use crate::{assert_conforms, fixture, qpdf};

/// Signature numérique (mise à jour incrémentale) sur un fichier intact et sur un fichier
/// modifié.
#[test]
fn a_digital_signature_produces_a_conforming_file() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    let p12 = std::fs::read(fixture("test-signer.p12")).unwrap();
    let identity =
        mega_core::digital_signature::SigningIdentity::from_pkcs12(&p12, "test").unwrap();
    let options = mega_core::digital_signature::SignOptions {
        reason: Some("Contrôle".to_string()),
        location: None,
        contact: None,
        appearance: None,
    };
    for (what, modify) in [
        ("signature d'un fichier intact", false),
        ("signature d'un fichier modifié", true),
    ] {
        let mut doc = Document::open(fixture("simple.pdf"), None).unwrap();
        if modify {
            let rotate: EditCommand = serde_json::from_value(
                serde_json::json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1}),
            )
            .unwrap();
            doc.apply(&rotate).unwrap();
        }
        let out = dir.path().join(format!("signe-{modify}.pdf"));
        doc.sign_to(&out, &identity, &options).unwrap();
        assert_conforms(&qpdf, &out, None, what);
    }
}

/// C2 (audit du Lot 13) : signer un fichier à structure inhabituelle (références hybrides,
/// révisions successives, linéarisé, table réparée) donne une signature **intacte** à la
/// relecture et un fichier que qpdf accepte.
#[test]
fn signing_unusual_structures_gives_intact_conforming_signatures() {
    let qpdf = qpdf();
    let dir = tempfile::tempdir().unwrap();
    let p12 = std::fs::read(fixture("test-signer.p12")).unwrap();
    let identity =
        mega_core::digital_signature::SigningIdentity::from_pkcs12(&p12, "test").unwrap();
    let options = mega_core::digital_signature::SignOptions {
        reason: None,
        location: None,
        contact: None,
        appearance: None,
    };
    for name in [
        "robustesse/xref-hybride.pdf",
        "robustesse/revisions.pdf",
        "robustesse/linearise.pdf",
        "robustesse/xref-decalee.pdf",
        "robustesse/sans-endobj.pdf",
        "robustesse/prefixe-parasite.pdf",
        "robustesse/pdf-2-0.pdf",
    ] {
        let mut doc = Document::open(fixture(name), None).unwrap();
        let text_before = doc.page_text(0).unwrap();
        let out = dir.path().join("signe.pdf");
        doc.sign_to(&out, &identity, &options)
            .unwrap_or_else(|e| panic!("{name} : signature refusée : {e}"));
        let signed = Document::open(&out, None).unwrap();
        let list = signed.signatures().unwrap();
        assert_eq!(list.len(), 1, "{name} : {list:?}");
        assert!(
            matches!(
                list[0].status,
                mega_core::digital_signature::SignatureStatus::Intact
            ),
            "{name} : {:?}",
            list[0].status
        );
        assert_eq!(
            signed.page_text(0).unwrap(),
            text_before,
            "{name} : contenu changé"
        );
        if let Some(qpdf) = &qpdf {
            assert_conforms(qpdf, &out, None, name);
        }
    }
}

/// Signature **visible** (widget avec apparence, mise à jour incrémentale) : tampon de texte
/// seul, tampon avec image, et tampon sur une page tournée (rotation portée par la `/Matrix`
/// de l'apparence). Chaque sortie est conforme et sa signature reste intacte.
#[test]
fn a_visible_signature_produces_a_conforming_file() {
    use mega_core::digital_signature::{SignAppearance, SignOptions, SignatureStatus};
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    let p12 = std::fs::read(fixture("test-signer.p12")).unwrap();
    let identity =
        mega_core::digital_signature::SigningIdentity::from_pkcs12(&p12, "test").unwrap();
    let image = dir.path().join("paraphe.png");
    image::RgbImage::from_pixel(60, 30, image::Rgb([20, 20, 120]))
        .save(&image)
        .unwrap();
    let image = image.to_string_lossy().into_owned();
    for (what, source, image_path) in [
        ("signature visible", "simple.pdf", None),
        (
            "signature visible avec image",
            "simple.pdf",
            Some(image.clone()),
        ),
        ("signature visible sur page tournée", "rotated.pdf", None),
    ] {
        let mut doc = Document::open(fixture(source), None).unwrap();
        let options = SignOptions {
            reason: Some("Contrôle".to_string()),
            appearance: Some(SignAppearance {
                page: 0,
                rect: mega_core::Rect {
                    x: 300.0,
                    y: 100.0,
                    w: 200.0,
                    h: 60.0,
                },
                image_path,
            }),
            ..SignOptions::default()
        };
        let out = dir.path().join("visible.pdf");
        doc.sign_to(&out, &identity, &options)
            .unwrap_or_else(|e| panic!("{what} : signature refusée : {e}"));
        assert_conforms(&qpdf, &out, None, what);
        let list = Document::open(&out, None).unwrap().signatures().unwrap();
        assert!(
            list.len() == 1 && matches!(list[0].status, SignatureStatus::Intact),
            "{what} : {list:?}"
        );
    }
}
