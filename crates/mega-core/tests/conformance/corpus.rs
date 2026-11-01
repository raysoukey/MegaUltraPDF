// SPDX-License-Identifier: GPL-3.0-or-later
//! Fichiers abîmés ou réels : la réécriture répare, le corpus s'ouvre sans panique et se
//! réenregistre conforme, veraPDF garde ses verdicts.

use std::path::{Path, PathBuf};
use std::process::Command;

use mega_core::Document;

use crate::{
    Case, assert_conforms, assert_mutool_reads, assert_pdfjs_reads, check, fixture, mutool,
    pdfjs_check, qpdf,
};

/// Garde-fou du contrôle lui-même : un fichier abîmé du corpus (objet sans `endobj`) est
/// bien refusé, sinon les tests ci-dessus ne prouveraient rien.
#[test]
#[should_panic(expected = "qpdf --check a échoué")]
fn the_check_rejects_a_damaged_file() {
    let Some(qpdf) = qpdf() else {
        panic!("qpdf --check a échoué (qpdf absent : garde-fou sans objet)");
    };
    assert_conforms(
        &qpdf,
        &fixture("robustesse/sans-endobj.pdf"),
        None,
        "corpus",
    );
}

/// Garde-fou de la relecture MuPDF : l'objet sans `endobj` y est signalé (« syntax error »).
/// Sans `MEGA_MUTOOL`, le garde-fou est sans objet.
#[test]
#[should_panic(expected = "mutool info a échoué")]
fn the_mutool_check_rejects_a_damaged_file() {
    let Some(mutool) = mutool() else {
        panic!("mutool info a échoué (MEGA_MUTOOL absent : garde-fou sans objet)");
    };
    let damaged = fixture("robustesse/sans-endobj.pdf");
    assert_mutool_reads(&mutool, &damaged, None, "corpus");
}

/// Garde-fou de la relecture pdf.js : un fichier chiffré lu sans son mot de passe est refusé.
/// Sans `MEGA_PDFJS_CHECK`, le garde-fou est sans objet.
#[test]
#[should_panic(expected = "pdf.js n'ouvre pas le fichier")]
fn the_pdfjs_check_rejects_an_unreadable_file() {
    let Some(script) = pdfjs_check() else {
        panic!("pdf.js n'ouvre pas le fichier (MEGA_PDFJS_CHECK absent : garde-fou sans objet)");
    };
    assert_pdfjs_reads(&script, &fixture("encrypted.pdf"), None, "chiffré");
}

/// Un fichier abîmé ou inhabituel, une fois modifié et enregistré par nous, devient un fichier
/// **sain** : la réécriture répare au lieu de recopier les défauts.
#[test]
fn damaged_inputs_are_written_back_as_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    use serde_json::json;
    for name in [
        "robustesse/prefixe-parasite.pdf",
        "robustesse/xref-decalee.pdf",
        "robustesse/sans-endobj.pdf",
        "robustesse/pdf-2-0.pdf",
        "robustesse/xref-hybride.pdf",
        "robustesse/revisions.pdf",
        "robustesse/linearise.pdf",
    ] {
        let rotate = json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1});
        check(&qpdf, &Case::new(name, name, vec![rotate]));
    }
}

/// veraPDF et le corpus, s'ils sont fournis (`MEGA_VERAPDF` : script `verapdf` ou `.bat` ;
/// `MEGA_CORPUS` : dossier `fixtures/corpus`). Sans eux, les tests veraPDF sont sautés.
fn verapdf_setup() -> Option<(PathBuf, PathBuf)> {
    let verapdf = std::env::var_os("MEGA_VERAPDF").filter(|v| !v.is_empty())?;
    let corpus = std::env::var_os("MEGA_CORPUS").filter(|v| !v.is_empty())?;
    Some((PathBuf::from(verapdf), PathBuf::from(corpus)))
}

/// Rapport veraPDF (`--format text`) d'un fichier pour un profil, et son verdict : vrai si la
/// sortie commence par `PASS`.
fn verapdf_report(verapdf: &Path, flavour: &str, file: &Path) -> (bool, String) {
    let report = Command::new(verapdf)
        .args(["--flavour", flavour, "--format", "text", "--verbose"])
        .arg(file)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&report.stdout).into_owned();
    (text.starts_with("PASS"), text)
}

/// Profil veraPDF d'un fichier du corpus : `0` (déduit de la déclaration XMP) pour PDF/A,
/// `ua1` / `ua2` d'après le nom pour PDF/UA.
fn flavour_of(name: &str) -> &'static str {
    if name.starts_with("pdfua-1") {
        "ua1"
    } else if name.starts_with("pdfua-2") {
        "ua2"
    } else {
        "0"
    }
}

/// Le filigrane du contrôle : translucide (0,3), pour exercer l'opacité et la transparence.
fn draft_watermark() -> mega_core::overlay::Watermark {
    mega_core::overlay::Watermark {
        text: "BROUILLON".to_string(),
        size: 40.0,
        opacity: 0.3,
        angle_deg: 45.0,
        color: mega_core::Color {
            r: 200,
            g: 30,
            b: 30,
        },
        pages: None,
    }
}

/// Un filigrane et une numérotation ne cassent la conformité d'**aucun** fichier conforme du
/// corpus : PDF/UA (artefacts), PDF/A-1 (ni transparence ni opacité), PDF/A-2 à 4 (couleur
/// indépendante du périphérique, groupe de transparence) — audit E6. Les échecs sont tous
/// listés avant l'assertion, pour voir d'un coup ce qui reste à corriger.
#[test]
fn watermark_and_page_numbers_keep_the_conformance_of_every_corpus_file() {
    let Some((verapdf, corpus)) = verapdf_setup() else {
        eprintln!("MEGA_VERAPDF / MEGA_CORPUS absents : contrôle veraPDF sauté");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    let mut checked = 0;
    for folder in ["pdfa", "pdfua"] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(corpus.join(folder))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
            .collect();
        files.sort();
        for source in files {
            let name = source.file_name().unwrap().to_string_lossy().into_owned();
            let mut doc = Document::open(&source, None).unwrap();
            doc.add_watermark(&draft_watermark()).unwrap();
            doc.add_page_numbers(&mega_core::overlay::PageNumbering {
                template: "Page {n} / {total}".to_string(),
                corner: mega_core::overlay::Corner::BottomCenter,
                margin: 20.0,
                size: 10.0,
                start: 1,
                pages: None,
            })
            .unwrap();
            let out = dir.path().join(&name);
            doc.save_as(&out).unwrap();
            let flavour = flavour_of(&name);
            let (pass, report) = verapdf_report(&verapdf, flavour, &out);
            checked += 1;
            if !pass {
                failures.push(format!(
                    "{name} (veraPDF {flavour})
{report}"
                ));
            }
        }
    }
    assert!(checked > 0, "corpus vide");
    assert!(
        failures.is_empty(),
        "{} fichier(s) sur {checked} ne passent plus :
{}",
        failures.len(),
        failures.join(
            "
"
        )
    );
}

/// Changer le titre d'un fichier PDF/A-4 n'écrit que le XMP : PDF/A-4 interdit `/Info`
/// (6.1.3-t4/t5), hors `/ModDate` existant (audit E6).
#[test]
fn retitling_a_pdfa4_file_keeps_its_conformance() {
    let Some((verapdf, corpus)) = verapdf_setup() else {
        eprintln!("MEGA_VERAPDF / MEGA_CORPUS absents : contrôle veraPDF sauté");
        return;
    };
    let source = corpus.join("pdfa").join("pdfa-4-6-1-2-t01-pass-a.pdf");
    let mut doc = Document::open(&source, None).unwrap();
    doc.set_metadata(&mega_core::properties::MetadataPatch {
        title: Some("Nouveau titre".to_string()),
        author: Some("Ève".to_string()),
        ..Default::default()
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("titre.pdf");
    doc.save_as(&out).unwrap();
    let (pass, report) = verapdf_report(&verapdf, "0", &out);
    assert!(
        pass,
        "PDF/A-4 retitré : veraPDF
{report}"
    );
}

// --- Corpus de PDF réels (`MEGA_CORPUS`, audit E1) ------------------------------------------

/// Mots de passe d'ouverture connus des fichiers chiffrés du corpus (`fixtures/corpus/MANIFEST.md`) :
/// les autres fichiers chiffrés servent à tester le refus.
const CORPUS_PASSWORDS: [&str; 3] = ["view", "attachment", "master"];

/// Les PDF du corpus (`MEGA_CORPUS`, sous-dossiers compris), triés ; `None` sans la variable.
fn corpus_files() -> Option<Vec<PathBuf>> {
    let root = crate::env_path("MEGA_CORPUS")?;
    let mut files = Vec::new();
    // Pile explicite : la profondeur des dossiers ne dépend pas de nous.
    let mut folders = vec![root];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Some(files)
}

/// Comment s'est passée l'ouverture d'un fichier du corpus.
enum Opened {
    /// Ouvert (avec ce mot de passe, s'il en fallait un). Le document est mis en `Box` (sur le
    /// tas) : il pèse un kilo-octet, et une enum a toujours la taille de sa plus grande
    /// variante (lint `clippy::large_enum_variant`).
    Yes(Box<Document>, Option<&'static str>),
    /// Chiffré, mot de passe inconnu : un refus normal.
    Protected,
    /// Refusé pour une autre raison (fichier abîmé, chiffrement non pris en charge) : signalé,
    /// pas une faute, tant que rien ne panique.
    Refused(String),
}

/// Ouvre `path`, sans mot de passe puis avec chacun de [`CORPUS_PASSWORDS`].
fn open_corpus_file(path: &Path) -> Opened {
    use mega_core::CoreError;
    match Document::open(path, None) {
        Ok(doc) => Opened::Yes(Box::new(doc), None),
        Err(CoreError::PasswordRequired | CoreError::WrongPassword) => CORPUS_PASSWORDS
            .iter()
            .find_map(|p| Document::open(path, Some(p)).ok().map(|d| (d, *p)))
            .map_or(Opened::Protected, |(doc, p)| {
                Opened::Yes(Box::new(doc), Some(p))
            }),
        Err(e) => Opened::Refused(e.to_string()),
    }
}

/// Le nom d'un fichier du corpus relatif à `MEGA_CORPUS` (`polices/cid.pdf`), pour les
/// messages.
fn corpus_name(path: &Path) -> String {
    let root = crate::env_path("MEGA_CORPUS").unwrap_or_default();
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Exécute `f` en rattrapant une panique, rendue en texte : un fichier qui fait paniquer le
/// moteur est listé, et les suivants sont quand même essayés.
///
/// \[Apprentissage\] **`catch_unwind`.** Une panique remonte la pile jusqu'à la fin du fil
/// d'exécution ; `std::panic::catch_unwind` l'arrête et la change en `Err`. Le compilateur
/// exige que la fermeture soit « sûre au déroulement » (`UnwindSafe`) : `AssertUnwindSafe`
/// déclare qu'on ne réutilisera pas un état laissé à moitié modifié (ici, chaque fichier a
/// ses propres variables). L'acteur de l'application fait de même (Lot 13, ADR-0017).
fn without_panic<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "panique sans message".to_string())
    })
}

/// Chaque fichier du corpus réel s'ouvre **sans panique** (un refus motivé est permis) et sa
/// première page livre son texte. Sans `MEGA_CORPUS`, sauté.
#[test]
fn every_corpus_file_opens_without_panicking() {
    let Some(files) = corpus_files() else {
        eprintln!("MEGA_CORPUS absent : ouverture du corpus sautée");
        return;
    };
    assert!(!files.is_empty(), "corpus vide");
    let (mut opened, mut protected, mut panics) = (0, 0, Vec::new());
    for path in &files {
        let name = corpus_name(path);
        let outcome = without_panic(|| match open_corpus_file(path) {
            Opened::Yes(doc, _) => {
                // Le texte peut manquer (scan) ou être refusé : seule une panique compte.
                let _ = doc.page_text(0);
                Opened::Yes(doc, None)
            }
            other => other,
        });
        match outcome {
            Ok(Opened::Yes(..)) => opened += 1,
            Ok(Opened::Protected) => protected += 1,
            Ok(Opened::Refused(e)) => eprintln!("corpus : {name} refusé : {e}"),
            Err(message) => panics.push(format!("{name} : {message}")),
        }
    }
    eprintln!(
        "corpus : {} fichiers, {opened} ouverts, {protected} protégés (mot de passe inconnu)",
        files.len()
    );
    assert!(
        panics.is_empty(),
        "paniques à l'ouverture :\n{}",
        panics.join("\n")
    );
}

/// Le mot de passe avec lequel qpdf ouvre `path` : celui qui a ouvert le document, aucun, ou
/// l'un des mots de passe connus. Un fichier dont seules les pièces jointes sont chiffrées
/// (`/EFF`) s'ouvre sans mot de passe dans PDFium, mais pas dans qpdf.
fn qpdf_password(
    qpdf: &Path,
    path: &Path,
    opened_with: Option<&'static str>,
) -> Option<&'static str> {
    let candidates = std::iter::once(opened_with)
        .chain(std::iter::once(None))
        .chain(CORPUS_PASSWORDS.iter().copied().map(Some));
    for password in candidates {
        let mut cmd = Command::new(qpdf);
        if let Some(p) = password {
            cmd.arg(format!("--password={p}"));
        }
        let out = cmd.arg("--show-npages").arg(path).output().unwrap();
        if !crate::output_text(&out).contains("invalid password") {
            return password;
        }
    }
    opened_with
}

/// Bilan de la réécriture du corpus : chaque fichier ouvert, première page tournée,
/// enregistré, puis passé à qpdf avec la même source de mot de passe que l'original.
struct CorpusRun {
    /// Fichiers réécrits et contrôlés.
    checked: usize,
    /// Défauts **ajoutés** : l'original passait qpdf, notre sortie non ; ou une panique.
    new_defects: Vec<String>,
    /// Défauts **hérités** : l'original échouait déjà à qpdf, notre sortie aussi.
    inherited: Vec<String>,
}

/// Réécrit tout le corpus (voir [`CorpusRun`]). Un refus d'éditer ou d'enregistrer est
/// seulement affiché.
fn rewrite_corpus(qpdf: &Path, files: &[PathBuf]) -> CorpusRun {
    let dir = tempfile::tempdir().unwrap();
    let rotate: mega_core::edit::EditCommand = serde_json::from_value(
        serde_json::json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1}),
    )
    .unwrap();
    let mut run = CorpusRun {
        checked: 0,
        new_defects: Vec::new(),
        inherited: Vec::new(),
    };
    for (i, path) in files.iter().enumerate() {
        let name = corpus_name(path);
        let out = dir.path().join(format!("{i}.pdf"));
        // `Ok(None)` : fichier non ouvert ; `Err` : refus d'éditer ou d'enregistrer.
        let outcome = without_panic(|| -> Result<Option<Option<&'static str>>, String> {
            let Opened::Yes(mut doc, password) = open_corpus_file(path) else {
                return Ok(None);
            };
            doc.apply(&rotate)
                .map_err(|e| format!("rotation refusée : {e}"))?;
            doc.save_as(&out)
                .map_err(|e| format!("enregistrement refusé : {e}"))?;
            Ok(Some(password))
        });
        let opened_with = match outcome {
            Ok(Ok(Some(password))) => password,
            Ok(Ok(None)) => continue,
            Ok(Err(refusal)) => {
                eprintln!("corpus : {name} : {refusal}");
                continue;
            }
            Err(message) => {
                run.new_defects
                    .push(format!("{name} : panique : {message}"));
                continue;
            }
        };
        let password = qpdf_password(qpdf, path, opened_with);
        let original = crate::qpdf_verdict(qpdf, path, password);
        if original
            .as_ref()
            .is_err_and(|report| report.contains("invalid password"))
        {
            // PDFium l'ouvre, mais qpdf demande un mot de passe que nous ne connaissons pas
            // (pièces jointes chiffrées) : rien à comparer.
            eprintln!("corpus : {name} : mot de passe inconnu de qpdf, contrôle sauté");
            continue;
        }
        run.checked += 1;
        if let Err(report) = crate::qpdf_verdict(qpdf, &out, password) {
            let line = format!("{name} : qpdf --check a échoué {report}");
            if original.is_ok() {
                run.new_defects.push(line);
            } else {
                run.inherited.push(line);
            }
        }
    }
    run
}

/// Chaque fichier du corpus qui s'ouvre, une fois sa première page tournée et enregistré par
/// nous, passe `qpdf --check` **si l'original le passait** : la réécriture n'ajoute aucun
/// défaut. Les défauts hérités d'un original déjà non conforme sont affichés (voir
/// `rewriting_repairs_damaged_corpus_files`). Sans `MEGA_CORPUS`, sauté.
#[test]
fn rotating_every_corpus_file_adds_no_defect() {
    let Some(qpdf) = qpdf() else { return };
    let Some(files) = corpus_files() else {
        eprintln!("MEGA_CORPUS absent : réécriture du corpus sautée");
        return;
    };
    let run = rewrite_corpus(&qpdf, &files);
    eprintln!(
        "corpus : {} fichiers tournés et contrôlés par qpdf, {} défauts hérités de l'original",
        run.checked,
        run.inherited.len()
    );
    for line in &run.inherited {
        eprintln!(
            "corpus (hérité) : {}",
            line.lines().next().unwrap_or_default()
        );
    }
    assert!(
        run.checked > 0,
        "aucun fichier du corpus n'a pu être réécrit"
    );
    assert!(
        run.new_defects.is_empty(),
        "{} fichier(s) sur {} :\n{}",
        run.new_defects.len(),
        run.checked,
        run.new_defects.join("\n")
    );
}

/// Exigence plus forte, pas encore tenue (constats de la tâche 21 du Lot 14) : un original
/// abîmé **dans sa structure** ressort conforme. Restent à réparer : F21-1, page sans
/// `/Resources` (`navigation/outline_goto_action.pdf`, `navigation/outlines_for_editor.pdf`) ;
/// F21-2, arbre de noms au nœud vide ou aux clés mal triées
/// (`pdfua/pdfua-1-7-18-7-t01-pass-a.pdf`, `producteurs/TAMReview.pdf`) ; F21-3, flux Flate
/// suivi de données parasites, recopié tel quel (`producteurs/form-filled-by-acrobat.pdf`).
/// Seuls les flux dont les **données** sont corrompues (LZW cassé, JPEG tronqué) restent
/// excusés : les réécrire demanderait de jeter du contenu.
#[test]
#[ignore = "constats F21-1 à F21-3 (Lot 14, tâche 21) : défauts hérités du corpus"]
fn rewriting_repairs_damaged_corpus_files() {
    let Some(qpdf) = qpdf() else { return };
    let Some(files) = corpus_files() else {
        eprintln!("MEGA_CORPUS absent : réécriture du corpus sautée");
        return;
    };
    let run = rewrite_corpus(&qpdf, &files);
    let unrepaired: Vec<&String> = run
        .inherited
        .iter()
        .filter(|line| !line.contains("error decoding stream data"))
        .collect();
    assert!(
        unrepaired.is_empty(),
        "{} original(aux) abîmé(s) non réparé(s) :\n{}",
        unrepaired.len(),
        unrepaired
            .iter()
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
