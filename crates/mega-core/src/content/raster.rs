// SPDX-License-Identifier: GPL-3.0-or-later
//! Caviardage **d'une partie d'image** (Lot 10, ADR-0014) : passe `lopdf` qui suit le caviardage
//! par objets de PDFium.
//!
//! Un cache noir posé sur une image laisse ses pixels dans le fichier : n'importe quel outil
//! d'extraction les retrouve. Ici, chaque image dessinée sur la page qui touche la zone est :
//! - **retirée** si la zone la couvre entièrement ;
//! - sinon **remplacée** par une copie dont les pixels sous la zone sont noirs (et opaques
//!   dans son masque `/SMask`), écrite comme un **nouvel** objet : l'image d'origine peut être
//!   partagée par d'autres pages, qui la gardent intacte ; si plus rien ne la cite,
//!   l'élagage de l'enregistrement la retire du fichier ;
//! - retirée aussi quand on ne sait pas la décoder (JPEG 2000, fax, 16 bits…) : en matière de
//!   caviardage, en enlever trop vaut mieux que pas assez.
//!
//! Pourquoi `lopdf` et pas PDFium : pdfium-render ne lit que les pixels « bruts » d'une image,
//! sans son masque de transparence ni son espace de couleur ; réécrire l'image par PDFium
//! rendrait opaques les parties transparentes d'un logo. Le flux `lopdf` garde l'espace de
//! couleur, le masque et tout le reste du dictionnaire d'origine.
//!
//! \[Apprentissage\] **Matrices affines** : une image se dessine dans le carré unité
//! `[0, 1] × [0, 1]`, que la matrice courante (`cm`, la « CTM ») emmène sur la page. Pour savoir
//! quels pixels tombent sous la zone, on fait le chemin inverse : on **inverse** la matrice
//! (voir [`invert`]) et on ramène les coins de la zone dans le carré unité, puis en pixels.

use std::collections::BTreeSet;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId, Stream};

use crate::error::{CoreError, CoreResult};
use crate::geometry::Rect;

/// Ce que la passe a fait des images qui touchent la zone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RasterReport {
    /// Images retirées entières (couvertes, ou impossibles à décoder).
    pub removed: u16,
    /// Images remplacées par une copie noircie sous la zone.
    pub redacted: u16,
}

/// Matrice affine PDF `[a b c d e f]` : `(x, y) ↦ (a x + c y + e, b x + d y + f)` (§8.3.3).
type Matrix = [f32; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Côté maximal d'une image que l'on accepte de décoder (au-delà : retirée entière).
const MAX_SIDE: i64 = 20_000;
/// Nombre maximal de pixels d'une image décodée (au-delà : retirée entière).
const MAX_PIXELS: i64 = 60_000_000;

fn structure(e: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidOperation(format!("caviardage des images : {e}"))
}

/// `a` puis `b` : la matrice qui applique `a`, puis `b` (convention PDF `a × b`).
fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// Image du point `(x, y)` par `m`.
fn apply(m: &Matrix, x: f32, y: f32) -> (f32, f32) {
    (x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5])
}

/// Inverse de `m`, ou `None` si elle écrase le plan (déterminant nul : image invisible).
fn invert(m: &Matrix) -> Option<Matrix> {
    let det = m[0] * m[3] - m[1] * m[2];
    if !det.is_finite() || det.abs() < 1e-9 {
        return None;
    }
    Some([
        m[3] / det,
        -m[1] / det,
        -m[2] / det,
        m[0] / det,
        (m[2] * m[5] - m[3] * m[4]) / det,
        (m[1] * m[4] - m[0] * m[5]) / det,
    ])
}

/// Boîte englobante (espace utilisateur) du carré unité transformé par `m`.
fn bounds_of(m: &Matrix) -> Rect {
    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].map(|(x, y)| apply(m, x, y));
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

fn intersects(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && outer.x + outer.w >= inner.x + inner.w
        && outer.y + outer.h >= inner.y + inner.h
}

/// Pixels d'une image `w × h` (rangée 0 en haut) que couvre `zone` (espace utilisateur),
/// l'image étant dessinée par la matrice `m` : `[x0, y0, x1, y1]` (bornes hautes exclues),
/// arrondi **vers l'extérieur** plus un pixel de marge (le lissage du rendu déborde), borné à
/// l'image ; `None` si la zone ne touche aucun pixel.
pub(crate) fn pixel_zone(m: &Matrix, zone: &Rect, w: u32, h: u32) -> Option<[u32; 4]> {
    let inv = invert(m)?;
    let corners = [
        (zone.x, zone.y),
        (zone.x + zone.w, zone.y),
        (zone.x, zone.y + zone.h),
        (zone.x + zone.w, zone.y + zone.h),
    ];
    let (wf, hf) = (w as f32, h as f32);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        let (u, v) = apply(&inv, x, y);
        // Carré unité → pixels : la rangée 0 est en haut de l'image (v = 1).
        let (px, py) = (u * wf, (1.0 - v) * hf);
        x0 = x0.min(px);
        y0 = y0.min(py);
        x1 = x1.max(px);
        y1 = y1.max(py);
    }
    let clamp = |v: f32, max: f32| v.clamp(0.0, max) as u32;
    let zone = [
        clamp(x0.floor() - 1.0, wf),
        clamp(y0.floor() - 1.0, hf),
        clamp(x1.ceil() + 1.0, wf),
        clamp(y1.ceil() + 1.0, hf),
    ];
    (zone[0] < zone[2] && zone[1] < zone[3]).then_some(zone)
}

/// Les échantillons 8 bits d'une image, rangée par rangée.
struct Raster {
    samples: Vec<u8>,
    width: u32,
    channels: usize,
}

impl Raster {
    /// Donne la valeur `value` (un échantillon par composante) aux pixels de `zone`.
    fn fill(&mut self, zone: [u32; 4], value: &[u8]) {
        let [x0, y0, x1, y1] = zone;
        let stride = self.width as usize * self.channels;
        for row in y0 as usize..y1 as usize {
            for col in x0 as usize..x1 as usize {
                let at = row * stride + col * self.channels;
                if let Some(pixel) = self.samples.get_mut(at..at + self.channels) {
                    pixel.copy_from_slice(value);
                }
            }
        }
    }
}

fn name_of(object: &Object) -> Option<&[u8]> {
    object.as_name().ok()
}

/// Nombre de composantes de l'espace de couleur `space` si nous savons noircir ses pixels :
/// `DeviceGray`, `DeviceRGB`, `DeviceCMYK`, ou `ICCBased` à 1, 3 ou 4 composantes.
fn channels_of(doc: &LoDocument, space: &Object) -> Option<usize> {
    let space = doc.dereference(space).ok()?.1;
    match space {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"G" => Some(1),
            b"DeviceRGB" | b"RGB" => Some(3),
            b"DeviceCMYK" | b"CMYK" => Some(4),
            _ => None,
        },
        Object::Array(items) if name_of(items.first()?) == Some(b"ICCBased") => {
            let profile = doc.dereference(items.get(1)?).ok()?.1.as_stream().ok()?;
            match profile.dict.get(b"N").ok()?.as_i64().ok()? {
                n @ (1 | 3 | 4) => usize::try_from(n).ok(),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Décode une image 8 bits en échantillons, ou `None` (format que nous ne savons pas
/// réécrire fidèlement : l'appelant la retire alors entière).
fn decode(doc: &LoDocument, stream: &Stream) -> Option<Raster> {
    let dict = &stream.dict;
    let width = dict.get(b"Width").ok()?.as_i64().ok()?;
    let height = dict.get(b"Height").ok()?.as_i64().ok()?;
    if !(1..=MAX_SIDE).contains(&width)
        || !(1..=MAX_SIDE).contains(&height)
        || width * height > MAX_PIXELS
    {
        return None;
    }
    // Masque de pochoir, tableau `/Decode` (couleurs inversées) ou masque par couleur
    // (`/Mask`) : le noir n'y serait pas forcément du noir. Image retirée entière.
    let image_mask = dict
        .get(b"ImageMask")
        .ok()
        .and_then(|v| v.as_bool().ok())
        .unwrap_or(false);
    if image_mask || dict.has(b"Decode") || dict.has(b"Mask") {
        return None;
    }
    if dict.get(b"BitsPerComponent").ok()?.as_i64().ok()? != 8 {
        return None;
    }
    let channels = channels_of(doc, dict.get(b"ColorSpace").ok()?)?;
    let (width, height) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    let expected = width as usize * height as usize * channels;
    let filters: Vec<&[u8]> = match dict.get(b"Filter") {
        Err(_) => Vec::new(),
        Ok(Object::Name(n)) => vec![n.as_slice()],
        Ok(Object::Array(items)) => items.iter().filter_map(name_of).collect(),
        Ok(_) => return None,
    };
    let samples = match filters.as_slice() {
        [] => stream.content.clone(),
        [b"DCTDecode"] if channels != 4 => {
            let decoded =
                image::load_from_memory_with_format(&stream.content, image::ImageFormat::Jpeg)
                    .ok()?;
            if decoded.width() != width || decoded.height() != height {
                return None;
            }
            if channels == 1 {
                decoded.to_luma8().into_raw()
            } else {
                decoded.to_rgb8().into_raw()
            }
        }
        list if list.iter().any(|f| {
            matches!(
                *f,
                b"DCTDecode" | b"JPXDecode" | b"CCITTFaxDecode" | b"JBIG2Decode" | b"Crypt"
            )
        }) =>
        {
            return None;
        }
        _ => stream.decompressed_content_with_limit(expected + 1).ok()?,
    };
    (samples.len() == expected).then_some(Raster {
        samples,
        width,
        channels,
    })
}

/// Un nouveau flux image : le dictionnaire d'origine (espace de couleur, intention de rendu,
/// métadonnées…) sans ses filtres, et les échantillons `samples` comprimés en Flate.
fn rewritten(original: &Dictionary, samples: Vec<u8>) -> Stream {
    let mut dict = original.clone();
    // Filtres et masque d'origine, mais aussi tout ce qui peut **garder l'image d'origine** :
    // images alternatives (`/Alternates`, imprimées à la place par certains lecteurs, §8.9.5.4),
    // métadonnées XMP (une vignette peut y être), image haute résolution externe (`/OPI`),
    // fichiers associés (`/AF`) (relecture du Lot 10).
    for key in [
        &b"Filter"[..],
        b"DecodeParms",
        b"Length",
        b"SMask",
        b"Alternates",
        b"Metadata",
        b"OPI",
        b"AF",
        b"DL",
        b"PieceInfo",
        b"Ref",
    ] {
        dict.remove(key);
    }
    let mut stream = Stream::new(dict, samples);
    // `compress` ne fait rien si le flux comprimé serait plus gros.
    let _ = stream.compress();
    stream
}

/// Ce qu'il faut faire d'une image qui touche la zone.
enum Outcome {
    /// La zone ne couvre en fait aucun de ses pixels : on la garde.
    Keep,
    /// La remplacer par l'objet donné (copie noircie).
    Replace(ObjectId),
    /// La retirer de la page.
    Remove,
}

/// Noircit sous `zone` l'image `id` dessinée par la matrice `m`.
fn redact_image(doc: &mut LoDocument, id: ObjectId, m: &Matrix, zone: &Rect) -> Outcome {
    let Some(stream) = doc.get_object(id).ok().and_then(|o| o.as_stream().ok()) else {
        return Outcome::Remove;
    };
    let Some(mut raster) = decode(doc, stream) else {
        return Outcome::Remove;
    };
    let height = (raster.samples.len() / (raster.width as usize * raster.channels)) as u32;
    let Some(pixels) = pixel_zone(m, zone, raster.width, height) else {
        return Outcome::Keep;
    };
    // Noir : 0 en gris et en RVB ; en CMJN, le noir est K = 1 (encre noire seule).
    let black: &[u8] = match raster.channels {
        4 => &[0, 0, 0, 255],
        3 => &[0, 0, 0],
        _ => &[0],
    };
    raster.fill(pixels, black);
    // Masque de transparence : la zone devient opaque (le noir doit se voir, et l'ancien
    // masque ne doit garder aucune forme de ce qui était caché).
    let mask = match stream.dict.get(b"SMask") {
        Err(_) => None,
        Ok(reference) => {
            let mask = reference
                .as_reference()
                .ok()
                .and_then(|r| doc.get_object(r).ok())
                .and_then(|o| o.as_stream().ok());
            let Some(mask) = mask else {
                return Outcome::Remove;
            };
            let Some(mut alpha) = decode(doc, mask).filter(|r| r.channels == 1) else {
                return Outcome::Remove;
            };
            let mask_height = (alpha.samples.len() / alpha.width as usize) as u32;
            if let Some(zone) = pixel_zone(m, zone, alpha.width, mask_height) {
                alpha.fill(zone, &[255]);
            }
            Some(rewritten(&mask.dict, alpha.samples))
        }
    };
    let mut image = rewritten(&stream.dict, raster.samples);
    if let Some(mask) = mask {
        let mask_id = doc.add_object(mask);
        image.dict.set("SMask", Object::Reference(mask_id));
    }
    Outcome::Replace(doc.add_object(image))
}

/// Boîte, en espace utilisateur, du formulaire XObject `id` dessiné sous la matrice `ctm` :
/// sa `/BBox` transformée par sa `/Matrix` puis par `ctm`. `None` si elle est illisible.
fn form_bounds(doc: &LoDocument, id: ObjectId, ctm: &Matrix) -> Option<Rect> {
    let dict = &doc.get_object(id).ok()?.as_stream().ok()?.dict;
    let numbers = |key: &[u8]| -> Option<Vec<f32>> {
        let array = doc
            .dereference(dict.get(key).ok()?)
            .ok()?
            .1
            .as_array()
            .ok()?
            .clone();
        array
            .iter()
            .map(|o| doc.dereference(o).ok().and_then(|(_, v)| v.as_float().ok()))
            .collect()
    };
    let bbox = numbers(b"BBox")?;
    let [x0, y0, x1, y1] = <[f32; 4]>::try_from(bbox.as_slice()).ok()?;
    // L'identité seulement si `/Matrix` est absente : présente mais illisible, la boîte est
    // inconnue (`None`), et l'appelant considère que le formulaire touche la zone.
    let form_matrix = match dict.get(b"Matrix") {
        Err(_) => IDENTITY,
        Ok(_) => <[f32; 6]>::try_from(numbers(b"Matrix")?.as_slice()).ok()?,
    };
    let full = multiply(&form_matrix, ctm);
    let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(x, y)| apply(&full, x, y));
    let (mut lx, mut ly, mut hx, mut hy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        lx = lx.min(x);
        ly = ly.min(y);
        hx = hx.max(x);
        hy = hy.max(y);
    }
    Some(Rect {
        x: lx,
        y: ly,
        w: hx - lx,
        h: hy - ly,
    })
}

/// Profondeur d'imbrication maximale analysée (formulaire dans un formulaire, motif dans un
/// formulaire…). Au-delà, on répond « peint une image » : le caviardage refuse plutôt que de
/// laisser passer (audit du Lot 13, R3). Les fichiers réels dépassent rarement 10 niveaux.
const MAX_NESTING: usize = 64;

/// Vrai si les ressources `resources` peignent une image **hors** d'un `Do` direct du flux
/// examiné : par un motif (tiling pattern), par le masque doux d'un état graphique
/// (`/ExtGState … /SMask /G`) et, si `xobjects`, par une image ou un formulaire XObject. La
/// recherche descend dans les formulaires, les motifs et les groupes de masques (ensemble
/// `seen` contre les cycles, profondeur bornée par [`MAX_NESTING`]) ; une entrée qu'on ne sait
/// pas résoudre compte comme une image : en matière de caviardage, on refuse plutôt que de
/// laisser passer (relecture du Lot 10).
///
/// \[Apprentissage\] **Pile explicite** : au lieu de s'appeler elle-même pour chaque
/// formulaire imbriqué (une chaîne de 5 000 formulaires épuisait la pile du thread et fermait
/// l'application), la fonction range le travail à faire dans un `Vec` (`pending`) et le
/// reprend dans une boucle `while let Some(…) = pending.pop()`. La mémoire utilisée est alors
/// celle du tas, et la profondeur se compte et se borne. Même idée que `reading_order::xy_cut`
/// (correctif 0.12.2).
fn hidden_images_in(
    doc: &LoDocument,
    resources: &Dictionary,
    seen: &mut BTreeSet<ObjectId>,
    xobjects: bool,
) -> bool {
    let mut pending = vec![(resources.clone(), xobjects, 0usize)];
    while let Some((resources, xobjects, depth)) = pending.pop() {
        let Some(streams) = painting_streams(doc, &resources, xobjects) else {
            return true;
        };
        for id in streams {
            if !seen.insert(id) {
                continue;
            }
            if depth >= MAX_NESTING {
                return true;
            }
            match stream_resources(doc, id, Some(&resources)) {
                Some(inner) => pending.push((inner, true, depth + 1)),
                None => return true,
            }
        }
    }
    false
}

/// Les flux qui dessinent depuis `resources` (formulaires si `xobjects`, motifs de pavage,
/// groupes de masques doux), à examiner à leur tour ; `None` dès qu'une image (ou une entrée
/// illisible) est trouvée.
fn painting_streams(
    doc: &LoDocument,
    resources: &Dictionary,
    xobjects: bool,
) -> Option<Vec<ObjectId>> {
    let entries = |key: &[u8]| -> Option<Vec<Object>> {
        match resources.get(key) {
            Err(_) => Some(Vec::new()),
            Ok(value) => doc
                .dereference(value)
                .ok()
                .and_then(|(_, o)| o.as_dict().ok().cloned())
                .map(|d| d.iter().map(|(_, v)| v.clone()).collect()),
        }
    };
    let xobject_list = entries(b"XObject")?;
    let patterns = entries(b"Pattern")?;
    let states = entries(b"ExtGState")?;
    let mut streams = Vec::new();
    if xobjects {
        for value in xobject_list {
            let id = value.as_reference().ok()?;
            let subtype = doc
                .get_object(id)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| s.dict.get(b"Subtype").ok())
                .and_then(|t| doc.dereference(t).ok())
                .and_then(|(_, t)| t.as_name().ok().map(<[u8]>::to_vec));
            match subtype.as_deref() {
                Some(b"Form") => streams.push(id),
                Some(b"PS") => {}
                // Une image, ou un XObject illisible : refus.
                _ => return None,
            }
        }
    }
    for value in patterns {
        // Un motif de type 1 (pavage) est un flux qui dessine ; un motif de type 2 (dégradé)
        // est un dictionnaire sans image.
        match doc.dereference(&value) {
            Ok((Some(id), Object::Stream(_))) => streams.push(id),
            Ok((_, Object::Dictionary(_))) => {}
            _ => return None,
        }
    }
    for value in states {
        let Ok((_, Object::Dictionary(state))) = doc.dereference(&value) else {
            return None;
        };
        let Ok(mask) = state.get(b"SMask") else {
            continue;
        };
        match doc.dereference(mask) {
            Ok((_, Object::Name(_))) => {}
            Ok((_, Object::Dictionary(mask))) => {
                streams.push(mask.get(b"G").and_then(Object::as_reference).ok()?);
            }
            _ => return None,
        }
    }
    Some(streams)
}

/// Les ressources du flux `id` : les siennes, sinon celles de son parent (`parent`, héritage
/// permis par la norme) ; `None` si le flux est illisible ou sans ressources résolubles.
fn stream_resources(
    doc: &LoDocument,
    id: ObjectId,
    parent: Option<&Dictionary>,
) -> Option<Dictionary> {
    let stream = doc.get_object(id).ok().and_then(|o| o.as_stream().ok())?;
    match stream.dict.get(b"Resources") {
        Err(_) => parent.cloned(),
        Ok(value) => match doc.dereference(value) {
            Ok((_, Object::Dictionary(d))) => Some(d.clone()),
            _ => None,
        },
    }
}

/// Vrai si le flux `id` (formulaire XObject, motif, groupe de masque) peint une image, lui ou
/// ce qu'il appelle (voir [`hidden_images_in`]). Sans `/Resources` propres, un flux utilise
/// celles de son parent ; s'il n'y en a pas, on refuse par prudence.
fn stream_paints_images(
    doc: &LoDocument,
    id: ObjectId,
    parent: Option<&Dictionary>,
    seen: &mut BTreeSet<ObjectId>,
) -> bool {
    if !seen.insert(id) {
        return false;
    }
    match stream_resources(doc, id, parent) {
        Some(resources) => hidden_images_in(doc, &resources, seen, true),
        None => true,
    }
}

/// Les six nombres d'un opérateur `cm`.
fn matrix_operands(op: &Operation) -> Option<Matrix> {
    let values: Vec<f32> = op
        .operands
        .iter()
        .filter_map(|o| o.as_float().ok())
        .collect();
    <[f32; 6]>::try_from(values.as_slice()).ok()
}

/// Caviarde les images de la page `page_id` qui touchent `zone` (points, espace
/// **utilisateur**) : flux de contenu réécrit, nouvelles images ajoutées aux ressources de la
/// page. Refuse une image dessinée à travers un formulaire XObject (hors de portée).
pub(crate) fn redact_images(
    doc: &mut LoDocument,
    page_id: ObjectId,
    zone: &Rect,
) -> CoreResult<RasterReport> {
    let mut resources = {
        let page = doc.get_dictionary(page_id).map_err(structure)?;
        match page.get(b"Resources") {
            Ok(obj) => doc
                .dereference(obj)
                .ok()
                .and_then(|(_, o)| o.as_dict().ok().cloned())
                .unwrap_or_default(),
            Err(_) => crate::overlay::inherited_resources(doc, page),
        }
    };
    let mut xobjects = resources
        .get(b"XObject")
        .ok()
        .and_then(|x| doc.dereference(x).ok())
        .and_then(|(_, x)| x.as_dict().ok().cloned())
        .unwrap_or_default();
    // Les images directes de la page passent par `Do` (traitées plus bas) : on cherche ici
    // celles des motifs et des masques.
    if hidden_images_in(doc, &resources, &mut BTreeSet::new(), false) {
        return Err(CoreError::InvalidOperation(
            "la page peint une image par un motif ou un masque : le caviardage d'images ne \
             peut pas l'atteindre"
                .to_string(),
        ));
    }
    let content = crate::crypt::page_content(doc, page_id).map_err(structure)?;
    let content = Content::decode(&content).map_err(structure)?;

    let mut report = RasterReport::default();
    let mut ctm = IDENTITY;
    let mut stack: Vec<Matrix> = Vec::new();
    let mut operations = Vec::with_capacity(content.operations.len());
    let mut fresh = 0u32;
    // Noms des images remplacées ou retirées : s'ils ne servent plus, on les ôte des
    // ressources, sinon l'image d'origine resterait atteignable (et dans le fichier).
    let mut touched: BTreeSet<Vec<u8>> = BTreeSet::new();
    for op in content.operations {
        match op.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => ctm = stack.pop().unwrap_or(IDENTITY),
            "cm" => {
                if let Some(m) = matrix_operands(&op) {
                    ctm = multiply(&m, &ctm);
                }
            }
            "BI" => {
                // Image en ligne : impossible à remplacer par une copie, retirée si touchée.
                if intersects(&bounds_of(&ctm), zone) {
                    report.removed = report.removed.saturating_add(1);
                    continue;
                }
            }
            "Do" => {
                let target = op
                    .operands
                    .first()
                    .and_then(|n| n.as_name().ok())
                    .and_then(|n| xobjects.get(n).ok())
                    .and_then(|r| r.as_reference().ok());
                // `/Subtype` peut être une référence : on la suit (revue de sécurité).
                let subtype = target
                    .and_then(|id| doc.get_object(id).ok())
                    .and_then(|o| o.as_stream().ok())
                    .and_then(|s| s.dict.get(b"Subtype").ok())
                    .and_then(|t| doc.dereference(t).ok())
                    .and_then(|(_, t)| t.as_name().ok().map(<[u8]>::to_vec));
                match (target, subtype.as_deref()) {
                    (Some(id), Some(b"Image")) => {
                        let bounds = bounds_of(&ctm);
                        if intersects(&bounds, zone) {
                            if let Some(name) = op.operands.first().and_then(|n| n.as_name().ok()) {
                                touched.insert(name.to_vec());
                            }
                            if contains(zone, &bounds) {
                                report.removed = report.removed.saturating_add(1);
                                continue;
                            }
                            match redact_image(doc, id, &ctm, zone) {
                                Outcome::Keep => {}
                                Outcome::Remove => {
                                    report.removed = report.removed.saturating_add(1);
                                    continue;
                                }
                                Outcome::Replace(new_id) => {
                                    let name = loop {
                                        fresh += 1;
                                        let name = format!("MegaRedact{fresh}");
                                        if !xobjects.has(name.as_bytes()) {
                                            break name;
                                        }
                                    };
                                    xobjects.set(name.as_str(), Object::Reference(new_id));
                                    report.redacted = report.redacted.saturating_add(1);
                                    operations.push(Operation::new(
                                        "Do",
                                        vec![Object::Name(name.into_bytes())],
                                    ));
                                    continue;
                                }
                            }
                        }
                    }
                    (Some(id), Some(b"Form")) => {
                        // Un formulaire qui dessine une image n'est refusé que s'il touche la
                        // zone : sa boîte (`/BBox`, transformée par `/Matrix` puis par la
                        // matrice courante, §8.10.1). Un logo loin de la zone ne bloque rien.
                        let mut seen = BTreeSet::new();
                        let touches = form_bounds(doc, id, &ctm)
                            .is_none_or(|bounds| intersects(&bounds, zone));
                        if touches && stream_paints_images(doc, id, Some(&resources), &mut seen) {
                            return Err(CoreError::InvalidOperation(
                                "une image de la page est dessinée à travers un groupe \
                                 XObject : le caviardage d'images ne peut pas l'atteindre"
                                    .to_string(),
                            ));
                        }
                    }
                    (Some(_), Some(b"PS")) => {}
                    // Cible introuvable ou de nature inconnue : on ne sait pas ce qu'elle
                    // dessine, on refuse plutôt que de laisser passer.
                    _ => {
                        return Err(CoreError::InvalidOperation(
                            "un objet XObject de la page est illisible : caviardage refusé"
                                .to_string(),
                        ));
                    }
                }
            }
            _ => {}
        }
        operations.push(op);
    }
    if report == RasterReport::default() {
        return Ok(report);
    }
    let used: BTreeSet<&[u8]> = operations
        .iter()
        .filter(|op| op.operator == "Do")
        .filter_map(|op| op.operands.first().and_then(|n| n.as_name().ok()))
        .collect();
    for name in &touched {
        if !used.contains(name.as_slice()) {
            xobjects.remove(name);
        }
    }
    let bytes = Content { operations }.encode().map_err(structure)?;
    let mut stream = Stream::new(Dictionary::new(), bytes);
    let _ = stream.compress();
    let content_id = doc.add_object(stream);
    resources.set("XObject", Object::Dictionary(xobjects));
    let page = doc.get_dictionary_mut(page_id).map_err(structure)?;
    page.set("Contents", Object::Reference(content_id));
    page.set("Resources", Object::Dictionary(resources));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn pixel_zone_projects_through_the_matrix() {
        // Image 64 × 64 posée en 100..300 × 100..300 ; zone = son quart haut-gauche.
        let m = [200.0, 0.0, 0.0, 200.0, 100.0, 100.0];
        let zone = pixel_zone(&m, &rect(100.0, 200.0, 100.0, 100.0), 64, 64).unwrap();
        assert_eq!(zone, [0, 0, 33, 33], "quart haut-gauche + 1 px de marge");
        assert_eq!(
            pixel_zone(&m, &rect(400.0, 400.0, 10.0, 10.0), 64, 64),
            None
        );
    }

    #[test]
    fn pixel_zone_handles_flipped_matrix() {
        // Image retournée verticalement (d < 0) : le haut de l'image est en bas de la page.
        let m = [200.0, 0.0, 0.0, -200.0, 100.0, 300.0];
        let zone = pixel_zone(&m, &rect(100.0, 100.0, 100.0, 100.0), 64, 64).unwrap();
        assert_eq!(
            zone,
            [0, 0, 33, 33],
            "bas de la page = haut de l'image retournée"
        );
        // Rotation d'un quart de tour : x de la page ↔ rangées de l'image.
        let r = [0.0, 200.0, -200.0, 0.0, 300.0, 100.0];
        let zone = pixel_zone(&r, &rect(100.0, 100.0, 100.0, 200.0), 64, 64).unwrap();
        assert_eq!(zone, [0, 0, 64, 33]);
    }

    #[test]
    fn invert_undoes_the_matrix() {
        let m = [2.0, 1.0, -1.0, 3.0, 10.0, 20.0];
        let inv = invert(&m).unwrap();
        let (x, y) = apply(&m, 5.0, 7.0);
        let (u, v) = apply(&inv, x, y);
        assert!((u - 5.0).abs() < 1e-4 && (v - 7.0).abs() < 1e-4, "{u} {v}");
        assert!(invert(&[0.0, 0.0, 0.0, 0.0, 1.0, 1.0]).is_none());
    }
}
