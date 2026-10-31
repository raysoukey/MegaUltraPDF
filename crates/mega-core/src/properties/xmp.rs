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
pub(super) fn xmp_raw<'a>(xmp: &'a str, name: &str) -> Option<&'a str> {
    let attribute = ['"', '\''].into_iter().find_map(|quote| {
        let start = format!(" {name}={quote}");
        let from = xmp.find(&start)? + start.len();
        Some(&xmp[from..from + xmp[from..].find(quote)?])
    });
    attribute.or_else(|| {
        let open = format!("<{name}>");
        let from = xmp.find(&open)? + open.len();
        Some(&xmp[from..from + xmp[from..].find(&format!("</{name}>"))?])
    })
}

/// Valeur d'une propriété XMP simple, nettoyée, sans limite autre que `MAX_METADATA_CHARS`.
pub(super) fn xmp_text(xmp: &str, name: &str) -> Option<String> {
    clean_xmp_text(xmp_raw(xmp, name)?)
}

/// Texte XMP prêt à afficher : CDATA déballé, entités XML décodées (nommées et numériques),
/// caractères de contrôle retirés (tabulation et fins de ligne deviennent des espaces),
/// espaces rognés, tronqué à `MAX_METADATA_CHARS`. `None` si rien ne reste : une donnée
/// de fichier n'est pas fiable.
pub(super) fn clean_xmp_text(raw: &str) -> Option<String> {
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
pub(super) fn decode_entities(raw: &str, out: &mut String) {
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
pub(super) fn xmp_of(doc: &lopdf::Document) -> Option<String> {
    let metadata = doc.catalog().ok()?.get(b"Metadata").ok()?;
    let (_, stream) = doc.dereference(metadata).ok()?;
    let xmp = crate::crypt::stream_bytes(stream.as_stream().ok()?)?;
    Some(String::from_utf8_lossy(&xmp).into_owned())
}

/// Vrai si le XMP déclare PDF/A (`pdfaid:part`) ou PDF/UA (`pdfuaid:part`) : un fichier qui
/// s'en réclame interdit les polices non incorporées, donc Helvetica (audit E6).
pub(crate) fn declares_conformance(doc: &lopdf::Document) -> bool {
    pdfa_claim_of(doc).is_some()
        || xmp_of(doc).is_some_and(|xmp| xmp_value(&xmp, "pdfuaid:part").is_some())
}

/// Valeur d'une propriété XMP courte (un chiffre, une lettre) : `pdfaid:part`.
pub(super) fn xmp_value(xmp: &str, name: &str) -> Option<String> {
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

/// Reporte `update` dans le flux XMP du catalogue, pour les propriétés **déjà présentes**
/// (aucune n'est créée : pas de générateur XMP). Un flux illisible est laissé tel quel.
pub(crate) fn update_xmp(doc: &mut lopdf::Document, update: &XmpUpdate<'_>) {
    edit_xmp(doc, |mut text| {
        for (tag, value) in [
            ("dc:title", update.title),
            ("dc:creator", update.author),
            ("dc:description", update.subject),
        ] {
            if let Some(v) = value
                && let Some(new) = replace_li(&text, tag, &xml_escape(v))
            {
                text = new;
            }
        }
        if let Some(v) = update.keywords
            && let Some(new) = replace_simple(&text, "pdf:Keywords", &xml_escape(v))
        {
            text = new;
        }
        text
    });
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
pub(super) fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Remplace le texte d'une entrée `<rdf:li>` de l'élément `tag` (titre, auteur, sujet : des
/// listes `rdf:Alt` / `rdf:Seq`) : celle de langue `x-default` (celle que les lecteurs
/// affichent) s'il y en a une, sinon la première ; `None` si l'élément ou l'entrée manque.
pub(super) fn replace_li(xmp: &str, tag: &str, value: &str) -> Option<String> {
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
        let attr = format!(" {tag}={quote}");
        if let Some(found) = xmp.find(&attr) {
            let start = found + attr.len();
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
}
