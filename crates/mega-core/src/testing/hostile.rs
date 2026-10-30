// SPDX-License-Identifier: GPL-3.0-or-later
//! PDF **malformés ou hostiles** pour l'audit de robustesse (Lot 13).
//!
//! Chaque générateur reproduit une classe de fichiers qu'on rencontre réellement : fichier
//! coupé par un téléchargement interrompu, table `xref` décalée par un outil maladroit,
//! fichier « hybride » ou mis à jour plusieurs fois, ou fichier fabriqué pour épuiser la
//! mémoire (bombe de décompression). `cargo xtask gen-fixtures` les écrit dans
//! `fixtures/robustesse/` ; `tests/robustness.rs` vérifie qu'aucun ne fait paniquer, figer ou
//! fermer l'application.
//!
//! \[Apprentissage\] On assemble ici des **octets** (`Vec<u8>`) et non des `String` : un flux
//! compressé contient des octets qui ne sont pas de l'UTF-8 valide. `extend_from_slice` ajoute
//! une tranche d'octets au bout du vecteur.

use super::sample_pdf;

/// Une page A4 avec le texte `label`, en Helvetica, objets numérotés de 1 à 5 :
/// catalogue, arbre des pages, page, contenu, police.
fn text_page_objects(label: &str, media_box: &str) -> Vec<Vec<u8>> {
    let content = format!("BT /F1 24 Tf 72 700 Td ({label}) Tj ET");
    vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [{media_box}] \
             /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
        )
        .into_bytes(),
        stream(b"", content.as_bytes()),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    ]
}

/// Un objet flux : dictionnaire `extra` (sans `/Length`), puis les octets `data`.
fn stream(extra: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = b"<< ".to_vec();
    out.extend_from_slice(extra);
    out.extend_from_slice(format!(" /Length {} >>\nstream\n", data.len()).as_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

/// Assemble les objets numérotés à partir de 1 derrière l'en-tête `header`, avec une table
/// `xref` dont chaque position est décalée de `shift` octets (0 : positions exactes) et
/// `trailer_extra` ajouté au dictionnaire de remorque. Rend le fichier et les positions
/// réelles des objets.
fn assemble(
    header: &str,
    objects: &[Vec<u8>],
    shift: usize,
    trailer_extra: &str,
) -> (Vec<u8>, Vec<usize>) {
    let mut out = format!("{header}\n").into_bytes();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for off in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", off + shift).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R {trailer_extra} >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    (out, offsets)
}

/// Fichier vide (0 octet) : un fichier créé mais jamais écrit.
pub fn empty_pdf() -> Vec<u8> {
    Vec::new()
}

/// Les premiers 50 % d'un PDF de 3 pages : téléchargement interrompu.
pub fn truncated_pdf() -> Vec<u8> {
    let mut bytes = sample_pdf(3, "Tronqué");
    bytes.truncate(bytes.len() / 2);
    bytes
}

/// 512 octets parasites avant `%PDF-` (en-tête de courriel, sortie d'un script) : toutes les
/// positions de la table `xref` sont décalées d'autant.
pub fn junk_prefix_pdf() -> Vec<u8> {
    let mut bytes = "Parasite avant l'en-tete.\n".repeat(20).into_bytes();
    bytes.truncate(512);
    let page = text_page_objects("Prefixe", "0 0 595 842");
    bytes.extend_from_slice(&assemble("%PDF-1.7", &page, 0, "").0);
    bytes
}

/// Table `xref` dont toutes les positions sont fausses de 7 octets : le lecteur doit
/// reconstruire la table en parcourant le fichier.
pub fn shifted_xref_pdf() -> Vec<u8> {
    let page = text_page_objects("Xref decalee", "0 0 595 842");
    assemble("%PDF-1.7", &page, 7, "").0
}

/// L'objet de contenu (objet 4) n'a pas son mot-clé `endobj`.
pub fn missing_endobj_pdf() -> Vec<u8> {
    let page = text_page_objects("Sans endobj", "0 0 595 842");
    let (bytes, _) = assemble("%PDF-1.7", &page, 0, "");
    let marker = b"endstream\nendobj\n";
    match bytes.windows(marker.len()).position(|w| w == marker) {
        Some(k) => {
            let keep = k + b"endstream\n".len();
            let mut out = bytes[..keep].to_vec();
            out.extend_from_slice(&bytes[k + marker.len()..]);
            out
        }
        None => bytes,
    }
}

/// En-tête `%PDF-2.0` (ISO 32000-2) sur une page ordinaire.
pub fn pdf20_pdf() -> Vec<u8> {
    assemble(
        "%PDF-2.0",
        &text_page_objects("PDF 2.0", "0 0 595 842"),
        0,
        "",
    )
    .0
}

/// Page de 14 400 × 14 400 pt, la plus grande que la norme admette (annexe C).
pub fn huge_media_box_pdf() -> Vec<u8> {
    assemble(
        "%PDF-1.7",
        &text_page_objects("Immense", "0 0 14400 14400"),
        0,
        "",
    )
    .0
}

/// Fichier **hybride** (§ 7.5.8.4) : une table `xref` classique, et un flux de références
/// croisées désigné par `/XRefStm` qui seul connaît la police, rangée dans un flux d'objets.
/// Produit par Word et d'anciens Acrobat pour rester lisible par les lecteurs PDF 1.4.
pub fn hybrid_xref_pdf() -> Vec<u8> {
    let mut objects = text_page_objects("Hybride", "0 0 595 842");
    // La police (objet 5) ne vit que dans le flux d'objets 6.
    let mut objstm = b"5 0 ".to_vec();
    let first = objstm.len();
    objstm.extend_from_slice(&objects[4]);
    objects[4] = b"null".to_vec();
    objects.push(stream(
        format!("/Type /ObjStm /N 1 /First {first}").as_bytes(),
        &objstm,
    ));
    // Flux de références croisées 7 : l'objet 5 est de type 2 (dans un flux d'objets), flux 6,
    // rang 0.
    objects.push(stream(
        b"/Type /XRef /Size 8 /W [1 2 1] /Index [5 1]",
        &[2, 0, 6, 0],
    ));
    // Toutes les substitutions gardent la même longueur : les positions restent justes.
    let (draft, offsets) = assemble("%PDF-1.5", &objects, 0, "/XRefStm 0000000000");
    let text = String::from_utf8(draft).expect("fichier ASCII");
    let entry5 = format!("{:010} 00000 n \n", offsets[4]);
    text.replacen(&entry5, "0000000000 00001 f \n", 1)
        .replacen(
            "/XRefStm 0000000000",
            &format!("/XRefStm {:010}", offsets[6]),
            1,
        )
        .into_bytes()
}

/// Trois révisions : la page est créée (« Revision 1 »), puis deux **mises à jour
/// incrémentales** remplacent son contenu, chacune avec sa table `xref` et un `/Prev` vers la
/// précédente (§ 7.5.6). Le texte affiché doit être celui de la dernière révision.
pub fn revisions_pdf() -> Vec<u8> {
    let page = text_page_objects("Revision 1", "0 0 595 842");
    let (mut out, _) = assemble("%PDF-1.7", &page, 0, "");
    let mut prev = startxref_of(&out);
    for n in 2..=3 {
        let content = format!("BT /F1 24 Tf 72 700 Td (Revision {n}) Tj ET");
        let offset = out.len();
        out.extend_from_slice(b"4 0 obj\n");
        out.extend_from_slice(&stream(b"", content.as_bytes()));
        out.extend_from_slice(b"\nendobj\n");
        let xref = out.len();
        out.extend_from_slice(
            format!(
                "xref\n4 1\n{offset:010} 00000 n \ntrailer\n<< /Size 6 /Root 1 0 R /Prev {prev} >>\n\
                 startxref\n{xref}\n%%EOF\n"
            )
            .as_bytes(),
        );
        prev = xref;
    }
    out
}

/// Position annoncée par le dernier `startxref` du fichier.
fn startxref_of(bytes: &[u8]) -> usize {
    let text = String::from_utf8_lossy(bytes);
    text.rfind("startxref\n")
        .and_then(|k| text[k + "startxref\n".len()..].lines().next())
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// Une chaîne de `depth` formulaires XObject imbriqués (chacun dessine le suivant) ; le
/// dernier peint une image 1 × 1. Sans limite de profondeur, un parcours récursif déborde la
/// pile et ferme l'application.
pub fn deep_xobjects_pdf(depth: usize) -> Vec<u8> {
    let mut objects = text_page_objects("Profond", "0 0 595 842");
    objects[2] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
                   /Resources << /Font << /F1 5 0 R >> /XObject << /X 6 0 R >> >> \
                   /Contents 4 0 R >>"
        .to_vec();
    objects[3] = stream(
        b"",
        b"BT /F1 24 Tf 72 700 Td (Profond) Tj ET q 100 0 0 100 72 500 cm /X Do Q",
    );
    // Objets 6 à 5 + depth : les formulaires ; 6 + depth : l'image.
    let image = 6 + depth;
    for k in 0..depth {
        let target = if k + 1 == depth { image } else { 7 + k };
        objects.push(stream(
            format!(
                "/Type /XObject /Subtype /Form /BBox [0 0 1 1] \
                 /Resources << /XObject << /X {target} 0 R >> >>"
            )
            .as_bytes(),
            b"/X Do",
        ));
    }
    objects.push(stream(
        b"/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray \
          /BitsPerComponent 8",
        &[128],
    ));
    assemble("%PDF-1.7", &objects, 0, "").0
}

/// Taille décompressée des bombes : au-delà de la limite par flux de `mega-core` (256 Mio).
pub const BOMB_SIZE: usize = 300 * 1024 * 1024;

/// **Bombe de décompression** : le contenu de la page et le flux XMP `/Metadata` se
/// décompressent chacun en 300 Mio d'espaces, pour quelques centaines de Ko dans le fichier.
pub fn flate_bomb_pdf() -> Vec<u8> {
    let mut objects = text_page_objects("Bombe", "0 0 595 842");
    let mut content = b"BT /F1 24 Tf 72 700 Td (Bombe) Tj ET".to_vec();
    content.resize(BOMB_SIZE, b' ');
    objects[3] = stream(b"/Filter /FlateDecode", &deflate(content));
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R /Metadata 6 0 R >>".to_vec();
    objects.push(stream(
        b"/Type /Metadata /Subtype /XML /Filter /FlateDecode",
        &deflate(vec![b' '; BOMB_SIZE]),
    ));
    assemble("%PDF-1.7", &objects, 0, "").0
}

/// Compresse `data` au format zlib (`/FlateDecode`) avec lopdf.
fn deflate(data: Vec<u8>) -> Vec<u8> {
    let mut s = lopdf::Stream::new(lopdf::Dictionary::new(), data);
    s.compress().expect("compression zlib");
    s.content
}

/// Image JPEG dont l'en-tête annonce 60 000 × 60 000 pixels (10,8 Go une fois décodée) alors
/// que le dictionnaire de l'image dit 8 × 8 : un décodeur qui ne vérifie l'en-tête qu'après
/// avoir décodé tente l'allocation.
pub fn jpeg_bomb_pdf() -> Vec<u8> {
    let mut jpeg = Vec::new();
    let pixels = image::RgbImage::from_pixel(8, 8, image::Rgb([200, 30, 30]));
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode_image(&pixels)
        .expect("encodage JPEG");
    // Marqueur SOF0 (FF C0) : longueur (2 octets), précision (1), hauteur (2), largeur (2).
    let sof = jpeg
        .windows(2)
        .position(|w| w == [0xFF, 0xC0])
        .expect("JPEG de base");
    jpeg[sof + 5..sof + 9].copy_from_slice(&[0xEA, 0x60, 0xEA, 0x60]);
    let mut objects = text_page_objects("Image piegee", "0 0 595 842");
    objects[2] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
                   /Resources << /Font << /F1 5 0 R >> /XObject << /Im 6 0 R >> >> \
                   /Contents 4 0 R >>"
        .to_vec();
    objects[3] = stream(
        b"",
        b"BT /F1 24 Tf 72 700 Td (Image piegee) Tj ET q 200 0 0 200 72 400 cm /Im Do Q",
    );
    objects.push(stream(
        b"/Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceRGB \
          /BitsPerComponent 8 /Filter /DCTDecode",
        &jpeg,
    ));
    assemble("%PDF-1.7", &objects, 0, "").0
}

/// Un document chiffré **par certificat** (`/Filter /Adobe.PubSec`, § 7.6.5) : seul le
/// détenteur de la clé privée d'un destinataire peut l'ouvrir ; aucun mot de passe ne sert.
/// L'application doit le dire, au lieu de demander un mot de passe en boucle. Le contenu
/// n'est pas réellement chiffré : seul compte le dictionnaire `/Encrypt`.
pub fn pubsec_pdf() -> Vec<u8> {
    let mut objects = text_page_objects("Certificat", "0 0 595 842");
    objects.push(
        b"<< /Filter /Adobe.PubSec /SubFilter /adbe.pkcs7.s5 /V 4 /Length 128 \
          /CF << /DefaultCryptFilter << /CFM /AESV2 /AuthEvent /DocOpen \
          /Recipients [<3082000130>] >> >> /StmF /DefaultCryptFilter /StrF /DefaultCryptFilter >>"
            .to_vec(),
    );
    assemble(
        "%PDF-1.7",
        &objects,
        0,
        "/Encrypt 6 0 R /ID [<0123456789ABCDEF0123456789ABCDEF> <0123456789ABCDEF0123456789ABCDEF>]",
    )
    .0
}

/// Tout le corpus généré ici, sous les noms de fichier de `fixtures/robustesse/`. Le fichier
/// linéarisé est produit à part par qpdf (`cargo xtask gen-fixtures`).
pub fn corpus() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("vide.pdf", empty_pdf()),
        ("tronque.pdf", truncated_pdf()),
        ("prefixe-parasite.pdf", junk_prefix_pdf()),
        ("xref-decalee.pdf", shifted_xref_pdf()),
        ("sans-endobj.pdf", missing_endobj_pdf()),
        ("pdf-2-0.pdf", pdf20_pdf()),
        ("page-immense.pdf", huge_media_box_pdf()),
        ("xref-hybride.pdf", hybrid_xref_pdf()),
        ("revisions.pdf", revisions_pdf()),
        ("xobjects-profonds.pdf", deep_xobjects_pdf(5000)),
        ("bombe-flate.pdf", flate_bomb_pdf()),
        ("jpeg-piege.pdf", jpeg_bomb_pdf()),
    ]
}
