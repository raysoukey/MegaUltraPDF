// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests d'intégration de `mega-core` : ouverture, métadonnées, rendu, erreurs.
//!
//! \[Apprentissage\] Les fichiers de `tests/` sont compilés comme des crates séparés qui
//! n'ont accès qu'à l'API publique, exactement comme un utilisateur de la bibliothèque.

use mega_core::{CoreError, Document, testing::sample_pdf};

fn fixture(name: &str) -> String {
    format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn opens_bytes_and_reads_metadata() {
    let doc = Document::from_bytes(sample_pdf(3, "Exemple"), None).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert_eq!(doc.title().as_deref(), Some("Exemple"));
    let s = doc.page_size(0).unwrap();
    assert!((s.width_pt - 595.0).abs() < 1.0 && (s.height_pt - 842.0).abs() < 1.0);
}

#[test]
fn open_rejects_non_pdf_bytes() {
    assert!(matches!(
        Document::from_bytes(b"pas un pdf".to_vec(), None),
        Err(CoreError::InvalidPdf(_))
    ));
}

#[test]
fn open_reports_missing_file() {
    assert!(matches!(
        Document::open("n/existe/pas.pdf", None),
        Err(CoreError::Io(_))
    ));
}

#[test]
fn render_out_of_range_page_fails() {
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    assert!(matches!(
        doc.render_page(5, 1.0),
        Err(CoreError::PageOutOfRange { index: 5, count: 1 })
    ));
}

#[test]
fn render_scale_controls_pixel_size() {
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    let size = doc.page_size(0).unwrap();
    let expected = (
        (size.width_pt * 2.0).round() as u32,
        (size.height_pt * 2.0).round() as u32,
    );
    let r = doc.render_page(0, 2.0).unwrap();
    assert_eq!((r.width_px, r.height_px), expected);
    assert_eq!(r.rgba.len(), (expected.0 * expected.1 * 4) as usize);
    let png = r.to_png().unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn roundtrip_to_bytes_keeps_pages() {
    let doc = Document::from_bytes(sample_pdf(2, "x"), None).unwrap();
    let again = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(again.page_count(), 2);
}

#[test]
fn open_encrypted_without_password_is_password_required() {
    let bytes = std::fs::read(fixture("encrypted.pdf")).unwrap();
    assert!(matches!(
        Document::from_bytes(bytes.clone(), None),
        Err(CoreError::PasswordRequired)
    ));
    assert_eq!(
        Document::from_bytes(bytes, Some("secret"))
            .unwrap()
            .page_count(),
        3
    );
}

#[test]
fn title_with_non_ascii_characters_roundtrips() {
    let doc = Document::from_bytes(sample_pdf(1, "Résumé — été"), None).unwrap();
    assert_eq!(doc.title().as_deref(), Some("Résumé — été"));
}

#[test]
fn io_error_message_mentions_the_cause_exactly_once() {
    let err = Document::open("n/existe/pas.pdf", None).unwrap_err();
    let full = mega_core::error::display_chain(&err);
    assert!(full.starts_with("Erreur d'accès au fichier"), "{full}");
    assert_eq!(full.matches("os error").count(), 1, "{full}");
}

#[test]
fn render_refuses_absurd_pixel_counts() {
    // Page de 14 400 × 14 400 pt (maximum du format) : 207 Mpx à l'échelle 1.
    let doc =
        Document::from_bytes(mega_core::testing::blank_pdf(14_400.0, 14_400.0), None).unwrap();
    assert!(matches!(
        doc.render_page(0, 1.0),
        Err(CoreError::RenderTooLarge { .. })
    ));
    // Une page A4 à l'échelle 8 (≈ 32 Mpx) reste acceptée.
    let a4 = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    assert!(a4.render_page(0, 8.0).is_ok());
}

#[test]
fn concurrent_first_use_is_safe() {
    // Régression macOS arm64 (CI du 2026-10-26) : plusieurs threads qui initialisaient
    // PDFium en même temps au premier appel faisaient planter le processus (SIGTRAP).
    // L'initialisation doit être strictement unique, les autres threads attendent.
    use std::sync::{Arc, Barrier};
    // Deuxième régression (CI Linux/macOS) : l'erreur du dernier chargement est globale
    // dans PDFium ; des chargements valides et invalides entrelacés transformaient
    // « fichier invalide » en « erreur inconnue ». Chaque thread fait donc les deux.
    let threads = 8;
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..5 {
                    let doc = Document::from_bytes(sample_pdf(1, "course"), None).unwrap();
                    assert!(doc.render_page(0, 1.0).unwrap().width_px > 0);
                    assert!(matches!(
                        Document::from_bytes(b"pas un pdf".to_vec(), None),
                        Err(CoreError::InvalidPdf(_))
                    ));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("un thread a échoué");
    }
}

#[test]
fn open_encrypted_with_wrong_password_is_wrong_password() {
    let bytes = std::fs::read(fixture("encrypted.pdf")).unwrap();
    assert!(matches!(
        Document::from_bytes(bytes, Some("faux")),
        Err(CoreError::WrongPassword)
    ));
}

#[test]
fn open_rejects_more_than_u16_max_pages() {
    let bytes = mega_core::testing::many_pages_pdf(65_536);
    assert!(matches!(
        Document::from_bytes(bytes, None),
        Err(CoreError::InvalidPdf(msg)) if msg.contains("65 535")
    ));
    // Juste sous la limite : accepté, et le compte est exact.
    let ok = Document::from_bytes(mega_core::testing::many_pages_pdf(65_535), None).unwrap();
    assert_eq!(ok.page_count(), 65_535);
}

// ---- Lot 1 : texte, recherche, signets, liens, propriétés, rotation ----

#[test]
fn text_boxes_read_page_label() {
    let doc = Document::from_bytes(sample_pdf(3, "x"), None).unwrap();
    let boxes = doc.text_boxes(1).unwrap();
    let text: String = boxes
        .iter()
        .map(|b| b.ch)
        .filter(|c| !c.is_whitespace())
        .collect();
    assert!(text.starts_with("Page2"), "{text}");
    assert!(boxes.iter().all(|b| b.rect.h > 0.0 && b.rect.w >= 0.0));
}

/// Texte attendu de `testing::out_of_order_pdf()`, dans l'ordre de lecture (correctif 0.12.2).
const READING_ORDER: &str = "Le 3 mars 2026\n\n\
Premiere ligne avec un mot GRAS au milieu\n\
Deuxieme ligne du paragraphe\n\
Troisieme ligne du paragraphe\n\n\
Utilisateur : ALICE\n\
Entreprise : B42\n\
Ville : Quebec\n\
Pays : Canada\n\
Telephone : 555-0100\n\n\
Gauche ligne un du texte\nGauche ligne deux du texte\nGauche ligne trois du texte\n\
Gauche ligne quatre du texte\nGauche ligne cinq du texte\n\n\
Droite ligne un du texte\nDroite ligne deux du texte\nDroite ligne trois du texte\n\
Droite ligne quatre du texte\nDroite ligne cinq du texte";

/// Correctif 0.12.2 : la couche texte suit l'ordre de lecture, pas l'ordre de dessin du fichier
/// (la sélection, une plage d'index, ramassait sinon des morceaux de toute la page).
#[test]
fn text_boxes_follow_reading_order() {
    let doc = Document::from_bytes(mega_core::testing::out_of_order_pdf(), None).unwrap();
    let text: String = doc.text_boxes(0).unwrap().iter().map(|b| b.ch).collect();
    assert!(text.starts_with("Le 3 mars 2026"), "{text}");
    // PDFium supprime l'espace de tête de « au milieu » (elle suit l'espace finale de « mot ») :
    // l'ordre compte ici ; la copie et `page_text` remettent l'espace d'après le blanc mesuré.
    assert!(text.contains("un mot GRAS"), "{text}");
    assert!(text.contains("au milieuDeuxieme"), "{text}");
    assert!(text.contains("Utilisateur :ALICEEntreprise :B42"), "{text}");
    assert!(text.contains("Telephone :555-0100Gauche"), "{text}");
    assert!(text.contains("cinq du texteDroite ligne un"), "{text}");
}

#[test]
fn page_text_follows_reading_order() {
    let doc = Document::from_bytes(mega_core::testing::out_of_order_pdf(), None).unwrap();
    assert_eq!(doc.page_text(0).unwrap(), READING_ORDER);
}

/// PDFium ajoute des sauts de ligne (`\r\n`) sans emprise : ils ne servent ni à la sélection
/// ni à la copie (qui recalcule les lignes) et faussaient le choix du caractère sous la souris.
#[test]
fn text_boxes_drop_generated_line_breaks() {
    let doc = Document::from_bytes(mega_core::testing::out_of_order_pdf(), None).unwrap();
    let boxes = doc.text_boxes(0).unwrap();
    assert!(!boxes.is_empty());
    assert!(boxes.iter().all(|b| b.ch != '\r' && b.ch != '\n'));
    assert!(boxes.iter().all(|b| b.rect.w > 0.0 && b.rect.h > 0.0));
}

/// Relecture (I5) : un bandeau vertical garde l'ordre de ses caractères au lieu d'être coupé
/// en morceaux de lignes horizontales.
#[test]
fn rotated_text_keeps_its_drawing_order() {
    let doc = Document::from_bytes(mega_core::testing::sidebar_pdf(), None).unwrap();
    let text = doc.page_text(0).unwrap();
    assert!(text.contains("Paragraphe horizontal"), "{text}");
    assert!(text.contains("arXiv:2310.0001"), "{text}");
    let boxes: String = doc.text_boxes(0).unwrap().iter().map(|b| b.ch).collect();
    assert!(boxes.contains("arXiv:2310.0001"), "{boxes}");
}

/// Relecture (I4, M4, M5) : chaque caractère porte son numéro de ligne et de bloc dans l'ordre
/// de lecture, pour que l'interface vise par bloc et copie comme l'export.
#[test]
fn text_boxes_carry_line_and_block_numbers() {
    let doc = Document::from_bytes(mega_core::testing::out_of_order_pdf(), None).unwrap();
    let boxes = doc.text_boxes(0).unwrap();
    assert!(
        boxes
            .windows(2)
            .all(|w| w[0].line <= w[1].line && w[0].block <= w[1].block)
    );
    let text: String = boxes.iter().map(|b| b.ch).collect();
    let at = |needle: &str| {
        text.find(needle)
            .map(|k| text[..k].chars().count())
            .unwrap()
    };
    // « Utilisateur : » et « ALICE » : même ligne ; la date est le premier bloc, seule.
    assert_eq!(boxes[at("Utilisateur")].line, boxes[at("ALICE")].line);
    assert_eq!(boxes[0].block, 0);
    assert_eq!(boxes[at("Premiere")].block, 1);
    let blocks = boxes.last().unwrap().block + 1;
    assert_eq!(blocks, 5, "date, paragraphe, liste, deux colonnes");
}

#[test]
fn text_boxes_empty_on_blank_page() {
    let doc = Document::from_bytes(mega_core::testing::blank_pdf(200.0, 200.0), None).unwrap();
    assert!(doc.text_boxes(0).unwrap().is_empty());
    let hits = doc
        .search_page(0, "rien", &mega_core::SearchOptions::default())
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn search_finds_each_page_once() {
    let doc = Document::from_bytes(sample_pdf(3, "x"), None).unwrap();
    let opts = mega_core::SearchOptions::default();
    for page in 0..3 {
        let hits = doc.search_page(page, "Page", &opts).unwrap();
        assert_eq!(hits.len(), 1, "page {page}");
        assert_eq!(hits[0].page, page);
        assert_eq!(hits[0].rects.len(), 1);
    }
}

#[test]
fn search_respects_case() {
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    let strict = mega_core::SearchOptions {
        match_case: true,
        whole_word: false,
    };
    assert!(doc.search_page(0, "page", &strict).unwrap().is_empty());
    assert_eq!(
        doc.search_page(0, "page", &mega_core::SearchOptions::default())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn outline_tree_has_child() {
    let doc = Document::from_bytes(mega_core::testing::outlined_pdf(), None).unwrap();
    let outline = doc.outline().unwrap();
    assert_eq!(outline.len(), 2);
    assert_eq!(outline[0].title, "Chapitre 1");
    assert_eq!(outline[0].page, Some(0));
    assert_eq!(outline[0].children.len(), 1);
    assert_eq!(outline[0].children[0].title, "Section 1.1");
    assert_eq!(outline[0].children[0].page, Some(1));
    assert_eq!(outline[1].page, Some(2));
    assert!(
        Document::from_bytes(sample_pdf(1, "x"), None)
            .unwrap()
            .outline()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn links_internal_and_uri() {
    use mega_core::LinkTarget;
    let doc = Document::from_bytes(mega_core::testing::outlined_pdf(), None).unwrap();
    let links = doc.links(0).unwrap();
    assert_eq!(links.len(), 2);
    assert!(
        links
            .iter()
            .any(|l| matches!(l.target, LinkTarget::Page(2)))
    );
    assert!(
        links
            .iter()
            .any(|l| matches!(&l.target, LinkTarget::Uri(u) if u == "https://example.org"))
    );
    assert!(links.iter().all(|l| l.rect.w > 0.0 && l.rect.h > 0.0));
    assert!(doc.links(1).unwrap().is_empty());
}

#[test]
fn properties_expose_title_and_version() {
    let doc = Document::from_bytes(sample_pdf(2, "Propriétés"), None).unwrap();
    let p = doc.properties().unwrap();
    assert_eq!(p.title.as_deref(), Some("Propriétés"));
    assert!(p.pdf_version.starts_with("1."), "{}", p.pdf_version);
    assert_eq!(p.page_count, 2);
    assert_eq!(p.producer.as_deref(), Some("MegaUltraPDF tests"));
}

#[test]
fn render_rotated_swaps_dimensions() {
    use mega_core::Rotation;
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    let upright = doc.render_page_with(0, 0.5, Rotation::None).unwrap();
    let turned = doc.render_page_with(0, 0.5, Rotation::Cw90).unwrap();
    assert!(upright.height_px > upright.width_px);
    assert_eq!(
        (turned.width_px, turned.height_px),
        (upright.height_px, upright.width_px)
    );
}

#[test]
fn outline_terminates_on_cycle_and_depth() {
    // Un PDF hostile : /Next qui boucle sur lui-même, et un enfant qui est son propre enfant.
    let doc = Document::from_bytes(mega_core::testing::cyclic_outline_pdf(), None).unwrap();
    let outline = doc.outline().unwrap();
    assert!(!outline.is_empty());
    assert!(outline.len() <= mega_core::outline::MAX_OUTLINE_NODES);
    fn depth(n: &mega_core::OutlineNode) -> usize {
        1 + n.children.iter().map(depth).max().unwrap_or(0)
    }
    assert!(outline.iter().map(depth).max().unwrap_or(0) <= mega_core::outline::MAX_OUTLINE_DEPTH);
}

/// A19 : les fixtures générées sont identiques d'une génération à l'autre (pas d'`/ID`
/// aléatoire), donc stables dans l'historique git.
#[test]
fn sample_pdf_is_deterministic() {
    let a = mega_core::testing::sample_pdf(2, "stable");
    let b = mega_core::testing::sample_pdf(2, "stable");
    assert!(a == b, "deux générations diffèrent");
}

/// C5 (audit du Lot 13) : la recherche suit l'**ordre de lecture**, comme la sélection : un mot
/// dessiné à part, une valeur dessinée après son libellé, une expression qui passe à la ligne
/// suivante sont trouvés ; chaque occurrence surligne un rectangle par ligne.
#[test]
fn search_follows_reading_order() {
    let doc = Document::from_bytes(mega_core::testing::out_of_order_pdf(), None).unwrap();
    let find = |q: &str, match_case: bool| {
        doc.search_page(
            0,
            q,
            &mega_core::SearchOptions {
                match_case,
                whole_word: false,
            },
        )
        .unwrap()
    };
    assert_eq!(
        find("mot GRAS au", true).len(),
        1,
        "fragment dessiné à part"
    );
    assert_eq!(
        find("Utilisateur : ALICE", true).len(),
        1,
        "valeur dessinée après"
    );
    let across = find("paragraphe Troisieme", true);
    assert_eq!(across.len(), 1, "expression sur deux lignes");
    assert_eq!(across[0].rects.len(), 2, "un rectangle par ligne");
    assert_eq!(find("alice", false).len(), 1, "sans casse");
    assert!(find("alice", true).is_empty(), "avec casse");
    assert_eq!(find("ligne", false).len(), 13);
}

/// C7 (audit du Lot 13) : des glyphes sans équivalent Unicode (police CID sans `/ToUnicode`)
/// **gardent leur place** dans la couche de texte : la sélection ne saute pas par-dessus.
/// L'audit craignait qu'ils soient écartés ; en fait PDFium prend leur code comme caractère
/// (ici « ģіމ »). Les remplacer par `U+FFFD` demanderait `FPDFText_HasUnicodeMapError`, que
/// pdfium-render 0.9.4 n'expose pas (limite documentée dans l'ADR-0018).
#[test]
fn glyphs_without_unicode_keep_their_place() {
    let doc =
        Document::from_bytes(mega_core::testing::hostile::cid_without_unicode_pdf(), None).unwrap();
    let boxes = doc.text_boxes(0).unwrap();
    let text: String = boxes.iter().map(|b| b.ch).collect();
    assert!(text.starts_with("Visible"), "{text:?}");
    assert_eq!(boxes.len(), "Visible".len() + 3, "{text:?}");
    assert!(
        boxes[7..].iter().all(|b| b.line != boxes[0].line),
        "{text:?}"
    );
}

/// Relecture de la PR B (critique) : des mots séparés par du crénage, sans caractère espace
/// (LaTeX), gardent leurs espaces dans la copie, l'export et la recherche.
#[test]
fn kerned_words_keep_their_spaces() {
    let doc = Document::from_bytes(mega_core::testing::kerned_pdf(), None).unwrap();
    assert_eq!(doc.page_text(0).unwrap().trim(), "Hello world foo bar");
    let boxes: String = doc.text_boxes(0).unwrap().iter().map(|b| b.ch).collect();
    assert_eq!(boxes.trim(), "Hello world foo bar");
    let find = |q: &str, whole_word: bool| {
        doc.search_page(
            0,
            q,
            &mega_core::SearchOptions {
                match_case: false,
                whole_word,
            },
        )
        .unwrap()
        .len()
    };
    assert_eq!(find("hello world", false), 1);
    assert_eq!(find("hello", true), 1);
    assert_eq!(find("world foo", true), 1);
}
