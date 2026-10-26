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
