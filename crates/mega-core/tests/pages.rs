// SPDX-License-Identifier: GPL-3.0-or-later
//! Opérations de pages : rotation enregistrée, suppression, insertion, déplacement,
//! extraction, fusion, scission.

use mega_core::testing::{outlined_pdf, sample_pdf};
use mega_core::{CoreError, Document, PageSize, Rotation};

/// Texte d'une page sans espaces (« Page2 »), pour vérifier l'ordre des pages.
fn label(doc: &Document, page: u16) -> String {
    doc.text_boxes(page)
        .unwrap()
        .iter()
        .map(|b| b.ch)
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn labels(doc: &Document) -> Vec<String> {
    (0..doc.page_count()).map(|i| label(doc, i)).collect()
}

fn three() -> Document {
    Document::from_bytes(sample_pdf(3, "trois"), None).unwrap()
}

#[test]
fn rotate_pages_is_persisted() {
    let mut doc = three();
    doc.rotate_pages(&[1], 1).unwrap();
    assert_eq!(doc.page_rotation(1).unwrap(), Rotation::Cw90);
    assert_eq!(doc.page_rotation(0).unwrap(), Rotation::None);
    let reloaded = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(reloaded.page_rotation(1).unwrap(), Rotation::Cw90);
    // Un quart de tour enregistré échange largeur et hauteur de la page rendue.
    let r = reloaded.render_page(1, 0.5).unwrap();
    assert!(r.width_px > r.height_px);
    // Tourner de trois quarts de plus revient au nord ; les tours négatifs marchent aussi.
    doc.rotate_pages(&[1], 3).unwrap();
    assert_eq!(doc.page_rotation(1).unwrap(), Rotation::None);
    doc.rotate_pages(&[0], -1).unwrap();
    assert_eq!(doc.page_rotation(0).unwrap(), Rotation::Cw270);
}

#[test]
fn delete_pages_keeps_others_in_order() {
    let mut doc = three();
    doc.delete_pages(&[1]).unwrap();
    assert_eq!(doc.page_count(), 2);
    assert_eq!(labels(&doc), vec!["Page1", "Page3"]);
    assert!(matches!(
        doc.delete_pages(&[7]),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

#[test]
fn delete_all_pages_is_refused() {
    let mut doc = three();
    assert!(matches!(
        doc.delete_pages(&[0, 1, 2]),
        Err(CoreError::InvalidOperation(_))
    ));
    assert_eq!(doc.page_count(), 3);
}

#[test]
fn insert_blank_page_has_requested_size() {
    let mut doc = three();
    let size = PageSize {
        width_pt: 300.0,
        height_pt: 400.0,
    };
    doc.insert_blank_page(1, size).unwrap();
    assert_eq!(doc.page_count(), 4);
    let s = doc.page_size(1).unwrap();
    assert!((s.width_pt - 300.0).abs() < 0.5 && (s.height_pt - 400.0).abs() < 0.5);
    assert!(doc.text_boxes(1).unwrap().is_empty());
    assert_eq!(labels(&doc), vec!["Page1", "", "Page2", "Page3"]);
    // Insertion en fin de document.
    doc.insert_blank_page(4, size).unwrap();
    assert_eq!(doc.page_count(), 5);
}

#[test]
fn insert_pages_from_another_document() {
    let mut doc = three();
    let other = Document::from_bytes(outlined_pdf(), None).unwrap();
    doc.insert_pages_from(&other, &[2, 0], 0).unwrap();
    assert_eq!(doc.page_count(), 5);
    // Les pages insérées gardent l'ordre demandé.
    assert_eq!(
        labels(&doc),
        vec!["Page3", "Page1", "Page1", "Page2", "Page3"]
    );
    assert_eq!(other.page_count(), 3, "la source n'est pas modifiée");
}

#[test]
fn move_pages_reorders() {
    let mut doc = three();
    doc.move_pages(&[2], 0).unwrap();
    assert_eq!(labels(&doc), vec!["Page3", "Page1", "Page2"]);
    // `to` est l'index d'insertion une fois les pages déplacées retirées.
    doc.move_pages(&[0], 2).unwrap();
    assert_eq!(labels(&doc), vec!["Page1", "Page2", "Page3"]);
    doc.move_pages(&[0, 1], 1).unwrap();
    assert_eq!(labels(&doc), vec!["Page3", "Page1", "Page2"]);
    assert!(matches!(
        doc.move_pages(&[0], 9),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

#[test]
fn extract_pages_creates_a_new_document() {
    let doc = three();
    // L'ordre demandé est respecté (comme pour l'insertion depuis un autre document).
    let bytes = doc.extract_pages(&[2, 0]).unwrap();
    let extracted = Document::from_bytes(bytes, None).unwrap();
    assert_eq!(labels(&extracted), vec!["Page3", "Page1"]);
    assert!(doc.extract_pages(&[]).is_err());
    assert_eq!(doc.page_count(), 3);
}

#[test]
fn merge_concatenates_documents() {
    let a = three();
    let b = Document::from_bytes(outlined_pdf(), None).unwrap();
    let merged = Document::from_bytes(mega_core::pages::merge(&[&a, &b]).unwrap(), None).unwrap();
    assert_eq!(merged.page_count(), 6);
    assert_eq!(label(&merged, 3), "Page1");
}

#[test]
fn split_by_parts() {
    let doc = three();
    let parts = mega_core::pages::split(&doc, &[vec![0, 1], vec![2]]).unwrap();
    assert_eq!(parts.len(), 2);
    let first = Document::from_bytes(parts[0].clone(), None).unwrap();
    let second = Document::from_bytes(parts[1].clone(), None).unwrap();
    assert_eq!(labels(&first), vec!["Page1", "Page2"]);
    assert_eq!(labels(&second), vec!["Page3"]);
    assert!(mega_core::pages::split(&doc, &[]).is_err());
}

// ---- Passe de correction après relecture (Lot 2) ----

use mega_core::pages::{SplitMode, plan_split, split_to_dir};

/// I6 : la règle de scission vit dans le cœur (CLI et application l'appellent).
#[test]
fn plan_split_by_ranges_every_and_bookmarks() {
    let doc = Document::from_bytes(sample_pdf(5, "plan"), None).unwrap();
    assert_eq!(
        plan_split(
            &doc,
            &SplitMode::Ranges {
                spec: "1-2,4-".into()
            }
        )
        .unwrap(),
        vec![vec![0, 1], vec![3, 4]]
    );
    assert_eq!(
        plan_split(&doc, &SplitMode::Every { n: 2 }).unwrap(),
        vec![vec![0, 1], vec![2, 3], vec![4]]
    );
    // Signets de premier niveau en pages 1 et 3 → [1-2] et [3].
    let outlined = Document::from_bytes(outlined_pdf(), None).unwrap();
    assert_eq!(
        plan_split(&outlined, &SplitMode::Bookmarks).unwrap(),
        vec![vec![0, 1], vec![2]]
    );
}

#[test]
fn plan_split_rejects_bad_input() {
    let doc = Document::from_bytes(sample_pdf(5, "plan"), None).unwrap();
    assert!(matches!(
        plan_split(&doc, &SplitMode::Ranges { spec: "9".into() }),
        Err(CoreError::InvalidPageSpec(_))
    ));
    assert!(matches!(
        plan_split(&doc, &SplitMode::Every { n: 0 }),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        plan_split(&doc, &SplitMode::Bookmarks),
        Err(CoreError::InvalidOperation(_))
    ));
}

/// I3 : scinder n'écrase rien sans `overwrite`, et refuse un préfixe qui sort du dossier.
#[test]
fn split_to_dir_refuses_existing_outputs_unless_forced() {
    let doc = three();
    let dir = tempfile::tempdir().unwrap();
    let parts = vec![vec![0u16], vec![1, 2]];
    let files = split_to_dir(&doc, &parts, dir.path(), "part", false).unwrap();
    assert_eq!(files.len(), 2);
    assert!(dir.path().join("part-2.pdf").exists());
    // Deuxième passage : refus avant toute écriture, les fichiers existants sont intacts.
    std::fs::write(dir.path().join("part-1.pdf"), b"modifie").unwrap();
    let err = split_to_dir(&doc, &parts, dir.path(), "part", false).unwrap_err();
    assert!(matches!(err, CoreError::InvalidOperation(ref m) if m.contains("existe déjà")));
    assert_eq!(
        std::fs::read(dir.path().join("part-1.pdf")).unwrap(),
        b"modifie"
    );
    // Forcé : réécrit.
    split_to_dir(&doc, &parts, dir.path(), "part", true).unwrap();
    let rewritten = std::fs::read(dir.path().join("part-1.pdf")).unwrap();
    assert_eq!(
        Document::from_bytes(rewritten, None).unwrap().page_count(),
        1
    );
    for bad in ["../x", "a/b", "a\\b", ""] {
        assert!(
            matches!(
                split_to_dir(&doc, &parts, dir.path(), bad, true),
                Err(CoreError::InvalidOperation(_))
            ),
            "préfixe refusé : {bad:?}"
        );
    }
}

/// `sample_pdf` réécrit par `lopdf` avec l'en-tête `version` (PDFium ne sait écrire que 1.0 à 1.7).
fn sample_with_header(version: &str, catalog_version: Option<&str>) -> Vec<u8> {
    let mut doc = lopdf::Document::load_mem(&sample_pdf(3, "t")).unwrap();
    doc.version = version.into();
    if let Some(v) = catalog_version {
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_object_mut(root).unwrap().as_dict_mut().unwrap();
        catalog.set("Version", lopdf::Object::Name(v.as_bytes().to_vec()));
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn rotate_merge_extract_and_protect_keep_the_2_0_header() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = Document::from_bytes(sample_with_header("2.0", None), None).unwrap();
    doc.rotate_pages(&[0], 1).unwrap();
    let path = dir.path().join("rotated.pdf");
    doc.save_as(&path).unwrap();
    assert!(
        std::fs::read(&path).unwrap().starts_with(b"%PDF-2.0"),
        "save_as"
    );

    // Une seconde édition part des octets enregistrés : la version reste.
    doc.rotate_pages(&[1], 1).unwrap();
    doc.save_as(&path).unwrap();
    assert!(
        std::fs::read(&path).unwrap().starts_with(b"%PDF-2.0"),
        "save_as bis"
    );

    assert!(
        doc.extract_pages(&[0, 1]).unwrap().starts_with(b"%PDF-2.0"),
        "extract"
    );

    let old = Document::from_bytes(sample_with_header("1.5", None), None).unwrap();
    let new = Document::from_bytes(sample_with_header("2.0", None), None).unwrap();
    assert!(
        mega_core::pages::merge(&[&old, &new])
            .unwrap()
            .starts_with(b"%PDF-2.0"),
        "merge"
    );
    assert!(
        mega_core::pages::merge(&[&old, &old])
            .unwrap()
            .starts_with(b"%PDF-1."),
        "merge 1.x"
    );

    let mut protected = Document::from_bytes(sample_with_header("2.0", None), None).unwrap();
    protected
        .protect(&mega_core::Protection {
            user_password: "u".to_string(),
            owner_password: "o".to_string(),
            permissions: mega_core::Permissions {
                print: true,
                copy: true,
                modify: true,
                annotate: true,
            },
        })
        .unwrap();
    let protected_path = dir.path().join("protected.pdf");
    protected.save_as(&protected_path).unwrap();
    assert!(
        std::fs::read(&protected_path)
            .unwrap()
            .starts_with(b"%PDF-2.0"),
        "protect"
    );
}

#[test]
fn catalog_version_entry_survives_a_rewrite() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = Document::from_bytes(sample_with_header("1.4", Some("1.7")), None).unwrap();
    doc.rotate_pages(&[0], 1).unwrap();
    let path = dir.path().join("v.pdf");
    doc.save_as(&path).unwrap();
    let out = std::fs::read(&path).unwrap();
    assert!(out.starts_with(b"%PDF-1.4"));
    let reread = lopdf::Document::load_mem(&out).unwrap();
    let root = reread.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let catalog = reread.get_object(root).unwrap().as_dict().unwrap();
    assert_eq!(catalog.get(b"Version").unwrap().as_name().unwrap(), b"1.7");
}

// --- Report du catalogue (Lot 14, audit E3) -------------------------------------------------

use mega_core::links::LinkTarget;
use mega_core::pages::merge;
use mega_core::testing::{form_parent_kid_pdf, rich_catalog_pdf};

fn rich() -> Document {
    Document::from_bytes(rich_catalog_pdf("1.7", None), None).unwrap()
}

/// Nom et valeur de chaque champ de formulaire, toutes pages confondues.
fn fields(doc: &Document) -> Vec<(String, Option<String>)> {
    (0..doc.page_count())
        .flat_map(|p| doc.form_fields(p).unwrap())
        .map(|f| (f.name, f.value))
        .collect()
}

#[test]
fn merging_two_rich_catalogs_keeps_bookmarks_attachments_fields_and_links() {
    let (a, b) = (rich(), rich());
    let merged = Document::from_bytes(merge(&[&a, &b]).unwrap(), None).unwrap();
    assert_eq!(merged.page_count(), 6);
    let outline = merged.outline().unwrap();
    let targets: Vec<(&str, Option<u16>)> =
        outline.iter().map(|n| (n.title.as_str(), n.page)).collect();
    assert_eq!(
        targets,
        [
            ("Vers chap2", Some(1)),
            ("Vers chap3", Some(2)),
            ("Direct p3", Some(2)),
            ("Vers chap2", Some(4)),
            ("Vers chap3", Some(5)),
            ("Direct p3", Some(5)),
        ]
    );
    let names: Vec<String> = merged
        .attachments()
        .unwrap()
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(names, ["notes (2).txt", "notes.txt"]);
    let names: Vec<String> = fields(&merged).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["nom", "nom (2)"]);
    // Les liens de chaque source visent leurs propres pages, même par un nom partagé.
    for (page, expected) in [(0, [1, 2, 2]), (3, [4, 5, 5])] {
        let links: Vec<LinkTarget> = merged
            .links(page)
            .unwrap()
            .into_iter()
            .map(|l| l.target)
            .collect();
        assert_eq!(
            links,
            expected.map(LinkTarget::Page),
            "liens de la page {page}"
        );
    }
}

#[test]
fn extracting_a_page_keeps_attachments_and_its_bookmark() {
    let bytes = rich().extract_pages(&[1]).unwrap();
    let part = Document::from_bytes(bytes, None).unwrap();
    assert_eq!(part.attachments().unwrap().len(), 1);
    let outline = part.outline().unwrap();
    assert_eq!(outline.len(), 1);
    assert_eq!(
        (outline[0].title.as_str(), outline[0].page),
        ("Vers chap2", Some(0))
    );
}

#[test]
fn a_merged_parent_kid_form_keeps_its_field() {
    // [E20d] : sans report, le `/Parent` du widget copié désignait un flux de contenu.
    let form = Document::from_bytes(form_parent_kid_pdf(), None).unwrap();
    let simple = Document::open(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/simple.pdf"),
        None,
    )
    .unwrap();
    let merged = Document::from_bytes(merge(&[&simple, &form]).unwrap(), None).unwrap();
    assert_eq!(
        fields(&merged),
        [("nom".to_string(), Some("ancien".to_string()))]
    );
}

#[test]
fn extracting_from_a_password_protected_source_carries_its_catalog() {
    let mut doc = rich();
    doc.protect(&mega_core::security::Protection {
        user_password: "u".to_string(),
        owner_password: "o".to_string(),
        permissions: mega_core::Permissions {
            print: true,
            copy: true,
            modify: true,
            annotate: true,
        },
    })
    .unwrap();
    // Protection non enregistrée (état PDFium), puis fichier enregistré rouvert.
    let saved = doc.to_bytes().unwrap();
    let reopened = Document::from_bytes(saved, Some("u")).unwrap();
    for source in [&doc, &reopened] {
        let part = Document::from_bytes(source.extract_pages(&[1]).unwrap(), None).unwrap();
        assert_eq!(part.attachments().unwrap().len(), 1);
        assert_eq!(part.outline().unwrap().len(), 1);
    }
}

#[test]
fn attachments_still_encrypted_in_an_identity_file_are_not_carried() {
    // `/EFF` : la pièce jointe resterait chiffrée dans un document neuf qui ne l'est pas.
    let doc = Document::from_bytes(
        mega_core::testing::attachments_only_encrypted_with_file_pdf(),
        None,
    )
    .unwrap();
    let part = Document::from_bytes(doc.extract_pages(&[0]).unwrap(), None).unwrap();
    assert!(part.attachments().unwrap().is_empty());
}

// --- Déplacement par l'arbre des pages (Lot 14, audit E4) -------------------------------------

#[test]
fn moving_a_page_keeps_its_links_tagging_and_the_bookmarks_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = rich();
    doc.move_pages(&[0], 2).unwrap();
    let path = dir.path().join("deplacee.pdf");
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    let reopened = Document::from_bytes(saved.clone(), None).unwrap();
    assert_eq!(labels(&reopened), ["Page2", "Page3", "Page1"]);
    // La page déplacée reste le même objet : ses liens, son widget et ce qui la vise suivent.
    let links: Vec<LinkTarget> = reopened
        .links(2)
        .unwrap()
        .into_iter()
        .map(|l| l.target)
        .collect();
    assert_eq!(
        links,
        [
            LinkTarget::Page(0),
            LinkTarget::Page(1),
            LinkTarget::Page(1)
        ]
    );
    assert_eq!(reopened.annotations(2).unwrap().len(), 4);
    let names: Vec<String> = fields(&reopened).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["nom"], "le widget reste dans /AcroForm /Fields");
    let outline: Vec<(String, Option<u16>)> = reopened
        .outline()
        .unwrap()
        .into_iter()
        .map(|n| (n.title, n.page))
        .collect();
    assert_eq!(
        outline,
        [
            ("Vers chap2".to_string(), Some(0)),
            ("Vers chap3".to_string(), Some(1)),
            ("Direct p3".to_string(), Some(1)),
        ]
    );
    // Balisage : la page garde sa clé dans l'arbre de structure et son contenu marqué.
    let lo = lopdf::Document::load_mem(&saved).unwrap();
    let moved = lo.get_pages()[&3];
    let page = lo.get_dictionary(moved).unwrap();
    assert_eq!(page.get(b"StructParents").unwrap().as_i64().unwrap(), 0);
    let content = lo.get_page_content(moved);
    assert!(content.windows(3).any(|w| w == b"BDC"));
}

#[test]
fn moving_a_page_in_an_aes256_document_keeps_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let source = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/encrypted.pdf");
    let mut doc = Document::open(source, Some("secret")).unwrap();
    let first = doc.page_text(0).unwrap();
    doc.move_pages(&[0], 2).unwrap();
    let path = dir.path().join("chiffre.pdf");
    doc.save_as(&path).unwrap();
    let reopened = Document::open(&path, Some("secret")).unwrap();
    assert_eq!(reopened.page_count(), 3);
    assert_eq!(reopened.page_text(2).unwrap(), first);
    assert!(matches!(
        Document::open(&path, None),
        Err(CoreError::PasswordRequired)
    ));
}

#[test]
fn a_refused_move_leaves_the_document_unchanged() {
    // Un déplacement refusé ne doit pas marquer le document modifié : l'enregistrement
    // réécrirait sinon le fichier (et romprait une signature).
    let dir = tempfile::tempdir().unwrap();
    let source = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/simple.pdf");
    let input = std::fs::read(source).unwrap();
    let mut doc = Document::open(source, None).unwrap();
    let count = doc.page_count();
    assert!(matches!(
        doc.move_pages(&[0], count),
        Err(CoreError::PageOutOfRange { .. })
    ));
    let path = dir.path().join("intact.pdf");
    doc.save_as(&path).unwrap();
    assert!(std::fs::read(&path).unwrap() == input, "octets identiques");
}

/// Relecture finale du Lot 14 (M3) : un déplacement sans effet (la sélection est déjà à sa
/// place) ne modifie rien : pas d'étape d'historique, et l'enregistrement recopie les octets
/// du fichier tels quels (une signature resterait intacte).
#[test]
fn a_move_that_changes_nothing_leaves_the_document_unmodified() {
    let original = sample_pdf(3, "immobile");
    let dir = tempfile::tempdir().unwrap();

    // Appel direct (chemin du CLI).
    let mut doc = Document::from_bytes(original.clone(), None).unwrap();
    doc.move_pages(&[1], 1).unwrap();
    let out = dir.path().join("direct.pdf");
    doc.save_as(&out).unwrap();
    assert!(std::fs::read(&out).unwrap() == original, "fichier réécrit");

    // Par l'historique (chemin de l'interface).
    let mut doc = Document::from_bytes(original.clone(), None).unwrap();
    doc.apply(&mega_core::EditCommand::MovePages {
        pages: vec![0, 1],
        to: 0,
    })
    .unwrap();
    assert!(!doc.is_modified());
    assert!(!doc.can_undo(), "aucune étape d'historique");
    let out = dir.path().join("historique.pdf");
    doc.save_as(&out).unwrap();
    assert!(std::fs::read(&out).unwrap() == original, "fichier réécrit");
}
