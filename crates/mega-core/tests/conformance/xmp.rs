// SPDX-License-Identifier: GPL-3.0-or-later
//! XMP compressé (constat F21-5, tâche 21b du Lot 14) : PDFium retire le `/Filter` d'un flux
//! de métadonnées compressé en gardant ses octets compressés ; le document lui est confié avec
//! ces flux décodés, à l'ouverture et à chaque rechargement depuis des octets de fichier
//! (après une signature, notamment). Ces cas passent aussi par qpdf.

use std::path::Path;
use std::process::Command;

use mega_core::Document;
use mega_core::edit::EditCommand;

use crate::{TOOL_LIMIT, assert_conforms, fixture, output_text, qpdf, run_with_timeout};

/// Une page, sans titre dans `/Info`, et un XMP **compressé** dont le titre est « Compressé ».
fn compressed_xmp_pdf() -> Vec<u8> {
    let xmp = "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
        <x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
        xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
        <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
        <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Compressé</rdf:li></rdf:Alt>\
        </dc:title></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>";
    let mut doc = lopdf::Document::load_mem(&mega_core::testing::sample_pdf(2, "xmp")).unwrap();
    let info = doc.trailer.get(b"Info").unwrap().as_reference().unwrap();
    doc.get_dictionary_mut(info).unwrap().remove(b"Title");
    let mut dict = lopdf::Dictionary::new();
    dict.set("Type", lopdf::Object::Name(b"Metadata".to_vec()));
    dict.set("Subtype", lopdf::Object::Name(b"XML".to_vec()));
    let id = doc.add_object(lopdf::Stream::new(dict, xmp.as_bytes().to_vec()));
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    doc.get_dictionary_mut(root)
        .unwrap()
        .set("Metadata", lopdf::Object::Reference(id));
    doc.compress();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

fn rotate(doc: &mut Document) {
    doc.apply(&EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
}

fn assert_title(doc: &Document, what: &str) {
    assert_eq!(
        doc.properties().unwrap().title.as_deref(),
        Some("Compressé"),
        "{what} : titre du XMP"
    );
}

/// Relecture de la tâche 21b : signer recharge le document depuis les octets signés ; le
/// rechargement passe par la même porte que l'ouverture, et le XMP compressé survit à
/// l'édition qui suit.
#[test]
fn a_signed_document_with_a_compressed_xmp_keeps_it_through_an_edit() {
    let Some(qpdf) = qpdf() else { return };
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
    let mut doc = Document::from_bytes(compressed_xmp_pdf(), None).unwrap();
    let signed = dir.path().join("signe.pdf");
    doc.sign_to(&signed, &identity, &options).unwrap();
    rotate(&mut doc);
    assert_title(&doc, "signé puis tourné");
    let out = dir.path().join("signe-tourne.pdf");
    doc.save_as(&out).unwrap();
    assert_conforms(&qpdf, &out, None, "signé puis tourné");
    assert_title(&Document::open(&out, None).unwrap(), "relu");
}

/// Un fichier non chiffré à flux d'objets et table de références en flux (qpdf
/// `--object-streams=generate`) dont le XMP est compressé : lu strictement par lopdf, il est
/// décodé à l'ouverture ; le titre survit à une édition et la sortie passe qpdf.
#[test]
fn an_object_stream_file_with_a_compressed_xmp_keeps_it_through_an_edit() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.pdf");
    std::fs::write(&source, compressed_xmp_pdf()).unwrap();
    let objstm = dir.path().join("objstm.pdf");
    let mut cmd = Command::new(&qpdf);
    cmd.arg("--object-streams=generate")
        .arg(&source)
        .arg(&objstm);
    let out = run_with_timeout(&mut cmd, TOOL_LIMIT).expect("qpdf injoignable ou bloqué");
    assert!(out.status.success(), "{}", output_text(&out));
    let raw = std::fs::read(&objstm).unwrap();
    assert!(raw.windows(7).any(|w| w == b"/ObjStm"), "flux d'objets");
    let mut doc = Document::open(&objstm, None).unwrap();
    rotate(&mut doc);
    assert_title(&doc, "flux d'objets, tourné");
    let saved = dir.path().join("objstm-tourne.pdf");
    doc.save_as(&saved).unwrap();
    assert_conforms(&qpdf, &saved, None, "flux d'objets, tourné");
    assert_title(&Document::open(&saved, None).unwrap(), "relu");
    assert_unfiltered_xmp(&saved);
}

/// Le XMP du catalogue de `path` est écrit sans `/Filter`, en clair.
fn assert_unfiltered_xmp(path: &Path) {
    let doc = lopdf::Document::load_mem(&std::fs::read(path).unwrap()).unwrap();
    let id = doc
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = doc.get_object(id).unwrap().as_stream().unwrap();
    assert!(
        !stream.dict.has(b"Filter") && stream.content.windows(8).any(|w| w == b"Compress"),
        "XMP écrit décodé"
    );
}
