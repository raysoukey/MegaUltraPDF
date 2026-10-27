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
