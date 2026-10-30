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
/// les index de ses caractères dans l'ordre de lecture (de gauche à droite, ou de droite à
/// gauche pour une ligne hébraïque ou arabe passée par [`layout_chars`]).
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

/// Comme [`layout`], avec les caractères : chaque ligne écrite de droite à gauche est remise
/// dans l'ordre logique (voir [`logical_order`]). La géométrie (lignes, segments, colonnes)
/// reste calculée de gauche à droite.
pub fn layout_chars(rects: &[Rect], chars: &[char]) -> Vec<Block> {
    let mut blocks = layout(rects);
    for line in blocks.iter_mut().flat_map(|block| block.iter_mut()) {
        logical_order(line, rects, chars);
    }
    blocks
}

/// Vrai pour un idéogramme ou un caractère des écritures chinoise, japonaise ou coréenne
/// (dont la ponctuation et les formes pleine chasse), les seules qui s'écrivent aussi en
/// colonnes verticales.
fn is_cjk(c: char) -> bool {
    matches!(
        u32::from(c),
        0x3000..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF
    )
}

/// Les colonnes d'**écriture verticale** de la page (audit du Lot 13, C4) : suites d'au moins
/// trois caractères CJK dessinés d'affilée, sans voisin collé sur leur ligne, chacun **sous** le précédent et à peu près à la
/// même abscisse. Découpées en lignes horizontales, elles donneraient une ligne d'un caractère
/// par rangée ; on les lit à part, dans leur ordre de dessin (de haut en bas, colonne par
/// colonne), comme le texte tourné. Des lettres latines empilées (une colonne de chiffres d'un
/// tableau) ne sont pas concernées.
pub fn vertical_runs(rects: &[Rect], chars: &[char]) -> Vec<Vec<usize>> {
    let below = |a: &Rect, b: &Rect| {
        let size = a.w.max(b.w).max(a.h).max(b.h);
        let same_column = ((a.x + a.w / 2.0) - (b.x + b.w / 2.0)).abs() < 0.5 * size;
        let gap = a.y - (b.y + b.h);
        same_column && gap > -0.3 * size && gap < 0.5 * size
    };
    let mut runs = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    for i in 0..rects.len().min(chars.len()) {
        let fits = is_cjk(chars[i])
            && usable(&rects[i])
            && run.last().is_some_and(|&p| below(&rects[p], &rects[i]));
        if fits {
            run.push(i);
            continue;
        }
        if run.len() >= MIN_VERTICAL_RUN {
            runs.push(std::mem::take(&mut run));
        }
        run.clear();
        if is_cjk(chars[i]) && usable(&rects[i]) {
            run.push(i);
        }
    }
    if run.len() >= MIN_VERTICAL_RUN {
        runs.push(run);
    }
    // Un caractère qui a un voisin collé à sa gauche ou à sa droite, sur sa ligne, appartient
    // à une ligne horizontale : un titre d'un caractère suivi d'une ligne à la même marge
    // ressemblerait sinon à une colonne (relecture de la PR B du Lot 13).
    let side_neighbour = |i: usize| {
        let r = &rects[i];
        (0..rects.len().min(chars.len())).any(|j| {
            let o = &rects[j];
            let overlap = (r.y + r.h).min(o.y + o.h) - r.y.max(o.y);
            let gap = (o.x - (r.x + r.w)).max(r.x - (o.x + o.w));
            j != i && usable(o) && overlap > 0.5 * r.h.min(o.h) && gap < 0.3 * r.w.max(o.w)
        })
    };
    runs.retain(|run| !run.iter().any(|&i| side_neighbour(i)));
    runs
}

/// Nombre minimal de caractères empilés pour parler de colonne verticale : deux caractères
/// l'un sous l'autre arrivent par hasard (fin d'une ligne, début de la suivante).
const MIN_VERTICAL_RUN: usize = 3;

/// Sens d'un caractère pour la lecture (version simplifiée de l'algorithme bidirectionnel
/// d'Unicode, UAX #9).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// Lettre d'une écriture de droite à gauche : hébreu, arabe, syriaque, thâna, N'Ko et
    /// leurs formes de présentation.
    Rtl,
    /// Lettre d'une autre écriture, lue de gauche à droite.
    Ltr,
    /// Chiffre (arabe, ou arabo-indien comme « ٢٠٢٤ ») : un nombre se lit toujours de gauche
    /// à droite, même dans une ligne arabe.
    Digit,
    /// Espace, ponctuation, symbole : prend le sens de ce qui l'entoure.
    Neutral,
}

fn direction(c: char) -> Direction {
    match u32::from(c) {
        0x0660..=0x0669 | 0x06F0..=0x06F9 => Direction::Digit,
        0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF => Direction::Rtl,
        _ if c.is_numeric() => Direction::Digit,
        _ if c.is_alphabetic() => Direction::Ltr,
        _ => Direction::Neutral,
    }
}

/// Remet dans l'ordre **logique** une ligne dont les lettres sont surtout hébraïques ou
/// arabes (audit du Lot 13, C3, et relecture de la PR B).
///
/// Un PDF dessine les glyphes là où ils s'affichent : l'hébreu « שלום » est posé ם ו ל ש de
/// gauche à droite. Pour une ligne dont les **lettres** sont surtout de droite à gauche (les
/// chiffres ne votent pas), on lit donc les caractères de droite à gauche, puis on remet de
/// gauche à droite :
/// - chaque suite de mots latins (« PDF Mega »), espaces intérieures comprises ;
/// - chaque nombre (« 2024 », « ٢٠٢٤ », « 3.14 »), séparateur entre deux chiffres compris.
///   Deux nombres séparés par une espace restent deux suites : « 10 20 » garde son ordre.
///
/// Une ligne latine qui contient un mot hébreu n'est pas touchée.
///
/// \[Apprentissage\] **Retourner une sous-tranche en place** : `line[k..=end]` désigne une
/// portion du vecteur sans la copier, et `.reverse()` en inverse l'ordre directement dans
/// `line`. Le `&mut [usize]` reçu en paramètre permet de modifier la ligne de l'appelant.
pub fn logical_order(line: &mut [usize], rects: &[Rect], chars: &[char]) {
    let count = |d: Direction| line.iter().filter(|&&i| direction(chars[i]) == d).count();
    if count(Direction::Rtl) <= count(Direction::Ltr) {
        return;
    }
    // De droite à gauche, par le centre de chaque caractère.
    line.sort_by(|&a, &b| {
        let center = |i: usize| rects[i].x + rects[i].w / 2.0;
        center(b).total_cmp(&center(a))
    });
    // Les caractères dans cet ordre, copiés : `line` sera modifiée pendant le parcours (un
    // retournement ne touche que la suite déjà parcourue).
    let ordered: Vec<char> = line.iter().map(|&i| chars[i]).collect();
    let mut k = 0;
    while k < line.len() {
        let end = match direction(ordered[k]) {
            Direction::Ltr => latin_run_end(&ordered, k),
            Direction::Digit => number_end(&ordered, k),
            _ => {
                k += 1;
                continue;
            }
        };
        line[k..=end].reverse();
        k = end + 1;
    }
}

/// Fin d'une suite latine qui commence en `k` : jusqu'à la dernière lettre latine avant une
/// lettre de droite à gauche ou un chiffre, les neutres intérieurs compris.
fn latin_run_end(ordered: &[char], k: usize) -> usize {
    let dir = |j: usize| direction(ordered[j]);
    let mut end = k;
    let mut j = k + 1;
    while j < ordered.len() && matches!(dir(j), Direction::Ltr | Direction::Neutral) {
        if dir(j) == Direction::Ltr {
            end = j;
        }
        j += 1;
    }
    end
}

/// Fin d'un nombre qui commence en `k` : ses chiffres, et un séparateur (`.`, `,`, `:`, `/`)
/// seulement s'il est pris entre deux chiffres (« 3.14 », « 1,000 », « 12:30 »).
fn number_end(ordered: &[char], k: usize) -> usize {
    let dir = |j: usize| direction(ordered[j]);
    let mut end = k;
    let mut j = k + 1;
    while j < ordered.len() {
        if dir(j) == Direction::Digit {
            end = j;
            j += 1;
        } else if matches!(ordered[j], '.' | ',' | ':' | '/' | '٫' | '٬')
            && j + 1 < ordered.len()
            && dir(j + 1) == Direction::Digit
        {
            j += 1;
        } else {
            break;
        }
    }
    end
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
                    // Écart dans un sens ou dans l'autre : une ligne lue de droite à gauche
                    // va vers les x décroissants.
                    let (a, b) = (&rects[p], &rects[c]);
                    let gap = (b.x - (a.x + a.w)).max(a.x - (b.x + b.w));
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

    /// Le texte d'une ligne posée **visuellement** (glyphes de gauche à droite, comme un PDF
    /// les dessine), lu par `layout_chars` puis `blocks_text`.
    fn read_visual(visual: &str) -> String {
        let chars: Vec<char> = visual.chars().collect();
        let rects: Vec<Rect> = (0..chars.len())
            .map(|i| Rect {
                x: 100.0 + i as f32 * W,
                y: 700.0,
                w: W,
                h: H,
            })
            .collect();
        blocks_text(&layout_chars(&rects, &chars), &chars, &rects)
    }

    /// C3 (audit du Lot 13) : une ligne en hébreu ou en arabe se lit de droite à gauche ; les
    /// nombres et les mots latins qu'elle contient restent de gauche à droite.
    #[test]
    fn right_to_left_lines_are_read_in_logical_order() {
        // « שלום » (« bonjour ») dessiné tel qu'il s'affiche : ם ו ל ש de gauche à droite.
        assert_eq!(read_visual("םולש"), "שלום");
        // « שלום 123 » : le nombre, à gauche à l'écran, vient après et se lit de gauche à droite.
        assert_eq!(read_visual("123 םולש"), "שלום 123");
        // Arabe avec un mot latin de deux syllabes au milieu : « مرحبا PDF Mega عالم ».
        assert_eq!(read_visual("ملاع PDF Mega ابحرم"), "مرحبا PDF Mega عالم");
        // Une ligne latine ne change pas, même avec un mot hébreu court.
        assert_eq!(read_visual("Shalom םולש world"), "Shalom םולש world");
    }

    /// Relecture de la PR B : les nombres d'une ligne de droite à gauche (chiffres arabes ou
    /// arabo-indiens) se lisent de gauche à droite, chacun pour soi, séparateurs compris ; la
    /// majorité ne compte que les lettres.
    #[test]
    fn numbers_in_right_to_left_lines_keep_their_order() {
        // « سنة ٢٠٢٤ » (année 2024 en chiffres arabo-indiens).
        assert_eq!(read_visual("٢٠٢٤ ةنس"), "سنة ٢٠٢٤");
        // Deux nombres : « שלום לכל העולם 10 20 ».
        assert_eq!(read_visual("20 10 םלועה לכל םולש"), "שלום לכל העולם 10 20");
        // Plus de chiffres que de lettres : la ligne reste hébraïque.
        assert_eq!(read_visual("20 10 םולש"), "שלום 10 20");
        // Un nombre décimal reste entier.
        assert_eq!(read_visual("3.14 םולש"), "שלום 3.14");
    }

    /// C4 (audit du Lot 13) : une colonne d'écriture verticale (caractères CJK empilés, dessinés
    /// de haut en bas) forme une suite ; une ligne CJK horizontale n'en forme pas.
    #[test]
    fn stacked_cjk_characters_form_a_vertical_run() {
        let square = |x: f32, y: f32| Rect {
            x,
            y,
            w: 12.0,
            h: 12.0,
        };
        let chars: Vec<char> = "縦書きです横書きです".chars().collect();
        let mut rects: Vec<Rect> = (0..5)
            .map(|k| square(500.0, 700.0 - 13.0 * k as f32))
            .collect();
        rects.extend((0..5).map(|k| square(100.0 + 12.0 * k as f32, 500.0)));
        assert_eq!(vertical_runs(&rects, &chars), vec![vec![0, 1, 2, 3, 4]]);
        // Des lettres latines empilées (une colonne de chiffres d'un tableau) ne comptent pas.
        let digits: Vec<char> = "12345".chars().collect();
        assert!(vertical_runs(&rects[..5], &digits).is_empty());
    }

    /// Relecture de la PR B : un titre japonais d'un seul caractère (« 序 »), suivi d'une ligne
    /// horizontale qui commence à la même marge, n'est pas une colonne verticale ; le premier
    /// caractère de la ligne reste dans sa ligne.
    #[test]
    fn a_one_character_heading_is_not_a_vertical_column() {
        // Interligne de 1,5 fois la taille, boîtes de 1,15 fois : écart vertical de 0,35.
        let square = |x: f32, y: f32| Rect {
            x,
            y,
            w: 12.0,
            h: 13.8,
        };
        let chars: Vec<char> = "序本文はここから始まります".chars().collect();
        let mut rects = vec![square(100.0, 700.0)];
        rects.extend((0..chars.len() - 1).map(|k| square(100.0 + 12.0 * k as f32, 682.0)));
        assert!(vertical_runs(&rects, &chars).is_empty());
        let lines: Vec<String> = layout_chars(&rects, &chars)
            .iter()
            .flatten()
            .map(|line| line.iter().map(|&i| chars[i]).collect())
            .collect();
        assert_eq!(
            lines,
            vec!["序".to_string(), "本文はここから始まります".to_string()]
        );
    }

    /// Propriété (audit du Lot 13, R9) : pour des rectangles **quelconques** (positions et
    /// tailles aléatoires, valeurs nulles, négatives, infinies ou NaN mêlées), l'ordre de
    /// lecture contient chaque caractère utilisable exactement une fois, sans paniquer.
    #[test]
    fn every_usable_box_appears_exactly_once_whatever_the_input() {
        use crate::testing::fuzz::XorShift;
        for seed in 0..300u64 {
            let mut rng = XorShift::new(seed);
            let count = rng.below(400);
            let value = |rng: &mut XorShift| -> f32 {
                match rng.below(20) {
                    0 => f32::NAN,
                    1 => f32::INFINITY,
                    2 => -3.0,
                    3 => 0.0,
                    _ => (rng.below(60_000) as f32) / 100.0,
                }
            };
            let rects: Vec<Rect> = (0..count)
                .map(|_| Rect {
                    x: value(&mut rng) * 2.0,
                    y: value(&mut rng) * 2.0,
                    w: value(&mut rng) / 20.0,
                    h: value(&mut rng) / 20.0,
                })
                .collect();
            let mut seen: Vec<usize> = layout(&rects).into_iter().flatten().flatten().collect();
            seen.sort_unstable();
            let expected: Vec<usize> = (0..count).filter(|&i| usable(&rects[i])).collect();
            assert_eq!(seen, expected, "graine {seed}");
        }
    }

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
