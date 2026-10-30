// SPDX-License-Identifier: GPL-3.0-or-later
//! Ordre de lecture de la couche texte (correctif 0.12.2, ADR-0016).
//!
//! PDFium rend les caractères d'une page dans l'ordre où le fichier les **dessine**, qui n'est
//! pas toujours l'ordre de lecture : un mot en gras dessiné à la fin, une colonne de valeurs
//! dessinée après celle des libellés, des lignes dans le désordre… La sélection (une plage
//! d'index) et la copie ramassaient alors des morceaux de toute la page. Ce module recalcule
//! l'ordre à partir de la **géométrie** seule : il ne connaît que des rectangles, ce qui le rend
//! testable sans PDF.

use crate::geometry::Rect;

/// Un bloc de texte lu d'un seul tenant : ses lignes, du haut vers le bas, chacune donnée par
/// les index de ses caractères, de gauche à droite.
pub type Block = Vec<Vec<usize>>;

/// Deux caractères sont sur la même ligne si leurs étendues verticales se recouvrent d'au
/// moins cette part de la plus petite hauteur (un exposant reste sur sa ligne).
const SAME_LINE_OVERLAP: f32 = 0.5;
/// Dans une ligne, un blanc plus large que ce multiple de la hauteur médiane `H` sépare deux
/// segments (une tabulation entre un libellé et sa valeur, ou deux colonnes).
const SEGMENT_GAP: f32 = 1.5;
/// Une bande verticale vide d'au moins ce multiple de `H` peut séparer deux colonnes…
const COLUMN_GAP: f32 = 1.0;
/// … à condition que chaque côté compte au moins ce nombre de lignes : une petite liste
/// « libellé : valeur » se lit ainsi ligne par ligne, et non colonne par colonne.
const COLUMN_MIN_LINES: usize = 4;
/// Un vide vertical d'au moins ce multiple de `H` sépare deux paragraphes.
const PARAGRAPH_GAP: f32 = 0.8;

/// Un morceau de ligne d'un seul tenant : sa ligne, ses caractères (de gauche à droite) et
/// son emprise.
struct Segment {
    line: usize,
    chars: Vec<usize>,
    bounds: Rect,
}

/// Les blocs de la page dans l'ordre de lecture. Les rectangles de largeur ou de hauteur
/// nulle (sauts de ligne ajoutés par PDFium) sont ignorés.
pub fn layout(rects: &[Rect]) -> Vec<Block> {
    let visible: Vec<usize> = (0..rects.len())
        .filter(|&i| rects[i].w > 0.0 && rects[i].h > 0.0)
        .collect();
    if visible.is_empty() {
        return Vec::new();
    }
    let h = median_height(rects, &visible);
    let lines = group_lines(rects, visible, h);
    let segments = split_segments(rects, &lines, h);
    let mut blocks = Vec::new();
    xy_cut(&segments, (0..segments.len()).collect(), h, &mut blocks);
    blocks
}

/// Hauteur médiane des caractères : l'unité de mesure des seuils (≈ la taille du texte).
fn median_height(rects: &[Rect], visible: &[usize]) -> f32 {
    let mut heights: Vec<f32> = visible.iter().map(|&i| rects[i].h).collect();
    heights.sort_by(f32::total_cmp);
    heights[heights.len() / 2]
}

/// Centre vertical d'un rectangle (l'axe y monte : plus grand = plus haut sur la page).
fn center_y(r: &Rect) -> f32 {
    r.y + r.h / 2.0
}

/// Regroupe les caractères en lignes, de la plus haute à la plus basse ; dans chaque ligne,
/// les caractères sont rangés de gauche à droite par `order_line`.
///
/// \[Apprentissage\] **Trier, puis balayer** : une fois les caractères triés par hauteur, une
/// ligne est une suite de voisins qui se recouvrent ; un seul passage suffit à les regrouper.
/// `sort_by(|a, b| …total_cmp…)` trie des `f32` (qui n'ont pas d'ordre total à cause de `NaN`).
fn group_lines(rects: &[Rect], mut visible: Vec<usize>, h: f32) -> Vec<Vec<usize>> {
    visible.sort_by(|&a, &b| center_y(&rects[b]).total_cmp(&center_y(&rects[a])));
    // Chaque ligne : ses caractères et sa bande verticale (bas, haut).
    let mut lines: Vec<(Vec<usize>, f32, f32)> = Vec::new();
    for i in visible {
        let r = &rects[i];
        let joins = lines.last().is_some_and(|(_, bottom, top)| {
            let overlap = top.min(r.y + r.h) - bottom.max(r.y);
            overlap >= SAME_LINE_OVERLAP * (top - bottom).min(r.h)
        });
        match lines.last_mut() {
            Some((chars, bottom, top)) if joins => {
                chars.push(i);
                *bottom = bottom.min(r.y);
                *top = top.max(r.y + r.h);
            }
            _ => lines.push((vec![i], r.y, r.y + r.h)),
        }
    }
    lines
        .into_iter()
        .map(|(chars, _, _)| order_line(rects, chars, h))
        .collect()
}

/// Range les caractères d'une ligne de gauche à droite **par suites** : une suite est une série
/// de caractères dessinés d'affilée, chacun à droite du précédent et sans grand saut. Les suites
/// sont ensuite rangées par leur premier caractère. Trier caractère par caractère entremêlerait
/// les lettres de deux mots dessinés au même endroit (filigranes superposés, faux gras) ; un
/// fragment dessiné plus tard (un mot en gras) forme sa propre suite et retrouve sa place.
fn order_line(rects: &[Rect], mut chars: Vec<usize>, h: f32) -> Vec<usize> {
    // L'index d'un caractère est son rang dans l'ordre de dessin.
    chars.sort_unstable();
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for c in chars {
        let continues = runs.last().and_then(|run| run.last()).is_some_and(|&prev| {
            let (p, r) = (&rects[prev], &rects[c]);
            r.x >= p.x && r.x <= p.x + p.w + 0.5 * h
        });
        match runs.last_mut() {
            Some(run) if continues => run.push(c),
            _ => runs.push(vec![c]),
        }
    }
    // `sort_by` est stable : deux suites qui commencent au même x gardent l'ordre de dessin.
    runs.sort_by(|a, b| rects[a[0]].x.total_cmp(&rects[b[0]].x));
    runs.concat()
}

/// Coupe chaque ligne aux grands blancs horizontaux.
fn split_segments(rects: &[Rect], lines: &[Vec<usize>], h: f32) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (line, chars) in lines.iter().enumerate() {
        let mut current: Vec<usize> = Vec::new();
        for &c in chars {
            if let Some(&prev) = current.last() {
                let gap = rects[c].x - (rects[prev].x + rects[prev].w);
                if gap > SEGMENT_GAP * h {
                    segments.push(segment(rects, line, std::mem::take(&mut current)));
                }
            }
            current.push(c);
        }
        if !current.is_empty() {
            segments.push(segment(rects, line, current));
        }
    }
    segments
}

/// Un segment et son emprise (union des rectangles de ses caractères).
fn segment(rects: &[Rect], line: usize, chars: Vec<usize>) -> Segment {
    let (mut left, mut bottom) = (f32::MAX, f32::MAX);
    let (mut right, mut top) = (f32::MIN, f32::MIN);
    for &c in &chars {
        let r = &rects[c];
        left = left.min(r.x);
        bottom = bottom.min(r.y);
        right = right.max(r.x + r.w);
        top = top.max(r.y + r.h);
    }
    Segment {
        line,
        chars,
        bounds: Rect {
            x: left,
            y: bottom,
            w: right - left,
            h: top - bottom,
        },
    }
}

/// Découpage « XY » récursif : coupe en colonnes s'il le faut, sinon en paragraphes, et range
/// chaque morceau final comme un bloc.
///
/// \[Apprentissage\] **Récursion** : la fonction s'appelle elle-même sur chaque moitié ; elle
/// s'arrête quand plus aucune coupe n'est possible (une « feuille »). Chaque appel reçoit un
/// ensemble strictement plus petit, donc la récursion se termine.
fn xy_cut(segments: &[Segment], ids: Vec<usize>, h: f32, out: &mut Vec<Block>) {
    if let Some((left, right)) = column_cut(segments, &ids, h) {
        xy_cut(segments, left, h, out);
        xy_cut(segments, right, h, out);
        return;
    }
    let bands = paragraph_bands(segments, ids, h);
    if bands.len() > 1 {
        for band in bands {
            xy_cut(segments, band, h, out);
        }
        return;
    }
    if let Some(band) = bands.into_iter().next() {
        out.push(leaf(segments, &band));
    }
}

/// Une coupe en colonnes : la plus large bande verticale vide (d'au moins `COLUMN_GAP × H`)
/// dont chaque côté compte au moins `COLUMN_MIN_LINES` lignes.
fn column_cut(segments: &[Segment], ids: &[usize], h: f32) -> Option<(Vec<usize>, Vec<usize>)> {
    // Projection horizontale : les intervalles couverts, fusionnés.
    let mut spans: Vec<(f32, f32)> = ids
        .iter()
        .map(|&s| {
            (
                segments[s].bounds.x,
                segments[s].bounds.x + segments[s].bounds.w,
            )
        })
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut covered: Vec<(f32, f32)> = Vec::new();
    for (start, end) in spans {
        match covered.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => covered.push((start, end)),
        }
    }
    // Les trous entre intervalles, du plus large au plus étroit.
    let mut gaps: Vec<(f32, f32)> = covered
        .windows(2)
        .map(|w| (w[0].1, w[1].0))
        .filter(|(a, b)| b - a >= COLUMN_GAP * h)
        .collect();
    gaps.sort_by(|a, b| (b.1 - b.0).total_cmp(&(a.1 - a.0)));
    for (gap_start, _) in gaps {
        let (left, right): (Vec<usize>, Vec<usize>) = ids
            .iter()
            .partition(|&&s| segments[s].bounds.x + segments[s].bounds.w <= gap_start);
        if distinct_lines(segments, &left) >= COLUMN_MIN_LINES
            && distinct_lines(segments, &right) >= COLUMN_MIN_LINES
        {
            return Some((left, right));
        }
    }
    None
}

/// Nombre de lignes différentes touchées par ces segments.
fn distinct_lines(segments: &[Segment], ids: &[usize]) -> usize {
    let mut lines: Vec<usize> = ids.iter().map(|&s| segments[s].line).collect();
    lines.sort_unstable();
    lines.dedup();
    lines.len()
}

/// Coupe aux grands vides verticaux (`PARAGRAPH_GAP × H`) : des bandes, de haut en bas.
fn paragraph_bands(segments: &[Segment], mut ids: Vec<usize>, h: f32) -> Vec<Vec<usize>> {
    let top = |s: usize| segments[s].bounds.y + segments[s].bounds.h;
    ids.sort_by(|&a, &b| top(b).total_cmp(&top(a)));
    // Chaque bande : ses segments et son bord bas.
    let mut bands: Vec<(Vec<usize>, f32)> = Vec::new();
    for s in ids {
        let bottom = segments[s].bounds.y;
        match bands.last_mut() {
            Some((members, band_bottom)) if top(s) > *band_bottom - PARAGRAPH_GAP * h => {
                members.push(s);
                *band_bottom = band_bottom.min(bottom);
            }
            _ => bands.push((vec![s], bottom)),
        }
    }
    bands.into_iter().map(|(members, _)| members).collect()
}

/// Un bloc final : ses lignes de haut en bas, chacune avec ses segments de gauche à droite.
fn leaf(segments: &[Segment], ids: &[usize]) -> Block {
    let mut ordered: Vec<usize> = ids.to_vec();
    // Les lignes sont numérotées du haut vers le bas : trier par (ligne, x).
    ordered.sort_by(|&a, &b| {
        segments[a]
            .line
            .cmp(&segments[b].line)
            .then(segments[a].bounds.x.total_cmp(&segments[b].bounds.x))
    });
    let mut block: Block = Vec::new();
    let mut current_line = None;
    for s in ordered {
        if current_line != Some(segments[s].line) {
            block.push(Vec::new());
            current_line = Some(segments[s].line);
        }
        if let Some(line) = block.last_mut() {
            line.extend_from_slice(&segments[s].chars);
        }
    }
    block
}

/// Le texte de blocs déjà ordonnés : une espace entre deux caractères d'une même ligne
/// séparés par un blanc sans caractère d'espace, un saut de ligne entre les lignes, une ligne
/// vide entre les blocs.
pub fn blocks_text(blocks: &[Block], chars: &[char], rects: &[Rect]) -> String {
    let mut text = String::new();
    for (b, block) in blocks.iter().enumerate() {
        if b > 0 {
            text.push_str("\n\n");
        }
        for (l, line) in block.iter().enumerate() {
            if l > 0 {
                text.push('\n');
            }
            let mut previous: Option<usize> = None;
            for &c in line {
                if let Some(p) = previous {
                    let gap = rects[c].x - (rects[p].x + rects[p].w);
                    let spaced = chars[p].is_whitespace() || chars[c].is_whitespace();
                    if !spaced && gap > 0.5 * rects[p].h.max(rects[c].h) {
                        text.push(' ');
                    }
                }
                text.push(chars[c]);
                previous = Some(c);
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Largeur et hauteur des caractères des tests (Helvetica 10 pt : environ 6 × 12).
    const W: f32 = 6.0;
    const H: f32 = 12.0;

    /// Une « page » synthétique : des mots posés (texte, x, y du bas), dans l'ordre de dessin.
    /// Rend les caractères et leurs rectangles, un caractère par lettre (espaces compris).
    fn page(words: &[(&str, f32, f32)]) -> (Vec<char>, Vec<Rect>) {
        let mut chars = Vec::new();
        let mut rects = Vec::new();
        for (text, x, y) in words {
            for (k, ch) in text.chars().enumerate() {
                chars.push(ch);
                rects.push(Rect {
                    x: x + k as f32 * W,
                    y: *y,
                    w: W,
                    h: H,
                });
            }
        }
        (chars, rects)
    }

    /// Le texte lu dans l'ordre calculé.
    fn read(words: &[(&str, f32, f32)]) -> String {
        let (chars, rects) = page(words);
        blocks_text(&layout(&rects), &chars, &rects)
    }

    #[test]
    fn lines_drawn_out_of_order_read_top_to_bottom() {
        // Trois lignes d'un paragraphe (interligne 14), dessinées 3, 1, 2.
        let text = read(&[
            ("troisieme", 72.0, 672.0),
            ("premiere", 72.0, 700.0),
            ("deuxieme", 72.0, 686.0),
        ]);
        assert_eq!(text, "premiere\ndeuxieme\ntroisieme");
    }

    #[test]
    fn fragment_drawn_last_lands_in_its_line() {
        // « un mot gras ici » : « gras » est dessiné à la fin du flux, au milieu de sa ligne.
        let text = read(&[
            ("un mot ", 72.0, 700.0),
            (" ici", 72.0 + 11.0 * W, 700.0),
            ("ligne suivante", 72.0, 686.0),
            ("gras", 72.0 + 7.0 * W, 700.0),
        ]);
        assert_eq!(text, "un mot gras ici\nligne suivante");
    }

    #[test]
    fn label_value_rows_read_row_by_row() {
        // Trois lignes « libellé → valeur » : tous les libellés, puis toutes les valeurs.
        let text = read(&[
            ("Nom:", 72.0, 700.0),
            ("Code:", 72.0, 686.0),
            ("Ville:", 72.0, 672.0),
            ("Alice", 250.0, 700.0),
            ("A12", 250.0, 686.0),
            ("Quebec", 250.0, 672.0),
        ]);
        assert_eq!(text, "Nom: Alice\nCode: A12\nVille: Quebec");
    }

    #[test]
    fn two_columns_read_column_by_column() {
        // Deux colonnes de 5 lignes dessinées en alternance gauche, droite.
        let mut words = Vec::new();
        let left = ["g1", "g2", "g3", "g4", "g5"];
        let right = ["d1", "d2", "d3", "d4", "d5"];
        for k in 0..5 {
            let y = 700.0 - 14.0 * k as f32;
            words.push((left[k], 72.0, y));
            words.push((right[k], 320.0, y));
        }
        assert_eq!(read(&words), "g1\ng2\ng3\ng4\ng5\n\nd1\nd2\nd3\nd4\nd5");
    }

    #[test]
    fn superscript_stays_on_its_line() {
        // Un exposant plus petit et surélevé de 4 pt reste sur sa ligne.
        let (mut chars, mut rects) = page(&[("x", 72.0, 700.0), ("suite", 72.0, 686.0)]);
        chars.insert(1, '2');
        rects.insert(
            1,
            Rect {
                x: 72.0 + W,
                y: 704.0,
                w: 4.0,
                h: 8.0,
            },
        );
        assert_eq!(blocks_text(&layout(&rects), &chars, &rects), "x2\nsuite");
    }

    #[test]
    fn zero_size_boxes_are_ignored() {
        // Les sauts de ligne ajoutés par PDFium n'ont pas d'emprise.
        let (mut chars, mut rects) = page(&[("ab", 72.0, 700.0)]);
        chars.push('\n');
        rects.push(Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
        let blocks = layout(&rects);
        assert_eq!(blocks, vec![vec![vec![0, 1]]]);
    }

    /// Deux mots dessinés au même endroit (filigranes superposés, faux gras) restent entiers au
    /// lieu d'avoir leurs lettres entremêlées par le tri horizontal.
    #[test]
    fn overlapping_words_stay_whole() {
        let text = read(&[("UN", 280.0, 400.0), ("DEUX", 274.0, 400.0)]);
        assert!(text == "DEUXUN" || text == "UNDEUX", "{text}");
    }

    #[test]
    fn empty_page_gives_no_block() {
        assert!(layout(&[]).is_empty());
    }

    #[test]
    fn paragraphs_are_separated_by_a_blank_line() {
        // Un écart vertical d'une ligne entière sépare deux paragraphes.
        let text = read(&[("fin", 72.0, 700.0), ("debut", 72.0, 670.0)]);
        assert_eq!(text, "fin\n\ndebut");
    }
}
