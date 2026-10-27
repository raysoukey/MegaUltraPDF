// SPDX-License-Identifier: GPL-3.0-or-later
//! Identités PKCS#12 et signatures CMS détachées (feature `signature`, Lot 8).
#![cfg(feature = "signature")]

use mega_core::CoreError;
use mega_core::digital_signature::{SignatureStatus, SigningIdentity, cms, sha256};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
const WHEN: i64 = 1_800_000_000; // janvier 2027 : dans la validité des certificats de test

fn p12(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}{name}")).unwrap()
}

fn rsa() -> SigningIdentity {
    SigningIdentity::from_pkcs12(&p12("test-signer.p12"), "test").unwrap()
}

fn ec() -> SigningIdentity {
    SigningIdentity::from_pkcs12(&p12("test-signer-ec.p12"), "test").unwrap()
}

#[test]
fn pkcs12_rsa_and_ec_identities_load() {
    assert_eq!(rsa().signer_name(), "MegaUltraPDF Test Signer");
    assert_eq!(ec().signer_name(), "MegaUltraPDF Test Signer EC");
}

#[test]
fn wrong_pkcs12_password_is_wrong_password() {
    let err = SigningIdentity::from_pkcs12(&p12("test-signer.p12"), "faux").unwrap_err();
    assert!(matches!(err, CoreError::WrongPassword), "{err:?}");
}

#[test]
fn garbage_is_not_a_certificate() {
    let err = SigningIdentity::from_pkcs12(b"ceci n'est pas un PKCS#12", "test").unwrap_err();
    assert!(
        matches!(err, CoreError::InvalidOperation(ref m) if m.contains("certificat")),
        "{err:?}"
    );
}

#[test]
fn cms_round_trip_verifies() {
    let digest = sha256(b"le contenu signe");
    for identity in [rsa(), ec()] {
        let der = cms::sign_digest(&identity, &digest, WHEN).unwrap();
        let check = cms::verify(&der, &digest);
        assert_eq!(
            check.status,
            SignatureStatus::Intact,
            "{}",
            identity.signer_name()
        );
        assert_eq!(
            check.signer.as_deref(),
            Some(identity.signer_name().as_str())
        );
        assert_eq!(check.signed_at, Some(WHEN));
    }
}

#[test]
fn cms_with_another_digest_is_invalid() {
    let der = cms::sign_digest(&rsa(), &sha256(b"original"), WHEN).unwrap();
    let check = cms::verify(&der, &sha256(b"modifie"));
    assert!(
        matches!(check.status, SignatureStatus::Invalid(_)),
        "{check:?}"
    );
}

#[test]
fn cms_followed_by_zero_padding_still_verifies() {
    // Dans un PDF, `/Contents` est plus grand que la signature et complété de zéros.
    let digest = sha256(b"rembourrage");
    let mut der = cms::sign_digest(&ec(), &digest, WHEN).unwrap();
    der.extend_from_slice(&[0u8; 300]);
    assert_eq!(cms::verify(&der, &digest).status, SignatureStatus::Intact);
}

#[test]
fn cms_with_a_damaged_signature_is_invalid() {
    let digest = sha256(b"abime");
    let mut der = cms::sign_digest(&rsa(), &digest, WHEN).unwrap();
    // Les derniers octets sont la valeur de signature RSA.
    let last = der.len() - 1;
    der[last] ^= 0x01;
    assert!(matches!(
        cms::verify(&der, &digest).status,
        SignatureStatus::Invalid(_)
    ));
}

#[test]
fn signing_time_outside_the_certificate_validity_is_invalid() {
    // Le certificat de test vaut dix ans à partir de sa création : 2099 est hors période.
    let digest = sha256(b"date");
    let der = cms::sign_digest(&rsa(), &digest, 4_070_908_800).unwrap();
    let check = cms::verify(&der, &digest);
    assert!(
        check.status
            == SignatureStatus::Invalid(
                mega_core::digital_signature::SigIssue::SigningTimeOutsideValidity
            ),
        "{check:?}"
    );
}

#[test]
fn rsa_keys_shorter_than_2048_bits_are_not_accepted() {
    let weak = SigningIdentity::from_pkcs12(&p12("test-signer-weak.p12"), "test").unwrap();
    let digest = sha256(b"faible");
    let der = cms::sign_digest(&weak, &digest, WHEN).unwrap();
    assert!(matches!(
        cms::verify(&der, &digest).status,
        SignatureStatus::Unsupported(_)
    ));
}

#[test]
fn not_a_cms_is_unsupported_or_invalid_without_panicking() {
    let check = cms::verify(b"\x30\x03\x02\x01\x00", &sha256(b"x"));
    assert!(!matches!(check.status, SignatureStatus::Intact));
}

/// Vérification croisée manuelle par OpenSSL : `MEGA_CMS_SAMPLES=<dossier> cargo test -p
/// mega-core --test digital_signature -- --ignored`, puis
/// `openssl cms -verify -binary -inform DER -in rsa.der -content data.bin -noverify`.
#[test]
#[ignore]
fn write_samples_for_openssl() {
    let dir = std::path::PathBuf::from(std::env::var("MEGA_CMS_SAMPLES").unwrap());
    let data = b"contenu signe pour openssl";
    std::fs::write(dir.join("data.bin"), data).unwrap();
    let digest = sha256(data);
    for (name, identity) in [("rsa", rsa()), ("ec", ec())] {
        let der = cms::sign_digest(&identity, &digest, WHEN).unwrap();
        std::fs::write(dir.join(format!("{name}.der")), der).unwrap();
    }
}

// --- Signer et vérifier un PDF (mise à jour incrémentale) -------------------------------

use mega_core::Document;
use mega_core::digital_signature::{SignOptions, SignatureInfo, signatures};
use mega_core::edit::EditCommand;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}{name}")).unwrap()
}

fn options() -> SignOptions {
    SignOptions {
        reason: Some("Approbation du contrat".to_string()),
        location: Some("Québec".to_string()),
        contact: None,
    }
}

/// Signe `bytes` dans un fichier temporaire et retourne les octets signés.
fn sign(bytes: Vec<u8>, identity: &SigningIdentity) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("signe.pdf");
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    doc.sign_to(&path, identity, &options()).unwrap();
    std::fs::read(&path).unwrap()
}

fn only(list: Vec<SignatureInfo>) -> SignatureInfo {
    assert_eq!(list.len(), 1, "{list:?}");
    list.into_iter().next().unwrap()
}

#[test]
fn signed_file_verifies_and_covers_the_whole_document() {
    for identity in [rsa(), ec()] {
        let signed = sign(fixture("simple.pdf"), &identity);
        let info = only(signatures(&signed).unwrap());
        assert_eq!(info.status, SignatureStatus::Intact, "{info:?}");
        assert!(info.covers_whole_document);
        assert_eq!(
            info.signer.as_deref(),
            Some(identity.signer_name().as_str())
        );
        assert_eq!(info.reason.as_deref(), Some("Approbation du contrat"));
        assert_eq!(info.location.as_deref(), Some("Québec"));
        assert_eq!(info.sub_filter, "adbe.pkcs7.detached");
        assert!(info.signed_at.is_some());
        // PDFium relit le fichier signé.
        assert_eq!(Document::from_bytes(signed, None).unwrap().page_count(), 3);
    }
}

#[test]
fn original_bytes_are_a_prefix_of_the_signed_file() {
    let original = fixture("simple.pdf");
    let signed = sign(original.clone(), &rsa());
    assert!(signed.starts_with(&original), "mise à jour incrémentale");
}

#[test]
fn flipping_a_byte_invalidates() {
    let mut signed = sign(fixture("simple.pdf"), &rsa());
    // Un octet du contenu d'origine (dans la première plage signée).
    let i = signed.len() / 3;
    signed[i] ^= 0x20;
    let info = only(signatures(&signed).unwrap());
    assert!(
        matches!(info.status, SignatureStatus::Invalid(_)),
        "{info:?}"
    );
}

#[test]
fn second_signature_keeps_the_first_valid() {
    let once = sign(fixture("simple.pdf"), &rsa());
    let twice = sign(once, &ec());
    let list = signatures(&twice).unwrap();
    assert_eq!(list.len(), 2, "{list:?}");
    assert!(
        list.iter().all(|s| s.status == SignatureStatus::Intact),
        "{list:?}"
    );
    let first = list
        .iter()
        .find(|s| s.signer.as_deref() == Some("MegaUltraPDF Test Signer"))
        .unwrap();
    let second = list
        .iter()
        .find(|s| s.signer.as_deref() == Some("MegaUltraPDF Test Signer EC"))
        .unwrap();
    assert!(!first.covers_whole_document, "révision antérieure");
    assert!(second.covers_whole_document);
    assert_ne!(first.field, second.field, "noms de champ uniques");
}

#[test]
fn untouched_save_keeps_the_signature() {
    let signed = sign(fixture("simple.pdf"), &rsa());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copie.pdf");
    let mut doc = Document::from_bytes(signed, None).unwrap();
    doc.save_as(&path).unwrap();
    let info = only(signatures(&std::fs::read(&path).unwrap()).unwrap());
    assert_eq!(info.status, SignatureStatus::Intact);
}

#[test]
fn document_signatures_read_the_file_even_after_an_edit() {
    let signed = sign(fixture("simple.pdf"), &rsa());
    let mut doc = Document::from_bytes(signed, None).unwrap();
    doc.apply(&EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    assert_eq!(
        only(doc.signatures().unwrap()).status,
        SignatureStatus::Intact
    );
}

#[test]
fn edited_and_saved_signed_file_is_reported_invalid() {
    let signed = sign(fixture("simple.pdf"), &rsa());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("modifie.pdf");
    let mut doc = Document::from_bytes(signed, None).unwrap();
    doc.apply(&EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    doc.save_as(&path).unwrap();
    let list = signatures(&std::fs::read(&path).unwrap()).unwrap();
    assert!(
        !list.is_empty() && list.iter().all(|s| s.status != SignatureStatus::Intact),
        "{list:?}"
    );
}

#[test]
fn signing_keeps_existing_form_fields() {
    let before = Document::from_bytes(fixture("form.pdf"), None).unwrap();
    let fields_before = before.form_fields(0).unwrap();
    let signed = sign(fixture("form.pdf"), &rsa());
    assert_eq!(
        only(signatures(&signed).unwrap()).status,
        SignatureStatus::Intact
    );
    let after = Document::from_bytes(signed, None).unwrap();
    let fields_after = after.form_fields(0).unwrap();
    for f in &fields_before {
        assert!(
            fields_after
                .iter()
                .any(|g| g.name == f.name && g.value == f.value),
            "champ perdu ou modifié : {}",
            f.name
        );
    }
}

#[test]
fn signing_a_file_with_object_streams_writes_a_valid_update() {
    let signed = sign(fixture("simple-objstm.pdf"), &rsa());
    assert_eq!(
        only(signatures(&signed).unwrap()).status,
        SignatureStatus::Intact
    );
    assert_eq!(Document::from_bytes(signed, None).unwrap().page_count(), 3);
}

#[test]
fn signing_an_encrypted_pdf_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = Document::from_bytes(fixture("encrypted.pdf"), Some("secret")).unwrap();
    let err = doc
        .sign_to(dir.path().join("x.pdf"), &rsa(), &options())
        .unwrap_err();
    assert!(matches!(err, CoreError::InvalidOperation(_)), "{err:?}");
    assert!(!dir.path().join("x.pdf").exists());
}

#[test]
fn signing_an_xfa_form_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = Document::from_bytes(fixture("xfa.pdf"), None).unwrap();
    let err = doc
        .sign_to(dir.path().join("x.pdf"), &rsa(), &options())
        .unwrap_err();
    assert!(
        matches!(err, CoreError::InvalidOperation(ref m) if m.contains("XFA")),
        "{err:?}"
    );
}

#[test]
fn signing_an_edited_document_signs_the_edited_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edite.pdf");
    let mut doc = Document::from_bytes(fixture("simple.pdf"), None).unwrap();
    doc.apply(&EditCommand::DeletePages { pages: vec![2] })
        .unwrap();
    doc.sign_to(&path, &rsa(), &options()).unwrap();
    assert!(!doc.is_modified(), "signer enregistre");
    let signed = std::fs::read(&path).unwrap();
    assert_eq!(
        only(signatures(&signed).unwrap()).status,
        SignatureStatus::Intact
    );
    assert_eq!(Document::from_bytes(signed, None).unwrap().page_count(), 2);
}

#[test]
fn unsigned_document_has_no_signatures() {
    assert!(signatures(&fixture("simple.pdf")).unwrap().is_empty());
    assert!(signatures(&fixture("form.pdf")).unwrap().is_empty());
}

/// Vérification croisée manuelle par pyHanko et qpdf : `MEGA_CMS_SAMPLES=<dossier> cargo
/// test -p mega-core --test digital_signature -- --ignored` écrit `ours.pdf` (RSA) et
/// `ours-ec.pdf`, puis `pyhanko sign validate --trust fixtures/test-signer-cert.pem ours.pdf`.
#[test]
#[ignore]
fn write_signed_samples_for_pyhanko() {
    let dir = std::path::PathBuf::from(std::env::var("MEGA_CMS_SAMPLES").unwrap());
    std::fs::write(dir.join("ours.pdf"), sign(fixture("simple.pdf"), &rsa())).unwrap();
    std::fs::write(dir.join("ours-ec.pdf"), sign(fixture("form.pdf"), &ec())).unwrap();
    std::fs::write(
        dir.join("ours-objstm.pdf"),
        sign(fixture("simple-objstm.pdf"), &rsa()),
    )
    .unwrap();
    let mut old = fixture("simple.pdf");
    old[5..8].copy_from_slice(b"1.4");
    std::fs::write(dir.join("ours-pdf14.pdf"), sign(old, &rsa())).unwrap();
    std::fs::write(
        dir.join("ours-ap.pdf"),
        sign(mega_core::testing::form_with_appearances_pdf(), &ec()),
    )
    .unwrap();
}

#[test]
fn third_party_signature_verifies() {
    // `fixtures/signed-external.pdf` : `simple.pdf` signé par pyHanko avec un autre
    // certificat (« External Test Signer ») : nous lisons les signatures des autres outils.
    let info = only(signatures(&fixture("signed-external.pdf")).unwrap());
    assert_eq!(info.status, SignatureStatus::Intact, "{info:?}");
    assert!(info.covers_whole_document);
    assert_eq!(info.signer.as_deref(), Some("External Test Signer"));
    assert_eq!(info.field, "Sig1");
    assert!(info.signed_at.is_some());
    eprintln!("format tiers : {}", info.sub_filter);
}

#[test]
fn third_party_pades_signature_verifies() {
    // Variante PAdES (`ETSI.CAdES.detached`, sans attribut signing-time : date lue dans `/M`).
    let info = only(signatures(&fixture("signed-external-pades.pdf")).unwrap());
    assert_eq!(info.sub_filter, "ETSI.CAdES.detached");
    assert_eq!(info.status, SignatureStatus::Intact, "{info:?}");
    assert!(info.covers_whole_document);
    assert!(info.signed_at.is_some(), "date tirée de /M");
}

#[test]
fn signature_fields_do_not_count_as_fillable_form_fields() {
    // Une signature invisible ajoute un champ `/Sig` : il ne doit pas faire apparaître un
    // onglet « Formulaire » sur un document qui n'a rien à remplir (il a son onglet).
    let signed = Document::from_bytes(sign(fixture("simple.pdf"), &rsa()), None).unwrap();
    assert_eq!(signed.form_info().unwrap().field_count, 0);
    let form_before = Document::from_bytes(fixture("form.pdf"), None)
        .unwrap()
        .form_info()
        .unwrap()
        .field_count;
    let form_signed = Document::from_bytes(sign(fixture("form.pdf"), &rsa()), None).unwrap();
    assert_eq!(form_signed.form_info().unwrap().field_count, form_before);
}

// --- Relecture du Lot 8 : entrées hostiles et cas limites -------------------------------

/// Une édition refusée ne doit pas faire croire que le document a changé : l'enregistrement
/// suivant écrit les octets signés tels quels.
#[test]
fn a_refused_edit_does_not_rewrite_a_signed_file_on_save() {
    let signed = sign(fixture("simple.pdf"), &rsa());
    let mut doc = Document::from_bytes(signed.clone(), None).unwrap();
    assert!(
        doc.apply(&EditCommand::DeletePages { pages: vec![99] })
            .is_err()
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copie.pdf");
    doc.save_as(&path).unwrap();
    assert!(std::fs::read(&path).unwrap() == signed, "octets inchangés");
}

/// Un arbre de champs qui se cite lui-même est lu une seule fois (pas de boucle infinie).
#[test]
fn self_referencing_signature_fields_are_read_once() {
    let list = signatures(&mega_core::testing::cyclic_signature_fields_pdf()).unwrap();
    assert_eq!(list.len(), 1, "{list:?}");
}

/// Sans `/ByteRange` nulle part, aucune signature possible : pas d'analyse du fichier.
#[test]
fn a_file_without_any_byte_range_has_no_signatures() {
    assert_eq!(signatures(b"pas un PDF").unwrap(), vec![]);
}

/// Certifié sans modification (P = 1) ou « formulaires et signature » seulement (P = 2) :
/// ajouter un **nouveau** champ de signature casserait la certification, donc refus.
#[test]
fn certified_documents_that_forbid_new_fields_are_refused() {
    for (p, refused) in [(1, true), (2, true), (3, false)] {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::from_bytes(mega_core::testing::certified_pdf(p), None).unwrap();
        let result = doc.sign_to(dir.path().join("signe.pdf"), &rsa(), &options());
        assert_eq!(result.is_err(), refused, "P = {p} : {result:?}");
    }
}

/// Un motif réécrit dans une révision ajoutée après la signature n'est pas signé : il ne
/// doit pas s'afficher sous la signature.
#[test]
fn display_data_rewritten_after_signing_is_not_shown() {
    use lopdf::{IncrementalDocument, Object};
    let signed = sign(fixture("simple.pdf"), &rsa());
    let prev = lopdf::Document::load_mem(&signed).unwrap();
    let sig_id = *prev
        .objects
        .iter()
        .find(|(_, o)| {
            o.as_dict()
                .is_ok_and(|d| d.get(b"Type").ok() == Some(&Object::Name(b"Sig".to_vec())))
        })
        .unwrap()
        .0;
    let mut inc = IncrementalDocument::create_from(signed.clone(), prev);
    inc.opt_clone_object_to_new_document(sig_id).unwrap();
    inc.new_document
        .get_dictionary_mut(sig_id)
        .unwrap()
        .set("Reason", Object::string_literal("Falsifie"));
    let mut forged = Vec::new();
    inc.save_to(&mut forged).unwrap();
    let info = only(signatures(&forged).unwrap());
    assert_eq!(info.status, SignatureStatus::Intact, "{info:?}");
    assert!(!info.covers_whole_document);
    assert_eq!(info.reason, None, "motif non signé masqué");
}

// --- Lot 9 : lecture stricte et révisions signées propres (C34-C38) ----------------------

use mega_core::digital_signature::SigIssue;

#[test]
fn byte_range_with_extra_entries_is_invalid() {
    let original = fixture("simple.pdf");
    let mut signed = sign(original.clone(), &rsa());
    // `/ByteRange[0 a b c      ]` : on glisse « /X » dans les espaces de bourrage.
    let key = b"/ByteRange[0 ";
    let at = original.len()
        + signed[original.len()..]
            .windows(key.len())
            .position(|w| w == key)
            .unwrap();
    let close = at + signed[at..].iter().position(|&c| c == b']').unwrap();
    assert!(&signed[close - 3..close] == b"   ", "bourrage attendu");
    signed[close - 3..close].copy_from_slice(b" /X");
    let info = only(signatures(&signed).unwrap());
    assert_eq!(
        info.status,
        SignatureStatus::Invalid(SigIssue::ByteRangeInvalid)
    );
}

#[test]
fn usage_rights_signature_is_reported_unsupported() {
    let list = signatures(&mega_core::testing::usage_rights_pdf()).unwrap();
    let info = only(list);
    assert_eq!(
        info.status,
        SignatureStatus::Unsupported(SigIssue::UsageRights)
    );
}

/// Le dictionnaire de la révision signée (la plus récente) de `bytes`.
fn latest(bytes: &[u8]) -> lopdf::Document {
    lopdf::Document::load_mem(bytes).unwrap()
}

#[test]
fn signed_widget_has_an_empty_appearance() {
    let signed = latest(&sign(fixture("simple.pdf"), &rsa()));
    let widget = signed
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| d.get(b"FT").ok() == Some(&lopdf::Object::Name(b"Sig".to_vec())))
        .unwrap();
    let ap = widget.get(b"AP").unwrap().as_dict().unwrap();
    let n = signed
        .get_object(ap.get(b"N").unwrap().as_reference().unwrap())
        .unwrap()
        .as_stream()
        .unwrap();
    let bbox: Vec<f32> = n
        .dict
        .get(b"BBox")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_float().unwrap())
        .collect();
    assert_eq!(bbox, vec![0.0; 4]);
}

/// `/NeedAppearances` de la révision la plus récente.
fn need_appearances(signed: &lopdf::Document) -> Option<lopdf::Object> {
    let form = match signed.catalog().unwrap().get(b"AcroForm").unwrap() {
        lopdf::Object::Reference(id) => signed.get_dictionary(*id).unwrap(),
        lopdf::Object::Dictionary(d) => d,
        _ => panic!("/AcroForm"),
    };
    form.get(b"NeedAppearances").ok().cloned()
}

#[test]
fn signed_revision_turns_need_appearances_off() {
    // Tous les champs ont leur apparence : la demande de régénération est retirée.
    let with_ap = mega_core::testing::form_with_appearances_pdf();
    let signed = latest(&sign(with_ap, &ec()));
    assert_eq!(
        need_appearances(&signed),
        Some(lopdf::Object::Boolean(false))
    );
    // Des champs sans apparence (form.pdf) : on la garde, sinon ils s'afficheraient vides.
    let signed = latest(&sign(fixture("form.pdf"), &ec()));
    assert_eq!(
        need_appearances(&signed),
        Some(lopdf::Object::Boolean(true))
    );
}

#[test]
fn signed_revision_renews_the_second_id() {
    let original = fixture("simple.pdf");
    let before = latest(&original);
    let signed = latest(&sign(original, &rsa()));
    let id = |d: &lopdf::Document| -> Vec<Vec<u8>> {
        d.trailer
            .get(b"ID")
            .ok()
            .and_then(|i| i.as_array().ok())
            .map(|a| a.iter().map(|s| s.as_str().unwrap().to_vec()).collect())
            .unwrap_or_default()
    };
    let (old, new) = (id(&before), id(&signed));
    assert_eq!(new.len(), 2, "{new:?}");
    if old.len() == 2 {
        assert_eq!(new[0], old[0], "premier identifiant conservé");
        assert_ne!(new[1], old[1], "second identifiant renouvelé");
    }
}

#[test]
fn old_header_is_upgraded_in_the_signed_revision() {
    let mut original = fixture("simple.pdf");
    assert!(original.starts_with(b"%PDF-1.7"));
    original[5..8].copy_from_slice(b"1.4");
    let signed = latest(&sign(original, &rsa()));
    assert_eq!(
        signed.catalog().unwrap().get(b"Version").unwrap(),
        &lopdf::Object::Name(b"1.6".to_vec())
    );
}

#[test]
fn xref_stream_trailer_is_clean() {
    let original = fixture("simple-objstm.pdf");
    let signed = sign(original.clone(), &rsa());
    let tail = &signed[original.len()..];
    let has = |needle: &[u8]| tail.windows(needle.len()).any(|w| w == needle);
    assert!(
        !has(b"/DecodeParms") && !has(b"/Predictor"),
        "{}",
        String::from_utf8_lossy(tail)
    );
}
