// SPDX-License-Identifier: GPL-3.0-or-later
//! Fichiers chiffrés **par qpdf** (comme les produit un autre logiciel) : après une édition et
//! un enregistrement, le XMP chiffré reste lisible avec le mot de passe, n'apparaît pas en
//! clair, et la sortie passe qpdf (constat F21-4, tâche 21b du Lot 14).
//!
//! PDFium écrit le flux `/Metadata` en clair dans un document chiffré sans le déclarer ;
//! `Document::to_bytes` le rechiffre. AES-256 (R6), AES-128 (R4) et RC4 (R3, R2 : le XMP y est
//! toujours chiffré) passent tous par le même chemin.

use std::path::Path;
use std::process::Command;

use mega_core::Document;
use mega_core::edit::EditCommand;
use mega_core::testing::rich_catalog_pdf;

use crate::{TOOL_LIMIT, assert_conforms, output_text, qpdf, run_with_timeout};

/// Le XMP du catalogue de `path`, lu et déchiffré par lopdf avec `password`.
fn catalog_xmp(path: &Path, password: &str) -> String {
    let bytes = std::fs::read(path).unwrap();
    let doc =
        lopdf::Document::load_mem_with_options(&bytes, lopdf::LoadOptions::with_password(password))
            .unwrap();
    let id = doc
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = doc.get_object(id).unwrap().as_stream().unwrap();
    let xmp = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    String::from_utf8_lossy(&xmp).into_owned()
}

/// Le XMP de `rich_catalog_pdf` (titre « Riche ») se relit, et il n'est pas en clair.
fn assert_xmp_intact(path: &Path, password: &str, what: &str) {
    let xmp = catalog_xmp(path, password);
    assert!(
        xmp.starts_with("<?xpacket") && xmp.contains(">Riche</rdf:li>"),
        "{what} : XMP détruit : {:?}",
        xmp.chars().take(60).collect::<String>()
    );
    let raw = std::fs::read(path).unwrap();
    assert!(
        !raw.windows(10).any(|w| w == b"<x:xmpmeta"),
        "{what} : XMP lisible en clair dans un fichier chiffré"
    );
}

/// `rich_catalog_pdf` chiffré par qpdf (`--encrypt secret proprio <args>`), puis deux fois
/// « tourner une page, enregistrer » : chaque enregistrement est une nouvelle sortie de PDFium.
#[test]
fn qpdf_encrypted_files_keep_their_xmp_through_edits() {
    let Some(qpdf) = qpdf() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("riche.pdf");
    std::fs::write(&source, rich_catalog_pdf("1.7", None)).unwrap();
    // (nom, options de qpdf, options de `--encrypt`)
    let ciphers: [(&str, &[&str], &[&str]); 5] = [
        ("AES-256", &[], &["256"]),
        // Flux d'objets et table de références en flux (relecture de la tâche 21b).
        ("AES-256 objstm", &["--object-streams=generate"], &["256"]),
        ("AES-128", &[], &["128", "--use-aes=y"]),
        ("RC4 128", &[], &["128", "--use-aes=n"]),
        ("RC4 40", &[], &["40"]),
    ];
    for (what, options, args) in ciphers {
        let encrypted = dir.path().join(format!("{}.pdf", what.replace(' ', "-")));
        let mut cmd = Command::new(&qpdf);
        // RC4 et les clés de 40 bits sont refusés par défaut depuis qpdf 11.
        cmd.arg("--allow-weak-crypto")
            .args(options)
            .args(["--encrypt", "secret", "proprio"])
            .args(args)
            .arg("--")
            .arg(&source)
            .arg(&encrypted);
        let out = run_with_timeout(&mut cmd, TOOL_LIMIT).expect("qpdf injoignable ou bloqué");
        assert!(
            out.status.success(),
            "{what} : chiffrement par qpdf\n{}",
            output_text(&out)
        );
        if !options.is_empty() {
            let raw = std::fs::read(&encrypted).unwrap();
            assert!(
                raw.windows(7).any(|w| w == b"/ObjStm"),
                "{what} : la source a des flux d'objets"
            );
        }
        assert_xmp_intact(&encrypted, "secret", &format!("{what} (source)"));
        let mut doc = Document::open(&encrypted, Some("secret")).unwrap();
        let saved = dir
            .path()
            .join(format!("{}-edite.pdf", what.replace(' ', "-")));
        for page in [0u16, 1] {
            doc.apply(&EditCommand::RotatePages {
                pages: vec![page],
                quarter_turns: 1,
            })
            .unwrap();
            doc.save_as(&saved).unwrap();
            let step = format!("{what}, enregistrement {}", page + 1);
            assert_conforms(&qpdf, &saved, Some("secret"), &step);
            assert_xmp_intact(&saved, "secret", &step);
        }
    }
}
