// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests de bout en bout du binaire `mega` : on lance le vrai exécutable.

use assert_cmd::Command;
use predicates::prelude::*;

fn fixture(name: &str) -> String {
    format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn mega() -> Command {
    Command::cargo_bin("mega").unwrap()
}

#[test]
fn info_prints_page_count_and_title() {
    mega()
        .args(["info", &fixture("simple.pdf")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pages : 3"))
        .stdout(predicate::str::contains("Titre : MegaUltraPDF"));
}

#[test]
fn info_reports_missing_file() {
    mega()
        .args(["info", "introuvable.pdf"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Erreur"))
        .stderr(predicate::function(|s: &str| {
            s.matches("os error").count() == 1
        }));
}

#[test]
fn info_handles_unicode_path() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("mes documents résumé.pdf");
    std::fs::copy(fixture("simple.pdf"), &p).unwrap();
    mega()
        .args(["info", p.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pages : 3"));
}

#[test]
fn info_encrypted_needs_password() {
    mega()
        .args(["info", &fixture("encrypted.pdf")])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("mot de passe"));
    mega()
        .args(["info", &fixture("encrypted.pdf"), "--password", "secret"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pages : 3"));
}

#[test]
fn render_writes_png() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("p1.png");
    mega()
        .args([
            "render",
            &fixture("simple.pdf"),
            "--page",
            "1",
            "--scale",
            "1.5",
            "--out",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.len() > 100);
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn render_rejects_zero_scale() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("p1.png");
    mega()
        .args([
            "render",
            &fixture("simple.pdf"),
            "--scale",
            "0",
            "--out",
            out.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("strictement positif"));
    assert!(!out.exists());
}

#[test]
fn info_encrypted_wrong_password() {
    mega()
        .args(["info", &fixture("encrypted.pdf"), "--password", "faux"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("incorrect"));
}

// ---- Lot 2 : merge, split, rotate, extract ----

fn pages_of(path: &std::path::Path) -> String {
    let out = mega()
        .args(["info", path.to_str().unwrap()])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find(|l| l.starts_with("Pages : "))
        .map(|l| l.trim_start_matches("Pages : ").to_string())
        .unwrap_or_default()
}

#[test]
fn merge_concatenates_files() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("fusion.pdf");
    mega()
        .args([
            "merge",
            &fixture("simple.pdf"),
            &fixture("outlined.pdf"),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(pages_of(&out), "6");
}

#[test]
fn split_every_n_pages_and_by_ranges() {
    let dir = tempfile::tempdir().unwrap();
    mega()
        .args([
            "split",
            &fixture("simple.pdf"),
            "--every",
            "2",
            "-o",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(pages_of(&dir.path().join("simple-1.pdf")), "2");
    assert_eq!(pages_of(&dir.path().join("simple-2.pdf")), "1");

    let dir2 = tempfile::tempdir().unwrap();
    mega()
        .args([
            "split",
            &fixture("simple.pdf"),
            "--ranges",
            "1,2-3",
            "--prefix",
            "part",
            "-o",
            dir2.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("part-2.pdf"));
    assert_eq!(pages_of(&dir2.path().join("part-1.pdf")), "1");
    assert_eq!(pages_of(&dir2.path().join("part-2.pdf")), "2");
}

#[test]
fn split_by_bookmarks() {
    let dir = tempfile::tempdir().unwrap();
    mega()
        .args([
            "split",
            &fixture("outlined.pdf"),
            "--bookmarks",
            "-o",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success();
    // Signets de premier niveau en pages 1 et 3 → [1-2] et [3].
    assert_eq!(pages_of(&dir.path().join("outlined-1.pdf")), "2");
    assert_eq!(pages_of(&dir.path().join("outlined-2.pdf")), "1");
}

#[test]
fn rotate_pages_and_report_in_info() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("tourne.pdf");
    mega()
        .args([
            "rotate",
            &fixture("simple.pdf"),
            "--pages",
            "1",
            "--degrees",
            "90",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    mega()
        .args(["info", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rotation page 1 : 90°"));
    mega()
        .args([
            "rotate",
            &fixture("simple.pdf"),
            "--pages",
            "1",
            "--degrees",
            "45",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("90"));
}

#[test]
fn extract_pages_and_reject_bad_spec() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("extrait.pdf");
    mega()
        .args([
            "extract",
            &fixture("simple.pdf"),
            "--pages",
            "2-3",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(pages_of(&out), "2");
    mega()
        .args([
            "extract",
            &fixture("simple.pdf"),
            "--pages",
            "9",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Plage de pages invalide"));
    assert_eq!(
        pages_of(&out),
        "2",
        "la sortie précédente n'est pas écrasée par un échec"
    );
}

// ---- Passe de correction après relecture (Lot 2) ----

#[test]
fn split_refuses_to_overwrite_without_force_and_rejects_every_zero() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().to_str().unwrap();
    mega()
        .args(["split", &fixture("simple.pdf"), "--every", "2", "-o", out])
        .assert()
        .success();
    std::fs::write(dir.path().join("simple-1.pdf"), b"retouche").unwrap();
    mega()
        .args(["split", &fixture("simple.pdf"), "--every", "2", "-o", out])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("existe déjà"));
    assert_eq!(
        std::fs::read(dir.path().join("simple-1.pdf")).unwrap(),
        b"retouche"
    );
    mega()
        .args([
            "split",
            &fixture("simple.pdf"),
            "--every",
            "2",
            "-o",
            out,
            "--force",
        ])
        .assert()
        .success();
    assert_eq!(pages_of(&dir.path().join("simple-1.pdf")), "2");
    mega()
        .args([
            "split",
            &fixture("simple.pdf"),
            "--every",
            "0",
            "-o",
            out,
            "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("1"));
}

// ---- Lot 3 : annotations ----

#[test]
fn annots_lists_annotations() {
    mega()
        .args(["annots", &fixture("annotated.pdf")])
        .assert()
        .success()
        .stdout(predicate::str::contains("p1 #0 note"))
        .stdout(predicate::str::contains("auteur=\"Alice\""))
        .stdout(predicate::str::contains("Note de test"))
        .stdout(predicate::str::contains("p1 #3 underline"));
    mega()
        .args(["annots", &fixture("annotated.pdf"), "--page", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("4 annotation"));
    mega()
        .args(["annots", &fixture("annotated.pdf"), "--page", "2"])
        .assert()
        .code(1);
}

// ---- Lot 4 : formulaires ----

#[test]
fn fields_and_flatten_commands() {
    mega()
        .args(["fields", &fixture("form.pdf")])
        .assert()
        .success()
        .stdout(predicate::str::contains("p1 nom [text] ="))
        .stdout(predicate::str::contains("p1 ville [combo] = Paris"))
        .stdout(predicate::str::contains("p1 ok [checkbox] = non"))
        .stdout(predicate::str::contains("8 champs"));
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("plat.pdf");
    mega()
        .args(["flatten", &fixture("form.pdf"), "-o", out.to_str().unwrap()])
        .assert()
        .success();
    mega()
        .args(["fields", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 champ"));
}

// ---- Lot 5 : contenu ----

#[test]
fn objects_lists_page_objects() {
    mega()
        .args(["objects", &fixture("simple.pdf"), "--page", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("p1 #0 text"))
        .stdout(predicate::str::contains("Page 1"))
        .stdout(predicate::str::contains("Helvetica"))
        .stdout(predicate::str::contains("1 objet"));
    mega()
        .args(["objects", &fixture("simple.pdf"), "--page", "9"])
        .assert()
        .code(1);
}

#[test]
fn watermark_number_and_crop_commands() {
    let dir = tempfile::tempdir().unwrap();
    let marked = dir.path().join("marque.pdf");
    mega()
        .args([
            "watermark",
            &fixture("simple.pdf"),
            "--text",
            "BROUILLON",
            "--opacity",
            "0.4",
            "-o",
            marked.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("3 page"));
    mega()
        .args(["objects", marked.to_str().unwrap(), "--page", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("BROUILLON"));
    let numbered = dir.path().join("numerote.pdf");
    mega()
        .args([
            "number",
            marked.to_str().unwrap(),
            "--template",
            "{n} / {total}",
            "--corner",
            "bottom_center",
            "-o",
            numbered.to_str().unwrap(),
        ])
        .assert()
        .success();
    mega()
        .args(["objects", numbered.to_str().unwrap(), "--page", "3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("3 / 3"));
    let cropped = dir.path().join("rogne.pdf");
    mega()
        .args([
            "crop",
            &fixture("simple.pdf"),
            "--margins",
            "50,50,50,50",
            "--pages",
            "1",
            "-o",
            cropped.to_str().unwrap(),
        ])
        .assert()
        .success();
    mega()
        .args(["info", cropped.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Page 1 : 495.3 × 741.9 pt"));
    mega()
        .args([
            "crop",
            &fixture("simple.pdf"),
            "--margins",
            "50,50",
            "-o",
            cropped.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("quatre"));
}

#[test]
fn redact_removes_text_from_output() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("caviarde.pdf");
    mega()
        .args([
            "redact",
            &fixture("simple.pdf"),
            "--page",
            "1",
            "--rect",
            "60,740,200,40",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 texte"));
    mega()
        .args(["objects", out.to_str().unwrap(), "--page", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Page 1").not())
        .stdout(predicate::str::contains("path"));
    mega()
        .args(["objects", out.to_str().unwrap(), "--page", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Page 2"));
}

/// C3 (relecture) : une annotation retirée par `mega redact` ne doit pas survivre en objet
/// orphelin dans le fichier produit (contenu secret compris).
#[test]
fn redact_output_does_not_keep_removed_annotation_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sans-note.pdf");
    mega()
        .args([
            "redact",
            &fixture("annotated.pdf"),
            "--page",
            "1",
            "--rect",
            "40,740,40,40",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 annotation"));
    let bytes = String::from_utf8_lossy(&std::fs::read(&out).unwrap()).into_owned();
    assert!(
        !bytes.contains("FEFF004E006F00740065"),
        "le contenu de la note a disparu"
    );
    assert!(!bytes.contains("(Alice)"), "son auteur aussi");
    mega()
        .args(["annots", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("3 annotation"));
}

// ---- Lot 6 : export, sécurité, optimisation, pièces jointes, comparaison ----

#[test]
fn export_images_and_text_commands() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("images");
    mega()
        .args([
            "export-images",
            &fixture("simple.pdf"),
            "-o",
            out.to_str().unwrap(),
            "--pages",
            "1-2",
            "--dpi",
            "72",
            "--prefix",
            "p",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("2 image"));
    assert!(out.join("p-1.png").exists() && out.join("p-2.png").exists());
    assert!(!out.join("p-3.png").exists());
    // Sans --force, refus d'écraser.
    mega()
        .args([
            "export-images",
            &fixture("simple.pdf"),
            "-o",
            out.to_str().unwrap(),
            "--prefix",
            "p",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("existe"));
    mega()
        .args([
            "export-images",
            &fixture("simple.pdf"),
            "-o",
            out.to_str().unwrap(),
            "--jpeg",
            "--quality",
            "60",
            "--prefix",
            "j",
        ])
        .assert()
        .success();
    assert!(out.join("j-3.jpg").exists());
    let txt = dir.path().join("texte.txt");
    mega()
        .args([
            "export-text",
            &fixture("simple.pdf"),
            "-o",
            txt.to_str().unwrap(),
        ])
        .assert()
        .success();
    let text = std::fs::read_to_string(&txt).unwrap();
    assert!(text.contains("===== Page 2 =====") && text.contains("Page 3"));
}

#[test]
fn meta_reads_and_writes_metadata() {
    mega()
        .args(["meta", &fixture("simple.pdf")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Titre : MegaUltraPDF"));
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("meta.pdf");
    mega()
        .args([
            "meta",
            &fixture("simple.pdf"),
            "--title",
            "Résumé",
            "--author",
            "Zoë",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    mega()
        .args(["meta", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Titre : Résumé"))
        .stdout(predicate::str::contains("Auteur : Zoë"));
}

#[test]
fn protect_and_unprotect_commands() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("verrou.pdf");
    mega()
        .args([
            "protect",
            &fixture("simple.pdf"),
            "--user",
            "secret",
            "--owner",
            "patron",
            "--no-print",
            "-o",
            locked.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("AES-256"));
    mega()
        .args(["info", locked.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("mot de passe"));
    mega()
        .args(["meta", locked.to_str().unwrap(), "--password", "secret"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Protégé : oui"))
        .stdout(predicate::str::contains("impression : non"));
    let open = dir.path().join("ouvert.pdf");
    mega()
        .args([
            "unprotect",
            locked.to_str().unwrap(),
            "--password",
            "patron",
            "-o",
            open.to_str().unwrap(),
        ])
        .assert()
        .success();
    mega()
        .args(["info", open.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pages : 3"));
    // Mot de passe propriétaire seul : s'ouvre sans mot de passe, mais ne se déprotège pas sans.
    mega()
        .args([
            "unprotect",
            &fixture("owner-only.pdf"),
            "-o",
            dir.path().join("x.pdf").to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("incorrect"));
}

#[test]
fn optimize_attachments_and_compare_commands() {
    let dir = tempfile::tempdir().unwrap();
    let small = dir.path().join("petit.pdf");
    mega()
        .args([
            "optimize",
            &fixture("simple.pdf"),
            "-o",
            small.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("octets"));
    assert!(small.exists());
    mega()
        .args(["attachments", &fixture("attachments.pdf")])
        .assert()
        .success()
        .stdout(predicate::str::contains("#0 données.csv"))
        .stdout(predicate::str::contains("#1 notes.txt (13 o)"))
        .stdout(predicate::str::contains("2 pièce"));
    let extracted = dir.path().join("notes.txt");
    mega()
        .args([
            "attachments",
            &fixture("attachments.pdf"),
            "--extract",
            "1",
            "-o",
            extracted.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(std::fs::read(&extracted).unwrap(), b"Bonjour monde");
    let diff = dir.path().join("diff.png");
    mega()
        .args([
            "compare",
            &fixture("simple.pdf"),
            &fixture("annotated.pdf"),
            "--page",
            "1",
            "-o",
            diff.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("pixels différents"));
    assert!(diff.exists());
}

// --- Lot 8 : signature et OCR --------------------------------------------------------------

#[test]
fn sign_then_signatures_reports_intact() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("signe.pdf");
    mega()
        .args([
            "sign",
            &fixture("simple.pdf"),
            "--p12",
            &fixture("test-signer.p12"),
        ])
        .args(["--reason", "Approuvé", "-o"])
        .arg(&out)
        .env("MEGA_P12_PASSWORD", "test")
        .assert()
        .success();
    mega()
        .arg("signatures")
        .arg(&out)
        .assert()
        .success()
        .stdout(predicate::str::contains("MegaUltraPDF Test Signer"))
        .stdout(predicate::str::contains("intègre"))
        .stdout(predicate::str::contains("Approuvé"));
}

#[test]
fn sign_with_a_wrong_password_fails_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("signe.pdf");
    mega()
        .args([
            "sign",
            &fixture("simple.pdf"),
            "--p12",
            &fixture("test-signer.p12"),
            "-o",
        ])
        .arg(&out)
        .env("MEGA_P12_PASSWORD", "faux")
        .assert()
        .failure()
        .stderr(predicate::str::contains("incorrect"));
    assert!(!out.exists());
}

#[test]
fn signatures_of_an_unsigned_file() {
    mega()
        .args(["signatures", &fixture("simple.pdf")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Aucune signature"));
}

#[test]
fn ocr_without_tesseract_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("ocr.pdf");
    mega()
        .args([
            "ocr",
            &fixture("scanned.pdf"),
            "--tesseract",
            "/nulle/part/tesseract",
            "-o",
        ])
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Tesseract"));
    assert!(!out.exists());
}
