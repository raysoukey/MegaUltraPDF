// SPDX-License-Identifier: GPL-3.0-or-later
//! Lecture et écriture du flux XMP (`/Metadata`) : propriétés, dates, conformité PDF/A, et
//! report des modifications de métadonnées. Séparé de `properties` pour garder ce fichier
//! lisible : tout ce qui parle XML y vit.

use super::MAX_METADATA_CHARS;

/// Texte d'une entrée `<rdf:li>` de l'élément `tag` : celle de langue `x-default` si
/// `prefer_default` et qu'elle existe, sinon la première.
pub(super) fn xmp_li(xmp: &str, tag: &str, prefer_default: bool) -> Option<String> {
    let start = xmp.find(&format!("<{tag}>"))?;
    let end = start + xmp[start..].find(&format!("</{tag}>"))?;
    let body = &xmp[start..end];
    let default = if prefer_default {
        ["xml:lang=\"x-default\"", "xml:lang='x-default'"]
            .iter()
            .find_map(|attr| body.find(attr))
            .and_then(|at| body[..at].rfind("<rdf:li"))
    } else {
        None
    };
    let li = default.or_else(|| body.find("<rdf:li"))?;
    let open_end = li + body[li..].find('>')? + 1;
    if body[..open_end].ends_with("/>") {
        return None;
    }
    let close = open_end + body[open_end..].find("</rdf:li>")?;
    clean_xmp_text(&body[open_end..close])
}

/// Texte brut d'une propriété XMP simple, écrite en attribut (`nom="2"`, guillemets ou
/// apostrophes) ou en élément (`<nom>2</nom>`, jusqu'à la balise fermante, pour qu'une
/// section CDATA ne la coupe pas). Partagé par `xmp_value` et `xmp_text`.
fn xmp_raw<'a>(xmp: &'a str, name: &str) -> Option<&'a str> {
    let attribute = ['"', '\''].into_iter().find_map(|quote| {
        let from = attribute_value_start(xmp, name, quote)?;
        Some(&xmp[from..from + xmp[from..].find(quote)?])
    });
    attribute.or_else(|| {
        let open = format!("<{name}>");
        let from = xmp.find(&open)? + open.len();
        Some(&xmp[from..from + xmp[from..].find(&format!("</{name}>"))?])
    })
}

/// Début de la valeur de l'attribut `name={quote}…` : le premier dont le nom est précédé d'un
/// blanc XML (espace, tabulation, fin de ligne : un outil peut aller à la ligne entre deux
/// attributs), pour que `xpdfaid:part` ne passe pas pour `pdfaid:part`.
fn attribute_value_start(xmp: &str, name: &str, quote: char) -> Option<usize> {
    let pattern = format!("{name}={quote}");
    let mut from = 0;
    while let Some(found) = xmp[from..].find(&pattern) {
        let at = from + found;
        if xmp[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
        {
            return Some(at + pattern.len());
        }
        from = at + pattern.len();
    }
    None
}

/// Valeur d'une propriété XMP simple, nettoyée, sans limite autre que `MAX_METADATA_CHARS`.
pub(super) fn xmp_text(xmp: &str, name: &str) -> Option<String> {
    clean_xmp_text(xmp_raw(xmp, name)?)
}

/// Texte XMP prêt à afficher : CDATA déballé, entités XML décodées (nommées et numériques),
/// caractères de contrôle retirés (tabulation et fins de ligne deviennent des espaces),
/// espaces rognés, tronqué à `MAX_METADATA_CHARS`. `None` si rien ne reste : une donnée
/// de fichier n'est pas fiable.
fn clean_xmp_text(raw: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = raw;
    while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix("<![CDATA[") {
            // Une section CDATA non fermée court jusqu'à la fin.
            let end = inner.find("]]>").unwrap_or(inner.len());
            out.push_str(&inner[..end]);
            rest = inner.get(end + 3..).unwrap_or("");
        } else {
            let end = rest.find("<![CDATA[").unwrap_or(rest.len());
            decode_entities(&rest[..end], &mut out);
            rest = &rest[end..];
        }
    }
    // Rogner **avant** de tronquer : des milliers de blancs en tête ne doivent pas remplir
    // toute la place et faire disparaître la valeur.
    let text: String = out
        .chars()
        .filter_map(|c| match c {
            '\t' | '\n' | '\r' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .skip_while(|c| c.is_whitespace())
        .take(MAX_METADATA_CHARS)
        .collect();
    let text = text.trim_end();
    (!text.is_empty()).then(|| text.to_string())
}

/// Ajoute à `out` le texte `raw` dont les entités XML sont décodées. Une entité inconnue ou
/// mal formée (`R&D;`, `&` sans `;` proche) reste telle quelle ; un numérique bien formé mais
/// invalide (`&#0;`, `&#xD800;`) est écarté.
fn decode_entities(raw: &str, out: &mut String) {
    // Longueur maximale d'une entité acceptée, en OCTETS : la recherche du `;` est bornée
    // (pas de balayage quadratique sur un flux hostile) et tombe sur une frontière de
    // caractère, `;` étant ASCII.
    const MAX_ENTITY_BYTES: usize = 12;
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let semi = rest
            .as_bytes()
            .iter()
            .take(MAX_ENTITY_BYTES)
            .position(|b| *b == b';');
        let Some(semi) = semi else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let body = &rest[1..semi];
        let decoded = match body {
            "lt" => Some(Some('<')),
            "gt" => Some(Some('>')),
            "quot" => Some(Some('"')),
            "apos" => Some(Some('\'')),
            "amp" => Some(Some('&')),
            _ => body.strip_prefix('#').and_then(|num| {
                let code = match num.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => num.parse().ok()?,
                };
                Some(char::from_u32(code))
            }),
        };
        match decoded {
            Some(c) => {
                out.extend(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
}

/// Date XMP (ISO 8601, `2020-01-02T03:04:05+01:00`) en date PDF (`D:20200102030405+01'00'`),
/// le format brut que le frontend sait mettre en forme.
pub(super) fn xmp_date_to_pdf(iso: &str) -> Option<String> {
    let tz_at = iso
        .char_indices()
        .skip(10)
        .find(|(_, c)| matches!(c, 'Z' | '+' | '-'))
        .map(|(i, _)| i);
    let (date, tz) = iso.split_at(tz_at.unwrap_or(iso.len()));
    // Les fractions de seconde n'ont pas de place dans une date PDF.
    let date = date.split('.').next().unwrap_or(date);
    let digits: String = date.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 4 || !digits.len().is_multiple_of(2) || digits.len() > 14 {
        return None;
    }
    let zone = match tz.as_bytes() {
        [b'Z', ..] => "Z".to_string(),
        [sign @ (b'+' | b'-'), ..] => {
            // `hh:mm`, `hhmm` ou `hh` (ISO 8601). `get` plutôt qu'un découpage direct : sur un
            // texte non ASCII, une borne au milieu d'un caractère rend `None` au lieu de paniquer.
            let rest = tz.get(1..)?;
            let (hh, mm) = match (rest.len(), rest.as_bytes().get(2)) {
                (2, _) => (rest, "00"),
                (4, _) => (rest.get(..2)?, rest.get(2..)?),
                (5, Some(b':')) => (rest.get(..2)?, rest.get(3..)?),
                _ => return None,
            };
            // Un fuseau mal formé rend la date illisible plutôt que fausse.
            if !hh.bytes().chain(mm.bytes()).all(|b| b.is_ascii_digit()) {
                return None;
            }
            format!("{}{hh}'{mm}'", char::from(*sign))
        }
        _ => String::new(),
    };
    Some(format!("D:{digits}{zone}"))
}

/// Conformité PDF/A déclarée par le flux XMP `/Metadata` du catalogue (`pdfaid:part`, et
/// `pdfaid:conformance` s'il existe), ex. `"PDF/A-2B"`.
pub(super) fn pdfa_claim_of(doc: &lopdf::Document) -> Option<String> {
    let xmp = xmp_of(doc)?;
    let part = xmp_value(&xmp, "pdfaid:part")?;
    let conformance = xmp_value(&xmp, "pdfaid:conformance").unwrap_or_default();
    Some(format!("PDF/A-{part}{}", conformance.to_uppercase()))
}

/// Texte du flux XMP `/Metadata` du catalogue, s'il existe.
fn xmp_of(doc: &lopdf::Document) -> Option<String> {
    let metadata = doc.catalog().ok()?.get(b"Metadata").ok()?;
    let (_, stream) = doc.dereference(metadata).ok()?;
    let xmp = crate::crypt::stream_bytes(stream.as_stream().ok()?)?;
    Some(String::from_utf8_lossy(&xmp).into_owned())
}

/// Conformité **déclarée** par le XMP du document : ce qu'on ajoute au fichier (filigrane,
/// numéros, couche OCR, métadonnées) doit la respecter, sans quoi un fichier conforme ne le
/// serait plus après une simple modification (audit E6).
///
/// Pourquoi une structure plutôt qu'un booléen : chaque partie interdit autre chose. PDF/A et
/// PDF/UA refusent les polices non incorporées (donc Helvetica) ; PDF/A refuse DeviceRGB sans
/// profil de sortie RVB ; PDF/A-1 refuse toute transparence ; PDF/A-4 refuse `/Info`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Conformance {
    /// Partie de PDF/A déclarée (`pdfaid:part` : 1 à 4), `None` sans déclaration PDF/A (ou
    /// une partie illisible).
    pub pdfa_part: Option<u8>,
    /// Le document déclare PDF/UA (`pdfuaid:part`).
    pub pdfua: bool,
}

/// Conformité déclarée par le XMP `/Metadata` du catalogue, lu et décodé **une** fois ;
/// `None` si le document ne déclare ni PDF/A ni PDF/UA.
pub(crate) fn conformance_of(doc: &lopdf::Document) -> Option<Conformance> {
    let xmp = xmp_of(doc)?;
    let pdfa = xmp_value(&xmp, "pdfaid:part");
    let pdfua = xmp_value(&xmp, "pdfuaid:part").is_some();
    if pdfa.is_none() && !pdfua {
        return None;
    }
    Some(Conformance {
        pdfa_part: pdfa.and_then(|part| part.parse().ok()),
        pdfua,
    })
}

/// Valeur d'une propriété XMP courte (un chiffre, une lettre) : `pdfaid:part`.
fn xmp_value(xmp: &str, name: &str) -> Option<String> {
    xmp_raw(xmp, name)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && v.len() <= 8)
}

/// Valeurs à reporter dans les métadonnées XMP (`None` = inchangé, vide = effacé).
pub(crate) struct XmpUpdate<'a> {
    pub title: Option<&'a str>,
    pub author: Option<&'a str>,
    pub subject: Option<&'a str>,
    pub keywords: Option<&'a str>,
}

/// Reporte `update` dans le flux XMP du catalogue. Une propriété absente est **créée** dans
/// le premier `rdf:Description` : sans cela, `/Info /Title` et `dc:title` divergeraient et
/// PDF/A (ISO 19005-1, 6.7.3) serait violé (audit E6) ; une valeur vide retire l'élément,
/// comme elle retire la clé de `/Info`. Sans flux `/Metadata`, rien n'est créé (pas de
/// générateur de paquet XMP). Un flux illisible est laissé tel quel.
pub(crate) fn update_xmp(doc: &mut lopdf::Document, update: &XmpUpdate<'_>) {
    edit_xmp(doc, |mut text| {
        for (tag, value, kind) in [
            ("dc:title", update.title, ListKind::Alt),
            ("dc:creator", update.author, ListKind::Seq),
            ("dc:description", update.subject, ListKind::Alt),
        ] {
            if let Some(v) = value {
                text = set_list_property(text, tag, kind, v);
            }
        }
        if let Some(v) = update.keywords {
            text = set_simple_property(text, "pdf:Keywords", v);
        }
        text
    });
}

/// Forme d'une propriété XMP à liste : `rdf:Alt` (une valeur par langue) ou `rdf:Seq`
/// (liste ordonnée, ex. les auteurs).
#[derive(Clone, Copy)]
enum ListKind {
    Alt,
    Seq,
}

/// Pose la propriété à liste `tag` : remplace l'entrée existante, la crée si elle manque,
/// retire l'élément si `value` est vide.
fn set_list_property(text: String, tag: &str, kind: ListKind, value: &str) -> String {
    if value.is_empty() {
        return remove_element(&text, tag).unwrap_or(text);
    }
    let escaped = xml_escape(value);
    if let Some(new) = replace_li(&text, tag, &escaped) {
        return new;
    }
    // Élément absent, ou présent mais sans entrée exploitable : on le refait proprement.
    let element = match kind {
        ListKind::Alt => format!(
            "<{tag}><rdf:Alt><rdf:li xml:lang=\"x-default\">{escaped}</rdf:li></rdf:Alt></{tag}>"
        ),
        ListKind::Seq => format!("<{tag}><rdf:Seq><rdf:li>{escaped}</rdf:li></rdf:Seq></{tag}>"),
    };
    // Si l'insertion échoue (aucun `rdf:Description`), le texte d'origine est rendu tel quel :
    // l'ancien élément, même inexploitable, n'est pas perdu pour rien.
    let base = remove_element(&text, tag);
    let inserted = insert_property(base.as_deref().unwrap_or(&text), tag, &element);
    inserted.unwrap_or(text)
}

/// Pose la propriété simple `tag` (élément ou attribut), de la même façon.
pub(super) fn set_simple_property(text: String, tag: &str, value: &str) -> String {
    if value.is_empty() {
        let removed = remove_element(&text, tag);
        return removed
            .or_else(|| replace_simple(&text, tag, ""))
            .unwrap_or(text);
    }
    let escaped = xml_escape(value);
    if let Some(new) = replace_simple(&text, tag, &escaped) {
        return new;
    }
    let element = format!("<{tag}>{escaped}</{tag}>");
    insert_property(&text, tag, &element).unwrap_or(text)
}

/// Retire l'élément `<tag>…</tag>` (ou `<tag/>`) ; `None` s'il n'y en a pas.
fn remove_element(xmp: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let mut from = 0;
    while let Some(found) = xmp[from..].find(&open) {
        let start = from + found;
        let after = &xmp[start + open.len()..];
        // `<dc:titleX` n'est pas `<dc:title` : la balise doit se terminer là (par `>`, `/` ou
        // tout blanc XML : espace, tabulation, fin de ligne).
        if !after
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c == '/' || c.is_whitespace())
        {
            from = start + open.len();
            continue;
        }
        let end = if let Some(close) = after.find('>').filter(|&i| after[..i].ends_with('/')) {
            start + open.len() + close + 1
        } else {
            let closing = format!("</{tag}>");
            start + open.len() + after.find(&closing)? + closing.len()
        };
        return Some(format!("{}{}", &xmp[..start], &xmp[end..]));
    }
    None
}

/// Fin (index du `>`) de la balise ouvrante qui commence en `start` : le premier `>` hors
/// guillemets, car une valeur d'attribut peut en contenir un.
///
/// \[Apprentissage\] Une **petite machine à états** : l'état est `quote` (`None` hors d'une
/// valeur, `Some(q)` dans une valeur ouverte par le guillemet `q`) ; chaque caractère fait
/// passer d'un état à l'autre, et le `match` sur le couple (état, caractère) énumère toutes les
/// transitions. Plus sûr qu'un `find('>')`, qui s'arrêterait dans `rdf:about="a>b"`.
fn open_tag_end(xmp: &str, start: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in xmp[start..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(start + i),
            _ => {}
        }
    }
    None
}

/// Insère `element` au début du `rdf:Description` dont la balise ouvrante déclare l'espace de
/// noms du préfixe de `tag` (le premier, si plusieurs : les fichiers PDF/A écrivent souvent une
/// description par schéma). Aucun ne le déclare : le préfixe (`dc`, `pdf`, `xmp`) est déclaré
/// sur la première description. Un `rdf:Description` vide (`<rdf:Description …/>`) est rouvert.
/// `None` s'il n'y a aucune description.
fn insert_property(xmp: &str, tag: &str, element: &str) -> Option<String> {
    let prefix = tag.split(':').next().unwrap_or("");
    let declares = format!("xmlns:{prefix}=");
    // Les balises ouvrantes `rdf:Description` : (début, index du `>`).
    let mut tags = Vec::new();
    let mut from = 0;
    while let Some(found) = xmp[from..].find("<rdf:Description") {
        let start = from + found;
        let after = xmp[start + "<rdf:Description".len()..].chars().next();
        from = start + "<rdf:Description".len();
        if !after.is_some_and(|c| c.is_whitespace() || c == '>' || c == '/') {
            continue;
        }
        tags.push((start, open_tag_end(xmp, start)?));
    }
    let declaring = tags
        .iter()
        .find(|(start, end)| xmp[*start..*end].contains(&declares));
    let (open_end, declaration) = match (declaring, tags.first()) {
        (Some((_, end)), _) => (*end, String::new()),
        (None, Some((_, end))) => {
            let uri = match prefix {
                "dc" => "http://purl.org/dc/elements/1.1/",
                "pdf" => "http://ns.adobe.com/pdf/1.3/",
                "xmp" => "http://ns.adobe.com/xap/1.0/",
                _ => return None,
            };
            (*end, format!(" xmlns:{prefix}=\"{uri}\""))
        }
        (None, None) => return None,
    };
    let self_closed = xmp[..open_end].ends_with('/');
    let head_end = if self_closed { open_end - 1 } else { open_end };
    let mut out = String::with_capacity(xmp.len() + element.len() + 64);
    out.push_str(&xmp[..head_end]);
    out.push_str(&declaration);
    out.push('>');
    out.push_str(element);
    if self_closed {
        out.push_str("</rdf:Description>");
    }
    out.push_str(&xmp[open_end + 1..]);
    Some(out)
}

/// Applique `edit` au texte du flux `/Metadata` du catalogue et le réécrit non compressé.
pub(super) fn edit_xmp(doc: &mut lopdf::Document, edit: impl FnOnce(String) -> String) {
    let Some(id) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Metadata").ok())
        .and_then(|m| m.as_reference().ok())
    else {
        return;
    };
    let Ok(stream) = doc
        .get_object_mut(id)
        .and_then(lopdf::Object::as_stream_mut)
    else {
        return;
    };
    let Some(data) = crate::crypt::stream_bytes(stream) else {
        return;
    };
    let Ok(text) = String::from_utf8(data) else {
        return;
    };
    let edited = edit(text);
    stream.dict.remove(b"Filter");
    stream.dict.remove(b"DecodeParms");
    stream.set_plain_content(edited.into_bytes());
}

/// Échappe `&`, `<`, `>`, `"` et `'` pour du texte XML.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Remplace le texte d'une entrée `<rdf:li>` de l'élément `tag` (titre, auteur, sujet : des
/// listes `rdf:Alt` / `rdf:Seq`) : celle de langue `x-default` (celle que les lecteurs
/// affichent) s'il y en a une, sinon la première ; `None` si l'élément ou l'entrée manque.
fn replace_li(xmp: &str, tag: &str, value: &str) -> Option<String> {
    let start = xmp.find(&format!("<{tag}>"))?;
    let end = start + xmp[start..].find(&format!("</{tag}>"))?;
    let default = ["xml:lang=\"x-default\"", "xml:lang='x-default'"]
        .iter()
        .find_map(|attr| xmp[start..end].find(attr))
        .and_then(|at| xmp[start..start + at].rfind("<rdf:li"));
    let li = match default {
        Some(at) => start + at,
        None => start + xmp[start..end].find("<rdf:li")?,
    };
    let open_end = li + xmp[li..end].find('>')? + 1;
    if xmp[..open_end].ends_with("/>") {
        return None;
    }
    let close = open_end + xmp[open_end..end].find("</rdf:li>")?;
    Some(format!("{}{value}{}", &xmp[..open_end], &xmp[close..]))
}

/// Remplace la valeur d'une propriété simple, écrite en élément (`<tag>v</tag>`) ou en
/// attribut (`tag="v"`) ; `None` si elle est absente.
pub(super) fn replace_simple(xmp: &str, tag: &str, value: &str) -> Option<String> {
    let open = format!("<{tag}>");
    if let Some(start) = xmp.find(&open) {
        let from = start + open.len();
        let end = from + xmp[from..].find(&format!("</{tag}>"))?;
        return Some(format!("{}{value}{}", &xmp[..from], &xmp[end..]));
    }
    // Attribut entre guillemets ou entre apostrophes (les deux sont du XML valide).
    for quote in ['"', '\''] {
        if let Some(start) = attribute_value_start(xmp, tag, quote) {
            let end = start + xmp[start..].find(quote)?;
            return Some(format!("{}{value}{}", &xmp[..start], &xmp[end..]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mineur du Lot 9 : dans une liste multilingue, c'est l'entrée `x-default` que les
    /// lecteurs affichent ; c'est elle qu'on remplace.
    #[test]
    fn xmp_title_replaces_the_x_default_entry() {
        let xmp = "<dc:title><rdf:Alt><rdf:li xml:lang=\"fr\">Ancien</rdf:li>\
                   <rdf:li xml:lang=\"x-default\">Old</rdf:li></rdf:Alt></dc:title>";
        let out = replace_li(xmp, "dc:title", "Nouveau").unwrap();
        assert!(out.contains("\"fr\">Ancien<"), "{out}");
        assert!(out.contains("\"x-default\">Nouveau<"), "{out}");
        // Sans `x-default`, la première entrée.
        let plain = "<dc:title><rdf:Alt><rdf:li>Old</rdf:li></rdf:Alt></dc:title>";
        assert!(
            replace_li(plain, "dc:title", "N")
                .unwrap()
                .contains("<rdf:li>N<")
        );
    }

    /// Mineur du Lot 9 : propriété en attribut entre apostrophes.
    #[test]
    fn xmp_attribute_with_single_quotes_is_replaced() {
        let xmp = "<rdf:Description pdf:Keywords='ancien'/>";
        let out = replace_simple(xmp, "pdf:Keywords", "neuf").unwrap();
        assert_eq!(out, "<rdf:Description pdf:Keywords='neuf'/>");
    }

    /// Relecture de la partie B : des blancs en tête ne mangent pas la valeur (on rogne, puis
    /// on tronque) ; la troncature garde `MAX_METADATA_CHARS` caractères.
    #[test]
    fn xmp_text_is_trimmed_before_being_truncated() {
        let padded = format!("{}Titre", " ".repeat(5_000));
        assert_eq!(clean_xmp_text(&padded).as_deref(), Some("Titre"));
        let long = "é".repeat(MAX_METADATA_CHARS + 50);
        let cleaned = clean_xmp_text(&format!("  {long}  ")).unwrap();
        assert_eq!(cleaned.chars().count(), MAX_METADATA_CHARS);
        assert_eq!(clean_xmp_text(" \n\t ").as_deref(), None);
    }

    /// Fuseau d'une date XMP : `+hh:mm`, `+hhmm` ou `+hh` ; des heures ou minutes qui ne sont
    /// pas des chiffres rendent la date illisible plutôt que fausse.
    #[test]
    fn xmp_date_time_zones() {
        let d = |s: &str| xmp_date_to_pdf(s);
        assert_eq!(
            d("2020-01-02T03:04:05+01:00").as_deref(),
            Some("D:20200102030405+01'00'")
        );
        assert_eq!(
            d("2020-01-02T03:04:05+0530").as_deref(),
            Some("D:20200102030405+05'30'")
        );
        assert_eq!(
            d("2020-01-02T03:04:05-08").as_deref(),
            Some("D:20200102030405-08'00'")
        );
        assert_eq!(
            d("2020-01-02T03:04:05Z").as_deref(),
            Some("D:20200102030405Z")
        );
        assert_eq!(
            d("2020-01-02T03:04:05").as_deref(),
            Some("D:20200102030405")
        );
        assert_eq!(d("2020-01-02T03:04:05+ab:cd"), None);
        assert_eq!(d("2020-01-02T03:04:05+01:x0"), None);
        assert_eq!(d("2020-01-02T03:04:05+1"), None);
    }

    const HEAD: &str = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">";
    const TAIL: &str = "</rdf:RDF></x:xmpmeta>";

    /// Une description par schéma : chaque propriété va dans la description qui déclare son
    /// préfixe, sinon le préfixe n'y serait pas lié (veraPDF).
    #[test]
    fn a_property_lands_in_the_description_that_declares_its_prefix() {
        let xmp = format!(
            "{HEAD}<rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\" pdfaid:part=\"1\"/>             <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"></rdf:Description>             <rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\"></rdf:Description>{TAIL}"
        );
        let out = set_list_property(xmp, "dc:title", ListKind::Alt, "T");
        let out = set_simple_property(out, "pdf:Keywords", "k");
        let dc = out.find("xmlns:dc=").unwrap();
        let pdf = out.find("xmlns:pdf=").unwrap();
        let title = out.find("<dc:title>").unwrap();
        let keywords = out.find("<pdf:Keywords>").unwrap();
        assert!(dc < title && title < pdf, "{out}");
        assert!(pdf < keywords, "{out}");
        assert_eq!(out.matches("xmlns:dc=").count(), 1, "{out}");
    }

    #[test]
    fn a_missing_namespace_is_declared_on_the_first_description() {
        let xmp = format!("{HEAD}<rdf:Description rdf:about=\"\"></rdf:Description>{TAIL}");
        let out = set_simple_property(xmp, "pdf:Keywords", "a");
        assert!(
            out.contains("<rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\"><pdf:Keywords>a</pdf:Keywords></rdf:Description>"),
            "{out}"
        );
    }

    #[test]
    fn a_self_closed_description_is_reopened() {
        let xmp = format!("{HEAD}<rdf:Description rdf:about=\"\"/>{TAIL}");
        let out = set_list_property(xmp, "dc:creator", ListKind::Seq, "Ève");
        assert!(
            out.contains("xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:creator><rdf:Seq><rdf:li>Ève</rdf:li></rdf:Seq></dc:creator></rdf:Description>"),
            "{out}"
        );
    }

    /// Revue de la partie C : un attribut peut suivre une tabulation ou une fin de ligne, et
    /// un nom plus long (`xpdfaid:part`) n'est pas le nôtre.
    #[test]
    fn attributes_after_any_whitespace_are_found() {
        let xmp = "<rdf:Description\n\tpdfaid:part=\"1\"\r\n pdf:Keywords='a'/>";
        assert_eq!(xmp_raw(xmp, "pdfaid:part"), Some("1"));
        assert_eq!(
            replace_simple(xmp, "pdf:Keywords", "b").as_deref(),
            Some("<rdf:Description\n\tpdfaid:part=\"1\"\r\n pdf:Keywords='b'/>")
        );
        assert_eq!(xmp_raw("<a xpdfaid:part=\"3\"/>", "pdfaid:part"), None);
        // Un élément suivi d'une fin de ligne avant ses attributs est bien retiré.
        assert_eq!(
            remove_element("<x><dc:title\nxml:lang=\"fr\">T</dc:title></x>", "dc:title").as_deref(),
            Some("<x></x>")
        );
    }

    /// Revue de la partie C : sans `rdf:Description` où insérer, l'ancien élément (même sans
    /// entrée exploitable) n'est pas perdu.
    #[test]
    fn a_failed_insertion_keeps_the_original_text() {
        let xmp = "<x:xmpmeta><dc:title><rdf:Alt/></dc:title></x:xmpmeta>".to_string();
        let out = set_list_property(xmp.clone(), "dc:title", ListKind::Alt, "Nouveau");
        assert_eq!(out, xmp);
    }

    #[test]
    fn a_greater_than_sign_inside_an_attribute_value_does_not_end_the_tag() {
        let xmp = format!("{HEAD}<rdf:Description rdf:about=\"a>b\" note='x>y'/>{TAIL}");
        let out = set_simple_property(xmp, "pdf:Keywords", "k");
        assert!(
            out.contains("rdf:about=\"a>b\" note='x>y' xmlns:pdf="),
            "{out}"
        );
        assert!(
            out.contains("<pdf:Keywords>k</pdf:Keywords></rdf:Description>"),
            "{out}"
        );
    }
}
