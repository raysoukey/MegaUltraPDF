// SPDX-License-Identifier: GPL-3.0-or-later
//! Commandes de contenu du Lot 5 : `objects`, `redact`, `watermark`, `number`, `crop`.
//!
//! Toutes les éditions passent par `EditCommand` et `Document::apply` (ADR-0007), comme
//! dans l'application : la ligne de commande est un banc d'essai du même code.

use std::path::Path;

use anyhow::Context;
use mega_core::overlay::{Corner, CropSpec, PageNumbering, Watermark};
use mega_core::{Color, Document, EditCommand, ObjectKind, PageObject, PageSpec, Rect};

/// Nom court et stable d'une nature d'objet (identique au `kind` sérialisé pour le frontend).
fn kind_name(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Text => "text",
        ObjectKind::Image => "image",
        ObjectKind::Path => "path",
        ObjectKind::Form => "form",
        ObjectKind::Shading => "shading",
        ObjectKind::Other => "other",
    }
}

/// Une ligne par objet : `p<page> #<index> <nature> [x y w h] police taille "<texte>"`.
fn line(page: u16, o: &PageObject) -> String {
    let mut s = format!(
        "p{} #{} {} [{:.0} {:.0} {:.0} {:.0}]",
        page + 1,
        o.index,
        kind_name(o.kind),
        o.bounds.x,
        o.bounds.y,
        o.bounds.w,
        o.bounds.h
    );
    if let Some(font) = &o.font {
        s.push(' ');
        s.push_str(&super::info::display_safe(font));
    }
    if let Some(size) = o.font_size {
        s.push_str(&format!(" {size:.1}pt"));
    }
    if let Some(text) = &o.text {
        let short: String = text.chars().take(60).collect();
        s.push_str(&format!(" \"{}\"", super::info::display_safe(&short)));
    }
    s
}

/// Numéro de page 1-based vérifié → index 0-based.
fn page_index(doc: &Document, page: u16) -> anyhow::Result<u16> {
    anyhow::ensure!(
        page >= 1 && page <= doc.page_count(),
        "la page doit être comprise entre 1 et {}",
        doc.page_count()
    );
    Ok(page - 1)
}

/// `mega objects` : liste les objets d'une page (ou de toutes).
pub fn objects(file: &Path, page: Option<u16>, password: Option<&str>) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let pages: Vec<u16> = match page {
        Some(p) => vec![page_index(&doc, p)?],
        None => (0..doc.page_count()).collect(),
    };
    let mut total = 0usize;
    for p in pages {
        for o in doc.page_objects(p)? {
            println!("{}", line(p, &o));
            total += 1;
        }
    }
    println!("{total} objet{}", if total > 1 { "s" } else { "" });
    Ok(())
}

/// Quatre nombres séparés par des virgules (« x,y,w,h » ou « g,b,d,h »).
fn four(spec: &str, what: &str) -> anyhow::Result<[f32; 4]> {
    let parts: Vec<f32> = spec
        .split(',')
        .map(|s| s.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .with_context(|| format!("{what} : quatre nombres attendus, séparés par des virgules"))?;
    let [a, b, c, d] = parts[..] else {
        anyhow::bail!("{what} : quatre nombres attendus, séparés par des virgules");
    };
    Ok([a, b, c, d])
}

/// Applique `command` à `file` et écrit le résultat dans `out` par `save_as` : même chemin
/// que l'application (passe de nettoyage `lopdf`, écriture atomique), jamais `to_bytes`
/// brut, qui conserverait les objets orphelins (annotations retirées comprises).
fn apply_and_write(
    file: &Path,
    command: &EditCommand,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<Document> {
    let mut doc = Document::open(file, password)?;
    doc.apply(command)?;
    doc.save_as(out)
        .with_context(|| format!("écriture de {}", out.display()))?;
    Ok(doc)
}

/// `mega redact` : caviarde une zone sur chacune des pages `spec` et écrit un nouveau
/// fichier.
pub fn redact(
    file: &Path,
    spec: &str,
    rect: &str,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let [x, y, w, h] = four(rect, "--rect")?;
    let mut doc = Document::open(file, password)?;
    let pages = PageSpec::parse(spec, doc.page_count())?;
    // L'appel direct donne le rapport ; la commande d'historique n'en a pas besoin ici.
    let mut total = [0u32; 6];
    for page in &pages {
        let r = doc.redact(*page, Rect { x, y, w, h })?;
        let counts = [
            r.texts,
            r.images_removed,
            r.images_redacted,
            r.paths_removed,
            r.paths_kept,
            r.annotations,
        ];
        for (sum, n) in total.iter_mut().zip(counts) {
            *sum += u32::from(n);
        }
    }
    let [texts, removed, blacked, paths, kept, annotations] = total;
    doc.save_as(out)
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!(
        "Caviardé sur {} page(s) : {texts} texte(s), {removed} image(s) retirée(s), \
         {blacked} image(s) noircie(s) sous la zone, {paths} tracé(s) retiré(s), \
         {kept} tracé(s) gardé(s) sous le cache, {annotations} annotation(s) : {}",
        pages.len(),
        out.display()
    );
    Ok(())
}

/// Pages visées : toutes si `spec` est absent.
fn pages_of(doc: &Document, spec: Option<&str>) -> anyhow::Result<Option<Vec<u16>>> {
    spec.map(|s| PageSpec::parse(s, doc.page_count()).map_err(anyhow::Error::from))
        .transpose()
}

/// `mega watermark` : filigrane en diagonale sur les pages visées.
#[allow(clippy::too_many_arguments)]
pub fn watermark(
    file: &Path,
    text: &str,
    size: f32,
    opacity: f32,
    angle: f32,
    pages: Option<&str>,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let pages = pages_of(&doc, pages)?;
    let count = pages.as_ref().map_or(doc.page_count() as usize, Vec::len);
    drop(doc);
    let spec = Watermark {
        text: text.to_string(),
        size,
        opacity,
        angle_deg: angle,
        color: Color {
            r: 128,
            g: 128,
            b: 128,
        },
        pages,
    };
    apply_and_write(file, &EditCommand::Watermark { spec }, out, password)?;
    println!("Filigrane posé sur {count} page(s) : {}", out.display());
    Ok(())
}

/// Position d'un numéro, depuis son nom sérialisé (`bottom_center`…).
fn corner(name: &str) -> anyhow::Result<Corner> {
    Ok(match name {
        "top_left" => Corner::TopLeft,
        "top_center" => Corner::TopCenter,
        "top_right" => Corner::TopRight,
        "bottom_left" => Corner::BottomLeft,
        "bottom_center" => Corner::BottomCenter,
        "bottom_right" => Corner::BottomRight,
        _ => anyhow::bail!(
            "position inconnue « {name} » (top_left, top_center, top_right, bottom_left, bottom_center, bottom_right)"
        ),
    })
}

/// `mega number` : numérote les pages d'après un modèle.
#[allow(clippy::too_many_arguments)]
pub fn number(
    file: &Path,
    template: &str,
    corner_name: &str,
    margin: f32,
    size: f32,
    start: u16,
    pages: Option<&str>,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let corner = corner(corner_name)?;
    let doc = Document::open(file, password)?;
    let pages = pages_of(&doc, pages)?;
    let count = pages.as_ref().map_or(doc.page_count() as usize, Vec::len);
    drop(doc);
    let spec = PageNumbering {
        template: template.to_string(),
        corner,
        margin,
        size,
        start,
        pages,
    };
    apply_and_write(file, &EditCommand::PageNumbers { spec }, out, password)?;
    println!("{count} page(s) numérotée(s) : {}", out.display());
    Ok(())
}

/// `mega crop` : recadre des pages par marges `gauche,bas,droite,haut`.
pub fn crop(
    file: &Path,
    margins: &str,
    pages: Option<&str>,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let margins = four(margins, "--margins")?;
    let doc = Document::open(file, password)?;
    let pages = pages_of(&doc, pages)?;
    let count = pages.as_ref().map_or(doc.page_count() as usize, Vec::len);
    drop(doc);
    let spec = CropSpec { pages, margins };
    let doc = apply_and_write(file, &EditCommand::CropPages { spec }, out, password)?;
    let s = doc.page_size(0)?;
    println!(
        "{count} page(s) recadrée(s), page 1 : {:.1} × {:.1} pt : {}",
        s.width_pt,
        s.height_pt,
        out.display()
    );
    Ok(())
}
