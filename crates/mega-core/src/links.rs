// SPDX-License-Identifier: GPL-3.0-or-later
//! Liens d'une page : zones cliquables vers une autre page ou une adresse.

use pdfium_render::prelude::PdfLink;
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::CoreResult;
use crate::geometry::Rect;

/// Ce qu'un lien vise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum LinkTarget {
    /// Une page du même document (index à partir de 0).
    Page(u16),
    /// Une adresse externe (`https://…`, `mailto:…`). Le frontend décide s'il l'ouvre.
    Uri(String),
}

/// Une zone cliquable de la page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageLink {
    /// Zone cliquable, en points PDF.
    pub rect: Rect,
    /// Cible du lien.
    pub target: LinkTarget,
}

impl Document {
    /// Les liens de la page `index` dont la cible est connue (page interne ou URI).
    pub fn links(&self, index: u16) -> CoreResult<Vec<PageLink>> {
        let _guard = engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(index)?)?;
        let links = page
            .links()
            .iter()
            .filter_map(|l| {
                let rect = Rect::from_pdf(l.rect().ok()?);
                Some(PageLink {
                    rect,
                    target: target_of(&l)?,
                })
            })
            .collect();
        Ok(links)
    }
}

/// Cible d'un lien PDFium : destination directe, action « aller à », ou action URI.
fn target_of(l: &PdfLink<'_>) -> Option<LinkTarget> {
    if let Some(index) = l.destination().and_then(|d| d.page_index().ok()) {
        return u16::try_from(index).ok().map(LinkTarget::Page);
    }
    let action = l.action()?;
    if let Some(local) = action.as_local_destination_action() {
        let index = local.destination().ok()?.page_index().ok()?;
        return u16::try_from(index).ok().map(LinkTarget::Page);
    }
    if let Some(uri) = action.as_uri_action() {
        return uri.uri().ok().map(LinkTarget::Uri);
    }
    None
}
