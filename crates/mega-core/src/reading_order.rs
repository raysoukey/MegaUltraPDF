// SPDX-License-Identifier: GPL-3.0-or-later
//! Ordre de lecture de la couche texte (correctif 0.12.2, ADR-0016).
//!
//! PDFium rend les caractères d'une page dans l'ordre où le fichier les **dessine**, qui n'est
//! pas toujours l'ordre de lecture : un mot en gras dessiné à la fin, une colonne de valeurs
//! dessinée après celle des libellés, des lignes dans le désordre… La sélection (une plage
//! d'index) et la copie ramassaient alors des morceaux de toute la page. Ce module recalcule
//! l'ordre à partir de la **géométrie** seule : il ne connaît que des rectangles, ce qui le rend
//! testable sans PDF. Les rectangles viennent d'un fichier non fiable : le calcul ne doit ni
//! paniquer ni épuiser la pile, quelle que soit leur disposition.

use crate::geometry::Rect;

/// Un bloc de texte lu d'un seul tenant : ses lignes, du haut vers le bas, chacune donnée par
/// les index de ses caractères, de gauche à droite.
pub type Block = Vec<Vec<usize>>;

/// Deux caractères sont sur la même ligne si leurs étendues verticales se recouvrent d'au
/// moins cette part de la plus petite hauteur (un exposant reste sur sa ligne).
const SAME_LINE_OVERLAP: f32 = 0.5;
/// Un caractère plus haut que ce multiple de la hauteur médiane `H` (lettrine, grande
/// parenthèse, métrique de police cassée) rejoint une ligne sans l'agrandir : sinon il
/// fusionnerait toutes les lignes qu'il couvre.
const TALL: f32 = 1.5;
/// Dans une ligne, un blanc plus large que ce multiple de `H` sépare deux segments (une
/// tabulation entre un libellé et sa valeur, ou deux colonnes).
const SEGMENT_GAP: f32 = 1.5;
/// Une bande verticale vide d'au moins ce multiple de `H` peut séparer deux colonnes…
const COLUMN_GAP: f32 = 1.0;
/// … si chaque côté ressemble à de la **prose** : des segments d'au moins ce multiple de `H`
/// de large (largeur médiane). Les libellés, numéros et valeurs courtes d'une liste ou d'un
/// tableau ne le sont pas : ils restent lus ligne par ligne…
const COLUMN_MIN_WIDTH: f32 = 12.0;
/// … et si chaque côté compte au moins ce nombre de lignes.
const COLUMN_MIN_LINES: usize = 2;
/// On n'essaie que les bandes vides les plus larges : les colonnes sont séparées par les plus
/// grands blancs, et cela borne le travail sur une page fabriquée avec des milliers de bandes.
const COLUMN_CANDIDATES: usize = 8;
/// Un vide vertical d'au moins ce multiple de `H` sépare deux paragraphes…
const PARAGRAPH_GAP: f32 = 0.8;
/// … et d'au moins ce multiple du plus petit interligne du bloc (texte en double interligne).
const PARAGRAPH_VS_LINE_GAP: f32 = 1.5;
/// Budget de travail du découpage (en segments examinés, par segment de la page) : au-delà, un
/// morceau est lu tel quel, ligne par ligne. Une page normale en utilise une petite fraction ;
/// une page fabriquée pour faire durer le calcul ne peut pas le rendre quadratique.
const WORK_PER_SEGMENT: usize = 64;

/// Un morceau de ligne d'un seul tenant : sa ligne, ses caractères (de gauche à droite) et
/// son emprise.
struct Segment {
    line: usize,
    chars: Vec<usize>,
    bounds: Rect,
}

/// Les blocs de la page dans l'ordre de lecture. Les rectangles de largeur ou de hauteur
/// nulle (sauts de ligne ajoutés par PDFium) ou non finis (fichier corrompu) sont ignorés.
pub fn layout(rects: &[Rect]) -> Vec<Block> {
    let visible: Vec<usize> = (0..rects.len()).filter(|&i| usable(&rects[i])).collect();
    if visible.is_empty() {
        return Vec::new();
    }
    let h = median_height(rects, &visible);
    let lines = group_lines(rects, visible, h);
    let segments = split_segments(rects, &lines, h);
    xy_cut(&segments, h)
}

/// Vrai si le rectangle peut servir au calcul : coordonnées finies, largeur et hauteur > 0.
fn usable(r: &Rect) -> bool {
    [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) && r.w > 0.0 && r.h > 0.0
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

/// Recouvrement vertical de deux étendues `[bas, haut]` (négatif si elles sont disjointes).
fn overlap(bottom: f32, top: f32, r: &Rect) -> f32 {
    top.min(r.y + r.h) - bottom.max(r.y)
}

/// Regroupe les caractères en lignes, de la plus haute à la plus basse ; dans chaque ligne,
/// les caractères sont rangés de gauche à droite par `order_line`.
///
/// \[Apprentissage\] **Trier, puis balayer** : une fois les caractères triés par hauteur, une
/// ligne est une suite de voisins qui se recouvrent ; un seul passage suffit à les regrouper.
/// `sort_by(|a, b| …total_cmp…)` trie des `f32` (qui n'ont pas d'ordre total à cause de `NaN`).
fn group_lines(rects: &[Rect], visible: Vec<usize>, h: f32) -> Vec<Vec<usize>> {
    // Les caractères trop hauts ne dessinent pas les lignes : on les rattache après coup.
    let (mut normal, tall): (Vec<usize>, Vec<usize>) =
        visible.into_iter().partition(|&i| rects[i].h <= TALL * h);
    normal.sort_by(|&a, &b| center_y(&rects[b]).total_cmp(&center_y(&rects[a])));
    // Chaque ligne : ses caractères et sa bande verticale (bas, haut).
    let mut lines: Vec<(Vec<usize>, f32, f32)> = Vec::new();
    for i in normal {
        let r = &rects[i];
        let joins = lines.last().is_some_and(|(_, bottom, top)| {
            overlap(*bottom, *top, r) >= SAME_LINE_OVERLAP * (top - bottom).min(r.h)
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
    // Un caractère haut rejoint la ligne qu'il recouvre le plus (à égalité, la plus haute :
    // une lettrine commence sa première ligne) ; sans recouvrement, il forme sa propre ligne.
    for t in tall {
        let r = &rects[t];
        let mut best: Option<(usize, f32)> = None;
        for (k, (_, bottom, top)) in lines.iter().enumerate() {
            let o = overlap(*bottom, *top, r);
            if o > 0.0 && best.is_none_or(|(_, b)| o > b) {
                best = Some((k, o));
            }
        }
        match best {
            Some((k, _)) => lines[k].0.push(t),
            None => lines.push((vec![t], r.y, r.y + r.h)),
        }
    }
    // Les lignes ajoutées pour des caractères hauts isolés reprennent leur place (de haut en bas).
    lines.sort_by(|a, b| (b.1 + b.2).total_cmp(&(a.1 + a.2)));
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

/// Découpage « XY » : chaque morceau est coupé en colonnes s'il le faut, sinon au plus grand
/// vide entre paragraphes ; un morceau qu'on ne peut plus couper devient un bloc.
///
/// \[Apprentissage\] **Une pile au lieu de la récursion** : une fonction récursive empile un
/// appel par niveau, et la pile d'un fil est petite (quelques Mo). Un fichier malveillant peut
/// provoquer des dizaines de milliers de niveaux et faire tomber l'application. Ici, les
/// morceaux à traiter attendent dans un `Vec` (sur le tas, sans limite pratique) : on dépile le
/// prochain à lire, et ses deux moitiés sont empilées « second, puis premier » pour que le
/// premier soit lu d'abord.
fn xy_cut(segments: &[Segment], h: f32) -> Vec<Block> {
    let budget = WORK_PER_SEGMENT * segments.len() + 1024;
    let mut work = 0;
    let mut blocks = Vec::new();
    let mut pending: Vec<Vec<usize>> = vec![(0..segments.len()).collect()];
    while let Some(ids) = pending.pop() {
        work += ids.len();
        let cut = if work > budget {
            None
        } else {
            column_cut(segments, &ids, h).or_else(|| paragraph_cut(segments, &ids, h))
        };
        match cut {
            Some((first, second)) => {
                pending.push(second);
                pending.push(first);
            }
            None => blocks.push(leaf(segments, &ids)),
        }
    }
    blocks
}

/// Une coupe en colonnes : parmi les plus larges bandes verticales vides (d'au moins
/// `COLUMN_GAP × H`), la première dont les deux côtés ressemblent à de la prose.
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
    for (gap_start, _) in gaps.into_iter().take(COLUMN_CANDIDATES) {
        let (left, right): (Vec<usize>, Vec<usize>) = ids
            .iter()
            .partition(|&&s| segments[s].bounds.x + segments[s].bounds.w <= gap_start);
        if is_column(segments, &left, h) && is_column(segments, &right, h) {
            return Some((left, right));
        }
    }
    None
}

/// Vrai si ces segments ont l'allure d'une colonne de texte : assez de lignes, et des segments
/// assez larges (une colonne de libellés, de numéros ou de valeurs courtes n'en est pas une).
fn is_column(segments: &[Segment], ids: &[usize], h: f32) -> bool {
    let mut lines: Vec<usize> = ids.iter().map(|&s| segments[s].line).collect();
    lines.sort_unstable();
    lines.dedup();
    if lines.len() < COLUMN_MIN_LINES {
        return false;
    }
    let mut widths: Vec<f32> = ids.iter().map(|&s| segments[s].bounds.w).collect();
    widths.sort_by(f32::total_cmp);
    widths[widths.len() / 2] >= COLUMN_MIN_WIDTH * h
}

/// Une coupe au **plus grand** vide vertical entre paragraphes : d'au moins `PARAGRAPH_GAP × H`
/// et d'au moins `PARAGRAPH_VS_LINE_GAP` fois le plus petit interligne du morceau. Couper au
/// plus grand vide d'abord laisse aux colonnes une chance d'être reconnues à l'étape suivante
/// (un titre au-dessus de deux colonnes, par exemple).
fn paragraph_cut(segments: &[Segment], ids: &[usize], h: f32) -> Option<(Vec<usize>, Vec<usize>)> {
    let top = |s: usize| segments[s].bounds.y + segments[s].bounds.h;
    let mut order: Vec<usize> = ids.to_vec();
    order.sort_by(|&a, &b| top(b).total_cmp(&top(a)));
    // Un vide sépare le morceau à la position `k` si le segment `k` est entièrement sous tous
    // les précédents.
    let mut gaps: Vec<(usize, f32)> = Vec::new();
    let mut lowest = f32::MAX;
    for (k, &s) in order.iter().enumerate() {
        if k > 0 && top(s) < lowest {
            gaps.push((k, lowest - top(s)));
        }
        lowest = lowest.min(segments[s].bounds.y);
    }
    let smallest = if gaps.len() >= 2 {
        gaps.iter().map(|g| g.1).fold(f32::MAX, f32::min)
    } else {
        0.0
    };
    let threshold = (PARAGRAPH_GAP * h).max(PARAGRAPH_VS_LINE_GAP * smallest);
    let (k, _) = gaps
        .into_iter()
        .filter(|g| g.1 >= threshold)
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let second = order.split_off(k);
    Some((order, second))
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

    /// Une ligne de prose de 30 caractères (180 pt, soit 15 H) : « <mot> » complété de points.
    fn prose(word: &str) -> String {
        format!("{word:.<30}")
    }

    #[test]
    fn two_columns_read_column_by_column() {
        // Deux colonnes de prose de 5 lignes dessinées en alternance gauche, droite.
        let left: Vec<String> = (1..=5).map(|k| prose(&format!("g{k}"))).collect();
        let right: Vec<String> = (1..=5).map(|k| prose(&format!("d{k}"))).collect();
        let mut words = Vec::new();
        for k in 0..5 {
            let y = 700.0 - 14.0 * k as f32;
            words.push((left[k].as_str(), 72.0, y));
            words.push((right[k].as_str(), 300.0, y));
        }
        let expected = format!("{}\n\n{}", left.join("\n"), right.join("\n"));
        assert_eq!(read(&words), expected);
    }

    /// Relecture (I3) : un titre sur toute la largeur, puis deux colonnes dont les paragraphes
    /// s'arrêtent à la même hauteur : la page ne doit pas être lue rangée par rangée.
    #[test]
    fn two_columns_under_a_title_with_aligned_paragraphs() {
        let title = "Titre du document sur toute la largeur de la page.";
        let left: Vec<String> = (1..=6).map(|k| prose(&format!("g{k}"))).collect();
        let right: Vec<String> = (1..=6).map(|k| prose(&format!("d{k}"))).collect();
        let ys = [700.0, 686.0, 672.0, 640.0, 626.0, 612.0];
        let mut words = vec![(title, 72.0, 760.0)];
        for k in 0..6 {
            words.push((left[k].as_str(), 72.0, ys[k]));
            words.push((right[k].as_str(), 300.0, ys[k]));
        }
        let column = |c: &[String]| format!("{}\n\n{}", c[..3].join("\n"), c[3..].join("\n"));
        let expected = format!("{title}\n\n{}\n\n{}", column(&left), column(&right));
        assert_eq!(read(&words), expected);
    }

    /// Relecture (I3) : la colonne de droite n'a que 3 lignes (dernière page d'un article).
    #[test]
    fn a_short_right_column_is_still_a_column() {
        let left: Vec<String> = (1..=6).map(|k| prose(&format!("g{k}"))).collect();
        let right: Vec<String> = (1..=3).map(|k| prose(&format!("d{k}"))).collect();
        let mut words = Vec::new();
        for (k, l) in left.iter().enumerate() {
            words.push((l.as_str(), 72.0, 700.0 - 14.0 * k as f32));
        }
        for (k, r) in right.iter().enumerate() {
            words.push((r.as_str(), 300.0, 700.0 - 14.0 * k as f32));
        }
        assert_eq!(
            read(&words),
            format!("{}\n\n{}", left.join("\n"), right.join("\n"))
        );
    }

    /// Relecture (I1) : une liste « libellé : valeur » de 5 lignes se lit encore ligne par ligne.
    #[test]
    fn five_label_value_rows_read_row_by_row() {
        let labels = ["Nom:", "Code:", "Ville:", "Pays:", "Tel:"];
        let values = ["Alice", "A12", "Quebec", "Canada", "555"];
        let mut words = Vec::new();
        for (k, label) in labels.iter().enumerate() {
            words.push((*label, 72.0, 700.0 - 14.0 * k as f32));
        }
        for (k, value) in values.iter().enumerate() {
            words.push((*value, 250.0, 700.0 - 14.0 * k as f32));
        }
        assert_eq!(
            read(&words),
            "Nom: Alice\nCode: A12\nVille: Quebec\nPays: Canada\nTel: 555"
        );
    }

    /// Relecture (I1) : une liste numérotée dont le texte commence loin du numéro.
    #[test]
    fn numbered_list_reads_row_by_row() {
        let items: Vec<String> = (1..=5).map(|k| prose(&format!("element {k}"))).collect();
        let mut words = Vec::new();
        let numbers = ["1.", "2.", "3.", "4.", "5."];
        for (k, number) in numbers.iter().enumerate() {
            words.push((*number, 72.0, 700.0 - 14.0 * k as f32));
        }
        for (k, item) in items.iter().enumerate() {
            words.push((item.as_str(), 108.0, 700.0 - 14.0 * k as f32));
        }
        let expected: Vec<String> = (0..5)
            .map(|k| format!("{} {}", numbers[k], items[k]))
            .collect();
        assert_eq!(read(&words), expected.join("\n"));
    }

    /// Relecture (I2) : une lettrine haute de trois lignes ne fusionne pas ces lignes.
    #[test]
    fn a_drop_cap_does_not_merge_lines() {
        let (mut chars, mut rects) = page(&[
            ("orem ipsum", 106.0, 700.0),
            ("dolor sit", 106.0, 686.0),
            ("amet cons", 106.0, 672.0),
        ]);
        chars.insert(0, 'L');
        rects.insert(
            0,
            Rect {
                x: 72.0,
                y: 672.0,
                w: 30.0,
                h: 40.0,
            },
        );
        assert_eq!(
            blocks_text(&layout(&rects), &chars, &rects),
            "Lorem ipsum\ndolor sit\namet cons"
        );
    }

    /// Relecture (M1) : des rectangles infinis ou invalides (fichier malveillant) sont ignorés.
    #[test]
    fn non_finite_boxes_are_ignored() {
        let (mut chars, mut rects) = page(&[("ab", 72.0, 700.0)]);
        chars.push('Z');
        rects.push(Rect {
            x: 72.0,
            y: 690.0,
            w: 6.0,
            h: f32::INFINITY,
        });
        chars.push('Y');
        rects.push(Rect {
            x: f32::NAN,
            y: 690.0,
            w: 6.0,
            h: 12.0,
        });
        assert_eq!(layout(&rects), vec![vec![vec![0, 1]]]);
    }

    /// Relecture (C1) : un « escalier » de 20 000 caractères (chacun sur sa ligne et sa bande
    /// verticale), fabriqué pour épuiser la pile, ne fait pas tomber l'application. Le calcul
    /// tourne sur un fil à petite pile (256 Kio) : une récursion profonde y déborderait.
    #[test]
    fn a_deep_staircase_does_not_overflow_the_stack() {
        let rects: Vec<Rect> = (0..20_000)
            .map(|k| Rect {
                x: k as f32 * 20.0,
                y: 300_000.0 - k as f32 * 14.0,
                w: 6.0,
                h: 12.0,
            })
            .collect();
        let handle = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || layout(&rects).iter().map(|b| b.len()).sum::<usize>())
            .unwrap();
        assert_eq!(handle.join().unwrap(), 20_000);
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
