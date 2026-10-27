// SPDX-License-Identifier: GPL-3.0-or-later
//! Génération de PDF d'exemple pour les tests (feature `testing`).
//!
//! PDFium sait créer des pages et y écrire du texte, mais n'offre aucun moyen
//! d'écrire les métadonnées (titre, auteur…). On construit donc à la main un
//! squelette PDF minimal qui contient le titre, on le charge dans PDFium, on y
//! ajoute les pages, puis on sauvegarde : PDFium conserve le dictionnaire `/Info`.
//!
//! Ce module ne sert qu'aux tests : il a le droit de paniquer (ADR-0005).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use pdfium_render::prelude::{
    PdfPageObjectsCommon, PdfPagePaperSize, PdfPageTextObject, PdfPoints, PdfiumError,
};

use crate::engine;

/// Un PDF sans page, dont la seule information est le titre dans `/Info`.
///
/// \[Apprentissage\] Un fichier PDF est une suite d'objets numérotés (`1 0 obj … endobj`),
/// suivie d'une table `xref` donnant la position en octets de chaque objet, puis d'un
/// `trailer` qui indique l'objet racine (`/Root`) et les métadonnées (`/Info`). Les
/// positions doivent être exactes : on les calcule en construisant le fichier.
pub fn minimal_pdf_with_title(title: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [] /Count 0 >>".to_string(),
        format!(
            "<< /Title {} /Producer (MegaUltraPDF tests) >>",
            pdf_text_string(title)
        ),
    ];

    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let xref_offset = out.len();
    out.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    out.push_str("0000000000 65535 f \n");
    for off in offsets {
        out.push_str(&format!("{off:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        objects.len() + 1
    ));
    out.into_bytes()
}

/// Un PDF écrit à la main avec `n` pages vides (arbre `/Pages` plat), pour tester les
/// limites de comptage. Aucun contenu, aucune police : le fichier reste petit et rapide.
pub fn many_pages_pdf(n: u32) -> Vec<u8> {
    // Objets : 1 = Catalog, 2 = Pages, 3..=n+2 = pages.
    let mut objects: Vec<String> = Vec::with_capacity(n as usize + 2);
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_string());
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", i + 3)).collect();
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {n} >>",
        kids.join(" ")
    ));
    for _ in 0..n {
        objects.push("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string());
    }
    assemble_pdf(&objects, "<< /Producer (MegaUltraPDF tests) >>")
}

/// Un PDF de 3 pages A4 écrit à la main avec des **signets** (« Chapitre 1 » → p.1, contenant
/// « Section 1.1 » → p.2 ; « Chapitre 2 » → p.3) et, en page 1, deux **liens** : un interne
/// vers la page 3 et un externe vers `https://example.org`.
///
/// Objets : 1 Catalog, 2 Pages, 3-5 Page, 6 Font, 7 Outlines, 8-10 items, 11 contenu p1,
/// 12-13 annotations Link, 14-15 contenus p2-p3.
pub fn outlined_pdf() -> Vec<u8> {
    let page = |contents: u32, annots: &str| {
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
             /Resources << /Font << /F1 6 0 R >> >> /Contents {contents} 0 R{annots} >>"
        )
    };
    let stream = |text: &str| {
        let body = format!("BT /F1 24 Tf 72 760 Td ({text}) Tj ET");
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len())
    };
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 7 0 R /PageMode /UseOutlines >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>".to_string(),
        page(11, " /Annots [12 0 R 13 0 R]"),
        page(14, ""),
        page(15, ""),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /Outlines /First 8 0 R /Last 10 0 R /Count 3 >>".to_string(),
        "<< /Title (Chapitre 1) /Parent 7 0 R /Next 10 0 R /First 9 0 R /Last 9 0 R /Count 1 \
         /Dest [3 0 R /Fit] >>"
            .to_string(),
        "<< /Title (Section 1.1) /Parent 8 0 R /Dest [4 0 R /Fit] >>".to_string(),
        "<< /Title (Chapitre 2) /Parent 7 0 R /Prev 8 0 R /Dest [5 0 R /Fit] >>".to_string(),
        stream("Page 1"),
        "<< /Type /Annot /Subtype /Link /Rect [72 700 200 720] /Border [0 0 0] \
         /Dest [5 0 R /Fit] >>"
            .to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [72 650 200 670] /Border [0 0 0] \
         /A << /S /URI /URI (https://example.org) >> >>"
            .to_string(),
        stream("Page 2"),
        stream("Page 3"),
    ];
    assemble_pdf(
        &objects,
        &format!(
            "<< /Title {} /Producer (MegaUltraPDF tests) >>",
            pdf_text_string("Document avec signets")
        ),
    )
}

/// Un PDF d'une page A4 portant quatre annotations « tierces » (écrites à la main, comme
/// le ferait un autre logiciel) : une note `Text` (auteur, date), un `Highlight` avec
/// `/QuadPoints` et opacité, un `Square` avec un flux d'apparence `/AP`, et un `Underline`
/// **sans** couleur. Sert à tester la lecture et le rendu des annotations.
pub fn annotated_pdf() -> Vec<u8> {
    annotated_pdf_impl(true)
}

/// Le même document sans son tableau `/Annots` (pour comparer les rendus).
pub fn annotated_pdf_without_annots() -> Vec<u8> {
    annotated_pdf_impl(false)
}

fn annotated_pdf_impl(with_annots: bool) -> Vec<u8> {
    let annots = if with_annots {
        " /Annots [5 0 R 6 0 R 7 0 R 8 0 R]"
    } else {
        ""
    };
    let content = "BT /F1 24 Tf 72 700 Td (Texte annote) Tj ET";
    let appearance = "1 0 0 RG 0 0 1 rg 2 w 1 1 198 98 re B";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
             /Resources << /Font << /F1 10 0 R >> >> /Contents 4 0 R{annots} >>"
        ),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
        format!(
            "<< /Type /Annot /Subtype /Text /Rect [50 750 70 770] /F 4 /Contents {} /T (Alice) \
             /CreationDate (D:20261026120000Z) /M (D:20261026123000Z) /C [1 0.8 0] >>",
            pdf_text_string("Note de test")
        ),
        "<< /Type /Annot /Subtype /Highlight /Rect [72 700 200 720] /F 4 \
         /QuadPoints [72 720 200 720 72 700 200 700] /C [1 1 0] /CA 0.5 /T (Bob) >>"
            .to_string(),
        "<< /Type /Annot /Subtype /Square /Rect [100 500 300 600] /F 4 /C [1 0 0] /IC [0 0 1] \
         /AP << /N 9 0 R >> >>"
            .to_string(),
        "<< /Type /Annot /Subtype /Underline /Rect [72 640 200 660] /F 4 \
         /QuadPoints [72 660 200 660 72 640 200 640] >>"
            .to_string(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 200 100] /Length {} >>\nstream\n{appearance}\nendstream",
            appearance.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    assemble_pdf(
        &objects,
        &format!(
            "<< /Title {} /Producer (MegaUltraPDF tests) >>",
            pdf_text_string("Document annoté")
        ),
    )
}

/// Une page avec une annotation `Ink` **tierce** lissée en courbes de Bézier (comme
/// Acrobat ou Xournal l'écrivent) : `/InkList` présent et apparence `m c c S`.
pub fn bezier_ink_pdf() -> Vec<u8> {
    let appearance = "0 0 1 RG 2 w 100 100 m 120 160 160 160 180 100 c 200 40 240 40 260 100 c S";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Annots [4 0 R] >>".to_string(),
        "<< /Type /Annot /Subtype /Ink /Rect [98 38 262 162] /F 4 /C [0 0 1] /BS << /W 2 >> \
         /InkList [[100 100 140 150 180 100 220 50 260 100]] /AP << /N 5 0 R >> >>"
            .to_string(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [98 38 262 162] /Length {} >>\nstream\n{appearance}\nendstream",
            appearance.len()
        ),
    ];
    assemble_pdf(&objects, "<< /Producer (MegaUltraPDF tests) >>")
}

/// Un formulaire AcroForm écrit à la main (`/NeedAppearances true`) : texte « nom » (vide),
/// texte multiligne « adresse », case « ok », groupe radio « choix » (A coché, B), combo
/// « ville » (Paris, Lyon ; Paris choisi), texte lecture seule « ref », texte requis « email ».
pub fn form_pdf() -> Vec<u8> {
    form_pdf_impl(true)
}

/// Le même formulaire **sans** `/NeedAppearances` (PDFium ne régénère alors pas les
/// apparences après une écriture de valeur).
pub fn form_pdf_without_need_appearances() -> Vec<u8> {
    form_pdf_impl(false)
}

fn form_pdf_impl(need_appearances: bool) -> Vec<u8> {
    let na = if need_appearances {
        " /NeedAppearances true"
    } else {
        ""
    };
    let on = "0 g 2 w 4 10 m 8 5 l 16 16 l S";
    let off = "0 G 1 w 0.5 0.5 19 19 re S";
    let labels = "BT /Helv 12 Tf 72 730 Td (Nom :) Tj 0 -110 Td (Adresse :) Tj ET";
    let text = |name: &str, value: &str, rect: &str, flags: u32| {
        format!(
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /V ({value}) /Rect [{rect}] \
             /F 4 /Ff {flags} /DA (/Helv 12 Tf 0 g) /MK << /BC [0 0 0] >> >>"
        )
    };
    let objects = vec![
        format!(
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 7 0 R 8 0 R 11 0 R 12 0 R 13 0 R]{na} \
             /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 4 0 R >> >> >> >>"
        ),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /Helv 4 0 R >> >> \
         /Contents 16 0 R /Annots [5 0 R 6 0 R 7 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R] >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
        text("nom", "", "72 700 300 724", 0),
        text("adresse", "", "72 620 300 680", 4096),
        "<< /Type /Annot /Subtype /Widget /FT /Btn /T (ok) /V /Off /AS /Off /Rect [72 580 92 600] /F 4 \
         /MK << /BC [0 0 0] /CA (4) >> /AP << /N << /Yes 14 0 R /Off 15 0 R >> >> >>"
            .to_string(),
        "<< /FT /Btn /Ff 32768 /T (choix) /V /A /Kids [9 0 R 10 0 R] >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /Parent 8 0 R /AS /A /Rect [72 540 92 560] /F 4 \
         /MK << /BC [0 0 0] /CA (l) >> /AP << /N << /A 14 0 R /Off 15 0 R >> >> >>"
            .to_string(),
        "<< /Type /Annot /Subtype /Widget /Parent 8 0 R /AS /Off /Rect [100 540 120 560] /F 4 \
         /MK << /BC [0 0 0] /CA (l) >> /AP << /N << /B 14 0 R /Off 15 0 R >> >> >>"
            .to_string(),
        "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (ville) /Opt [(Paris) (Lyon)] /V (Paris) \
         /Rect [72 500 200 524] /F 4 /DA (/Helv 12 Tf 0 g) >>"
            .to_string(),
        text("ref", "REF-001", "72 460 200 484", 1),
        text("email", "", "72 420 300 444", 2),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length {} >>\nstream\n{on}\nendstream",
            on.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length {} >>\nstream\n{off}\nendstream",
            off.len()
        ),
        format!(
            "<< /Length {} >>\nstream\n{labels}\nendstream",
            labels.len()
        ),
    ];
    assemble_pdf(
        &objects,
        &format!(
            "<< /Title {} /Producer (MegaUltraPDF tests) >>",
            pdf_text_string("Formulaire")
        ),
    )
}

/// Un champ texte **séparé de son widget** (dictionnaire de champ parent + widget enfant),
/// comme en produisent iText ou LibreOffice : `/V` vit sur le parent.
pub fn form_parent_kid_pdf() -> Vec<u8> {
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] /NeedAppearances true \
         /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 4 0 R >> >> >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /Helv 4 0 R >> >> \
         /Annots [6 0 R] >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
        "<< /FT /Tx /T (nom) /V (ancien) /Ff 0 /DA (/Helv 12 Tf 0 g) /Kids [6 0 R] >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /Parent 5 0 R /Rect [72 700 300 724] /F 4 \
         /MK << /BC [0 0 0] >> >>"
            .to_string(),
    ];
    assemble_pdf(&objects, "<< /Producer (MegaUltraPDF tests) >>")
}

/// Une page avec un `/AcroForm` portant une clé `/XFA` : formulaire XFA (non pris en charge).
pub fn xfa_pdf() -> Vec<u8> {
    let xfa = "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\"/>";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [] /XFA 4 0 R >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        format!("<< /Length {} >>\nstream\n{xfa}\nendstream", xfa.len()),
    ];
    assemble_pdf(&objects, "<< /Producer (MegaUltraPDF tests) >>")
}

/// Un PDF **hostile** : signet dont `/Next` pointe sur lui-même, et signet dont le premier
/// enfant est lui-même. Sert à vérifier que la lecture des signets est bornée.
pub fn cyclic_outline_pdf() -> Vec<u8> {
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        "<< /Type /Outlines /First 5 0 R /Last 5 0 R /Count 2 >>".to_string(),
        // /Next sur lui-même : la chaîne des frères ne se termine jamais.
        "<< /Title (Boucle) /Parent 4 0 R /Next 5 0 R /First 6 0 R /Last 6 0 R /Count 1 \
         /Dest [3 0 R /Fit] >>"
            .to_string(),
        // /First sur lui-même : la descente ne se termine jamais.
        "<< /Title (Abîme) /Parent 5 0 R /First 6 0 R /Last 6 0 R /Count 1 /Dest [3 0 R /Fit] >>"
            .to_string(),
    ];
    assemble_pdf(&objects, "<< /Producer (MegaUltraPDF tests) >>")
}

/// Assemble des objets PDF numérotés à partir de 1, puis l'objet `/Info` en dernier,
/// avec une table `xref` aux positions exactes.
fn assemble_pdf(objects: &[String], info: &str) -> Vec<u8> {
    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let info_id = objects.len() + 1;
    offsets.push(out.len());
    out.push_str(&format!("{info_id} 0 obj\n{info}\nendobj\n"));
    let xref_offset = out.len();
    out.push_str(&format!("xref\n0 {}\n", info_id + 1));
    out.push_str("0000000000 65535 f \n");
    for off in offsets {
        out.push_str(&format!("{off:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R /Info {info_id} 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        info_id + 1
    ));
    out.into_bytes()
}

/// Encode une chaîne de texte PDF en hexadécimal UTF-16BE précédé du BOM `FEFF`.
///
/// Une chaîne littérale `(…)` sans BOM est lue en *PDFDocEncoding*, un encodage
/// 8 bits proche de Latin-1 : des octets UTF-8 y donneraient du charabia pour tout
/// caractère non ASCII. Le BOM UTF-16 lève l'ambiguïté (ISO 32000-1, §7.9.2.2).
fn pdf_text_string(text: &str) -> String {
    let mut hex = String::from("<FEFF");
    for unit in text.encode_utf16() {
        hex.push_str(&format!("{unit:04X}"));
    }
    hex.push('>');
    hex
}

/// Un PDF d'une page vierge de `width_pt` × `height_pt` points (pour tester les bornes).
///
/// # Panics
/// Si PDFium n'est pas disponible : ce générateur n'est utilisé que par des tests.
pub fn blank_pdf(width_pt: f32, height_pt: f32) -> Vec<u8> {
    let guard = engine::guard();
    let mut doc = engine::load_bytes(&guard, minimal_pdf_with_title("vierge"), None)
        .expect("PDFium doit être disponible et le squelette PDF valide");
    doc.pages_mut()
        .create_page_at_end(PdfPagePaperSize::Custom(
            PdfPoints::new(width_pt),
            PdfPoints::new(height_pt),
        ))
        .expect("création de page");
    doc.save_to_bytes().expect("sérialisation")
}

/// Un PDF A4 de `pages` pages, titré `title`, chaque page portant « Page N » en Helvetica.
///
/// # Panics
/// Si PDFium n'est pas disponible : ce générateur n'est utilisé que par des tests.
pub fn sample_pdf(pages: u16, title: &str) -> Vec<u8> {
    try_sample_pdf(pages, title).expect("génération du PDF d'exemple")
}

fn try_sample_pdf(pages: u16, title: &str) -> Result<Vec<u8>, PdfiumError> {
    let guard = engine::guard();
    let mut doc = engine::load_bytes(&guard, minimal_pdf_with_title(title), None)
        .expect("PDFium doit être disponible et le squelette PDF valide");
    let font = doc.fonts_mut().helvetica();
    for n in 1..=pages {
        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::a4())?;
        let mut text =
            PdfPageTextObject::new(&doc, format!("Page {n}"), font, PdfPoints::new(24.0))?;
        text.translate(PdfPoints::new(72.0), PdfPoints::new(760.0))?;
        page.objects_mut().add_text_object(text)?;
    }
    doc.save_to_bytes()
}
