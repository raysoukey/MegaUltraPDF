// SPDX-License-Identifier: GPL-3.0-or-later
//! Filigrane, numérotation (en-tête / pied), recadrage (ADR-0010) et couche de texte
//! invisible (OCR, Lot 8 : même technique, mode de rendu `3 Tr`).
//!
//! Filigrane et numéros sont des **flux de contenu ajoutés par `lopdf`** après le contenu
//! existant : `q … Q` isole l'état graphique, un `ExtGState` porte l'opacité, une police
//! Helvetica (Standard 14, encodage WinAnsi) est ajoutée aux ressources de chaque page ; un
//! texte hors WinAnsi (grec, cyrillique…) utilise la police Unicode incorporée
//! (module `fonts::unicode`, Lot 10).
//! Rien ne dépend de PDFium : le résultat est exact dans tous les lecteurs. PDFium sert
//! seulement à **mesurer** la largeur du texte (pour centrer ou aligner à droite) et à
//! recharger le document ensuite. Le recadrage est un `/CropBox` posé par PDFium.
//!
//! \[Apprentissage\] **Deux phases, un seul verrou à la fois** : on calcule d'abord tout ce
//! qui demande PDFium (repères, titre, largeurs), chaque appel prenant et rendant le jeton ;
//! puis `lopdf` travaille sur les octets sans PDFium ; enfin le document est rechargé. Le
//! verrou de `engine::guard` n'est pas réentrant : imbriquer ces appels bloquerait.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::annotations::{Color, to_pdf_rect};
use crate::content::win_ansi_byte;
use crate::crypt::LopdfSession;
use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};
use crate::fonts;
use crate::fonts::unicode::{Type0Writer, UnicodeFont};
use crate::geometry::{PageFrame, Rect, Rotation};
use crate::properties::Conformance;

/// Longueur maximale d'un texte de filigrane ou de numérotation (après substitution).
pub const MAX_OVERLAY_TEXT_CHARS: usize = 200;
/// Nom de ressource de la police Helvetica que nous ajoutons aux pages.
const FONT_KEY: &str = "MegaF1";
/// Préfixe du nom de ressource de la police Unicode (DejaVu Sans, `Type0`) que nous ajoutons
/// aux pages quand un texte sort de l'alphabet WinAnsi (Lot 10). Chaque passe écrit **sa**
/// police (ses propres CID) sous un nom libre (`MegaU1`, `MegaU2`…) : réutiliser un nom ferait
/// relire les textes d'une passe précédente avec la table de la nouvelle police.
const UNICODE_FONT_PREFIX: &str = "MegaU";
/// Préfixe du nom de ressource des états graphiques (opacité) que nous ajoutons.
const GS_PREFIX: &str = "MegaGS";
/// Préfixe du nom de ressource de l'espace de couleur calibré (`CalRGB`) que nous ajoutons
/// aux pages d'un document PDF/A (`MegaCS1`, `MegaCS2`… : un nom libre de la page).
const COLOR_SPACE_PREFIX: &str = "MegaCS";
/// Point blanc D65 (CIE 1931, Y = 1), celui de sRGB.
const D65_WHITE_POINT: [f32; 3] = [0.9505, 1.0, 1.089];
/// Gamma et matrice (colonnes R, G, B → X, Y, Z) de sRGB (CEI 61966-2-1, gamma approché à
/// 2,2) : sans eux, un `CalRGB` lirait nos composantes comme des X, Y, Z (matrice identité) et
/// un lecteur qui convertit vraiment les couleurs (PDFium, pdf.js) les fausserait. La somme de
/// chaque ligne redonne le point blanc : le blanc reste blanc.
const SRGB_GAMMA: f32 = 2.2;
const SRGB_MATRIX: [f32; 9] = [
    0.4124, 0.2126, 0.0193, 0.3576, 0.7152, 0.1192, 0.1805, 0.0722, 0.9505,
];
/// Ouverture du contenu marqué d'un ajout décoratif (filigrane, couche OCR) : un artefact sans
/// propriétés (ISO 32000-1 §14.8.2.2).
const ARTIFACT: &[u8] = b"/Artifact BMC\n";
/// Ouverture du contenu marqué d'un numéro de page : un artefact **de pagination**
/// (§14.8.2.2.2), que les technologies d'assistance savent reconnaître comme tel.
const PAGINATION_ARTIFACT: &[u8] = b"/Artifact <</Type /Pagination>> BDC\n";

/// Un filigrane : texte centré sur la page, incliné, translucide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Watermark {
    /// Le texte (au plus [`MAX_OVERLAY_TEXT_CHARS`] ; Helvetica si WinAnsi, sinon la police
    /// Unicode incorporée).
    pub text: String,
    /// Taille de police en points.
    pub size: f32,
    /// Opacité 0-1 (exclu 0).
    pub opacity: f32,
    /// Inclinaison en degrés (sens trigonométrique ; 45 = montant vers la droite).
    pub angle_deg: f32,
    /// Couleur.
    pub color: Color,
    /// Pages concernées (toutes si `None`).
    pub pages: Option<Vec<u16>>,
}

/// Où poser un numéro de page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    /// En-tête, à gauche.
    TopLeft,
    /// En-tête, centré.
    TopCenter,
    /// En-tête, à droite.
    TopRight,
    /// Pied, à gauche.
    BottomLeft,
    /// Pied, centré.
    BottomCenter,
    /// Pied, à droite.
    BottomRight,
}

/// Numérotation des pages (ou tout en-tête / pied de page à base de modèle).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageNumbering {
    /// Modèle : `{n}` numéro, `{total}` nombre de pages, `{titre}` (ou `{title}`) titre du
    /// document.
    pub template: String,
    /// Position.
    pub corner: Corner,
    /// Marge depuis les bords, en points.
    pub margin: f32,
    /// Taille de police en points.
    pub size: f32,
    /// Numéro de la première page du document.
    pub start: u16,
    /// Pages concernées (toutes si `None`).
    pub pages: Option<Vec<u16>>,
}

/// Recadrage par marges depuis la MediaBox, en points **affichés** : gauche, bas, droite,
/// haut. Quatre marges nulles = retirer le recadrage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CropSpec {
    /// Pages concernées (toutes si `None`).
    pub pages: Option<Vec<u16>>,
    /// Marges `[gauche, bas, droite, haut]`.
    pub margins: [f32; 4],
}

fn invalid(msg: &str) -> CoreError {
    CoreError::InvalidOperation(msg.to_string())
}

fn structure(e: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidOperation(format!("structure du PDF illisible : {e}"))
}

/// Un mot de la couche de texte **invisible** (OCR) : son texte et sa boîte, en espace affiché
/// (origine en bas à gauche de la page telle qu'on la voit). Le mot est étiré pour remplir la
/// largeur de sa boîte, si bien que la recherche et la sélection tombent sur l'image du mot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvisibleWord {
    /// Le texte reconnu (Helvetica si WinAnsi, sinon la police Unicode incorporée ; un
    /// caractère absent de celle-ci reste trouvable par la recherche).
    pub text: String,
    /// Boîte du mot, en points, espace affiché.
    pub rect: Rect,
}

/// Part de la hauteur de boîte sous la ligne de base (jambages descendants d'Helvetica).
const DESCENT_RATIO: f32 = 0.2;

/// Un texte prêt à écrire : page, contenu, style, matrice texte en coordonnées affichées.
struct Placement {
    page: u16,
    text: String,
    size: f32,
    color: Color,
    opacity: f32,
    /// Vrai si le texte sort de WinAnsi : il est écrit avec la police Unicode.
    unicode: bool,
    /// Matrice `Tm` (rotation + position de la ligne de base), en espace affiché.
    tm: [f32; 6],
    frame: PageFrame,
}

/// Nombre au format des flux de contenu (pas de notation exponentielle).
pub(crate) fn num(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Chaîne hexadécimale WinAnsi `<…>` pour `Tj` ; `None` si un caractère n'est pas encodable.
pub(crate) fn hex_win_ansi(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len() * 2 + 2);
    out.push('<');
    for c in text.chars() {
        out.push_str(&format!("{:02X}", win_ansi_byte(c)?));
    }
    out.push('>');
    Some(out)
}

fn check_text(text: &str) -> CoreResult<()> {
    if text.trim().is_empty() {
        return Err(invalid("le texte ne peut pas être vide"));
    }
    if text.chars().count() > MAX_OVERLAY_TEXT_CHARS {
        return Err(invalid("texte trop long (200 caractères au plus)"));
    }
    fonts::check_drawable(text)
}

impl Document {
    /// Pages visées, uniques et triées ; toutes si `None` ; chacune doit exister.
    fn target_pages(&self, pages: &Option<Vec<u16>>) -> CoreResult<Vec<u16>> {
        let count = self.page_count();
        match pages {
            None => Ok((0..count).collect()),
            Some(list) => {
                if list.is_empty() {
                    return Err(invalid("aucune page sélectionnée"));
                }
                let set: BTreeSet<u16> = list.iter().copied().collect();
                for &p in &set {
                    if p >= count {
                        return Err(CoreError::PageOutOfRange { index: p, count });
                    }
                }
                Ok(set.into_iter().collect())
            }
        }
    }

    /// La conformité que le document déclare (PDF/A, PDF/UA), lue dans la vue `lopdf` gardée :
    /// rien à relire tant que le document n'a pas changé. Elle décide de la police (incorporée
    /// même en WinAnsi), de l'espace de couleur et de la transparence du texte ajouté (audit
    /// E6). À lire **avant** `mark_changed`, qui vide cette vue.
    fn conformance(&self) -> Option<Conformance> {
        self.lopdf_view()
            .and_then(crate::properties::conformance_of)
    }

    /// Pose un filigrane sur les pages visées.
    pub fn add_watermark(&mut self, w: &Watermark) -> CoreResult<()> {
        // Lue avant `mark_changed`, qui vide la vue `lopdf` gardée.
        let conformance = self.conformance();
        check_text(&w.text)?;
        if !(w.size.is_finite() && (4.0..=400.0).contains(&w.size)) {
            return Err(invalid(
                "la taille doit être comprise entre 4 et 400 points",
            ));
        }
        if !(w.opacity > 0.0 && w.opacity <= 1.0) {
            return Err(invalid("l'opacité doit être comprise entre 0 (exclu) et 1"));
        }
        if !w.angle_deg.is_finite() {
            return Err(invalid("angle invalide"));
        }
        let pages = self.target_pages(&w.pages)?;
        let embed = conformance.is_some();
        let width = text_width_in(&w.text, w.size, embed);
        let (sin, cos) = w.angle_deg.to_radians().sin_cos();
        let mut placements = Vec::with_capacity(pages.len());
        for page in pages {
            let frame = self.page_frame(page)?;
            let (pw, ph) = frame.displayed_size();
            // Le texte est centré sur la page : son origine (début de ligne de base) est
            // décalée de (−largeur/2, −0,35 × taille) dans son propre repère, puis tournée.
            let (ox, oy) = (-width / 2.0, -0.35 * w.size);
            let tx = pw / 2.0 + ox * cos - oy * sin;
            let ty = ph / 2.0 + ox * sin + oy * cos;
            placements.push(Placement {
                page,
                text: w.text.clone(),
                size: w.size,
                color: w.color,
                opacity: w.opacity,
                unicode: embed || fonts::needs_unicode(&w.text),
                tm: [cos, sin, -sin, cos, tx, ty],
                frame,
            });
        }
        // Après les gardes : un refus laisse le document intact (et ses octets réécrits tels
        // quels à l'enregistrement).
        self.mark_changed();
        self.apply_placements(placements, conformance, ARTIFACT)
    }

    /// Numérote les pages visées d'après le modèle.
    pub fn add_page_numbers(&mut self, n: &PageNumbering) -> CoreResult<()> {
        // Lue avant `mark_changed`, qui vide la vue `lopdf` gardée.
        let conformance = self.conformance();
        if n.template.trim().is_empty() {
            return Err(invalid("le modèle ne peut pas être vide"));
        }
        if !(n.size.is_finite() && (4.0..=200.0).contains(&n.size)) {
            return Err(invalid(
                "la taille doit être comprise entre 4 et 200 points",
            ));
        }
        if !(n.margin.is_finite() && n.margin >= 0.0) {
            return Err(invalid("la marge doit être positive ou nulle"));
        }
        let pages = self.target_pages(&n.pages)?;
        let embed = conformance.is_some();
        let total = self.page_count();
        let title = self.title().unwrap_or_default();
        let mut placements = Vec::with_capacity(pages.len());
        for page in pages {
            let number = u32::from(n.start) + u32::from(page);
            let text = n
                .template
                .replace("{n}", &number.to_string())
                .replace("{total}", &total.to_string())
                .replace("{titre}", &title)
                .replace("{title}", &title)
                .trim()
                .to_string();
            check_text(&text)?;
            let width = text_width_in(&text, n.size, embed);
            let frame = self.page_frame(page)?;
            let (pw, ph) = frame.displayed_size();
            let x = match n.corner {
                Corner::TopLeft | Corner::BottomLeft => n.margin,
                Corner::TopCenter | Corner::BottomCenter => (pw - width) / 2.0,
                Corner::TopRight | Corner::BottomRight => pw - n.margin - width,
            };
            // En tête, c'est le haut des majuscules (≈ 0,75 × taille) qui respecte la marge.
            let y = match n.corner {
                Corner::TopLeft | Corner::TopCenter | Corner::TopRight => {
                    ph - n.margin - 0.75 * n.size
                }
                _ => n.margin,
            };
            if x < -1.0 || y < -1.0 || x > pw + 1.0 || y > ph + 1.0 {
                return Err(invalid("la marge sort de la page"));
            }
            placements.push(Placement {
                page,
                unicode: embed || fonts::needs_unicode(&text),
                text,
                size: n.size,
                color: Color { r: 0, g: 0, b: 0 },
                opacity: 1.0,
                tm: [1.0, 0.0, 0.0, 1.0, x, y],
                frame,
            });
        }
        // Après les gardes, celles de la boucle comprises (texte de chaque numéro, marge).
        self.mark_changed();
        self.apply_placements(placements, conformance, PAGINATION_ARTIFACT)
    }

    /// Recadre les pages visées (`/CropBox`) par marges depuis la MediaBox, en points
    /// affichés ; quatre marges nulles retirent le recadrage.
    pub fn crop_pages(&mut self, c: &CropSpec) -> CoreResult<()> {
        self.mark_changed();
        if c.margins.iter().any(|m| !m.is_finite() || *m < 0.0) {
            return Err(invalid("les marges doivent être positives ou nulles"));
        }
        let pages = self.target_pages(&c.pages)?;
        // PDFium ne lit que la `/MediaBox` propre à la page : l'héritée vient de `lopdf`,
        // lue avant la phase PDFium (le verrou n'est pas réentrant).
        let inherited: BTreeMap<u16, Rect> = {
            let doc = crate::crypt::load_limited(&self.to_bytes()?, None).map_err(structure)?;
            let ids = doc.get_pages();
            pages
                .iter()
                .filter_map(|p| {
                    let id = ids.get(&(u32::from(*p) + 1))?;
                    Some((*p, inherited_media_box(&doc, *id)?))
                })
                .collect()
        };
        {
            let _guard = engine::guard();
            for page in pages {
                let index = self.checked_index_unlocked(page)?;
                let mut pdf_page = self.inner().pages().get(index)?;
                let rotate = pdf_page
                    .rotation()
                    .map(Rotation::from_pdfium)
                    .unwrap_or_default()
                    .degrees();
                let media = inherited
                    .get(&page)
                    .copied()
                    .or_else(|| {
                        pdf_page
                            .boundaries()
                            .media()
                            .ok()
                            .map(|b| Rect::from_pdf(b.bounds))
                    })
                    .unwrap_or_else(|| {
                        let (w, h) = (pdf_page.width().value, pdf_page.height().value);
                        let (w, h) = if rotate % 180 == 90 { (h, w) } else { (w, h) };
                        Rect {
                            x: 0.0,
                            y: 0.0,
                            w,
                            h,
                        }
                    });
                let frame = PageFrame {
                    rotate,
                    bbox: media,
                };
                let (pw, ph) = frame.displayed_size();
                let [left, bottom, right, top] = c.margins;
                let (w, h) = (pw - left - right, ph - bottom - top);
                if w < 1.0 || h < 1.0 {
                    return Err(invalid("les marges ne laissent plus de page"));
                }
                let displayed = Rect {
                    x: left,
                    y: bottom,
                    w,
                    h,
                };
                let crop = frame.to_user_space(displayed);
                pdf_page.boundaries_mut().set_crop(to_pdf_rect(&crop))?;
            }
        }
        // PDFium garde en cache les dimensions d'une page chargée : on recharge.
        let bytes = self.to_bytes()?;
        self.reload_from_bytes(bytes)
    }

    /// Ajoute à la page une couche de texte **invisible** (mode de rendu `3 Tr`) : chaque mot
    /// est placé dans sa boîte et étiré horizontalement (`Tz`) à sa largeur. Le rendu ne
    /// change pas ; l'extraction, la recherche et la copie trouvent les mots (OCR).
    ///
    /// La couche est marquée comme **artefact** (`/Artifact BMC`), sans quoi un PDF balisé
    /// deviendrait invalide (PDF/UA-1 §7.1 : tout contenu est balisé ou artefact). Conséquence :
    /// dans un PDF balisé, un lecteur d'écran **ignore** ce texte ; il sert à la recherche et à
    /// la copie, pas à la lecture vocale. Rendre un scan accessible demanderait de baliser le
    /// texte reconnu dans l'arbre de structure, ce que cette version ne fait pas.
    pub fn add_invisible_words(&mut self, page: u16, words: &[InvisibleWord]) -> CoreResult<()> {
        let count = self.page_count();
        if page >= count {
            return Err(CoreError::PageOutOfRange { index: page, count });
        }
        // Après la garde : un refus laisse le document intact (et ses octets réécrits tels
        // quels à l'enregistrement).
        self.mark_changed();
        let frame = self.page_frame(page)?;
        // Largeur d'**avance** à 100 pt (table AFM, proportionnelle à la taille) : c'est elle
        // que `Tz` étire, et que les lecteurs utilisent pour placer la sélection.
        let bytes = self.to_bytes()?;
        // Un document chiffré est réécrit en clair puis rechiffré à l'identique (Lot 10).
        let mut session = LopdfSession::load(&bytes, self.password())?;
        let page_id = *session
            .doc
            .get_pages()
            .get(&(u32::from(page) + 1))
            .ok_or_else(|| structure("page introuvable"))?;
        let embed = crate::properties::conformance_of(&session.doc).is_some();
        let unicode_key =
            free_resource_name(&session.doc, &[page_id], b"Font", UNICODE_FONT_PREFIX);
        // Les mots hors WinAnsi passent par la police Unicode ; un caractère qu'elle n'a pas
        // (CJK) est permis ici : le texte est invisible, seule compte sa valeur `/ToUnicode`.
        let mut writer = Type0Writer::new();
        let mut ops = String::new();
        for w in words {
            // Une espace finale est écrite (elle sépare les mots d'une ligne pour l'extraction
            // et la recherche) mais n'entre pas dans la largeur à remplir.
            let text = w.text.trim_start();
            let word = text.trim_end();
            let r = w.rect;
            // Une boîte vide, négative ou infinie (OCR fautif) est ignorée : elle écrirait
            // « inf » ou « NaN » dans le flux de contenu.
            let finite = [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite());
            if word.is_empty() || !finite || !(r.w > 0.0 && r.h > 0.0) {
                continue;
            }
            let width_100 = text_width_in(word, 100.0, embed);
            if width_100 <= 0.0 {
                continue;
            }
            let size = r.h;
            let natural = width_100 * size / 100.0;
            let scale = 100.0 * r.w / natural;
            let (key, hex) = if embed || fonts::needs_unicode(text) {
                (unicode_key.as_str(), writer.encode(text))
            } else {
                let hex = hex_win_ansi(text).ok_or_else(|| invalid("texte non encodable"))?;
                (FONT_KEY, hex)
            };
            ops.push_str(&format!(
                "/{key} {} Tf\n{} Tz\n1 0 0 1 {} {} Tm\n{hex} Tj\n",
                num(size),
                num(scale),
                num(r.x),
                num(r.y + DESCENT_RATIO * r.h),
            ));
        }
        if ops.is_empty() {
            return Ok(());
        }
        let doc = &mut session.doc;
        let font_id = helvetica_font(doc);
        let mut entries = vec![("Font", FONT_KEY, font_id)];
        if !writer.is_empty() {
            entries.push(("Font", unicode_key.as_str(), writer.write(doc)?));
        }
        ensure_resources(doc, page_id, &entries)?;
        let m = frame.matrix_to_user_space();
        let cm = if frame.is_identity() {
            String::new()
        } else {
            format!(
                "{} {} {} {} {} {} cm\n",
                num(m[0]),
                num(m[1]),
                num(m[2]),
                num(m[3]),
                num(m[4]),
                num(m[5])
            )
        };
        let content = format!("\nq\n{cm}BT\n3 Tr\n{ops}ET\nQ\n");
        let mut wrapped = BTreeSet::new();
        append_content(
            doc,
            page_id,
            &mut wrapped,
            Some(ARTIFACT),
            content.into_bytes(),
            None,
        )?;
        let out = session.finish()?;
        self.reload_from_bytes(out)
    }

    /// Écrit les textes dans le fichier (phase `lopdf`), puis recharge le document.
    ///
    /// `conformance` (lue avant toute modification) décide de la façon d'écrire la couleur :
    /// - document PDF/A (toute partie) : couleur dans un espace **indépendant du périphérique**
    ///   (`CalRGB`, ressource `/ColorSpace` de la page, opérateurs `cs` / `sc`), car DeviceRGB
    ///   (`rg`) n'y est permis qu'avec un profil de sortie RVB (PDF/A-2 6.2.4.3) ; le groupe de
    ///   transparence éventuel fusionne dans ce même espace ;
    /// - PDF/A-1 : aucune transparence (6.4) ; l'opacité est **simulée** en éclaircissant la
    ///   couleur vers le blanc (`c' = c·α + (1 − α)`), ce qui donne le même rendu sur une
    ///   page blanche (le texte d'origine qu'il recouvre, lui, ne transparaît plus) ;
    /// - sinon : `rg` (DeviceRGB), comme avant.
    ///
    /// `artifact` ouvre le contenu marqué qui enveloppe le texte ajouté ([`ARTIFACT`] ou
    /// [`PAGINATION_ARTIFACT`]).
    fn apply_placements(
        &mut self,
        placements: Vec<Placement>,
        conformance: Option<Conformance>,
        artifact: &[u8],
    ) -> CoreResult<()> {
        let pdfa_part = conformance.and_then(|c| c.pdfa_part);
        let bytes = self.to_bytes()?;
        // Un document chiffré est réécrit en clair puis rechiffré à l'identique (Lot 10).
        let mut session = LopdfSession::load(&bytes, self.password())?;
        let doc = &mut session.doc;
        let page_ids: BTreeMap<u32, ObjectId> = doc.get_pages();
        // Une police de chaque sorte au plus, partagée par toutes les pages : Helvetica pour
        // les textes WinAnsi, la police Unicode (sous-ensemble des caractères de **tous** les
        // textes) pour les autres. Les chaînes sont donc encodées avant d'écrire la police.
        let helvetica = placements
            .iter()
            .any(|p| !p.unicode)
            .then(|| helvetica_font(doc));
        let mut writer = Type0Writer::new();
        let mut encoded = Vec::with_capacity(placements.len());
        for p in &placements {
            encoded.push(if p.unicode {
                writer.encode(&p.text)
            } else {
                hex_win_ansi(&p.text).ok_or_else(|| invalid("texte non encodable"))?
            });
        }
        let unicode = if writer.is_empty() {
            None
        } else {
            Some(writer.write(doc)?)
        };
        let targets: Vec<ObjectId> = placements
            .iter()
            .filter_map(|p| page_ids.get(&(u32::from(p.page) + 1)).copied())
            .collect();
        let unicode_key = free_resource_name(doc, &targets, b"Font", UNICODE_FONT_PREFIX);
        // PDF/A : un espace `CalRGB` partagé par toutes les pages, sous un nom libre partout.
        let calibrated = pdfa_part.map(|_| {
            let name = free_resource_name(doc, &targets, b"ColorSpace", COLOR_SPACE_PREFIX);
            let space = calibrated_rgb();
            let id = doc.add_object(space.clone());
            (name, id, space)
        });
        let mut states: BTreeMap<u32, (String, ObjectId)> = BTreeMap::new();
        let mut wrapped: BTreeSet<ObjectId> = BTreeSet::new();
        for (mut p, hex) in placements.into_iter().zip(encoded) {
            if pdfa_part == Some(1) {
                p.color = over_white(p.color, p.opacity);
                p.opacity = 1.0;
            }
            let (font_key, font_id) = if p.unicode {
                (unicode_key.as_str(), unicode)
            } else {
                (FONT_KEY, helvetica)
            };
            let font_id = font_id.ok_or_else(|| structure("police manquante"))?;
            let page_id = *page_ids
                .get(&(u32::from(p.page) + 1))
                .ok_or_else(|| structure("page introuvable"))?;
            // Un état graphique par opacité distincte (clé au millième).
            let key = (p.opacity * 1000.0).round() as u32;
            let (gs_name, gs_id) = states
                .entry(key)
                .or_insert_with(|| {
                    let id = doc.add_object(Object::Dictionary(Dictionary::from_iter(vec![
                        ("Type", Object::Name(b"ExtGState".to_vec())),
                        ("CA", Object::Real(p.opacity)),
                        ("ca", Object::Real(p.opacity)),
                    ])));
                    (format!("{GS_PREFIX}{key}"), id)
                })
                .clone();
            let mut entries = vec![("Font", font_key, font_id), ("ExtGState", &gs_name, gs_id)];
            if let Some((name, id, _)) = &calibrated {
                entries.push(("ColorSpace", name.as_str(), *id));
            }
            ensure_resources(doc, page_id, &entries)?;
            let m = p.frame.matrix_to_user_space();
            let cm = if p.frame.is_identity() {
                String::new()
            } else {
                format!(
                    "{} {} {} {} {} {} cm\n",
                    num(m[0]),
                    num(m[1]),
                    num(m[2]),
                    num(m[3]),
                    num(m[4]),
                    num(m[5])
                )
            };
            let components = format!(
                "{} {} {}",
                num(f32::from(p.color.r) / 255.0),
                num(f32::from(p.color.g) / 255.0),
                num(f32::from(p.color.b) / 255.0),
            );
            let fill = match &calibrated {
                Some((name, _, _)) => format!("/{name} cs\n{components} sc"),
                None => format!("{components} rg"),
            };
            let content = format!(
                "\nq\n{cm}/{gs_name} gs\n{fill}\nBT\n/{font_key} {} Tf\n{} {} {} {} {} {} Tm\n{hex} Tj\nET\nQ\n",
                num(p.size),
                num(p.tm[0]),
                num(p.tm[1]),
                num(p.tm[2]),
                num(p.tm[3]),
                num(p.tm[4]),
                num(p.tm[5]),
            );
            // Un groupe de transparence pour un contenu translucide (jamais en PDF/A-1, où
            // l'opacité vaut désormais 1), dans l'espace de la couleur écrite.
            let group_space = (p.opacity < 1.0).then(|| match &calibrated {
                Some((_, _, space)) => space.clone(),
                None => Object::Name(b"DeviceRGB".to_vec()),
            });
            append_content(
                doc,
                page_id,
                &mut wrapped,
                Some(artifact),
                content.into_bytes(),
                group_space,
            )?;
        }
        let out = session.finish()?;
        self.reload_from_bytes(out)
    }
}

/// Largeur d'avance d'un texte à `size` points, dans la police qui l'écrira : Helvetica
/// (table AFM : la même pour tous les lecteurs, sans passer par PDFium) ou, hors WinAnsi, la
/// police Unicode incorporée (ses largeurs sont aussi celles écrites dans `/W`).
pub(crate) fn text_width(text: &str, size: f32) -> f32 {
    text_width_in(text, size, false)
}

/// Comme `text_width`, mais `embed` impose la police Unicode même pour un texte WinAnsi
/// (document PDF/A ou PDF/UA : Helvetica non incorporée y est refusée).
fn text_width_in(text: &str, size: f32, embed: bool) -> f32 {
    if embed || fonts::needs_unicode(text) {
        return UnicodeFont::embedded().text_width(text, size);
    }
    let bytes: Vec<u8> = text
        .chars()
        .map(|c| win_ansi_byte(c).unwrap_or(b'?'))
        .collect();
    fonts::text_width(&bytes, size)
}

/// L'espace de couleur `[/CalRGB << /WhitePoint … /Gamma … /Matrix … >>]` qui reproduit sRGB
/// (ISO 32000-1 §8.6.5.3) : indépendant du périphérique, donc permis en PDF/A sans profil de
/// sortie, et rendu comme `rg` par un lecteur qui ne convertit pas les couleurs.
fn calibrated_rgb() -> Object {
    let reals = |values: &[f32]| Object::Array(values.iter().map(|v| Object::Real(*v)).collect());
    Object::Array(vec![
        Object::Name(b"CalRGB".to_vec()),
        Object::Dictionary(Dictionary::from_iter(vec![
            ("WhitePoint", reals(&D65_WHITE_POINT)),
            ("Gamma", reals(&[SRGB_GAMMA; 3])),
            ("Matrix", reals(&SRGB_MATRIX)),
        ])),
    ])
}

/// La couleur `color` posée à l'opacité `alpha` sur du blanc, devenue opaque :
/// `c' = c·α + (1 − α)` pour chaque composante (sur 0-1 ; ici sur 0-255). Sert à PDF/A-1,
/// qui interdit la transparence (6.4) : sur une page blanche, le rendu est le même.
fn over_white(color: Color, alpha: f32) -> Color {
    let blend = |c: u8| -> u8 {
        let value = f32::from(c) * alpha + 255.0 * (1.0 - alpha);
        // [Apprentissage] `as u8` sature (0 en dessous, 255 au-dessus) depuis Rust 1.45 : le
        // `clamp` dit l'intention, la conversion ne peut pas déborder.
        value.round().clamp(0.0, 255.0) as u8
    };
    Color {
        r: blend(color.r),
        g: blend(color.g),
        b: blend(color.b),
    }
}

/// Un nom `<prefix><n>` absent du sous-dictionnaire `category` (`/Font`, `/ColorSpace`…) des
/// ressources (propres ou héritées) de toutes les pages `pages` : réutiliser un nom pris
/// remplacerait la ressource d'un autre contenu de la page.
pub(crate) fn free_resource_name(
    doc: &LoDocument,
    pages: &[ObjectId],
    category: &[u8],
    prefix: &str,
) -> String {
    let taken: BTreeSet<Vec<u8>> = pages
        .iter()
        .filter_map(|id| doc.get_dictionary(*id).ok())
        .flat_map(|page| {
            let resources = match page.get(b"Resources") {
                Ok(obj) => doc
                    .dereference(obj)
                    .ok()
                    .and_then(|(_, o)| o.as_dict().ok().cloned())
                    .unwrap_or_default(),
                Err(_) => inherited_resources(doc, page),
            };
            resources
                .get(category)
                .ok()
                .and_then(|f| doc.dereference(f).ok())
                .and_then(|(_, f)| f.as_dict().ok().cloned())
                .unwrap_or_default()
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>()
        })
        .collect();
    (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|name| !taken.contains(name.as_bytes()))
        .unwrap_or_else(|| format!("{prefix}0"))
}

/// Ajoute à `doc` une police Helvetica (Standard 14, encodage WinAnsi) avec ses largeurs
/// (`/FirstChar`, `/LastChar`, `/Widths` : B24, Lot 9) et rend sa référence.
pub(crate) fn helvetica_font(doc: &mut LoDocument) -> ObjectId {
    let mut font = Dictionary::from_iter(vec![
        ("Type", Object::Name(b"Font".to_vec())),
        ("Subtype", Object::Name(b"Type1".to_vec())),
        ("BaseFont", Object::Name(b"Helvetica".to_vec())),
        ("Encoding", Object::Name(b"WinAnsiEncoding".to_vec())),
    ]);
    for (key, value) in fonts::widths_entries() {
        font.set(key, value);
    }
    doc.add_object(Object::Dictionary(font))
}

/// `/MediaBox` de la page `page_id`, héritée d'un ancêtre `/Pages` si besoin (§7.7.3.4).
fn inherited_media_box(doc: &LoDocument, page_id: ObjectId) -> Option<Rect> {
    let mut node = doc.get_dictionary(page_id).ok()?;
    for _ in 0..32 {
        if let Ok(value) = node.get(b"MediaBox")
            && let Ok((_, Object::Array(items))) = doc.dereference(value)
        {
            let n: Vec<f32> = items.iter().filter_map(|o| o.as_float().ok()).collect();
            let [x0, y0, x1, y1] = n[..] else {
                return None;
            };
            return Some(Rect {
                x: x0.min(x1),
                y: y0.min(y1),
                w: (x1 - x0).abs(),
                h: (y1 - y0).abs(),
            });
        }
        let parent = node.get(b"Parent").and_then(Object::as_reference).ok()?;
        node = doc.get_dictionary(parent).ok()?;
    }
    None
}

/// Ressources héritées d'un ancêtre `/Pages` (copie), ou un dictionnaire vide.
pub(crate) fn inherited_resources(doc: &LoDocument, page: &Dictionary) -> Dictionary {
    let mut node = page.clone();
    for _ in 0..32 {
        if let Ok(res) = node.get(b"Resources")
            && let Ok((_, Object::Dictionary(d))) = doc.dereference(res)
        {
            return d.clone();
        }
        let Ok(parent) = node.get(b"Parent").and_then(Object::as_reference) else {
            break;
        };
        let Ok(parent) = doc.get_dictionary(parent) else {
            break;
        };
        node = parent.clone();
    }
    Dictionary::new()
}

/// Donne à la page un dictionnaire `/Resources` **propre** (copie de l'hérité si besoin) qui
/// référence nos objets (`entries` : sous-dictionnaire, nom, objet ; par exemple la police
/// sous `/Font`). Les sous-dictionnaires partagés par référence sont copiés : on n'altère pas
/// ceux des autres pages.
pub(crate) fn ensure_resources(
    doc: &mut LoDocument,
    page_id: ObjectId,
    entries: &[(&str, &str, ObjectId)],
) -> CoreResult<()> {
    let mut resources = {
        let page = doc.get_dictionary(page_id).map_err(structure)?;
        match page.get(b"Resources") {
            Ok(obj) => doc
                .dereference(obj)
                .ok()
                .and_then(|(_, o)| o.as_dict().ok().cloned())
                .unwrap_or_default(),
            Err(_) => inherited_resources(doc, page),
        }
    };
    for &(key, name, id) in entries {
        let mut sub = resources
            .get(key.as_bytes())
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_dict().ok().cloned())
            .unwrap_or_default();
        sub.set(name, Object::Reference(id));
        resources.set(key, Object::Dictionary(sub));
    }
    doc.get_dictionary_mut(page_id)
        .map_err(structure)?
        .set("Resources", Object::Dictionary(resources));
    Ok(())
}

/// Ajoute `content` après le contenu existant de la page, que l'on enferme une fois dans
/// `q … Q` pour que son état graphique (transformation en cours, couleur) ne déteigne pas.
///
/// Le contenu décoratif (filigrane, numéros, couche OCR) n'est pas du contenu réel : il est
/// enveloppé dans un artefact, `artifact` (`Some(/Artifact BMC)` ou
/// `Some(/Artifact <<…>> BDC)`) … `EMC` (ISO 32000-1 §14.8.2.2), sans quoi un PDF balisé
/// (PDF/UA-1 §7.1) serait invalide. `None` pour du contenu réel (image insérée, Lot 14),
/// écrit sans marquage, comme le faisait PDFium.
///
/// `group_space` (`Some` quand ce contenu pose un état graphique à opacité < 1) : la page
/// reçoit un groupe de transparence (§11.6.6) de cet espace de fusion si elle n'en a pas, ce
/// qu'exige PDF/A-2 (6.2.10) sans profil de sortie. `/I` (groupe isolé) n'est pas posé : sa
/// valeur par défaut (faux) laisse le filigrane se composer directement sur le contenu de la
/// page qu'il recouvre, ce que l'on veut.
pub(crate) fn append_content(
    doc: &mut LoDocument,
    page_id: ObjectId,
    wrapped: &mut BTreeSet<ObjectId>,
    artifact: Option<&[u8]>,
    content: Vec<u8>,
    group_space: Option<Object>,
) -> CoreResult<()> {
    let content = match artifact {
        Some(artifact) => {
            let mut marked = artifact.to_vec();
            marked.extend_from_slice(&content);
            marked.extend_from_slice(b"\nEMC\n");
            marked
        }
        None => content,
    };
    let existing = doc.get_page_contents(page_id);
    let mut list: Vec<Object> = Vec::with_capacity(existing.len() + 3);
    let stream = |doc: &mut LoDocument, bytes: &[u8]| {
        doc.add_object(Object::Stream(Stream::new(
            Dictionary::new(),
            bytes.to_vec(),
        )))
    };
    // Une enveloppe posée par un appel précédent (premier flux `q`, un flux `Q` plus loin) est
    // réutilisée : emboîter un `q … Q` par superposition finirait par dépasser la profondeur
    // que certains lecteurs tolèrent (Acrobat : 28 niveaux).
    // PDFium peut avoir recomprimé nos petits flux au passage : on compare le contenu décodé.
    let raw = |doc: &LoDocument, id: &ObjectId| -> Vec<u8> {
        doc.get_object(*id)
            .ok()
            .and_then(|o| o.as_stream().ok())
            .and_then(crate::crypt::stream_bytes)
            .map(|c| c.trim_ascii().to_vec())
            .unwrap_or_default()
    };
    let already_wrapped = existing.first().is_some_and(|id| raw(doc, id) == b"q")
        && existing.iter().skip(1).any(|id| raw(doc, id) == b"Q");
    if already_wrapped {
        wrapped.insert(page_id);
    }
    if wrapped.insert(page_id) && !existing.is_empty() {
        list.push(Object::Reference(stream(doc, b"q\n")));
        list.extend(existing.iter().map(|id| Object::Reference(*id)));
        list.push(Object::Reference(stream(doc, b"\nQ\n")));
    } else {
        list.extend(existing.iter().map(|id| Object::Reference(*id)));
    }
    list.push(Object::Reference(stream(doc, &content)));
    let page = doc.get_dictionary_mut(page_id).map_err(structure)?;
    page.set("Contents", Object::Array(list));
    if let Some(space) = group_space
        && !page.has(b"Group")
    {
        page.set(
            "Group",
            Object::Dictionary(Dictionary::from_iter(vec![
                ("S", Object::Name(b"Transparency".to_vec())),
                ("CS", space),
            ])),
        );
    }
    Ok(())
}
