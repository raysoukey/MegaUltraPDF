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
    PdfFormField, PdfFormFieldCommon, PdfFormType, PdfPageAnnotationCommon, PdfPageAnnotationType,
    PdfRenderConfig,
};
use serde::{Deserialize, Serialize};

mod signature;
mod write;

pub use signature::SignatureSource;

use crate::annotations::{MAX_ANNOTATIONS_PER_PAGE, MAX_STRING_CHARS};
use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};
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

/// Valeur à écrire dans un champ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldValue {
    /// Texte (champ texte).
    Text {
        /// Le texte.
        text: String,
    },
    /// Coché ou non (case à cocher).
    Checked {
        /// L'état.
        checked: bool,
    },
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
                .take(MAX_ANNOTATIONS_PER_PAGE)
                .filter(|a| a.as_form_field().is_some())
                .count() as u32;
        }
        Ok(FormInfo {
            form_type,
            field_count,
        })
    }

    /// Écrit une valeur dans le champ `index` de la page.
    ///
    /// Texte → champ texte, combo ou liste (valeur parmi les options) ; `Checked` → case
    /// (vrai/faux) ou bouton radio (vrai : il devient l'élu du groupe). Lecture seule,
    /// boutons, signatures : refusés. L'écriture est faite par `lopdf` (`/V`, `/AS` en noms,
    /// `/NeedAppearances true`), puis le document est **rechargé** dans PDFium, qui régénère
    /// les apparences.
    pub fn set_field_value(&mut self, page: u16, index: u16, value: &FieldValue) -> CoreResult<()> {
        let patch = {
            let _guard = engine::guard();
            let pdf_page = self
                .inner()
                .pages()
                .get(self.checked_index_unlocked(page)?)?;
            let count = pdf_page.annotations().len();
            if usize::from(index) >= count || usize::from(index) >= MAX_ANNOTATIONS_PER_PAGE {
                return Err(invalid(&format!(
                    "champ {index} introuvable (la page a {count} annotations)"
                )));
            }
            let ann = pdf_page.annotations().get(usize::from(index))?;
            let Some(field) = ann.as_form_field() else {
                return Err(invalid("cette annotation n'est pas un champ de formulaire"));
            };
            let read_only = match field {
                PdfFormField::Text(f) => f.is_read_only(),
                PdfFormField::Checkbox(f) => f.is_read_only(),
                PdfFormField::RadioButton(f) => f.is_read_only(),
                PdfFormField::ComboBox(f) => f.is_read_only(),
                PdfFormField::ListBox(f) => f.is_read_only(),
                _ => true,
            };
            if read_only {
                return Err(invalid("ce champ est en lecture seule"));
            }
            match (field, value) {
                (PdfFormField::Text(_), FieldValue::Text { text }) => {
                    if text.chars().count() > MAX_STRING_CHARS {
                        return Err(invalid(&format!(
                            "texte trop long (maximum {MAX_STRING_CHARS} caractères)"
                        )));
                    }
                    write::Patch::Text(text.clone())
                }
                (PdfFormField::ComboBox(c), FieldValue::Text { text }) => {
                    if !c
                        .options()
                        .iter()
                        .any(|o| o.label().map(|l| l == text).unwrap_or(false))
                    {
                        return Err(invalid("cette valeur n'est pas une option de la liste"));
                    }
                    write::Patch::Text(text.clone())
                }
                (PdfFormField::ListBox(l), FieldValue::Text { text }) => {
                    if !l
                        .options()
                        .iter()
                        .any(|o| o.label().map(|v| v == text).unwrap_or(false))
                    {
                        return Err(invalid("cette valeur n'est pas une option de la liste"));
                    }
                    write::Patch::Text(text.clone())
                }
                (PdfFormField::Checkbox(_), FieldValue::Checked { checked }) => {
                    write::Patch::Checked(*checked)
                }
                (PdfFormField::RadioButton(_), FieldValue::Checked { checked: true }) => {
                    write::Patch::RadioOn
                }
                (PdfFormField::RadioButton(_), FieldValue::Checked { checked: false }) => {
                    return Err(invalid(
                        "un bouton radio ne se décoche pas : cochez-en un autre",
                    ));
                }
                _ => return Err(invalid("valeur incompatible avec ce champ")),
            }
        };
        let bytes = self.to_bytes()?;
        let patched = write::patch_field(&bytes, usize::from(page), usize::from(index), &patch)?;
        self.reload_from_bytes(patched)
    }

    /// Aplatit toutes les pages : champs et annotations deviennent du contenu fixe.
    ///
    /// Chaque page est d'abord chargée dans l'environnement de formulaire (rendu minuscule) :
    /// PDFium n'y régénère les apparences des champs qu'à ce moment, et `flatten` ne dessine
    /// que les apparences. Le dictionnaire `/AcroForm` est ensuite retiré (`lopdf`) pour ne
    /// pas laisser de champs orphelins aux autres lecteurs.
    pub fn flatten_all(&mut self) -> CoreResult<()> {
        let had_form = self.form_info()?.form_type != FormType::None;
        {
            let _guard = engine::guard();
            let count = self.count_unlocked();
            for i in 0..count {
                let mut page = self.inner().pages().get(self.checked_index_unlocked(i)?)?;
                load_form_environment(&page)?;
                page.flatten()?;
            }
        }
        let bytes = self.to_bytes()?;
        let bytes = if had_form {
            write::strip_acroform(&bytes)?
        } else {
            bytes
        };
        self.reload_from_bytes(bytes)
    }

    /// Les champs de la page `page`, dans l'ordre de `/Annots`.
    pub fn form_fields(&self, page: u16) -> CoreResult<Vec<FormField>> {
        let _guard = engine::guard();
        let pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        if pdf_page
            .annotations()
            .iter()
            .any(|a| a.annotation_type() == PdfPageAnnotationType::Widget)
        {
            // Les widgets absents de `/AcroForm /Fields` n'apparaissent qu'une fois la page
            // chargée dans l'environnement de formulaire ; idem pour les apparences.
            load_form_environment(&pdf_page)?;
        }
        let mut out = Vec::new();
        for (index, ann) in pdf_page
            .annotations()
            .iter()
            .take(MAX_ANNOTATIONS_PER_PAGE)
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

fn invalid(msg: &str) -> CoreError {
    CoreError::InvalidOperation(msg.to_string())
}

/// Charge la page dans l'environnement de formulaire de PDFium par un rendu minuscule.
fn load_form_environment(page: &pdfium_render::prelude::PdfPage<'_>) -> CoreResult<()> {
    let config = PdfRenderConfig::new()
        .set_target_width(8)
        .render_form_data(true)
        .render_annotations(true);
    page.render_with_config(&config)?;
    Ok(())
}

/// Nom, lecture seule, requis : communs à tous les genres (trait `PdfFormFieldCommon`).
fn common<F: PdfFormFieldCommon>(f: &F) -> (String, bool, bool) {
    (
        clamp(f.name().unwrap_or_default()),
        f.is_read_only(),
        f.is_required(),
    )
}

/// Un PDF est une donnée non fiable : chaque chaîne lue est bornée.
fn clamp(s: String) -> String {
    if s.chars().count() > MAX_STRING_CHARS {
        s.chars().take(MAX_STRING_CHARS).collect()
    } else {
        s
    }
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
                    // La valeur d'un champ mot de passe n'est pas exposée (ni affichée,
                    // ni journalisée) : l'utilisateur en saisit une nouvelle.
                    if t.is_password() {
                        None
                    } else {
                        Some(clamp(t.value().unwrap_or_default()))
                    },
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
                    c.group_value().map(clamp),
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
                    r.group_value().map(clamp),
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
                    c.value().map(clamp),
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
                    l.value().map(clamp),
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
            label: clamp(o.label().cloned().unwrap_or_default()),
            selected: o.is_set(),
        })
        .collect()
}
