// SPDX-License-Identifier: GPL-3.0-or-later
//! Formulaires AcroForm : lecture des champs, remplissage, aplatissement, signature visuelle.
//!
//! \[Apprentissage\] **Ressources natives et `Drop`** : PDFium ne « voit » les champs qu'à
//! travers un *environnement de formulaire* (`FPDFDOC_InitFormFillEnvironment`), une
//! ressource native que pdfium-render crée avec le document et libère dans son `Drop`.
//! Nous n'en gérons pas la vie nous-mêmes : elle suit celle de `Document` (RAII), et tout
//! accès passe par le jeton `engine::guard()` comme le reste.
//!
//! Un champ est une annotation `Widget` de la page : son `index` est sa position dans
//! `/Annots`, comme pour les annotations du Lot 3.

use pdfium_render::prelude::{
    PdfFormField, PdfFormFieldCommon, PdfFormType, PdfPageAnnotationCommon,
};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::CoreResult;
use crate::geometry::Rect;

/// Nature du formulaire d'un document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormType {
    /// Aucun formulaire.
    None,
    /// Formulaire AcroForm (pris en charge).
    AcroForm,
    /// Formulaire XFA (non pris en charge : affiché tel quel, non remplissable).
    Xfa,
}

/// Genre de champ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// Zone de texte (mono ou multiligne, mot de passe).
    Text,
    /// Case à cocher.
    Checkbox,
    /// Bouton radio (un parmi un groupe).
    Radio,
    /// Liste déroulante.
    Combo,
    /// Liste à choix.
    List,
    /// Bouton poussoir.
    Button,
    /// Champ de signature numérique (non géré).
    Signature,
    /// Autre.
    Unknown,
}

/// Une option d'une liste ou d'un combo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldOption {
    /// Libellé affiché.
    pub label: String,
    /// Sélectionnée.
    pub selected: bool,
}

/// Un champ de formulaire tel que lu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    /// Page (index 0-based).
    pub page: u16,
    /// Position de l'annotation `Widget` dans `/Annots` de la page.
    pub index: u16,
    /// Nom complet du champ (`/T`, hérité du parent pour un groupe radio).
    pub name: String,
    /// Genre.
    pub kind: FieldKind,
    /// Valeur texte (`/V`) pour texte, combo, liste ; valeur du groupe pour case/radio.
    pub value: Option<String>,
    /// Coché (case, radio).
    pub checked: Option<bool>,
    /// Options (combo, liste).
    pub options: Vec<FieldOption>,
    /// Emprise sur la page, en points PDF.
    pub rect: Rect,
    /// Lecture seule (`/Ff` bit 1).
    pub read_only: bool,
    /// Requis (`/Ff` bit 2).
    pub required: bool,
    /// Multiligne (texte).
    pub multiline: bool,
    /// Mot de passe (texte).
    pub password: bool,
}

/// Résumé du formulaire d'un document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormInfo {
    /// Nature.
    pub form_type: FormType,
    /// Nombre de widgets sur l'ensemble des pages.
    pub field_count: u32,
}

impl Document {
    /// Nature du formulaire et nombre de champs (toutes pages).
    pub fn form_info(&self) -> CoreResult<FormInfo> {
        let _guard = engine::guard();
        let form_type = match self.inner().form().map(|f| f.form_type()) {
            None | Some(PdfFormType::None) => FormType::None,
            Some(PdfFormType::Acrobat) => FormType::AcroForm,
            Some(PdfFormType::XfaFull) | Some(PdfFormType::XfaForeground) => FormType::Xfa,
        };
        let mut field_count = 0u32;
        for page in self.inner().pages().iter() {
            field_count += page
                .annotations()
                .iter()
                .filter(|a| a.as_form_field().is_some())
                .count() as u32;
        }
        Ok(FormInfo {
            form_type,
            field_count,
        })
    }

    /// Les champs de la page `page`, dans l'ordre de `/Annots`.
    pub fn form_fields(&self, page: u16) -> CoreResult<Vec<FormField>> {
        let _guard = engine::guard();
        let pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        let mut out = Vec::new();
        for (index, ann) in pdf_page
            .annotations()
            .iter()
            .take(crate::annotations::MAX_ANNOTATIONS_PER_PAGE)
            .enumerate()
        {
            let Some(field) = ann.as_form_field() else {
                continue;
            };
            let rect = Rect::from_pdf(ann.bounds()?);
            out.push(read_field(page, index as u16, rect, field));
        }
        Ok(out)
    }
}

/// Nom, lecture seule, requis : communs à tous les genres (trait `PdfFormFieldCommon`).
fn common<F: PdfFormFieldCommon>(f: &F) -> (String, bool, bool) {
    (
        f.name().unwrap_or_default(),
        f.is_read_only(),
        f.is_required(),
    )
}

/// \[Apprentissage\] **`match` sur une `enum` de la bibliothèque** : `PdfFormField` est une
/// enum de pdfium-render, une variante par type de champ, chacune portant un type différent.
/// On la traduit en notre `FormField` plat, plus simple à sérialiser pour l'interface.
fn read_field(page: u16, index: u16, rect: Rect, field: &PdfFormField) -> FormField {
    let (name, read_only, required, kind, value, checked, options, multiline, password) =
        match field {
            PdfFormField::Text(t) => {
                let (n, ro, rq) = common(t);
                (
                    n,
                    ro,
                    rq,
                    FieldKind::Text,
                    Some(t.value().unwrap_or_default()),
                    None,
                    Vec::new(),
                    t.is_multiline(),
                    t.is_password(),
                )
            }
            PdfFormField::Checkbox(c) => {
                let (n, ro, rq) = common(c);
                (
                    n,
                    ro,
                    rq,
                    FieldKind::Checkbox,
                    c.group_value(),
                    Some(c.is_checked().unwrap_or(false)),
                    Vec::new(),
                    false,
                    false,
                )
            }
            PdfFormField::RadioButton(r) => {
                let (n, ro, rq) = common(r);
                (
                    n,
                    ro,
                    rq,
                    FieldKind::Radio,
                    r.group_value(),
                    Some(r.is_checked().unwrap_or(false)),
                    Vec::new(),
                    false,
                    false,
                )
            }
            PdfFormField::ComboBox(c) => {
                let (n, ro, rq) = common(c);
                (
                    n,
                    ro,
                    rq,
                    FieldKind::Combo,
                    c.value(),
                    None,
                    options_of(c.options().iter()),
                    false,
                    false,
                )
            }
            PdfFormField::ListBox(l) => {
                let (n, ro, rq) = common(l);
                (
                    n,
                    ro,
                    rq,
                    FieldKind::List,
                    l.value(),
                    None,
                    options_of(l.options().iter()),
                    false,
                    false,
                )
            }
            PdfFormField::PushButton(b) => {
                let (n, ro, rq) = common(b);
                (
                    n,
                    ro,
                    rq,
                    FieldKind::Button,
                    None,
                    None,
                    Vec::new(),
                    false,
                    false,
                )
            }
            PdfFormField::Signature(s) => {
                let (n, ro, rq) = common(s);
                (
                    n,
                    ro,
                    rq,
                    FieldKind::Signature,
                    None,
                    None,
                    Vec::new(),
                    false,
                    false,
                )
            }
            PdfFormField::Unknown(u) => {
                let (n, ro, rq) = common(u);
                (
                    n,
                    ro,
                    rq,
                    FieldKind::Unknown,
                    None,
                    None,
                    Vec::new(),
                    false,
                    false,
                )
            }
        };
    FormField {
        page,
        index,
        name,
        kind,
        value,
        checked,
        options,
        rect,
        read_only,
        required,
        multiline,
        password,
    }
}

fn options_of(
    iter: impl Iterator<Item = pdfium_render::prelude::PdfFormFieldOption>,
) -> Vec<FieldOption> {
    iter.take(1024)
        .map(|o| FieldOption {
            label: o.label().cloned().unwrap_or_default(),
            selected: o.is_set(),
        })
        .collect()
}
