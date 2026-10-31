// SPDX-License-Identifier: GPL-3.0-or-later
//! Étiquettes de pages (`/PageLabels`, §12.4.2) des pages copiées (audit E3).
//!
//! Une étiquette se décrit par plages : « à partir de la page 0, chiffres romains ; à partir
//! de la page 1, chiffres arabes depuis 1 ». Les plages de la source ne valent plus après une
//! extraction ou une fusion : on calcule, page copiée par page copiée, l'étiquette qu'elle
//! portait, puis on regroupe les pages consécutives d'une même plage.

use lopdf::{Dictionary, Document as LoDocument, Object};

use super::tree_entries;

/// Plages d'étiquettes d'un document : (première page, style), triées.
fn ranges(doc: &LoDocument) -> Vec<(i64, Dictionary)> {
    let Some(labels) = doc.catalog().ok().and_then(|c| c.get(b"PageLabels").ok()) else {
        return Vec::new();
    };
    let mut ranges: Vec<(i64, Dictionary)> = tree_entries(doc, labels, b"Nums")
        .into_iter()
        .filter_map(|(start, style)| {
            let start = start.as_i64().ok()?;
            let style = doc.dereference(&style).ok()?.1.as_dict().ok()?.clone();
            Some((start, style))
        })
        .collect();
    ranges.sort_by_key(|(start, _)| *start);
    ranges
}

/// Style « chiffres arabes à partir de `start` ».
fn decimal(start: i64) -> Dictionary {
    let mut style = Dictionary::new();
    style.set("S", Object::Name(b"D".to_vec()));
    style.set("St", start);
    style
}

/// Le style de la plage `range` (`/S`, `/P`) qui fait commencer l'étiquette au numéro `start`.
fn shifted(range: &Dictionary, start: i64) -> Dictionary {
    let mut style = Dictionary::new();
    for key in [&b"S"[..], b"P"] {
        if let Ok(value @ (Object::Name(_) | Object::String(..))) = range.get(key) {
            style.set(key.to_vec(), value.clone());
        }
    }
    style.set("St", start);
    style
}

/// Reporte les étiquettes des pages `src_pages`, copiées à partir de l'index `dst_first` de
/// `dst`. Sans étiquettes dans la source, rien n'est écrit, sauf lors d'une fusion où une
/// source précédente en avait : ses pages garderaient sinon la numérotation de la plage
/// précédente, on les numérote donc comme leur position dans le document.
pub(crate) fn carry_page_labels(
    src: &LoDocument,
    dst: &mut LoDocument,
    src_pages: &[u16],
    dst_first: usize,
) {
    let source = ranges(src);
    let mut labels = ranges(dst);
    let first = i64::try_from(dst_first).unwrap_or(i64::MAX);
    if source.is_empty() {
        if labels.is_empty() || src_pages.is_empty() {
            return;
        }
        labels.push((first, decimal(first.saturating_add(1))));
    } else {
        if labels.is_empty() && first > 0 {
            // Les pages des sources précédentes, sans étiquettes, gardent leur numéro.
            labels.push((0, decimal(1)));
        }
        // (plage source, page source) de la page précédente : une page qui continue la même
        // plage n'ouvre pas de nouvelle plage.
        let mut previous: Option<(usize, i64)> = None;
        for (i, &page) in src_pages.iter().enumerate() {
            let page = i64::from(page);
            let at = first.saturating_add(i64::try_from(i).unwrap_or(i64::MAX));
            let Some(r) = source.iter().rposition(|(start, _)| *start <= page) else {
                labels.push((at, decimal(page + 1)));
                previous = None;
                continue;
            };
            if previous == Some((r, page - 1)) {
                previous = Some((r, page));
                continue;
            }
            let (start, range) = &source[r];
            let number = range
                .get(b"St")
                .and_then(Object::as_i64)
                .unwrap_or(1)
                .saturating_add(page.saturating_sub(*start));
            labels.push((at, shifted(range, number)));
            previous = Some((r, page));
        }
    }
    let nums: Vec<Object> = labels
        .into_iter()
        .flat_map(|(start, style)| [Object::Integer(start), Object::Dictionary(style)])
        .collect();
    let mut tree = Dictionary::new();
    tree.set("Nums", nums);
    if let Ok(catalog) = dst.catalog_mut() {
        catalog.set("PageLabels", tree);
    }
}
