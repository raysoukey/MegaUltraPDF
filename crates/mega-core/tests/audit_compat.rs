// SPDX-License-Identifier: GPL-3.0-or-later
//! Banc de mesure **jetable** de l'audit de compatibilité PDF (31 octobre 2026).
//!
//! Ce fichier n'est pas une suite de tests : chaque expérience **imprime un verdict**
//! (`[E1a] libellé => valeur`) sans jamais faire échouer la commande, pour que le rapport
//! d'audit repose sur des mesures et non sur des déductions. Seule la mise en place d'une
//! expérience (lecture d'une fixture, construction d'un squelette PDF) a le droit de
//! paniquer. Les expériences sont ignorées par défaut ; lancer :
//!
//! ```text
//! MEGA_QPDF='C:\Program Files\qpdf 12.4.2\bin\qpdf.exe' \
//!   cargo test -p mega-core --test audit_compat -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Les sorties PDF sont écrites dans `target/audit-compat/` pour inspection. Au Lot 14, les
//! expériences dont le verdict devient une exigence seront converties en vrais tests et ce
//! fichier supprimé.
//!
//! Outils externes facultatifs (variables d'environnement) : `MEGA_QPDF`, `MEGA_MUTOOL`,
//! `MEGA_PDFJS_CHECK` (script Node), `MEGA_VERAPDF`. Absents, ils donnent « outil absent ».

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;

use lopdf::{Dictionary, Document as Lo, Object, ObjectId};
use mega_core::Document;
use mega_core::digital_signature::{SignAppearance, SignOptions, SigningIdentity};
use mega_core::edit::EditCommand;
use mega_core::testing;

// --- Helpers ------------------------------------------------------------------------------

const IGNORE: &str = "banc de mesure, lancer avec --ignored";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixture(name)).unwrap()
}

fn out_dir() -> PathBuf {
    let d = PathBuf::from(format!(
        "{}/../../target/audit-compat",
        env!("CARGO_MANIFEST_DIR")
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn save(id: &str, bytes: &[u8]) -> PathBuf {
    let p = out_dir().join(format!("{id}.pdf"));
    std::fs::write(&p, bytes).unwrap();
    p
}

fn verdict(id: &str, label: &str, value: impl std::fmt::Display) {
    println!("[{id}] {label} => {value}");
}

/// Exécute une expérience ; une panique de mise en place devient un verdict `PANIQUE`.
fn guarded(id: &str, f: impl FnOnce()) {
    if let Err(e) = catch_unwind(AssertUnwindSafe(f)) {
        let msg = e
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "?".to_string());
        verdict(id, "PANIQUE (mise en place)", msg.replace('\n', " | "));
    }
}

fn qpdf() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(v) = std::env::var_os("MEGA_QPDF").filter(|v| !v.is_empty()) {
        candidates.push(PathBuf::from(v));
    }
    candidates.push(PathBuf::from(r"C:\Program Files\qpdf 12.4.2\bin\qpdf.exe"));
    candidates.push(PathBuf::from("qpdf"));
    candidates.into_iter().find(|exe| {
        Command::new(exe)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

fn qpdf_check(path: &Path, password: Option<&str>) -> String {
    let Some(q) = qpdf() else {
        return "qpdf absent".into();
    };
    let mut c = Command::new(q);
    if let Some(p) = password {
        c.arg(format!("--password={p}"));
    }
    let o = c.arg("--check").arg(path).output().unwrap();
    let rep = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    let warnings: Vec<&str> = rep.lines().filter(|l| l.contains("WARNING")).collect();
    let errors: Vec<&str> = rep.lines().filter(|l| l.contains("ERROR")).collect();
    let detail = warnings
        .iter()
        .chain(errors.iter())
        .take(3)
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "qpdf code {:?}, warnings {}, erreurs {}{}",
        o.status.code(),
        warnings.len(),
        errors.len(),
        if detail.is_empty() {
            String::new()
        } else {
            format!(" [{detail}]")
        }
    )
}

fn qpdf_show_encryption(path: &Path, password: &str) -> String {
    let Some(q) = qpdf() else {
        return "qpdf absent".into();
    };
    let o = Command::new(q)
        .arg(format!("--password={password}"))
        .arg("--show-encryption")
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ; ")
}

/// Enveloppe générique d'un outil externe désigné par une variable d'environnement.
fn external(env: &str, args: &[&str]) -> String {
    let Some(exe) = std::env::var_os(env).filter(|v| !v.is_empty()) else {
        return format!("outil absent ({env})");
    };
    let o = match Command::new(exe).args(args).output() {
        Ok(o) => o,
        Err(e) => return format!("{env} : lancement impossible : {e}"),
    };
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    let noisy: Vec<&str> = out
        .lines()
        .filter(|l| {
            let l = l.to_ascii_lowercase();
            l.contains("warning") || l.contains("error") || l.contains("fail")
        })
        .take(3)
        .collect();
    format!(
        "code {:?}, lignes {}, alertes {} {:?}",
        o.status.code(),
        out.lines().count(),
        noisy.len(),
        noisy
    )
}

fn mutool_check(path: &Path, password: Option<&str>) -> String {
    let p = path.to_string_lossy().into_owned();
    let mut args = vec!["info"];
    if let Some(pw) = password {
        args.push("-p");
        args.push(pw);
    }
    args.push(&p);
    external("MEGA_MUTOOL", &args)
}

fn pdfjs_check(path: &Path, password: Option<&str>) -> String {
    let Some(script) = std::env::var_os("MEGA_PDFJS_CHECK").filter(|v| !v.is_empty()) else {
        return "outil absent (MEGA_PDFJS_CHECK)".into();
    };
    let script = script.to_string_lossy().into_owned();
    let p = path.to_string_lossy().into_owned();
    let mut args = vec![script.as_str()];
    if let Some(pw) = password {
        args.push("--password");
        args.push(pw);
    }
    args.push(p.as_str());
    let o = match Command::new("node").args(&args).output() {
        Ok(o) => o,
        Err(e) => return format!("node : {e}"),
    };
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    format!(
        "code {:?} : {}",
        o.status.code(),
        out.lines().last().unwrap_or("").trim()
    )
}

fn verapdf_check(path: &Path) -> String {
    let p = path.to_string_lossy().into_owned();
    external("MEGA_VERAPDF", &["--format", "text", &p])
}

fn lo(bytes: &[u8]) -> Lo {
    Lo::load_mem_with_options(
        bytes,
        lopdf::LoadOptions {
            max_decompressed_size: Some(256 << 20),
            ..lopdf::LoadOptions::default()
        },
    )
    .unwrap()
}

fn deref(doc: &Lo, o: &Object) -> Object {
    doc.dereference(o)
        .map(|(_, o)| o.clone())
        .unwrap_or(Object::Null)
}

fn root(doc: &Lo) -> Dictionary {
    doc.catalog().unwrap().clone()
}

fn keys(d: &Dictionary) -> Vec<String> {
    let mut v: Vec<String> = d
        .iter()
        .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
        .collect();
    v.sort();
    v
}

/// Suit `path` depuis le catalogue en déréférençant chaque étape.
fn has_path(doc: &Lo, path: &[&str]) -> bool {
    let mut cur = Object::Dictionary(root(doc));
    for k in path {
        let Ok(d) = cur.as_dict() else {
            return false;
        };
        let Ok(v) = d.get(k.as_bytes()) else {
            return false;
        };
        cur = deref(doc, v);
    }
    true
}

fn info(doc: &Lo) -> Option<Dictionary> {
    let i = doc.trailer.get(b"Info").ok()?;
    deref(doc, i).as_dict().ok().cloned()
}

fn trailer_ids(doc: &Lo) -> Vec<String> {
    doc.trailer
        .get(b"ID")
        .ok()
        .and_then(|i| i.as_array().ok())
        .map(|a| {
            a.iter()
                .map(|s| {
                    s.as_str()
                        .map(|b| b.iter().map(|x| format!("{x:02x}")).collect::<String>())
                        .unwrap_or_else(|_| "?".into())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn page_ids(doc: &Lo) -> Vec<ObjectId> {
    doc.get_pages().into_values().collect()
}

fn annots(doc: &Lo, page: ObjectId) -> Vec<Dictionary> {
    let Ok(p) = doc.get_dictionary(page) else {
        return vec![];
    };
    let Ok(a) = p.get(b"Annots") else {
        return vec![];
    };
    let arr = deref(doc, a);
    let Ok(items) = arr.as_array() else {
        return vec![];
    };
    items
        .iter()
        .filter_map(|i| deref(doc, i).as_dict().ok().cloned())
        .collect()
}

fn content(doc: &Lo, page: ObjectId) -> Vec<u8> {
    doc.get_page_content_with_limit(page, 256 << 20)
        .unwrap_or_default()
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

/// (nombre, octets) des flux **sans** `/Filter`, hors `/Metadata`.
fn unfiltered_streams(doc: &Lo) -> (usize, usize) {
    let mut n = 0;
    let mut bytes = 0;
    for o in doc.objects.values() {
        if let Ok(s) = o.as_stream() {
            let is_meta = s.dict.get(b"Type").ok().and_then(|t| t.as_name().ok())
                == Some(b"Metadata".as_slice());
            if s.dict.get(b"Filter").is_err() && !is_meta {
                n += 1;
                bytes += s.content.len();
            }
        }
    }
    (n, bytes)
}

/// Programmes de police incorporés : (id, taille brute, `/Length1`).
fn font_files(doc: &Lo) -> Vec<(ObjectId, usize, Option<i64>)> {
    let mut v = Vec::new();
    for (id, o) in &doc.objects {
        if let Ok(d) = o.as_dict()
            && d.get(b"Type").ok().and_then(|t| t.as_name().ok())
                == Some(b"FontDescriptor".as_slice())
        {
            for key in ["FontFile", "FontFile2", "FontFile3"] {
                if let Ok(r) = d.get(key.as_bytes())
                    && let Ok(rid) = r.as_reference()
                    && let Ok(s) = doc.get_object(rid).and_then(|o| o.as_stream())
                {
                    let _ = id;
                    v.push((
                        rid,
                        s.content.len(),
                        s.dict.get(b"Length1").ok().and_then(|l| l.as_i64().ok()),
                    ));
                }
            }
        }
    }
    v.sort();
    v.dedup();
    v
}

/// Comment `s` apparaît dans `bytes` : littéral, hexadécimal Latin-1, UTF-16BE avec BOM.
fn leaks(bytes: &[u8], s: &str) -> Vec<&'static str> {
    let mut v = Vec::new();
    if contains(bytes, s) {
        v.push("litteral");
    }
    let hex: String = s.bytes().map(|b| format!("{b:02X}")).collect();
    if contains(bytes, &hex) || contains(bytes, &hex.to_ascii_lowercase()) {
        v.push("hex");
    }
    let utf16: String = std::iter::once(0xFEFFu16)
        .chain(s.encode_utf16())
        .map(|u| format!("{u:04X}"))
        .collect();
    if contains(bytes, &utf16) {
        v.push("utf16");
    }
    v
}

/// Pour chaque occurrence de `s` dans `bytes`, l'en-tête `N G obj` qui la précède et les
/// octets qui l'entourent.
fn carriers(bytes: &[u8], s: &str) -> Vec<String> {
    let mut v = Vec::new();
    let needle = s.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let head = bytes[..i]
                .windows(4)
                .rposition(|w| w == b" obj")
                .map(|p| {
                    let start = bytes[..p]
                        .iter()
                        .rposition(|&c| c == b'\n')
                        .map_or(0, |x| x + 1);
                    String::from_utf8_lossy(&bytes[start..p + 4]).into_owned()
                })
                .unwrap_or_else(|| "?".into());
            let ctx_start = i.saturating_sub(30);
            let ctx_end = (i + needle.len() + 10).min(bytes.len());
            let ctx = String::from_utf8_lossy(&bytes[ctx_start..ctx_end]).replace('\n', " ");
            v.push(format!("{head} : …{ctx}…"));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    v
}

fn apply(doc: &mut Document, json: serde_json::Value) -> Result<(), String> {
    let c: EditCommand = serde_json::from_value(json.clone()).unwrap();
    doc.apply(&c).map_err(|e| e.to_string())
}

fn header(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(8)]).into_owned()
}

fn cat_version(doc: &Lo) -> String {
    match root(doc).get(b"Version") {
        Ok(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
        _ => "absent".into(),
    }
}

fn mod_date(doc: &Lo) -> String {
    info(doc)
        .and_then(|i| i.get(b"ModDate").ok().cloned())
        .map(|o| format!("{o:?}"))
        .unwrap_or_else(|| "absent".into())
}

fn dark_pixels(img: &mega_core::RenderedPage, x: f32, y: f32, w: f32, h: f32) -> usize {
    let mut n = 0;
    let top = (img.height_px as f32 - y - h).max(0.0) as u32;
    let bottom = (img.height_px as f32 - y).min(img.height_px as f32) as u32;
    let left = x.max(0.0) as u32;
    let right = (x + w).min(img.width_px as f32) as u32;
    for py in top..bottom {
        for px in left..right {
            let i = ((py * img.width_px + px) * 4) as usize;
            if img.rgba[i] < 128 {
                n += 1;
            }
        }
    }
    n
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> serde_json::Value {
    serde_json::json!({ "x": x, "y": y, "w": w, "h": h })
}

fn style() -> serde_json::Value {
    serde_json::json!({
        "color": { "r": 200, "g": 0, "b": 0 },
        "fill": null,
        "opacity": 1.0,
        "stroke_width": 2.0,
        "font_size": 12.0
    })
}

fn annotation(kind: &str, geometry: serde_json::Value, contents: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "add_annotation",
        "page": 0,
        "annotation": {
            "kind": kind,
            "geometry": geometry,
            "style": style(),
            "contents": contents,
            "author": "Audit"
        }
    })
}

fn rsa() -> SigningIdentity {
    SigningIdentity::from_pkcs12(&fixture_bytes("test-signer.p12"), "test").unwrap()
}

fn ec() -> SigningIdentity {
    SigningIdentity::from_pkcs12(&fixture_bytes("test-signer-ec.p12"), "test").unwrap()
}

/// Assemble un PDF classique (table `xref`) à partir des corps d'objets numérotés à partir
/// de 1 ; `trailer_extra` complète `/Size` et `/Root 1 0 R`.
fn assemble(header: &str, objects: &[Vec<u8>], trailer_extra: &str) -> Vec<u8> {
    let mut out: Vec<u8> = format!("%PDF-{header}\n%\u{e2}\u{e3}\u{cf}\u{d3}\n").into_bytes();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R {trailer_extra} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn stream(dict: &str, content: &[u8]) -> Vec<u8> {
    let mut v = format!("<< {dict} /Length {} >>\nstream\n", content.len()).into_bytes();
    v.extend_from_slice(content);
    v.extend_from_slice(b"\nendstream");
    v
}

fn s(x: &str) -> Vec<u8> {
    x.as_bytes().to_vec()
}

// --- Générateurs --------------------------------------------------------------------------

/// Chaîne de `depth` nœuds `/Pages` imbriqués, attributs hérités à différents niveaux.
fn deep_pages_pdf(depth: usize) -> Vec<u8> {
    // 1 Catalog, 2 Font, 3 Contents, 4..4+depth-1 nœuds, dernier = page.
    let first_node = 4;
    let leaf = first_node + depth;
    let mut objects: Vec<Vec<u8>> = vec![
        s("<< /Type /Catalog /Pages 4 0 R >>"),
        s("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"),
        stream("", b"BT /F1 24 Tf 72 500 Td (Profond) Tj ET"),
    ];
    for level in 0..depth {
        let id = first_node + level;
        let parent = if level == 0 {
            String::new()
        } else {
            format!(" /Parent {} 0 R", id - 1)
        };
        let mut extra = String::new();
        if level == 0 {
            extra.push_str(" /MediaBox [0 0 400 600]");
        }
        if level == 10 {
            extra.push_str(" /Rotate 90");
        }
        if level == 25 {
            extra.push_str(" /Resources << /Font << /F1 2 0 R >> >>");
        }
        objects.push(s(&format!(
            "<< /Type /Pages /Kids [{} 0 R] /Count 1{parent}{extra} >>",
            id + 1
        )));
    }
    objects.push(s(&format!(
        "<< /Type /Page /Parent {} 0 R /Contents 3 0 R >>",
        leaf - 1
    )));
    assemble("1.7", &objects, "")
}

fn dejavu() -> Vec<u8> {
    std::fs::read(format!(
        "{}/assets/fonts/DejaVuSans.ttf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn glyph_id(font: &[u8], c: char) -> u16 {
    ttf_parser::Face::parse(font, 0)
        .unwrap()
        .glyph_index(c)
        .unwrap()
        .0
}

const DESCRIPTOR: &str = "/FontBBox [-1021 -463 1793 1232] /ItalicAngle 0 /Ascent 928 /Descent -236 \
                          /CapHeight 700 /StemV 80";

/// `kind` : `truetype-symbolic`, `helvetica-differences`, `cid-nounicode`, `cid-unicode`.
fn fonts_pdf(kind: &str) -> Vec<u8> {
    let font = dejavu();
    let text = if kind.starts_with("cid") {
        "<0001>"
    } else {
        "(A)"
    };
    let content = format!("BT /F1 24 Tf 72 700 Td {text} Tj ET");
    let mut objects: Vec<Vec<u8>> = vec![
        s("<< /Type /Catalog /Pages 2 0 R >>"),
        s("<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
        s(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R \
           /Resources << /Font << /F1 5 0 R >> >> >>",
        ),
        stream("", content.as_bytes()),
    ];
    match kind {
        "helvetica-differences" => {
            objects.push(s("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
                            /Encoding << /Type /Encoding /BaseEncoding /WinAnsiEncoding /Differences [65 /eacute] >> >>"));
        }
        "truetype-symbolic" => {
            objects.push(s("<< /Type /Font /Subtype /TrueType /BaseFont /DejaVuSans /FirstChar 65 /LastChar 66 \
                            /Widths [684 686] /FontDescriptor 6 0 R \
                            /Encoding << /Type /Encoding /Differences [65 /B] >> >>"));
            objects.push(s(&format!(
                "<< /Type /FontDescriptor /FontName /DejaVuSans /Flags 4 {DESCRIPTOR} /FontFile2 7 0 R >>"
            )));
            objects.push(stream(&format!("/Length1 {}", font.len()), &font));
        }
        _ => {
            let gid = glyph_id(&font, 'Ω');
            let with_unicode = kind == "cid-unicode";
            let tu = if with_unicode {
                " /ToUnicode 9 0 R"
            } else {
                ""
            };
            objects.push(s(&format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /DejaVuSans /Encoding /Identity-H \
                 /DescendantFonts [6 0 R]{tu} >>"
            )));
            objects.push(s("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /DejaVuSans \
                            /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
                            /FontDescriptor 7 0 R /DW 600 /W [1 [800]] /CIDToGIDMap 8 0 R >>"));
            objects.push(s(&format!(
                "<< /Type /FontDescriptor /FontName /DejaVuSans /Flags 32 {DESCRIPTOR} /FontFile2 10 0 R >>"
            )));
            let map = vec![0u8, 0, (gid >> 8) as u8, (gid & 0xff) as u8];
            objects.push(stream("", &map));
            let cmap = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
                        /CMapName /Audit def 1 begincodespacerange <0000> <FFFF> endcodespacerange \
                        1 beginbfchar <0001> <03A9> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end";
            objects.push(stream("", cmap.as_bytes()));
            objects.push(stream(&format!("/Length1 {}", font.len()), &font));
        }
    }
    assemble("1.7", &objects, "")
}

/// Un champ de signature dont le `/SubFilter` est `subfilter`, avec un `/ByteRange` cohérent
/// et un `/Contents` factice (zéros).
fn sig_subfilter_pdf(subfilter: &str) -> Vec<u8> {
    let zeros = "0".repeat(128);
    let objects: Vec<Vec<u8>> = vec![
        s("<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] /SigFlags 3 >> >>"),
        s("<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
        s(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R] /Contents 4 0 R >>",
        ),
        stream("", b"0 0 1 rg 10 10 50 50 re f"),
        s(
            "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Sig1) /V 6 0 R /Rect [0 0 0 0] /F 132 /P 3 0 R >>",
        ),
        s(&format!(
            "<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /{subfilter} \
             /ByteRange [0 0000000000 0000000000 0000000000] /Contents <{zeros}> /M (D:20261031000000Z) >>"
        )),
    ];
    let mut bytes = assemble("1.7", &objects, "");
    let marker = b"/Contents <";
    let p = bytes
        .windows(marker.len())
        .position(|w| w == marker)
        .unwrap();
    let start = p + marker.len() - 1;
    let end = start + bytes[start..].iter().position(|&c| c == b'>').unwrap() + 1;
    let total = bytes.len();
    let br = format!("[0 {start:010} {end:010} {:010}]", total - end);
    let brm = b"/ByteRange ";
    let q = bytes.windows(brm.len()).position(|w| w == brm).unwrap() + brm.len();
    bytes[q..q + br.len()].copy_from_slice(br.as_bytes());
    bytes
}

// --- Mesures communes ---------------------------------------------------------------------

const CATALOG_PATHS: &[&[&str]] = &[
    &["StructTreeRoot"],
    &["MarkInfo"],
    &["OCProperties"],
    &["Names", "Dests"],
    &["Names", "JavaScript"],
    &["Names", "EmbeddedFiles"],
    &["Dests"],
    &["PageLabels"],
    &["Lang"],
    &["OpenAction"],
    &["AA"],
    &["ViewerPreferences"],
    &["PageMode"],
    &["Metadata"],
    &["OutputIntents"],
    &["Outlines"],
    &["AcroForm"],
];

/// Ce que la sortie a conservé du catalogue riche.
fn measure_catalog(id: &str, input: &[u8], out: &[u8], password: Option<&str>) {
    let din = lo(input);
    let dout = match catch_unwind(|| lo(out)) {
        Ok(d) => d,
        Err(_) => {
            verdict(id, "lopdf relit la sortie", "NON");
            return;
        }
    };
    let kin = keys(&root(&din));
    let kout = keys(&root(&dout));
    let lost: Vec<&String> = kin.iter().filter(|k| !kout.contains(k)).collect();
    let added: Vec<&String> = kout.iter().filter(|k| !kin.contains(k)).collect();
    verdict(id, "clés du catalogue perdues", format!("{lost:?}"));
    verdict(id, "clés du catalogue ajoutées", format!("{added:?}"));
    let mut missing = Vec::new();
    for p in CATALOG_PATHS {
        if has_path(&din, p) && !has_path(&dout, p) {
            missing.push(p.join("/"));
        }
    }
    verdict(id, "structures perdues", format!("{missing:?}"));
    let pages = page_ids(&dout);
    let c0 = pages
        .first()
        .map(|p| content(&dout, *p))
        .unwrap_or_default();
    verdict(
        id,
        "p1 : BDC/EMC/MCID",
        format!(
            "BDC {} EMC {} MCID {}",
            contains(&c0, "BDC"),
            contains(&c0, "EMC"),
            contains(&c0, "MCID")
        ),
    );
    let c1 = pages.get(1).map(|p| content(&dout, *p)).unwrap_or_default();
    verdict(
        id,
        "p2 : /OC … BDC",
        format!("OC {} BDC {}", contains(&c1, "/OC"), contains(&c1, "BDC")),
    );
    let sp: Vec<String> = pages
        .iter()
        .map(|p| {
            dout.get_dictionary(*p)
                .ok()
                .and_then(|d| d.get(b"StructParents").ok())
                .map(|o| format!("{o:?}"))
                .unwrap_or_else(|| "-".into())
        })
        .collect();
    verdict(id, "/StructParents par page", format!("{sp:?}"));
    verdict(
        id,
        "en-tête / /Version / version lopdf",
        format!(
            "{} / {} / {}",
            header(out),
            cat_version(&dout),
            dout.version
        ),
    );
    verdict(id, "/ModDate (passe lopdf)", mod_date(&dout));
    verdict(id, "/ID", format!("{:?}", trailer_ids(&dout)));
    verdict(id, "pages (lopdf)", pages.len());
    match Document::from_bytes(out.to_vec(), password) {
        Ok(doc) => {
            let outline: Vec<String> = doc
                .outline()
                .map(|o| {
                    o.iter()
                        .map(|n| format!("{}→{:?}", n.title, n.page))
                        .collect()
                })
                .unwrap_or_default();
            verdict(id, "signets (PDFium)", format!("{outline:?}"));
            let links: Vec<String> = doc
                .links(0)
                .map(|l| l.iter().map(|l| format!("{:?}", l.target)).collect())
                .unwrap_or_default();
            verdict(id, "liens p1 (PDFium)", format!("{links:?}"));
            verdict(id, "pages (PDFium)", doc.page_count());
            verdict(
                id,
                "texte p1",
                doc.page_text(0).unwrap_or_default().replace('\n', "⏎"),
            );
        }
        Err(e) => verdict(id, "PDFium rouvre la sortie", format!("NON : {e}")),
    }
    let path = save(id, out);
    verdict(id, "qpdf", qpdf_check(&path, password));
    verdict(id, "mutool", mutool_check(&path, password));
    verdict(id, "pdf.js", pdfjs_check(&path, password));
    verdict(id, "veraPDF", verapdf_check(&path));
}

fn save_doc(id: &str, doc: &mut Document) -> Vec<u8> {
    let path = out_dir().join(format!("{id}.pdf"));
    doc.save_as(&path).unwrap();
    std::fs::read(&path).unwrap()
}

// --- E1 : conservation du catalogue -------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e1_catalog_preservation() {
    let _ = IGNORE;
    for (hdr, ver) in [
        ("1.4", None),
        ("1.7", None),
        ("2.0", None),
        ("1.4", Some("1.7")),
    ] {
        let tag = format!(
            "E1a-{hdr}{}",
            ver.map(|v| format!("-v{v}")).unwrap_or_default()
        );
        guarded(&tag, || {
            let input = testing::rich_catalog_pdf(hdr, ver);
            save(&format!("{tag}-entree"), &input);
            let din = lo(&input);
            verdict(
                &tag,
                "entrée : structures présentes",
                CATALOG_PATHS.iter().filter(|p| has_path(&din, p)).count(),
            );
            let mut doc = Document::from_bytes(input.clone(), None).unwrap();
            let unchanged = save_doc(&format!("{tag}-sans-edition"), &mut doc);
            verdict(
                &tag,
                "save_as sans édition = octets identiques",
                unchanged == input,
            );
            let raw = doc.to_bytes().unwrap();
            save(&format!("{tag}-pdfium-brut"), &raw);
            verdict(&tag, "to_bytes brut : en-tête", header(&raw));
            let raw_lo = lo(&raw);
            let lost_raw: Vec<String> = CATALOG_PATHS
                .iter()
                .filter(|p| has_path(&din, p) && !has_path(&raw_lo, p))
                .map(|p| p.join("/"))
                .collect();
            verdict(
                &tag,
                "to_bytes brut : structures perdues",
                format!("{lost_raw:?}"),
            );
            verdict(&tag, "to_bytes brut : /Version", cat_version(&raw_lo));
            let cleaned = mega_core::cleanup::before_save(&raw, None, None)
                .unwrap()
                .bytes;
            verdict(&tag, "before_save a modifié les octets", cleaned != raw);
            measure_catalog(&tag, &input, &cleaned, None);
        });
    }

    let input = testing::rich_catalog_pdf("1.7", None);
    let cases: Vec<(&str, serde_json::Value)> = vec![
        (
            "E1b-rotate",
            serde_json::json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1}),
        ),
        (
            "E1c-delete-p2",
            serde_json::json!({"kind": "delete_pages", "pages": [1]}),
        ),
        (
            "E1d-move-p3-first",
            serde_json::json!({"kind": "move_pages", "pages": [2], "to": 0}),
        ),
        (
            "E1e-delete-object-p1",
            serde_json::json!({"kind": "delete_object", "page": 0, "index": 0}),
        ),
        (
            "E1e-delete-object-p2",
            serde_json::json!({"kind": "delete_object", "page": 1, "index": 0}),
        ),
    ];
    for (id, cmd) in cases {
        let input = input.clone();
        guarded(id, || {
            let mut doc = Document::from_bytes(input.clone(), None).unwrap();
            match apply(&mut doc, cmd) {
                Ok(()) => {
                    let out = save_doc(id, &mut doc);
                    measure_catalog(id, &input, &out, None);
                }
                Err(e) => verdict(id, "commande refusée", e),
            }
        });
    }

    guarded("E1f-merge", || {
        let a = Document::from_bytes(testing::rich_catalog_pdf("1.7", None), None).unwrap();
        let b = Document::from_bytes(testing::rich_catalog_pdf("1.5", None), None).unwrap();
        let out = mega_core::pages::merge(&[&a, &b]).unwrap();
        measure_catalog("E1f-merge", &input, &out, None);
    });
    guarded("E1g-extract", || {
        let doc = Document::from_bytes(input.clone(), None).unwrap();
        let out = doc.extract_pages(&[1]).unwrap();
        measure_catalog("E1g-extract", &input, &out, None);
    });
    guarded("E1g-split", || {
        let doc = Document::from_bytes(input.clone(), None).unwrap();
        let parts = mega_core::pages::split(&doc, &[vec![0, 1], vec![2]]).unwrap();
        verdict("E1g-split", "parties", parts.len());
        measure_catalog("E1g-split-0", &input, &parts[0], None);
    });
}

// --- E2 : repli de before_save et caviardage ----------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e2_cleanup_fallback_and_redaction() {
    let secret = "SECRET-ZX9";
    guarded("E2", || {
        let input = mega_core::testing::identity_encrypted_note_pdf(secret);
        save("E2-entree", &input);
        verdict(
            "E2",
            "lopdf relit l'entrée",
            catch_unwind(|| lo(&input)).is_ok(),
        );
        let fallback = mega_core::cleanup::before_save(&input, None, None)
            .unwrap()
            .bytes;
        verdict(
            "E2",
            "étape 0 : before_save rend les octets inchangés (repli)",
            fallback == input,
        );
        let mut doc = Document::from_bytes(input.clone(), None).unwrap();
        verdict("E2", "PDFium ouvre (sans mot de passe)", true);
        verdict("E2", "security()", format!("{:?}", doc.security()));
        verdict(
            "E2",
            "annotations lues",
            format!(
                "{:?}",
                doc.annotations(0).map(|a| a
                    .iter()
                    .map(|x| format!("{:?}", x.kind))
                    .collect::<Vec<_>>())
            ),
        );
        verdict(
            "E2",
            "fuites dans l'entrée",
            format!("{:?}", leaks(&input, secret)),
        );
        // Suppression de la note.
        match apply(
            &mut doc,
            serde_json::json!({"kind": "delete_annotation", "page": 0, "index": 0}),
        ) {
            Ok(()) => {
                let out = save_doc("E2-delete-annotation", &mut doc);
                verdict(
                    "E2",
                    "après delete_annotation : fuites",
                    format!("{:?}", leaks(&out, secret)),
                );
                let sz = out.len();
                verdict("E2", "après delete_annotation : taille", sz);
                match catch_unwind(|| lo(&out)) {
                    Ok(d) => {
                        verdict("E2", "sortie : /ModDate", mod_date(&d));
                        verdict(
                            "E2",
                            "objets porteurs du secret (octets)",
                            format!("{:?}", carriers(&out, secret)),
                        );
                        let n = page_ids(&d)
                            .first()
                            .map(|p| annots(&d, *p).len())
                            .unwrap_or(0);
                        verdict("E2", "annotations restantes (lopdf)", n);
                    }
                    Err(_) => verdict("E2", "lopdf relit la sortie", "NON"),
                }
                let p = save("E2-delete-annotation", &out);
                verdict("E2", "qpdf", qpdf_check(&p, None));
            }
            Err(e) => verdict("E2", "delete_annotation refusée", e),
        }
        // Caviardage sur la note.
        let mut doc2 = Document::from_bytes(input.clone(), None).unwrap();
        match apply(
            &mut doc2,
            serde_json::json!({"kind": "redact", "page": 0, "rect": rect(40.0, 40.0, 40.0, 40.0)}),
        ) {
            Ok(()) => {
                let out = save_doc("E2-redact", &mut doc2);
                verdict(
                    "E2",
                    "redact accepté ; fuites",
                    format!("{:?}", leaks(&out, secret)),
                );
            }
            Err(e) => verdict("E2", "redact refusé (message)", e),
        }
        // Témoin : même document **sans** /Encrypt (nettoyage normal).
        let plain = testing::note_with_popup_pdf(secret);
        let mut doc3 = Document::from_bytes(plain, None).unwrap();
        apply(
            &mut doc3,
            serde_json::json!({"kind": "delete_annotation", "page": 0, "index": 0}),
        )
        .unwrap();
        let out = save_doc("E2-temoin-sans-encrypt", &mut doc3);
        verdict(
            "E2",
            "témoin sans /Encrypt : fuites après delete_annotation",
            format!("{:?}", leaks(&out, secret)),
        );
    });
}

// --- E3 : NeedAppearances ------------------------------------------------------------------

fn need_appearances(doc: &Lo) -> String {
    root(doc)
        .get(b"AcroForm")
        .map(|a| deref(doc, a))
        .ok()
        .and_then(|a| {
            a.as_dict()
                .ok()
                .and_then(|d| d.get(b"NeedAppearances").ok().cloned())
        })
        .map(|o| format!("{o:?}"))
        .unwrap_or_else(|| "absent".into())
}

fn widget_ap(doc: &Lo, name: &str) -> String {
    for p in page_ids(doc) {
        for a in annots(doc, p) {
            let t = a
                .get(b"T")
                .ok()
                .and_then(|t| t.as_str().ok())
                .map(|b| b.to_vec());
            if t.as_deref() == Some(name.as_bytes()) {
                let generated = a.get(b"PDFIUM_HasGeneratedAP").is_ok();
                let ap = a.get(b"AP").map(|ap| deref(doc, ap)).ok();
                let n_len = ap
                    .as_ref()
                    .and_then(|ap| {
                        ap.as_dict()
                            .ok()
                            .and_then(|d| d.get(b"N").ok().map(|n| deref(doc, n)))
                    })
                    .and_then(|n| n.as_stream().ok().map(|s| s.content.len()));
                return format!(
                    "AP {} /N octets {:?} PDFIUM_HasGeneratedAP {}",
                    ap.is_some(),
                    n_len,
                    generated
                );
            }
        }
    }
    "widget introuvable".into()
}

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e3_need_appearances() {
    for (src, bytes) in [
        ("form", fixture_bytes("form.pdf")),
        ("form-sans-NA", testing::form_pdf_without_need_appearances()),
    ] {
        for render_first in [false, true] {
            let id = format!(
                "E3-{src}-{}",
                if render_first {
                    "rendu-puis-save"
                } else {
                    "save-direct"
                }
            );
            let bytes = bytes.clone();
            guarded(&id, || {
                let din = lo(&bytes);
                verdict(&id, "entrée : /NeedAppearances", need_appearances(&din));
                let mut doc = Document::from_bytes(bytes, None).unwrap();
                apply(&mut doc, serde_json::json!({"kind": "set_field_value", "page": 0, "index": 0, "value": {"kind": "text", "text": "Dupont"}})).unwrap();
                if render_first {
                    let _ = doc.render_page(0, 0.5).unwrap();
                }
                let out = save_doc(&id, &mut doc);
                let dout = lo(&out);
                verdict(&id, "sortie : /NeedAppearances", need_appearances(&dout));
                verdict(&id, "widget nom", widget_ap(&dout, "nom"));
                verdict(
                    &id,
                    "qpdf",
                    qpdf_check(&out_dir().join(format!("{id}.pdf")), None),
                );
            });
        }
    }
}

// --- E4 : /ID -------------------------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e4_trailer_id() {
    for (src, pw) in [("simple.pdf", None), ("encrypted.pdf", Some("secret"))] {
        let id = format!("E4-{src}");
        guarded(&id, || {
            let input = fixture_bytes(src);
            let before = trailer_ids(&lo(&input));
            verdict(&id, "entrée /ID", format!("{before:?}"));
            let doc = Document::from_bytes(input.clone(), pw).unwrap();
            let raw = doc.to_bytes().unwrap();
            let raw_ids = catch_unwind(|| trailer_ids(&lo(&raw))).unwrap_or_default();
            verdict(&id, "to_bytes nu /ID", format!("{raw_ids:?}"));
            let mut doc = Document::from_bytes(input, pw).unwrap();
            apply(
                &mut doc,
                serde_json::json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1}),
            )
            .unwrap();
            let out = save_doc(&id, &mut doc);
            let after = catch_unwind(|| trailer_ids(&lo(&out))).unwrap_or_default();
            verdict(&id, "après rotation + save /ID", format!("{after:?}"));
            let v = match (before.len(), after.len()) {
                (2, 2) if before == after => "identique",
                (2, 2) if before[0] == after[0] => "premier gardé, second renouvelé",
                (2, 2) => "les deux renouvelés",
                _ => "absent ou incomplet",
            };
            verdict(&id, "verdict /ID", v);
            if let Some(pw) = pw {
                verdict(
                    &id,
                    "se rouvre avec le mot de passe",
                    Document::from_bytes(out.clone(), Some(pw)).is_ok(),
                );
                verdict(
                    &id,
                    "qpdf --show-encryption",
                    qpdf_show_encryption(&out_dir().join(format!("{id}.pdf")), pw),
                );
            }
            verdict(
                &id,
                "qpdf",
                qpdf_check(&out_dir().join(format!("{id}.pdf")), pw),
            );
        });
    }
}

// --- E5 : compression des flux lopdf -------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e5_stream_compression() {
    let cases: Vec<(&str, &str, serde_json::Value)> = vec![
        (
            "E5-watermark",
            "simple.pdf",
            serde_json::json!({"kind": "watermark", "spec": {"text": "CONFIDENTIEL", "size": 60.0,
            "opacity": 0.3, "angle_deg": 45.0, "color": {"r": 200, "g": 0, "b": 0}, "pages": null}}),
        ),
        (
            "E5-ink",
            "simple.pdf",
            annotation(
                "ink",
                serde_json::json!({"kind": "strokes", "strokes": [[[100.0, 100.0], [150.0, 160.0], [200.0, 120.0]]]}),
                "encre",
            ),
        ),
        (
            "E5-page-numbers",
            "simple.pdf",
            serde_json::json!({"kind": "page_numbers", "spec": {"template": "Page {n} / {total}",
            "corner": "bottom_center", "margin": 20.0, "size": 10.0, "start": 1, "pages": null}}),
        ),
    ];
    for (id, src, cmd) in cases {
        guarded(id, || {
            let input = fixture_bytes(src);
            let (n0, b0) = unfiltered_streams(&lo(&input));
            let mut doc = Document::from_bytes(input.clone(), None).unwrap();
            apply(&mut doc, cmd).unwrap();
            let out = save_doc(id, &mut doc);
            let dout = lo(&out);
            let (n1, b1) = unfiltered_streams(&dout);
            verdict(
                id,
                "flux sans /Filter avant → après (nombre, octets)",
                format!("({n0}, {b0}) → ({n1}, {b1})"),
            );
            verdict(
                id,
                "taille fichier avant → après",
                format!("{} → {}", input.len(), out.len()),
            );
            let filters: Vec<String> = page_ids(&dout)
                .iter()
                .map(|p| {
                    let d = dout.get_dictionary(*p).unwrap();
                    let c = d
                        .get(b"Contents")
                        .map(|c| deref(&dout, c))
                        .unwrap_or(Object::Null);
                    match c {
                        Object::Stream(s) => format!("{:?}", s.dict.get(b"Filter").ok()),
                        Object::Array(a) => format!("tableau de {}", a.len()),
                        _ => "?".into(),
                    }
                })
                .collect();
            verdict(id, "/Filter des flux de contenu", format!("{filters:?}"));
        });
    }
    guarded("E5-ocr", || {
        let input = fixture_bytes("scanned.pdf");
        let (n0, b0) = unfiltered_streams(&lo(&input));
        let mut doc = Document::from_bytes(input.clone(), None).unwrap();
        let words = vec![
            mega_core::overlay::InvisibleWord {
                text: "Page".into(),
                rect: mega_core::Rect {
                    x: 72.0,
                    y: 740.0,
                    w: 60.0,
                    h: 24.0,
                },
            },
            mega_core::overlay::InvisibleWord {
                text: "Ωμέγα".into(),
                rect: mega_core::Rect {
                    x: 140.0,
                    y: 740.0,
                    w: 60.0,
                    h: 24.0,
                },
            },
        ];
        doc.add_invisible_words(0, &words).unwrap();
        let out = save_doc("E5-ocr", &mut doc);
        let (n1, b1) = unfiltered_streams(&lo(&out));
        verdict(
            "E5-ocr",
            "flux sans /Filter avant → après",
            format!("({n0}, {b0}) → ({n1}, {b1})"),
        );
        verdict(
            "E5-ocr",
            "taille fichier avant → après",
            format!("{} → {}", input.len(), out.len()),
        );
        verdict(
            "E5-ocr",
            "texte p1 relu",
            Document::from_bytes(out, None)
                .unwrap()
                .page_text(0)
                .unwrap()
                .replace('\n', "⏎"),
        );
    });
}

// --- E6 : DejaVu via PDFium ----------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e6_dejavu_font_embedding() {
    let dv = dejavu().len();
    guarded("E6-insert-text", || {
        let input = fixture_bytes("simple.pdf");
        let mut doc = Document::from_bytes(input.clone(), None).unwrap();
        apply(&mut doc, serde_json::json!({"kind": "insert_text", "page": 0, "at": [100.0, 600.0], "text": "Ω",
            "style": {"family": "unicode", "bold": false, "italic": false, "size": 14.0, "color": {"r": 0, "g": 0, "b": 0}}})).unwrap();
        let out = save_doc("E6-insert-text", &mut doc);
        let ff = font_files(&lo(&out));
        verdict("E6-insert-text", "DejaVuSans.ttf octets", dv);
        verdict(
            "E6-insert-text",
            "programmes de police (id, octets bruts, Length1)",
            format!("{ff:?}"),
        );
        verdict(
            "E6-insert-text",
            "taille fichier avant → après",
            format!("{} → {}", input.len(), out.len()),
        );
    });
    guarded("E6-two-annotations", || {
        let input = fixture_bytes("simple.pdf");
        let mut doc = Document::from_bytes(input.clone(), None).unwrap();
        apply(
            &mut doc,
            annotation(
                "text",
                serde_json::json!({"kind": "rect", "rect": rect(100.0, 600.0, 200.0, 30.0)}),
                "Ωμέγα",
            ),
        )
        .unwrap();
        // Une passe lopdf force le rechargement (jeton de police remis à zéro).
        apply(
            &mut doc,
            serde_json::json!({"kind": "set_metadata", "patch": {"title": "Deux polices"}}),
        )
        .unwrap();
        apply(
            &mut doc,
            annotation(
                "text",
                serde_json::json!({"kind": "rect", "rect": rect(100.0, 500.0, 200.0, 30.0)}),
                "Жук",
            ),
        )
        .unwrap();
        let raw = doc.to_bytes().unwrap();
        let ff_raw = font_files(&lo(&raw));
        verdict(
            "E6-two-annotations",
            "to_bytes brut : programmes de police",
            format!("{ff_raw:?}"),
        );
        let out = save_doc("E6-two-annotations", &mut doc);
        let ff = font_files(&lo(&out));
        verdict(
            "E6-two-annotations",
            "après before_save : programmes de police",
            format!("{ff:?}"),
        );
        verdict("E6-two-annotations", "taille fichier", out.len());
        verdict(
            "E6-two-annotations",
            "qpdf",
            qpdf_check(&out_dir().join("E6-two-annotations.pdf"), None),
        );
    });
}

// --- E7 : AES-256 sur %PDF-1.4 --------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e7_aes256_on_pdf14() {
    guarded("E7", || {
        let mut input = fixture_bytes("simple.pdf");
        input[5..8].copy_from_slice(b"1.4");
        let mut doc = Document::from_bytes(input, None).unwrap();
        apply(&mut doc, serde_json::json!({"kind": "protect", "protection": {"user_password": "u",
            "owner_password": "o", "permissions": {"print": true, "copy": false, "modify": false, "annotate": true}}})).unwrap();
        let out = save_doc("E7", &mut doc);
        verdict("E7", "en-tête", header(&out));
        let d = Lo::load_mem_with_options(
            &out,
            lopdf::LoadOptions {
                password: Some("u".into()),
                ..lopdf::LoadOptions::default()
            },
        )
        .unwrap();
        verdict("E7", "/Version du catalogue", cat_version(&d));
        let enc = out
            .windows(9)
            .position(|w| w == b"/Standard")
            .map(|p| {
                let end = out[p..]
                    .iter()
                    .position(|&c| c == b'>')
                    .map_or(out.len(), |e| p + e);
                String::from_utf8_lossy(&out[p..end.min(p + 400)])
                    .chars()
                    .filter(|c| !c.is_control())
                    .collect::<String>()
            })
            .unwrap_or_else(|| "introuvable".into());
        let enc: String = enc
            .split_whitespace()
            .filter(|t| !t.starts_with('<') && !t.chars().all(|c| c.is_ascii_hexdigit()))
            .collect::<Vec<_>>()
            .join(" ");
        let enc = enc.split("/O").next().unwrap_or(&enc).to_string();
        verdict("E7", "/Encrypt (clés, sans O/U)", enc);
        verdict(
            "E7",
            "/Extensions",
            format!("{:?}", root(&d).get(b"Extensions").ok()),
        );
        let p = out_dir().join("E7.pdf");
        verdict("E7", "qpdf --check", qpdf_check(&p, Some("u")));
        verdict(
            "E7",
            "qpdf --show-encryption",
            qpdf_show_encryption(&p, "u"),
        );
        verdict("E7", "mutool", mutool_check(&p, Some("u")));
        verdict("E7", "pdf.js", pdfjs_check(&p, Some("u")));
        verdict(
            "E7",
            "PDFium rouvre avec u",
            Document::from_bytes(out.clone(), Some("u")).is_ok(),
        );
        verdict(
            "E7",
            "PDFium rouvre avec o",
            Document::from_bytes(out, Some("o")).is_ok(),
        );
    });
}

// --- E8 : insert_image ----------------------------------------------------------------------

fn describe_images(doc: &Lo) -> Vec<String> {
    let mut v = Vec::new();
    for (id, o) in &doc.objects {
        if let Ok(st) = o.as_stream()
            && st.dict.get(b"Subtype").ok().and_then(|t| t.as_name().ok())
                == Some(b"Image".as_slice())
        {
            let g = |k: &str| {
                st.dict
                    .get(k.as_bytes())
                    .map(|x| format!("{x:?}"))
                    .unwrap_or_else(|_| "-".into())
            };
            v.push(format!(
                "{id:?} {}x{} bpc {} cs {} filter {} smask {} octets {}",
                g("Width"),
                g("Height"),
                g("BitsPerComponent"),
                g("ColorSpace"),
                g("Filter"),
                g("SMask"),
                st.content.len()
            ));
        }
    }
    v
}

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e8_insert_image() {
    let dir = out_dir();
    let jpg = dir.join("E8-source.jpg");
    let png = dir.join("E8-source.png");
    guarded("E8-prepare", || {
        let mut img = image::RgbImage::new(200, 150);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8]);
        }
        img.save(&jpg).unwrap();
        let mut rgba = image::RgbaImage::new(200, 150);
        for (x, _y, p) in rgba.enumerate_pixels_mut() {
            *p = image::Rgba([20, 120, 200, if x < 100 { 255 } else { 0 }]);
        }
        rgba.save(&png).unwrap();
    });
    for (id, src) in [("E8-jpeg", &jpg), ("E8-png-alpha", &png)] {
        guarded(id, || {
            let input = fixture_bytes("simple.pdf");
            let mut doc = Document::from_bytes(input.clone(), None).unwrap();
            apply(&mut doc, serde_json::json!({"kind": "insert_image", "page": 0, "rect": rect(100.0, 100.0, 200.0, 150.0),
                "path": src.to_string_lossy()})).unwrap();
            let out = save_doc(id, &mut doc);
            verdict(
                id,
                "image source octets",
                std::fs::metadata(src).unwrap().len(),
            );
            verdict(id, "w*h*3", 200 * 150 * 3);
            verdict(
                id,
                "images en sortie",
                format!("{:?}", describe_images(&lo(&out))),
            );
            verdict(
                id,
                "taille fichier avant → après",
                format!("{} → {}", input.len(), out.len()),
            );
            verdict(
                id,
                "qpdf",
                qpdf_check(&out_dir().join(format!("{id}.pdf")), None),
            );
        });
    }
}

// --- E9 : qpdf hors conformance.rs ----------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e9_qpdf_uncovered_paths() {
    use serde_json::json;
    let dir = out_dir();
    let jpg = dir.join("E9-source.jpg");
    image::RgbImage::from_pixel(60, 40, image::Rgb([200, 30, 30]))
        .save(&jpg)
        .unwrap();
    let heavy = dir.join("E9-heavy-images.pdf");
    std::fs::write(&heavy, testing::heavy_images_pdf()).unwrap();
    let cases: Vec<(&str, &str, Option<&str>, Vec<serde_json::Value>)> = vec![
        (
            "E9-unprotect",
            "encrypted.pdf",
            Some("secret"),
            vec![json!({"kind": "unprotect"})],
        ),
        (
            "E9-checkbox",
            "form.pdf",
            None,
            vec![
                json!({"kind": "set_field_value", "page": 0, "index": 2, "value": {"kind": "checked", "checked": true}}),
            ],
        ),
        (
            "E9-radio",
            "form.pdf",
            None,
            vec![
                json!({"kind": "set_field_value", "page": 0, "index": 3, "value": {"kind": "checked", "checked": true}}),
            ],
        ),
        (
            "E9-combo",
            "form.pdf",
            None,
            vec![
                json!({"kind": "set_field_value", "page": 0, "index": 5, "value": {"kind": "text", "text": "Lyon"}}),
            ],
        ),
        (
            "E9-ellipse",
            "simple.pdf",
            None,
            vec![annotation(
                "ellipse",
                json!({"kind": "rect", "rect": rect(100.0, 500.0, 200.0, 100.0)}),
                "",
            )],
        ),
        (
            "E9-arrow",
            "simple.pdf",
            None,
            vec![annotation(
                "arrow",
                json!({"kind": "line", "from": [100.0, 300.0], "to": [300.0, 350.0]}),
                "",
            )],
        ),
        (
            "E9-underline",
            "out-of-order.pdf",
            None,
            vec![annotation(
                "underline",
                json!({"kind": "quads", "rects": [rect(70.0, 640.0, 200.0, 14.0)]}),
                "",
            )],
        ),
        (
            "E9-strike-out",
            "out-of-order.pdf",
            None,
            vec![annotation(
                "strike_out",
                json!({"kind": "quads", "rects": [rect(70.0, 640.0, 200.0, 14.0)]}),
                "",
            )],
        ),
        (
            "E9-squiggly",
            "out-of-order.pdf",
            None,
            vec![annotation(
                "squiggly",
                json!({"kind": "quads", "rects": [rect(70.0, 640.0, 200.0, 14.0)]}),
                "",
            )],
        ),
        (
            "E9-stamp",
            "simple.pdf",
            None,
            vec![annotation(
                "stamp",
                json!({"kind": "rect", "rect": rect(100.0, 400.0, 200.0, 40.0)}),
                "APPROUVÉ",
            )],
        ),
        (
            "E9-stamp-unicode",
            "simple.pdf",
            None,
            vec![annotation(
                "stamp",
                json!({"kind": "rect", "rect": rect(100.0, 400.0, 200.0, 40.0)}),
                "Ωμέγα",
            )],
        ),
        (
            "E9-watermark-unicode",
            "simple.pdf",
            None,
            vec![
                json!({"kind": "watermark", "spec": {"text": "Ωμέγα — Жук", "size": 60.0,
            "opacity": 0.3, "angle_deg": 45.0, "color": {"r": 200, "g": 0, "b": 0}, "pages": null}}),
            ],
        ),
        (
            "E9-page-numbers-unicode",
            "simple.pdf",
            None,
            vec![
                json!({"kind": "page_numbers", "spec": {"template": "Σελίδα {n}",
            "corner": "bottom_center", "margin": 20.0, "size": 10.0, "start": 1, "pages": null}}),
            ],
        ),
        (
            "E9-insert-text-helvetica",
            "simple.pdf",
            None,
            vec![
                json!({"kind": "insert_text", "page": 0, "at": [100.0, 600.0], "text": "Helvetica été",
            "style": {"family": "helvetica", "bold": true, "italic": false, "size": 14.0, "color": {"r": 0, "g": 0, "b": 0}}}),
            ],
        ),
        (
            "E9-set-object-text-unicode",
            "simple.pdf",
            None,
            vec![json!({"kind": "set_object_text", "page": 0, "index": 0, "text": "Σελίδα 1"})],
        ),
        (
            "E9-insert-image-jpeg",
            "simple.pdf",
            None,
            vec![
                json!({"kind": "insert_image", "page": 0, "rect": rect(100.0, 100.0, 120.0, 80.0), "path": jpg.to_string_lossy()}),
            ],
        ),
        (
            "E9-encrypted-rotate-watermark-annot",
            "encrypted.pdf",
            Some("secret"),
            vec![
                json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1}),
                json!({"kind": "watermark", "spec": {"text": "CONFIDENTIEL", "size": 60.0, "opacity": 0.3, "angle_deg": 45.0, "color": {"r": 200, "g": 0, "b": 0}, "pages": null}}),
                annotation(
                    "note",
                    json!({"kind": "rect", "rect": rect(80.0, 700.0, 20.0, 20.0)}),
                    "note chiffrée",
                ),
            ],
        ),
        (
            "E9-encrypted-redact",
            "encrypted.pdf",
            Some("secret"),
            vec![json!({"kind": "redact", "page": 0, "rect": rect(60.0, 740.0, 300.0, 60.0)})],
        ),
        (
            "E9-owner-only-edit",
            "owner-only.pdf",
            None,
            vec![json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1})],
        ),
    ];
    for (id, src, pw, cmds) in cases {
        guarded(id, || {
            let mut doc = Document::open(fixture(src), pw).unwrap();
            for c in cmds {
                if let Err(e) = apply(&mut doc, c) {
                    verdict(id, "commande refusée", e);
                    return;
                }
            }
            let _ = save_doc(id, &mut doc);
            let p = out_dir().join(format!("{id}.pdf"));
            verdict(id, "qpdf", qpdf_check(&p, pw));
            verdict(id, "mutool", mutool_check(&p, pw));
            verdict(id, "pdf.js", pdfjs_check(&p, pw));
        });
    }
    guarded("E9-optimize-images", || {
        let mut doc = Document::open(&heavy, None).unwrap();
        let before = doc.file_bytes().len();
        match apply(
            &mut doc,
            json!({"kind": "optimize", "options": {"image_quality": 60, "max_image_px": 800, "compress_streams": true}}),
        ) {
            Ok(()) => {
                let out = save_doc("E9-optimize-images", &mut doc);
                verdict(
                    "E9-optimize-images",
                    "taille avant → après",
                    format!("{before} → {}", out.len()),
                );
                verdict(
                    "E9-optimize-images",
                    "images",
                    format!("{:?}", describe_images(&lo(&out))),
                );
                let p = out_dir().join("E9-optimize-images.pdf");
                verdict("E9-optimize-images", "qpdf", qpdf_check(&p, None));
                verdict("E9-optimize-images", "mutool", mutool_check(&p, None));
            }
            Err(e) => verdict("E9-optimize-images", "refusé", e),
        }
    });
    for (id, src, appearance) in [
        (
            "E9-sign-visible-simple",
            "simple.pdf",
            SignAppearance {
                page: 1,
                rect: mega_core::Rect {
                    x: 100.0,
                    y: 100.0,
                    w: 220.0,
                    h: 70.0,
                },
                image_path: None,
            },
        ),
        (
            "E9-sign-visible-rotated",
            "rotated.pdf",
            SignAppearance {
                page: 0,
                rect: mega_core::Rect {
                    x: 50.0,
                    y: 50.0,
                    w: 220.0,
                    h: 70.0,
                },
                image_path: None,
            },
        ),
        (
            "E9-sign-visible-image",
            "simple.pdf",
            SignAppearance {
                page: 0,
                rect: mega_core::Rect {
                    x: 100.0,
                    y: 100.0,
                    w: 220.0,
                    h: 70.0,
                },
                image_path: Some(jpg.to_string_lossy().into_owned()),
            },
        ),
    ] {
        guarded(id, || {
            let mut doc = Document::open(fixture(src), None).unwrap();
            let p = out_dir().join(format!("{id}.pdf"));
            let options = SignOptions {
                reason: Some("Audit".into()),
                appearance: Some(appearance),
                ..SignOptions::default()
            };
            doc.sign_to(&p, &rsa(), &options).unwrap();
            let signed = Document::open(&p, None).unwrap();
            verdict(
                id,
                "statut",
                format!(
                    "{:?}",
                    signed
                        .signatures()
                        .unwrap()
                        .iter()
                        .map(|s| format!("{:?}", s.status))
                        .collect::<Vec<_>>()
                ),
            );
            verdict(id, "qpdf", qpdf_check(&p, None));
            verdict(id, "mutool", mutool_check(&p, None));
            verdict(id, "pdf.js", pdfjs_check(&p, None));
        });
    }
    guarded("E9-merge", || {
        let a = Document::open(fixture("simple.pdf"), None).unwrap();
        let b = Document::open(fixture("outlined.pdf"), None).unwrap();
        let c = Document::from_bytes(testing::rich_catalog_pdf("1.7", None), None).unwrap();
        let out = mega_core::pages::merge(&[&a, &b, &c]).unwrap();
        let p = save("E9-merge", &out);
        verdict(
            "E9-merge",
            "pages",
            Document::from_bytes(out.clone(), None)
                .unwrap()
                .page_count(),
        );
        verdict("E9-merge", "qpdf", qpdf_check(&p, None));
        verdict("E9-merge", "mutool", mutool_check(&p, None));
        verdict("E9-merge", "pdf.js", pdfjs_check(&p, None));
    });
    guarded("E9-split-extract", || {
        let doc = Document::open(fixture("outlined.pdf"), None).unwrap();
        let parts = mega_core::pages::split(&doc, &[vec![0], vec![1, 2]]).unwrap();
        for (i, part) in parts.iter().enumerate() {
            let p = save(&format!("E9-split-{i}"), part);
            verdict(
                "E9-split-extract",
                &format!("split {i} qpdf"),
                qpdf_check(&p, None),
            );
        }
        let ex = doc.extract_pages(&[2, 0]).unwrap();
        let p = save("E9-extract", &ex);
        verdict(
            "E9-split-extract",
            "extract [2,0] qpdf",
            qpdf_check(&p, None),
        );
        verdict(
            "E9-split-extract",
            "extract [2,0] texte p1",
            Document::from_bytes(ex, None)
                .unwrap()
                .page_text(0)
                .unwrap()
                .replace('\n', "⏎"),
        );
    });
}

// --- E10 : clés des annotations créées -----------------------------------------------------

fn describe_annot(doc: &Lo, a: &Dictionary) -> String {
    let g = |k: &str| {
        a.get(k.as_bytes())
            .map(|x| format!("{x:?}"))
            .unwrap_or_else(|_| "-".into())
    };
    let ap = a.get(b"AP").map(|ap| deref(doc, ap)).ok();
    let n = ap.as_ref().and_then(|ap| {
        ap.as_dict()
            .ok()
            .and_then(|d| d.get(b"N").ok().map(|n| deref(doc, n)))
    });
    let n_desc = n
        .as_ref()
        .and_then(|n| n.as_stream().ok())
        .map(|s| {
            let d = &s.dict;
            let gg = |k: &str| {
                d.get(k.as_bytes())
                    .map(|x| format!("{x:?}"))
                    .unwrap_or_else(|_| "-".into())
            };
            format!(
                "N{{BBox {} Matrix {} FormType {} Resources {} octets {} Filter {}}}",
                gg("BBox"),
                gg("Matrix"),
                gg("FormType"),
                d.get(b"Resources").is_ok(),
                s.content.len(),
                gg("Filter")
            )
        })
        .unwrap_or_else(|| "N absent".into());
    format!(
        "{} clés={:?} F={} C={} CA={} NM={} P={} M={} CreationDate={} T={} IT={} InkList={} Rect={} QuadPoints={} HasGeneratedAP={} {}",
        g("Subtype"),
        keys(a),
        g("F"),
        g("C"),
        g("CA"),
        g("NM"),
        g("P"),
        g("M"),
        g("CreationDate"),
        g("T"),
        g("IT"),
        a.get(b"InkList")
            .map(|l| l.as_array().map(|x| x.len()).unwrap_or(0))
            .unwrap_or(0),
        g("Rect"),
        a.get(b"QuadPoints").is_ok(),
        a.get(b"PDFIUM_HasGeneratedAP").is_ok(),
        n_desc
    )
}

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e10_created_annotation_keys() {
    use serde_json::json;
    guarded("E10", || {
        let mut doc = Document::open(fixture("simple.pdf"), None).unwrap();
        for cmd in [
            annotation(
                "note",
                json!({"kind": "rect", "rect": rect(80.0, 700.0, 20.0, 20.0)}),
                "une note",
            ),
            annotation(
                "highlight",
                json!({"kind": "quads", "rects": [rect(70.0, 740.0, 200.0, 24.0)]}),
                "",
            ),
            annotation(
                "rect",
                json!({"kind": "rect", "rect": rect(100.0, 500.0, 200.0, 100.0)}),
                "",
            ),
            annotation(
                "stamp",
                json!({"kind": "rect", "rect": rect(100.0, 400.0, 200.0, 40.0)}),
                "APPROUVÉ",
            ),
            annotation(
                "text",
                json!({"kind": "rect", "rect": rect(100.0, 300.0, 200.0, 40.0)}),
                "Texte libre",
            ),
        ] {
            apply(&mut doc, cmd).unwrap();
        }
        let raw = doc.to_bytes().unwrap();
        let draw = lo(&raw);
        for a in annots(&draw, page_ids(&draw)[0]) {
            verdict(
                "E10-brut",
                "annotation (to_bytes nu)",
                describe_annot(&draw, &a),
            );
        }
        let out = save_doc("E10", &mut doc);
        let dout = lo(&out);
        for a in annots(&dout, page_ids(&dout)[0]) {
            verdict(
                "E10",
                "annotation (fichier enregistré)",
                describe_annot(&dout, &a),
            );
        }
        let p = out_dir().join("E10.pdf");
        verdict("E10", "qpdf", qpdf_check(&p, None));
        verdict("E10", "mutool", mutool_check(&p, None));
        verdict("E10", "pdf.js", pdfjs_check(&p, None));
        let reread = Document::from_bytes(out, None).unwrap();
        for a in reread.annotations(0).unwrap() {
            verdict(
                "E10-relu",
                "annotation relue (mega-core)",
                format!(
                    "{:?} color {:?} editable {} rect {:?}",
                    a.kind, a.color, a.editable, a.rect
                ),
            );
        }
    });
}

// --- L11 : arbre profond et héritage --------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn l11_deep_page_tree_and_inheritance() {
    guarded("L11", || {
        let input = deep_pages_pdf(50);
        save("L11-entree", &input);
        let doc = Document::from_bytes(input.clone(), None).unwrap();
        verdict("L11", "pages", doc.page_count());
        verdict(
            "L11",
            "taille p1 (attendu 600×400 après /Rotate 90)",
            format!("{:?}", doc.page_size(0)),
        );
        verdict("L11", "rotation p1", format!("{:?}", doc.page_rotation(0)));
        let img = doc.render_page(0, 1.0).unwrap();
        verdict(
            "L11",
            "rendu px",
            format!("{}×{}", img.width_px, img.height_px),
        );
        verdict(
            "L11",
            "pixels sombres",
            dark_pixels(&img, 0.0, 0.0, img.width_px as f32, img.height_px as f32),
        );
        verdict("L11", "texte", doc.page_text(0).unwrap().replace('\n', "⏎"));
        let mut doc = doc;
        apply(
            &mut doc,
            serde_json::json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1}),
        )
        .unwrap();
        let out = save_doc("L11", &mut doc);
        let dout = lo(&out);
        verdict("L11", "sortie : /ModDate (passe lopdf)", mod_date(&dout));
        verdict("L11", "sortie : pages (lopdf)", page_ids(&dout).len());
        let reread = Document::from_bytes(out, None).unwrap();
        verdict(
            "L11",
            "sortie : rotation p1 (attendu 180)",
            format!("{:?}", reread.page_rotation(0)),
        );
        verdict(
            "L11",
            "sortie : taille p1",
            format!("{:?}", reread.page_size(0)),
        );
        verdict(
            "L11",
            "sortie : texte",
            reread.page_text(0).unwrap().replace('\n', "⏎"),
        );
        verdict("L11", "qpdf", qpdf_check(&out_dir().join("L11.pdf"), None));
    });
    guarded("L11-inherited-mediabox", || {
        let doc = Document::from_bytes(testing::inherited_media_box_pdf(), None).unwrap();
        verdict(
            "L11-inherited-mediabox",
            "taille (attendu 400×600)",
            format!("{:?}", doc.page_size(0)),
        );
        verdict(
            "L11-inherited-mediabox",
            "frame",
            format!("{:?}", doc.page_frame(0)),
        );
    });
}

// --- L12 : UserUnit --------------------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn l12_user_unit() {
    guarded("L12", || {
        let doc = Document::from_bytes(testing::user_unit_pdf(), None).unwrap();
        verdict(
            "L12",
            "taille rapportée (/MediaBox 100 pt, /UserUnit 10)",
            format!("{:?}", doc.page_size(0)),
        );
        let img = doc.render_page(0, 1.0).unwrap();
        verdict(
            "L12",
            "rendu px",
            format!("{}×{}", img.width_px, img.height_px),
        );
        verdict("L12", "pdf_version", doc.properties().unwrap().pdf_version);
    });
}

// --- L13 : destinations nommées -------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn l13_named_destinations() {
    guarded("L13", || {
        let doc = Document::from_bytes(testing::rich_catalog_pdf("1.7", None), None).unwrap();
        let outline: Vec<String> = doc
            .outline()
            .unwrap()
            .iter()
            .map(|n| format!("{}→{:?}", n.title, n.page))
            .collect();
        verdict(
            "L13",
            "signets (chaîne nommée, /A GoTo /D nom, directe)",
            format!("{outline:?}"),
        );
        let links: Vec<String> = doc
            .links(0)
            .unwrap()
            .iter()
            .map(|l| format!("{:?}@{:?}", l.target, l.rect))
            .collect();
        verdict("L13", "liens p1 (même trois formes)", format!("{links:?}"));
        verdict(
            "L13",
            "annotations p1 (genres)",
            format!(
                "{:?}",
                doc.annotations(0)
                    .unwrap()
                    .iter()
                    .map(|a| format!("{:?}", a.kind))
                    .collect::<Vec<_>>()
            ),
        );
    });
}

// --- L14 : polices ---------------------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn l14_fonts_text_extraction() {
    for (kind, expected) in [
        ("helvetica-differences", "é"),
        ("truetype-symbolic", "A ou B ?"),
        ("cid-nounicode", "code brut ou U+FFFD ?"),
        ("cid-unicode", "Ω"),
    ] {
        let id = format!("L14-{kind}");
        guarded(&id, || {
            let bytes = fonts_pdf(kind);
            let p = save(&id, &bytes);
            let doc = Document::from_bytes(bytes, None).unwrap();
            let text = doc.page_text(0).unwrap();
            verdict(
                &id,
                &format!("texte extrait (attendu {expected})"),
                format!(
                    "{text:?} = {:?}",
                    text.chars()
                        .map(|c| format!("U+{:04X}", c as u32))
                        .collect::<Vec<_>>()
                ),
            );
            let boxes = doc.text_boxes(0).unwrap();
            verdict(&id, "boîtes", boxes.len());
            let img = doc.render_page(0, 1.0).unwrap();
            verdict(
                &id,
                "pixels sombres autour du glyphe",
                dark_pixels(&img, 60.0, 690.0, 60.0, 40.0),
            );
            verdict(
                &id,
                "objets de page (police)",
                format!(
                    "{:?}",
                    doc.page_objects(0)
                        .unwrap()
                        .iter()
                        .map(|o| (o.font.clone(), o.text.clone()))
                        .collect::<Vec<_>>()
                ),
            );
            verdict(&id, "qpdf", qpdf_check(&p, None));
        });
    }
    guarded("L14-temoin-cid", || {
        let doc =
            Document::from_bytes(mega_core::testing::hostile::cid_without_unicode_pdf(), None)
                .unwrap();
        verdict(
            "L14-temoin-cid",
            "texte (hostile::cid_without_unicode_pdf)",
            format!("{:?}", doc.page_text(0).unwrap()),
        );
    });
}

// --- L15 : annotations tierces ---------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn l15_third_party_annotations() {
    guarded("L15", || {
        let with = testing::third_party_annots_pdf();
        // Témoin : la même page sans `/Annots`.
        let mut witness_doc = lo(&with);
        witness_doc
            .get_dictionary_mut((3, 0))
            .unwrap()
            .remove(b"Annots");
        let mut without = Vec::new();
        witness_doc.save_to(&mut without).unwrap();
        save("L15-entree", &with);
        let doc = Document::from_bytes(with.clone(), None).unwrap();
        let witness = Document::from_bytes(without, None).unwrap();
        let img = doc.render_page(0, 1.0).unwrap();
        let img0 = witness.render_page(0, 1.0).unwrap();
        let zones = [
            ("Polygon", 50.0, 400.0, 100.0, 80.0),
            ("FreeText", 200.0, 400.0, 150.0, 40.0),
            ("Square", 50.0, 250.0, 100.0, 100.0),
            ("Link", 200.0, 250.0, 150.0, 50.0),
            ("FileAttachment", 400.0, 400.0, 20.0, 20.0),
            ("Text", 400.0, 250.0, 20.0, 20.0),
            ("Popup", 300.0, 100.0, 150.0, 100.0),
        ];
        for (name, x, y, w, h) in zones {
            verdict(
                "L15",
                &format!("{name} : pixels sombres avec / sans annots"),
                format!(
                    "{} / {}",
                    dark_pixels(&img, x, y, w, h),
                    dark_pixels(&img0, x, y, w, h)
                ),
            );
        }
        for a in doc.annotations(0).unwrap() {
            verdict(
                "L15",
                "lue comme",
                format!(
                    "#{} {:?} rect {:?} color {:?} contents {:?} editable {}",
                    a.index, a.kind, a.rect, a.color, a.contents, a.editable
                ),
            );
        }
        verdict(
            "L15",
            "liens",
            format!(
                "{:?}",
                doc.links(0)
                    .unwrap()
                    .iter()
                    .map(|l| format!("{:?}", l.target))
                    .collect::<Vec<_>>()
            ),
        );
        verdict(
            "L15",
            "pièces jointes listées",
            format!("{:?}", doc.attachments().map(|a| a.len())),
        );
        let raw = doc.to_bytes().unwrap();
        let draw = lo(&raw);
        let generated: Vec<String> = annots(&draw, page_ids(&draw)[0])
            .iter()
            .map(|a| {
                format!(
                    "{:?}:AP={} gen={}",
                    a.get(b"Subtype").ok(),
                    a.get(b"AP").is_ok(),
                    a.get(b"PDFIUM_HasGeneratedAP").is_ok()
                )
            })
            .collect();
        verdict(
            "L15",
            "to_bytes nu après rendu : /AP par annotation",
            format!("{generated:?}"),
        );
        let cleaned = mega_core::cleanup::before_save(&raw, None, None)
            .unwrap()
            .bytes;
        let dc = lo(&cleaned);
        let after: Vec<String> = annots(&dc, page_ids(&dc)[0])
            .iter()
            .map(|a| {
                format!(
                    "{:?}:AP={} NM={} P={}",
                    a.get(b"Subtype").ok(),
                    a.get(b"AP").is_ok(),
                    a.get(b"NM").is_ok(),
                    a.get(b"P").is_ok()
                )
            })
            .collect();
        verdict(
            "L15",
            "après before_save : annotations",
            format!("{after:?}"),
        );
        let p = save("L15-sortie", &cleaned);
        verdict("L15", "qpdf", qpdf_check(&p, None));
        verdict("L15", "mutool", mutool_check(&p, None));
    });
}

// --- L16 : /StmF /Identity --------------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn l16_identity_crypt_filter() {
    guarded("L16", || {
        let bytes = testing::attachments_only_encrypted_pdf();
        let mut doc = Document::from_bytes(bytes, None).unwrap();
        verdict("L16", "texte", format!("{:?}", doc.page_text(0)));
        verdict(
            "L16",
            "titre",
            format!("{:?}", doc.properties().map(|p| p.title)),
        );
        verdict("L16", "security()", format!("{:?}", doc.security()));
        let w = mega_core::Watermark {
            text: "X".into(),
            size: 40.0,
            opacity: 0.5,
            angle_deg: 0.0,
            color: mega_core::annotations::Color { r: 0, g: 0, b: 0 },
            pages: None,
        };
        verdict(
            "L16",
            "add_watermark",
            format!("{:?}", doc.add_watermark(&w).map_err(|e| e.to_string())),
        );
        verdict(
            "L16",
            "set_metadata",
            format!(
                "{:?}",
                doc.set_metadata(&mega_core::MetadataPatch {
                    title: Some("T".into()),
                    ..Default::default()
                })
                .map_err(|e| e.to_string())
            ),
        );
        let out = save_doc("L16", &mut doc);
        verdict(
            "L16",
            "save_as : /Encrypt encore présent",
            contains(&out, "/Encrypt"),
        );
        verdict("L16", "qpdf", qpdf_check(&out_dir().join("L16.pdf"), None));
    });
}

// --- L17 : métadonnées --------------------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn l17_metadata_decoding() {
    for (kind, expected) in [
        ("xmp-only", "« Ancien » (XMP) ou None"),
        ("pdfdoc-e9", "Café"),
        ("pdfdoc-80-a0", "• € (PDFDocEncoding)"),
        ("utf8-raw", "Café ou CafÃ©"),
    ] {
        let id = format!("L17-{kind}");
        guarded(&id, || {
            let bytes = testing::metadata_variant_pdf(kind);
            let doc = Document::from_bytes(bytes, None).unwrap();
            let props = doc.properties().unwrap();
            verdict(
                &id,
                &format!("properties().title (attendu {expected})"),
                format!("{:?}", props.title),
            );
            verdict(
                &id,
                "Document::title() (PDFium)",
                format!("{:?}", doc.title()),
            );
            verdict(
                &id,
                "author / modification_date",
                format!("{:?} / {:?}", props.author, props.modification_date),
            );
        });
    }
}

// --- L18 : signatures -------------------------------------------------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn l18_signature_formats() {
    for sf in [
        "adbe.pkcs7.sha1",
        "ETSI.RFC3161",
        "adbe.x509.rsa_sha1",
        "adbe.pkcs7.detached",
    ] {
        let id = format!("L18-{sf}");
        guarded(&id, || {
            let bytes = sig_subfilter_pdf(sf);
            save(&id, &bytes);
            let r = mega_core::digital_signature::signatures(&bytes);
            verdict(
                &id,
                "signatures(bytes)",
                format!(
                    "{:?}",
                    r.as_ref()
                        .map(|v| v
                            .iter()
                            .map(|s| format!(
                                "{} {:?} covers={}",
                                s.sub_filter, s.status, s.covers_whole_document
                            ))
                            .collect::<Vec<_>>())
                        .map_err(|e| e.to_string())
                ),
            );
            let doc = Document::from_bytes(bytes, None);
            verdict(&id, "PDFium ouvre", doc.is_ok());
        });
    }
    guarded("L18-multi", || {
        let mut bytes = fixture_bytes("simple.pdf");
        for (i, identity) in [rsa(), ec(), rsa()].iter().enumerate() {
            let mut doc = Document::from_bytes(bytes.clone(), None).unwrap();
            let p = out_dir().join(format!("L18-multi-{i}.pdf"));
            let options = SignOptions {
                reason: Some(format!("Signature {i}")),
                ..SignOptions::default()
            };
            match doc.sign_to(&p, identity, &options) {
                Ok(()) => bytes = std::fs::read(&p).unwrap(),
                Err(e) => {
                    verdict(
                        "L18-multi",
                        &format!("signature {i} refusée"),
                        e.to_string(),
                    );
                    return;
                }
            }
        }
        let list = mega_core::digital_signature::signatures(&bytes).unwrap();
        verdict(
            "L18-multi",
            "trois signatures successives",
            format!(
                "{:?}",
                list.iter()
                    .map(|s| format!(
                        "{} {:?} covers={} reason={:?}",
                        s.field, s.status, s.covers_whole_document, s.reason
                    ))
                    .collect::<Vec<_>>()
            ),
        );
        verdict(
            "L18-multi",
            "qpdf",
            qpdf_check(&out_dir().join("L18-multi-2.pdf"), None),
        );
    });
    guarded("L18-pades", || {
        let bytes = fixture_bytes("signed-external-pades.pdf");
        let list = mega_core::digital_signature::signatures(&bytes).unwrap();
        verdict(
            "L18-pades",
            "signed-external-pades.pdf",
            format!(
                "{:?}",
                list.iter()
                    .map(|s| format!("{} {:?}", s.sub_filter, s.status))
                    .collect::<Vec<_>>()
            ),
        );
        verdict(
            "L18-pades",
            "contient ETSI.RFC3161",
            contains(&bytes, "ETSI.RFC3161"),
        );
    });
}

// --- L19 : texte extrait sur le corpus réel (écarts relevés par scripts/audit/read-bench.sh) ------

/// Pour chaque fichier du corpus réel où le banc de lecture a vu un écart de texte avec poppler,
/// MuPDF et pdf.js, dit si l'écart vient de PDFium (aucune boîte de texte) ou de notre mise en
/// ordre (`reading_order`), et compte les caractères hors du plan multilingue de base.
#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn l19_corpus_text() {
    let _ = IGNORE;
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus");
    for (rel, password, needle) in [
        ("polices/Embedded_font.pdf", None, "除"),
        ("polices/nonembedded_type1_tounicode.pdf", None, "é"),
        ("producteurs/bug1937438_from_word.pdf", None, "\u{1D70B}"),
        ("polices/ArabicCIDTrueType.pdf", None, " "),
        ("polices/Type3WordSpacing.pdf", None, "ab ba"),
        ("polices/perf_text_cjk_nonembedded.pdf", None, "喘"),
        ("local/edge-article.pdf", None, "😀"),
        ("local/edge-article.pdf", None, "ﬁ"),
        (
            "chiffrement/enc-R6-U-view-O-master.pdf",
            Some("view"),
            "Potato",
        ),
    ] {
        let tag = format!("L19-{}", rel.rsplit('/').next().unwrap_or(rel));
        let path = corpus.join(rel);
        if !path.exists() {
            verdict(
                &tag,
                "fichier",
                "absent (lancer scripts/audit/fetch-corpus.sh)",
            );
            continue;
        }
        guarded(&tag, || {
            let doc = Document::open(&path, password).expect("ouverture");
            let boxes = doc.text_boxes(0).expect("text_boxes");
            let text = doc.page_text(0).expect("page_text");
            let astral = boxes.iter().filter(|b| (b.ch as u32) > 0xFFFF).count();
            let spaces = boxes.iter().filter(|b| b.ch == ' ').count();
            let nonfinite = boxes
                .iter()
                .filter(|b| {
                    !(b.rect.x.is_finite()
                        && b.rect.y.is_finite()
                        && b.rect.w.is_finite()
                        && b.rect.h.is_finite())
                })
                .count();
            verdict(&tag, "boîtes PDFium (après drawn_chars)", boxes.len());
            verdict(&tag, "caractères hors BMP", astral);
            verdict(&tag, "espaces", spaces);
            verdict(&tag, "boîtes non finies", nonfinite);
            verdict(
                &tag,
                "page_text (80 car.)",
                text.chars().take(80).collect::<String>().replace('\n', "⏎"),
            );
            verdict(&tag, &format!("contient {needle:?}"), text.contains(needle));
            let hits = doc
                .search_page(0, needle, &Default::default())
                .map(|h| h.len())
                .unwrap_or(usize::MAX);
            verdict(&tag, &format!("search_page {needle:?}"), hits);
        });
    }
}

// --- E20 : remesures demandées par la relecture normative -----------------------------------

#[test]
#[ignore = "banc de mesure, lancer avec --ignored"]
fn e20_remeasures() {
    let _ = IGNORE;
    // E20a : déplacer la page 1 (liens, BDC, /StructParents) en dernier.
    guarded("E20a-move-p1-last", || {
        let input = testing::rich_catalog_pdf("1.7", None);
        let mut doc = Document::from_bytes(input.clone(), None).unwrap();
        apply(
            &mut doc,
            serde_json::json!({"kind": "move_pages", "pages": [0], "to": 2}),
        )
        .unwrap();
        let out = save_doc("E20a-move-p1-last", &mut doc);
        measure_catalog("E20a-move-p1-last", &input, &out, None);
        let doc = Document::from_bytes(out.clone(), None).unwrap();
        verdict(
            "E20a-move-p1-last",
            "texte p3 (ex-p1)",
            doc.page_text(2).unwrap_or_default(),
        );
        let links: Vec<String> = doc
            .links(2)
            .unwrap()
            .iter()
            .map(|l| format!("{:?}", l.target))
            .collect();
        verdict(
            "E20a-move-p1-last",
            "liens p3 (ex-p1)",
            format!("{links:?}"),
        );
        verdict(
            "E20a-move-p1-last",
            "annotations p3 (ex-p1)",
            doc.annotations(2).map(|a| a.len()).unwrap_or(usize::MAX),
        );
        let d = lo(&out);
        let ids = page_ids(&d);
        let c = d.get_page_content(ids[2]);
        verdict(
            "E20a-move-p1-last",
            "p3 : BDC/EMC/MCID",
            format!(
                "BDC {} EMC {} MCID {}",
                contains(&c, "BDC"),
                contains(&c, "EMC"),
                contains(&c, "MCID")
            ),
        );
    });

    // E20b : supprimer un objet sur une page qui en a deux, chacun balisé.
    guarded("E20b-delete-one-of-two", || {
        let input = testing::two_marked_objects_pdf();
        let mut doc = Document::from_bytes(input, None).unwrap();
        apply(
            &mut doc,
            serde_json::json!({"kind": "delete_object", "page": 0, "index": 0}),
        )
        .unwrap();
        let out = save_doc("E20b-delete-one-of-two", &mut doc);
        let d = lo(&out);
        let c = d.get_page_content(page_ids(&d)[0]);
        verdict(
            "E20b-delete-one-of-two",
            "contenu régénéré",
            String::from_utf8_lossy(&c).replace('\n', " "),
        );
        verdict(
            "E20b-delete-one-of-two",
            "MCID 0 / MCID 1 / Deux",
            format!(
                "{} / {} / {}",
                contains(&c, "MCID 0"),
                contains(&c, "MCID 1"),
                contains(&c, "Deux")
            ),
        );
    });

    // E20c : page supprimée mais encore citée par un signet et une destination nommée.
    guarded("E20c-delete-cited-page", || {
        let input = testing::rich_catalog_pdf("1.7", None);
        let mut doc = Document::from_bytes(input, None).unwrap();
        apply(
            &mut doc,
            serde_json::json!({"kind": "delete_pages", "pages": [1]}),
        )
        .unwrap();
        let out = save_doc("E20c-delete-cited-page", &mut doc);
        let d = lo(&out);
        let tree = page_ids(&d);
        let page_dicts: Vec<ObjectId> = d
            .objects
            .iter()
            .filter(|(_, o)| {
                o.as_dict()
                    .ok()
                    .and_then(|x| x.get(b"Type").ok())
                    .and_then(|t| t.as_name().ok())
                    == Some(b"Page".as_slice())
            })
            .map(|(id, _)| *id)
            .collect();
        verdict(
            "E20c-delete-cited-page",
            "pages dans l arbre / dictionnaires de page",
            format!("{} / {}", tree.len(), page_dicts.len()),
        );
        let ghosts: Vec<bool> = page_dicts
            .iter()
            .filter(|id| !tree.contains(id))
            .map(|id| {
                let c = d.get_page_content(*id);
                String::from_utf8_lossy(&c).contains("Page 2")
            })
            .collect();
        verdict(
            "E20c-delete-cited-page",
            "page fantôme contenant « Page 2 »",
            format!("{ghosts:?}"),
        );
    });

    // E20d : /Parent des widgets après merge (champ parent/enfant style iText).
    guarded("E20d-merge-widget-parent", || {
        let a = Document::from_bytes(testing::form_parent_kid_pdf(), None).unwrap();
        let b = Document::from_bytes(fixture_bytes("simple.pdf"), None).unwrap();
        let out = mega_core::pages::merge(&[&b, &a]).unwrap();
        save("E20d-merge-widget-parent", &out);
        let d = lo(&out);
        let mut report = Vec::new();
        for pid in page_ids(&d) {
            for ann in annots(&d, pid) {
                if ann.get(b"Subtype").ok().and_then(|s| s.as_name().ok())
                    != Some(b"Widget".as_slice())
                {
                    continue;
                }
                let parent = ann.get(b"Parent").ok().map(|p| deref(&d, p));
                let desc = match parent {
                    Some(Object::Dictionary(pd)) => format!(
                        "Parent -> dict avec /T {} /FT {}",
                        pd.has(b"T"),
                        pd.has(b"FT")
                    ),
                    Some(o) => format!("Parent -> {o:?}"),
                    None => "sans /Parent".into(),
                };
                let name = ann
                    .get(b"T")
                    .ok()
                    .and_then(|t| t.as_str().ok())
                    .map(|t| String::from_utf8_lossy(t).into_owned());
                report.push(format!("widget /T {name:?} : {desc}"));
            }
        }
        verdict("E20d-merge-widget-parent", "widgets", format!("{report:?}"));
        let doc = Document::from_bytes(out, None).unwrap();
        let fields: Vec<String> = (0..doc.page_count())
            .flat_map(|p| doc.form_fields(p).unwrap_or_default())
            .map(|f| format!("{}={:?}", f.name, f.value))
            .collect();
        verdict(
            "E20d-merge-widget-parent",
            "form_fields (PDFium)",
            format!("{fields:?}"),
        );
    });

    // E20e : chaîne UTF-8 avec BOM (PDF 2.0) dans /Info /Title.
    guarded("E20e-utf8-bom-title", || {
        let mut d = lo(&fixture_bytes("simple.pdf"));
        let info_id = d.trailer.get(b"Info").unwrap().as_reference().unwrap();
        if let Ok(Object::Dictionary(info)) = d.get_object_mut(info_id) {
            info.set(
                "Title",
                Object::String(
                    b"\xEF\xBB\xBFCaf\xC3\xA9".to_vec(),
                    lopdf::StringFormat::Literal,
                ),
            );
        }
        let mut bytes = Vec::new();
        d.save_to(&mut bytes).unwrap();
        let doc = Document::from_bytes(bytes, None).unwrap();
        verdict(
            "E20e-utf8-bom-title",
            "properties().title (attendu Café)",
            format!("{:?}", doc.properties().map(|p| p.title)),
        );
        verdict(
            "E20e-utf8-bom-title",
            "Document::title() (PDFium)",
            format!("{:?}", doc.title()),
        );
    });
}
