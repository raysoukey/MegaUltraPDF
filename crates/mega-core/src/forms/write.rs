// SPDX-License-Identifier: GPL-3.0-or-later
//! Écriture des valeurs de champs **par `lopdf`** : `/V`, `/AS`, `/NeedAppearances`.
//!
//! \[Apprentissage\] **Deux moteurs, deux représentations d'un même fichier** (ADR-0003).
//! PDFium sait afficher et régénérer des apparences, mais pdfium-render 0.9.4 écrit les
//! états de cases comme des *chaînes* (`(/Yes)`) au lieu de *noms* (`/Yes`), et n'atteint ni
//! le champ parent d'un widget ni le dictionnaire `/AcroForm`. `lopdf` lit le fichier comme
//! un arbre d'objets (dictionnaires, tableaux, noms, chaînes) et nous laisse poser exactement
//! les clés que la norme demande (§12.7.4). Le document est ensuite **rechargé** dans PDFium,
//! qui régénère les apparences grâce à `/NeedAppearances true`.

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};

use crate::crypt::LopdfSession;
use crate::error::{CoreError, CoreResult};

/// Ce qu'il faut écrire.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Patch {
    /// Texte d'un champ texte, combo ou liste (`/V` chaîne).
    Text(String),
    /// Case à cocher : `/AS` et `/V` au nom d'état « on » ou `/Off`.
    Checked(bool),
    /// Bouton radio : devient l'élu du groupe (`/V` du parent, `/AS` de chaque bouton).
    RadioOn,
}

fn structure(e: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidOperation(format!("structure du formulaire illisible : {e}"))
}

/// Chaîne de texte PDF : littérale si ASCII, sinon UTF-16BE avec BOM (§7.9.2.2).
pub(crate) fn pdf_text(s: &str) -> Object {
    if s.is_ascii() {
        Object::string_literal(s)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in s.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}

/// Référence du `index`-ième élément de `/Annots` de la page `page` (0-based).
fn annotation_id(doc: &Document, page: usize, index: usize) -> CoreResult<ObjectId> {
    let page_id = doc
        .get_pages()
        .into_iter()
        .nth(page)
        .map(|(_, id)| id)
        .ok_or_else(|| structure("page introuvable"))?;
    let page_dict = doc.get_dictionary(page_id).map_err(structure)?;
    let annots = page_dict.get(b"Annots").map_err(structure)?;
    let (_, annots) = doc.dereference(annots).map_err(structure)?;
    let array = annots.as_array().map_err(structure)?;
    array
        .get(index)
        .and_then(|o| o.as_reference().ok())
        .ok_or_else(|| structure("annotation introuvable"))
}

/// Nom de l'état « coché » d'un widget : la clé de `/AP /N` qui n'est pas `Off`.
fn on_state(doc: &Document, widget: &Dictionary) -> Vec<u8> {
    let Ok(ap) = widget.get(b"AP") else {
        return b"Yes".to_vec();
    };
    let Ok((_, ap)) = doc.dereference(ap) else {
        return b"Yes".to_vec();
    };
    let Ok(ap) = ap.as_dict() else {
        return b"Yes".to_vec();
    };
    let Ok(n) = ap.get(b"N") else {
        return b"Yes".to_vec();
    };
    let Ok((_, n)) = doc.dereference(n) else {
        return b"Yes".to_vec();
    };
    let Ok(n) = n.as_dict() else {
        return b"Yes".to_vec();
    };
    n.iter()
        .map(|(k, _)| k.clone())
        .find(|k| k.as_slice() != b"Off")
        .unwrap_or_else(|| b"Yes".to_vec())
}

/// Le dictionnaire qui porte la valeur : le widget lui-même s'il est fusionné avec son
/// champ, sinon son `/Parent` (§12.7.3.1 : `/V` est hérité).
fn field_id(doc: &Document, widget_id: ObjectId) -> CoreResult<ObjectId> {
    let widget = doc.get_dictionary(widget_id).map_err(structure)?;
    if widget.has(b"T") || !widget.has(b"Parent") {
        return Ok(widget_id);
    }
    widget
        .get(b"Parent")
        .ok()
        .and_then(|p| p.as_reference().ok())
        .ok_or_else(|| structure("parent du champ introuvable"))
}

/// Pose `/NeedAppearances true` dans `/AcroForm` (créé s'il manque).
fn set_need_appearances(doc: &mut Document) -> CoreResult<()> {
    let acro = doc
        .catalog()
        .map_err(structure)?
        .get(b"AcroForm")
        .ok()
        .cloned();
    match acro {
        Some(Object::Reference(id)) => {
            doc.get_dictionary_mut(id)
                .map_err(structure)?
                .set("NeedAppearances", Object::Boolean(true));
        }
        Some(Object::Dictionary(mut d)) => {
            d.set("NeedAppearances", Object::Boolean(true));
            doc.catalog_mut()
                .map_err(structure)?
                .set("AcroForm", Object::Dictionary(d));
        }
        _ => {
            let mut d = Dictionary::new();
            d.set("NeedAppearances", Object::Boolean(true));
            doc.catalog_mut()
                .map_err(structure)?
                .set("AcroForm", Object::Dictionary(d));
        }
    }
    Ok(())
}

/// Charge le document à modifier ; un document chiffré est déchiffré avec `password` et
/// sera rechiffré à l'identique par [`save`] (Lot 10).
fn load(bytes: &[u8], password: Option<&str>) -> CoreResult<LopdfSession> {
    LopdfSession::load(bytes, password)
}

fn save(session: LopdfSession) -> CoreResult<Vec<u8>> {
    session.finish()
}

/// Applique `patch` au widget `index` de la page `page` et retourne le document modifié.
pub(crate) fn patch_field(
    bytes: &[u8],
    password: Option<&str>,
    page: usize,
    index: usize,
    patch: &Patch,
) -> CoreResult<Vec<u8>> {
    let mut session = load(bytes, password)?;
    let doc = &mut session.doc;
    let widget_id = annotation_id(doc, page, index)?;
    let field = field_id(doc, widget_id)?;
    match patch {
        Patch::Text(text) => {
            doc.get_dictionary_mut(field)
                .map_err(structure)?
                .set("V", pdf_text(text));
            if field != widget_id {
                // Un `/V` périmé sur le widget tromperait les lecteurs qui ne remontent pas.
                doc.get_dictionary_mut(widget_id)
                    .map_err(structure)?
                    .remove(b"V");
            }
        }
        Patch::Checked(checked) => {
            let widget = doc.get_dictionary(widget_id).map_err(structure)?;
            let on = on_state(doc, widget);
            let state = if *checked { on } else { b"Off".to_vec() };
            doc.get_dictionary_mut(widget_id)
                .map_err(structure)?
                .set("AS", Object::Name(state.clone()));
            doc.get_dictionary_mut(field)
                .map_err(structure)?
                .set("V", Object::Name(state));
        }
        Patch::RadioOn => {
            let widget = doc.get_dictionary(widget_id).map_err(structure)?;
            let on = on_state(doc, widget);
            doc.get_dictionary_mut(field)
                .map_err(structure)?
                .set("V", Object::Name(on.clone()));
            let kids: Vec<ObjectId> = doc
                .get_dictionary(field)
                .map_err(structure)?
                .get(b"Kids")
                .ok()
                .and_then(|k| k.as_array().ok())
                .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
                .unwrap_or_else(|| vec![widget_id]);
            for kid in kids {
                let state = if kid == widget_id {
                    on.clone()
                } else {
                    b"Off".to_vec()
                };
                doc.get_dictionary_mut(kid)
                    .map_err(structure)?
                    .set("AS", Object::Name(state));
            }
        }
    }
    set_need_appearances(doc)?;
    save(session)
}

/// Retire `/AcroForm` du catalogue (après aplatissement : plus de champs orphelins).
pub(crate) fn strip_acroform(bytes: &[u8], password: Option<&str>) -> CoreResult<Vec<u8>> {
    let mut session = load(bytes, password)?;
    let catalog = session.doc.catalog_mut().map_err(structure)?;
    if catalog.remove(b"AcroForm").is_none() {
        return Ok(bytes.to_vec());
    }
    save(session)
}
