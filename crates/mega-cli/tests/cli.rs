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
        .stderr(predicate::str::contains("Erreur"));
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
