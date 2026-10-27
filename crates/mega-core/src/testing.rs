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

/// Une page **tournée** : MediaBox 400 × 600 avec `/Rotate 90` (affichée 600 × 400), le
/// texte « Page 1 » en haut à gauche de l'espace utilisateur, et un champ texte `nom`
/// (`/Rect [50 500 250 530]`). Sert à vérifier la conversion espace affiché ↔ utilisateur.
pub fn rotated_pdf() -> Vec<u8> {
    let content = "BT /Helv 24 Tf 72 560 Td (Page 1) Tj ET";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [6 0 R] /NeedAppearances true          /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 4 0 R >> >> >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /Rotate 90          /Resources << /Font << /Helv 4 0 R >> >> /Contents 5 0 R /Annots [6 0 R] >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
        format!(
            "<< /Length {} >>
stream
{content}
endstream",
            content.len()
        ),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (nom) /V () /Rect [50 500 250 530] /F 4          /DA (/Helv 12 Tf 0 g) /MK << /BC [0 0 0] >> >>"
            .to_string(),
    ];
    assemble_pdf(
        &objects,
        &format!(
            "<< /Title {} /Producer (MegaUltraPDF tests) >>",
            pdf_text_string("Page tournée")
        ),
    )
}

/// Une page A4 « riche » pour le caviardage : deux lignes de texte (« Secret 123 » en
/// (72, 700), « Public » en (72, 650)), une image 2 × 2 posée en (300, 680) sur 100 × 50,
/// un chemin bleu en (72, 500) et une annotation `Square` tierce qui couvre la première ligne.
pub fn rich_pdf() -> Vec<u8> {
    let content = "BT /Helv 24 Tf 72 700 Td (Secret 123) Tj ET                    BT /Helv 24 Tf 72 650 Td (Public) Tj ET                    q 100 0 0 50 300 680 cm /Im1 Do Q                    0 0 1 rg 72 500 100 40 re f";
    let pixels = "00FF FF00>";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842]          /Resources << /Font << /Helv 4 0 R >> /XObject << /Im1 6 0 R >> >>          /Contents 5 0 R /Annots [7 0 R] >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
        format!(
            "<< /Length {} >>
stream
{content}
endstream",
            content.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray              /BitsPerComponent 8 /Filter /ASCIIHexDecode /Length {} >>
stream
{pixels}
endstream",
            pixels.len()
        ),
        "<< /Type /Annot /Subtype /Square /Rect [60 690 250 730] /F 4 /C [1 0 0] >>".to_string(),
    ];
    assemble_pdf(
        &objects,
        &format!(
            "<< /Title {} /Producer (MegaUltraPDF tests) >>",
            pdf_text_string("Document riche")
        ),
    )
}

/// Une page dont le seul texte (« Cache ») est dessiné **à travers un formulaire XObject**
/// (`/Fx1 Do`), comme le produit un aplatissement de formulaire ou une page importée : le
/// caviardage par objets ne peut pas l'atteindre et doit le signaler.
pub fn xobject_text_pdf() -> Vec<u8> {
    let content = "q /Fx1 Do Q";
    let form = "BT /Helv 24 Tf 72 700 Td (Cache) Tj ET";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
         /Resources << /XObject << /Fx1 5 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 595 842] \
             /Resources << /Font << /Helv 6 0 R >> >> /Length {} >>\nstream\n{form}\nendstream",
            form.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_string(),
    ];
    assemble_pdf(
        &objects,
        &format!(
            "<< /Title {} /Producer (MegaUltraPDF tests) >>",
            pdf_text_string("Texte en XObject")
        ),
    )
}

/// Contenus que la régénération de flux de PDFium **perd ou casse** (relecture pdf-expert) :
/// `kind` vaut `"type3"` (texte en police Type 3), `"inline"` (image en ligne `BI … EI`) ou
/// `"shading"` (ombrage `sh`). Chaque page porte aussi un texte Helvetica ordinaire.
///
/// # Panics
/// Si `kind` n'est pas l'un des trois noms (générateur de tests).
pub fn fragile_content_pdf(kind: &str) -> Vec<u8> {
    let (content, extra_resource, extra_objects): (String, String, Vec<String>) = match kind {
        "type3" => (
            "BT /Helv 24 Tf 72 700 Td (Texte) Tj ET BT /T3 48 Tf 72 600 Td (a) Tj ET".to_string(),
            "/Font << /Helv 4 0 R /T3 6 0 R >>".to_string(),
            vec![
                "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 750 750] \
                 /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /square 7 0 R >> \
                 /Encoding << /Type /Encoding /Differences [97 /square] >> \
                 /FirstChar 97 /LastChar 97 /Widths [1000] /Resources << >> >>"
                    .to_string(),
                {
                    let glyph = "1000 0 0 0 750 750 d1 0 0 750 750 re f";
                    format!("<< /Length {} >>\nstream\n{glyph}\nendstream", glyph.len())
                },
            ],
        ),
        "inline" => (
            "BT /Helv 24 Tf 72 700 Td (Texte) Tj ET \
             q 100 0 0 50 300 680 cm BI /W 2 /H 2 /CS /G /BPC 8 /F /AHx ID 00FFFF00> EI Q"
                .to_string(),
            "/Font << /Helv 4 0 R >>".to_string(),
            vec![],
        ),
        "shading" => (
            "BT /Helv 24 Tf 72 700 Td (Texte) Tj ET q 72 500 200 50 re W n /Sh1 sh Q".to_string(),
            "/Font << /Helv 4 0 R >> /Shading << /Sh1 << /ShadingType 2 /ColorSpace /DeviceRGB \
             /Coords [72 0 272 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] \
             /C1 [0 0 1] /N 1 >> /Extend [true true] >> >>"
                .to_string(),
            vec![],
        ),
        other => panic!("contenu fragile inconnu : {other}"),
    };
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << {extra_resource} >> \
             /Contents 5 0 R >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_string(),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
    ];
    objects.extend(extra_objects);
    assemble_pdf(
        &objects,
        &format!(
            "<< /Title {} /Producer (MegaUltraPDF tests) >>",
            pdf_text_string("Contenu fragile")
        ),
    )
}

/// Une page A4 avec trois images XObject, construite avec `lopdf` : une image RGB 800 × 600
/// presque unie (200, 30, 30, bruitée) compressée Flate (candidate à la recompression JPEG), une image grise
/// 300 × 200 déjà en JPEG, et une petite image RGB avec `/SMask` (à laisser intacte). Les
/// trois sont dessinées : la rouge en (50, 400) sur 400 × 300, la grise en (50, 100) sur
/// 300 × 200, la masquée en (400, 100) sur 100 × 100.
///
/// # Panics
/// Si lopdf ou `image` échouent sur des données que nous venons de produire (tests).
pub fn heavy_images_pdf() -> Vec<u8> {
    use lopdf::{Dictionary, Object, Stream};
    let mut doc = lopdf::Document::with_version("1.7");
    let image_dict = |w: i64, h: i64, cs: &str| {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"XObject".to_vec()));
        d.set("Subtype", Object::Name(b"Image".to_vec()));
        d.set("Width", w);
        d.set("Height", h);
        d.set("ColorSpace", Object::Name(cs.as_bytes().to_vec()));
        d.set("BitsPerComponent", 8);
        d
    };
    // 1. RGB « presque unie » : (200, 30, 30) avec un léger bruit (±8) que Flate compresse
    //    mal et que le JPEG lisse, comme une photo. Générateur congruentiel, reproductible.
    let mut seed: u32 = 12_345;
    let mut noise = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((seed >> 24) % 17) as i16 - 8
    };
    let pixels: Vec<u8> = (0..800 * 600)
        .flat_map(|_| [200i16, 30, 30].map(|c| (c + noise()).clamp(0, 255) as u8))
        .collect();
    let mut red = Stream::new(image_dict(800, 600, "DeviceRGB"), pixels);
    red.compress().expect("compression Flate");
    let red_id = doc.add_object(red);
    // 2. Grise, JPEG.
    let gray = image::GrayImage::from_fn(300, 200, |x, _| image::Luma([(x % 256) as u8]));
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
        .encode_image(&gray)
        .expect("encodage JPEG");
    let mut gray_dict = image_dict(300, 200, "DeviceGray");
    gray_dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    let gray_id = doc.add_object(Stream::new(gray_dict, jpeg.into_inner()).with_compression(false));
    // 3. Petite RGB avec masque alpha.
    let smask = Stream::new(image_dict(8, 8, "DeviceGray"), vec![128u8; 64]);
    let smask_id = doc.add_object(smask);
    let mut masked_dict = image_dict(8, 8, "DeviceRGB");
    masked_dict.set("SMask", Object::Reference(smask_id));
    let masked_id = doc.add_object(Stream::new(masked_dict, [0u8, 0, 255].repeat(64)));
    let content = "q 400 0 0 300 50 400 cm /ImR Do Q q 300 0 0 200 50 100 cm /ImG Do Q \
                   q 100 0 0 100 400 100 cm /ImM Do Q";
    let content_id = doc.add_object(Stream::new(Dictionary::new(), content.as_bytes().to_vec()));
    let mut xobjects = Dictionary::new();
    xobjects.set("ImR", Object::Reference(red_id));
    xobjects.set("ImG", Object::Reference(gray_id));
    xobjects.set("ImM", Object::Reference(masked_id));
    let mut resources = Dictionary::new();
    resources.set("XObject", Object::Dictionary(xobjects));
    let pages_id = doc.new_object_id();
    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set("MediaBox", vec![0.into(), 0.into(), 595.into(), 842.into()]);
    page.set("Resources", Object::Dictionary(resources));
    page.set("Contents", Object::Reference(content_id));
    let page_id = doc.add_object(page);
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", vec![Object::Reference(page_id)]);
    pages.set("Count", 1);
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("sérialisation lopdf");
    out
}

/// Une page avec deux **pièces jointes** (`/Names /EmbeddedFiles`, §7.11.4) écrites à la
/// main : `notes.txt` (« Bonjour monde », 13 octets) et `données.csv` (20 octets, nom non
/// ASCII dans `/UF` en UTF-16BE). Le texte de la page dit « Pièces jointes ».
pub fn attachments_pdf() -> Vec<u8> {
    let notes = "Bonjour monde";
    let csv = "a,b\n1,2\n3,4\n5,6\n7,8";
    let content = "BT /Helv 18 Tf 72 760 Td (Pieces jointes) Tj ET";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles << /Names [(donnees.csv) 8 0 R (notes.txt) 6 0 R] >> >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
         /Resources << /Font << /Helv 4 0 R >> >> /Contents 5 0 R >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_string(),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
        format!(
            "<< /Type /Filespec /F (notes.txt) /UF {} /EF << /F 7 0 R >> >>",
            pdf_text_string("notes.txt")
        ),
        format!(
            "<< /Type /EmbeddedFile /Subtype /text#2Fplain /Length {} /Params << /Size {} >> >>\nstream\n{notes}\nendstream",
            notes.len(),
            notes.len()
        ),
        format!(
            "<< /Type /Filespec /F (donnees.csv) /UF {} /EF << /F 9 0 R >> >>",
            pdf_text_string("données.csv")
        ),
        format!(
            "<< /Type /EmbeddedFile /Subtype /text#2Fcsv /Length {} /Params << /Size {} >> >>\nstream\n{csv}\nendstream",
            csv.len(),
            csv.len()
        ),
    ];
    assemble_pdf(
        &objects,
        &format!(
            "<< /Title {} /Producer (MegaUltraPDF tests) >>",
            pdf_text_string("Pièces jointes")
        ),
    )
}

/// Images que l'optimisation ne doit **pas** toucher (relecture pdf-expert) : une photo RGB
/// 400 × 300 bruitée avec un masque `/SMask` gris 400 × 300 en dégradé (le masque, image grise
/// 8 bits sans masque, ressemble à une candidate), une image RGB JPEG avec `/DecodeParms
/// << /ColorTransform 0 >>`, et un flux XMP `/Metadata` non compressé dans le catalogue
/// (comme l'exige PDF/A-1).
///
/// # Panics
/// Si lopdf ou `image` échouent sur des données que nous venons de produire (tests).
pub fn tricky_images_pdf() -> Vec<u8> {
    use lopdf::{Dictionary, Object, Stream};
    let mut doc = lopdf::Document::with_version("1.7");
    let image_dict = |w: i64, h: i64, cs: &str| {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"XObject".to_vec()));
        d.set("Subtype", Object::Name(b"Image".to_vec()));
        d.set("Width", w);
        d.set("Height", h);
        d.set("ColorSpace", Object::Name(cs.as_bytes().to_vec()));
        d.set("BitsPerComponent", 8);
        d
    };
    let mut seed: u32 = 777;
    let mut noise = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((seed >> 24) % 33) as i16 - 16
    };
    // Masque : dégradé horizontal (Flate le compresse mal une fois bruité).
    let mask_pixels: Vec<u8> = (0..300)
        .flat_map(|_| (0..400).map(|x| (x * 255 / 399) as u8))
        .map(|v| (i16::from(v) + noise() / 4).clamp(0, 255) as u8)
        .collect();
    let mut mask = Stream::new(image_dict(400, 300, "DeviceGray"), mask_pixels);
    mask.compress().expect("compression Flate");
    let mask_id = doc.add_object(mask);
    let photo_pixels: Vec<u8> = (0..400 * 300)
        .flat_map(|_| [120i16, 160, 90].map(|c| (c + noise()).clamp(0, 255) as u8))
        .collect();
    let mut photo_dict = image_dict(400, 300, "DeviceRGB");
    photo_dict.set("SMask", Object::Reference(mask_id));
    let mut photo = Stream::new(photo_dict, photo_pixels);
    photo.compress().expect("compression Flate");
    let photo_id = doc.add_object(photo);
    // JPEG RGB avec /DecodeParms /ColorTransform 0.
    let rgb = image::RgbImage::from_fn(300, 200, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
        .encode_image(&rgb)
        .expect("encodage JPEG");
    let mut dct_dict = image_dict(300, 200, "DeviceRGB");
    dct_dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    let mut parms = Dictionary::new();
    parms.set("ColorTransform", 0);
    dct_dict.set("DecodeParms", Object::Dictionary(parms));
    let dct_id = doc.add_object(Stream::new(dct_dict, jpeg.into_inner()).with_compression(false));
    // XMP non compressé.
    let xmp = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/"><pdfaid:part>1</pdfaid:part></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end="w"?>"#;
    let mut xmp_dict = Dictionary::new();
    xmp_dict.set("Type", Object::Name(b"Metadata".to_vec()));
    xmp_dict.set("Subtype", Object::Name(b"XML".to_vec()));
    let xmp_id =
        doc.add_object(Stream::new(xmp_dict, xmp.as_bytes().to_vec()).with_compression(false));
    let content = "q 400 0 0 300 50 450 cm /ImP Do Q q 300 0 0 200 50 150 cm /ImJ Do Q";
    let content_id = doc.add_object(Stream::new(Dictionary::new(), content.as_bytes().to_vec()));
    let mut xobjects = Dictionary::new();
    xobjects.set("ImP", Object::Reference(photo_id));
    xobjects.set("ImJ", Object::Reference(dct_id));
    let mut resources = Dictionary::new();
    resources.set("XObject", Object::Dictionary(xobjects));
    let pages_id = doc.new_object_id();
    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set("MediaBox", vec![0.into(), 0.into(), 595.into(), 842.into()]);
    page.set("Resources", Object::Dictionary(resources));
    page.set("Contents", Object::Reference(content_id));
    let page_id = doc.add_object(page);
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", vec![Object::Reference(page_id)]);
    pages.set("Count", 1);
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    catalog.set("Metadata", Object::Reference(xmp_id));
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("sérialisation lopdf");
    out
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

/// Un PDF **hostile** pour la lecture des signatures : un champ `/FT /Sig` dont `/Kids`
/// le cite deux fois lui-même, et que `/Fields` cite trois fois. Sans mémoire des nœuds
/// visités, le parcours ferait des milliards d'appels.
pub fn cyclic_signature_fields_pdf() -> Vec<u8> {
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 4 0 R 4 0 R] >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        "<< /FT /Sig /T (Boucle) /V 5 0 R /Kids [4 0 R 4 0 R] >>".to_string(),
        "<< /Type /Sig /SubFilter /adbe.pkcs7.detached /ByteRange [0 1 2 3] /Contents <00> >>"
            .to_string(),
    ];
    assemble_pdf(&objects, "<< /Producer (MegaUltraPDF tests) >>")
}

/// Un document **certifié** (`/Perms /DocMDP`) dont les modifications permises valent `p`
/// (1 : aucune ; 2 : remplir les formulaires et signer ; 3 : aussi annoter). La signature
/// de certification elle-même est factice : seul le niveau de permission compte ici.
pub fn certified_pdf(p: i64) -> Vec<u8> {
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /Perms << /DocMDP 4 0 R >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        format!(
            "<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached \
             /ByteRange [0 0 0 0] /Contents <00> /Reference [<< /Type /SigRef \
             /TransformMethod /DocMDP /TransformParams << /Type /TransformParams /P {p} \
             /V /1.2 >> >>] >>"
        ),
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

/// Un PDF « numérisé » de 3 pages A4 : chaque page n'est qu'une image en niveaux de gris
/// (rendu à 150 dpi de [`sample_pdf`], « Page N »), sans aucun texte extractible. Sert aux
/// tests de l'OCR (Lot 8) et à `fixtures/scanned.pdf`.
///
/// # Panics
/// Si PDFium n'est pas disponible : ce générateur n'est utilisé que par des tests.
pub fn scanned_pdf() -> Vec<u8> {
    use lopdf::{Dictionary, Document as LoDocument, Object, Stream};

    let source =
        crate::Document::from_bytes(sample_pdf(3, "numérisé"), None).expect("PDF d'exemple valide");
    let mut doc = LoDocument::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for index in 0..source.page_count() {
        let size = source.page_size(index).expect("taille de page");
        let image = source
            .render_page(index, 150.0 / 72.0)
            .expect("rendu de la page");
        // RGBA → gris (moyenne pondérée de la luminance).
        let gray: Vec<u8> = image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|px| {
                let l =
                    0.299 * f32::from(px[0]) + 0.587 * f32::from(px[1]) + 0.114 * f32::from(px[2]);
                l.round().clamp(0.0, 255.0) as u8
            })
            .collect();
        let mut picture = Stream::new(
            Dictionary::from_iter(vec![
                ("Type", Object::Name(b"XObject".to_vec())),
                ("Subtype", Object::Name(b"Image".to_vec())),
                ("Width", Object::Integer(i64::from(image.width_px))),
                ("Height", Object::Integer(i64::from(image.height_px))),
                ("ColorSpace", Object::Name(b"DeviceGray".to_vec())),
                ("BitsPerComponent", Object::Integer(8)),
            ]),
            gray,
        );
        picture.compress().expect("compression de l'image");
        let picture_id = doc.add_object(picture);
        let content = format!(
            "q\n{} 0 0 {} 0 0 cm\n/Im0 Do\nQ\n",
            size.width_pt, size.height_pt
        );
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
        let page_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Page".to_vec())),
            ("Parent", Object::Reference(pages_id)),
            (
                "MediaBox",
                Object::Array(vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Real(size.width_pt),
                    Object::Real(size.height_pt),
                ]),
            ),
            (
                "Resources",
                Object::Dictionary(Dictionary::from_iter(vec![(
                    "XObject",
                    Object::Dictionary(Dictionary::from_iter(vec![(
                        "Im0",
                        Object::Reference(picture_id),
                    )])),
                )])),
            ),
            ("Contents", Object::Reference(content_id)),
        ]));
        kids.push(Object::Reference(page_id));
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Pages".to_vec())),
            ("Kids", Object::Array(kids)),
            ("Count", Object::Integer(count)),
        ])),
    );
    let catalog_id = doc.add_object(Dictionary::from_iter(vec![
        ("Type", Object::Name(b"Catalog".to_vec())),
        ("Pages", Object::Reference(pages_id)),
    ]));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("sérialisation");
    out
}
