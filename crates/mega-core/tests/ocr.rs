// SPDX-License-Identifier: GPL-3.0-or-later
//! OCR par la commande `tesseract` (feature `ocr`) : lecture de sa sortie TSV, recherche de
//! l'exécutable, passage pixels → espace affiché, couche de texte invisible.
//!
//! Les tests qui lancent vraiment Tesseract ne s'exécutent que s'il est installé ; sinon ils
//! l'écrivent sur la sortie d'erreur et passent (la CI Linux l'installe).
#![cfg(feature = "ocr")]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use mega_core::edit::EditCommand;
use mega_core::ocr::{self, OcrLine, OcrWord};
use mega_core::testing::{blank_pdf, sample_pdf, scanned_pdf};
use mega_core::text::SearchOptions;
use mega_core::{CoreError, Document};

const TSV: &str =
    "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext
1\t1\t0\t0\t0\t0\t0\t0\t2480\t3508\t-1\t
2\t1\t1\t0\t0\t0\t308\t269\t304\t93\t-1\t
3\t1\t1\t1\t0\t0\t308\t269\t304\t93\t-1\t
4\t1\t1\t1\t1\t0\t308\t269\t304\t93\t-1\t
5\t1\t1\t1\t1\t1\t308\t270\t221\t92\t96.5\tPage
5\t1\t1\t1\t1\t2\t564\t269\t48\t72\t95.25\t2
5\t1\t1\t1\t1\t3\t620\t269\t10\t72\t-1\t
4\t1\t1\t1\t2\t0\t308\t400\t200\t50\t-1\t
5\t1\t1\t1\t2\t1\t308\t400\t200\t50\t88.0\tété
";

fn tesseract() -> Option<PathBuf> {
    let found = ocr::find_tesseract();
    if found.is_none() {
        eprintln!("tesseract introuvable : test OCR réel ignoré");
    }
    found
}

#[test]
fn parse_tsv_keeps_lines_and_their_words() {
    let lines = ocr::parse_tsv(TSV);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0],
        OcrLine {
            left: 308,
            top: 269,
            width: 304,
            height: 93,
            words: vec![
                OcrWord {
                    text: "Page".into(),
                    left: 308,
                    top: 270,
                    width: 221,
                    height: 92,
                    confidence: 96.5,
                },
                OcrWord {
                    text: "2".into(),
                    left: 564,
                    top: 269,
                    width: 48,
                    height: 72,
                    confidence: 95.25,
                },
            ],
        }
    );
    // Mot vide (confiance −1) ignoré ; texte UTF-8 conservé.
    assert_eq!(lines[1].words[0].text, "été");
}

#[test]
fn parse_tsv_tolerates_garbage() {
    assert!(ocr::parse_tsv("").is_empty());
    assert!(ocr::parse_tsv("pas du tout\tdu TSV\n5\tx\ty").is_empty());
}

#[test]
fn search_candidates_order() {
    let exe = if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    };
    let app = Path::new("/app/tess").join(exe);
    let env = OsString::from("/env/tesseract");
    let path_var = std::env::join_paths([Path::new("/a"), Path::new("/b")]).unwrap();
    let list = ocr::search_candidates(Some(&app), Some(env), Some(path_var));
    assert_eq!(list[0], app);
    assert_eq!(list[1], PathBuf::from("/env/tesseract"));
    assert_eq!(list[2], Path::new("/a").join(exe));
    assert_eq!(list[3], Path::new("/b").join(exe));
    assert!(list.len() > 4, "dossiers d'installation usuels en dernier");
}

#[test]
fn lines_map_to_displayed_space_with_spaces_between_words() {
    // Image 2480 × 3508 px d'une page A4 (595,28 × 841,89 pt) rendue à 300 dpi.
    let words = ocr::lines_to_words(&ocr::parse_tsv(TSV), 2480, 3508, 595.28, 841.89);
    assert_eq!(words.len(), 3);
    // Une échelle par axe : l'image n'a pas exactement les proportions de la page.
    let (kx, ky) = (595.28 / 2480.0, 841.89 / 3508.0);
    // « Page » : x et largeur du mot, hauteur et position de la ligne (axe y vers le haut).
    assert!((words[0].rect.x - 308.0 * kx).abs() < 0.01);
    assert!((words[0].rect.w - 221.0 * kx).abs() < 0.01);
    assert!((words[0].rect.h - 93.0 * ky).abs() < 0.01);
    assert!((words[0].rect.y - (841.89 - (269.0 + 93.0) * ky)).abs() < 0.01);
    // Même ligne : même hauteur et même base ; espace explicite sauf en fin de ligne.
    assert_eq!(words[0].rect.y, words[1].rect.y);
    assert_eq!(words[0].text, "Page ");
    assert_eq!(words[1].text, "2");
    assert_eq!(words[2].text, "été");
}

#[test]
fn invisible_ocr_words_make_the_page_searchable_across_words() {
    let mut doc = Document::from_bytes(blank_pdf(595.28, 841.89), None).unwrap();
    assert!(!doc.page_has_text(0).unwrap());
    let words = ocr::lines_to_words(&ocr::parse_tsv(TSV), 2480, 3508, 595.28, 841.89);
    doc.add_invisible_words(0, &words).unwrap();
    assert!(doc.page_has_text(0).unwrap());
    let hits = doc
        .search_page(0, "Page 2", &SearchOptions::default())
        .unwrap();
    assert_eq!(hits.len(), 1, "« Page 2 » trouvé d'un seul tenant");
}

#[test]
fn page_has_text_on_a_text_page() {
    let doc = Document::from_bytes(sample_pdf(1, "texte"), None).unwrap();
    assert!(doc.page_has_text(0).unwrap());
}

#[test]
fn ocr_page_refuses_when_tesseract_is_missing() {
    let mut doc = Document::from_bytes(scanned_pdf(), None).unwrap();
    let err = doc
        .ocr_page_with(0, &["eng".to_string()], Path::new("/nulle/part/tesseract"))
        .unwrap_err();
    assert!(
        matches!(err, CoreError::InvalidOperation(ref m) if m.contains("Tesseract")),
        "{err:?}"
    );
    assert!(!doc.page_has_text(0).unwrap(), "aucune modification");
}

#[test]
fn ocr_page_refuses_an_unknown_language() {
    let Some(exe) = tesseract() else { return };
    let mut doc = Document::from_bytes(scanned_pdf(), None).unwrap();
    let err = doc
        .ocr_page_with(0, &["zzz".to_string()], &exe)
        .unwrap_err();
    assert!(
        matches!(err, CoreError::InvalidOperation(ref m) if m.contains("zzz")),
        "{err:?}"
    );
}

#[test]
fn ocr_makes_a_scanned_page_searchable_and_is_undoable() {
    let Some(_) = tesseract() else { return };
    let mut doc = Document::from_bytes(scanned_pdf(), None).unwrap();
    let options = SearchOptions::default();
    assert!(doc.search_page(1, "Page 2", &options).unwrap().is_empty());
    doc.apply(&EditCommand::OcrPage {
        page: 1,
        languages: vec!["eng".to_string()],
    })
    .unwrap();
    let hits = doc.search_page(1, "Page 2", &options).unwrap();
    assert_eq!(hits.len(), 1, "texte reconnu : {:?}", doc.page_text(1));
    doc.undo().unwrap();
    assert!(doc.search_page(1, "Page 2", &options).unwrap().is_empty());
}
