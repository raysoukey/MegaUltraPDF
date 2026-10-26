// SPDX-License-Identifier: GPL-3.0-or-later
//! Spécifications de pages saisies par l'utilisateur : « 1-3,5,8- ».
//!
//! L'utilisateur compte les pages à partir de 1 ; le reste du code à partir de 0. Toute
//! la conversion et la validation se font ici, une fois pour toutes (CLI et interface).

use std::collections::BTreeSet;

use crate::error::{CoreError, CoreResult};

/// Analyse de spécifications de pages. Type sans état : seulement des fonctions associées.
pub struct PageSpec;

impl PageSpec {
    /// `"1-3,5,8-"` → indices 0-based, triés, sans doublon. Bornes : `1..=page_count`.
    ///
    /// \[Apprentissage\] `BTreeSet` garde ses éléments triés et uniques ; collecter dedans
    /// puis `into_iter().collect()` donne directement le `Vec` attendu.
    pub fn parse(spec: &str, page_count: u16) -> CoreResult<Vec<u16>> {
        let groups = Self::ranges(spec, page_count)?;
        let set: BTreeSet<u16> = groups.into_iter().flatten().collect();
        Ok(set.into_iter().collect())
    }

    /// Comme [`parse`](Self::parse), mais garde un groupe par partie séparée par des virgules
    /// (pour scinder : « 1-2,3-5 » → deux documents).
    pub fn ranges(spec: &str, page_count: u16) -> CoreResult<Vec<Vec<u16>>> {
        if spec.trim().is_empty() {
            return Err(CoreError::InvalidPageSpec(
                "aucune page indiquée".to_string(),
            ));
        }
        spec.split(',')
            .map(|part| Self::parse_part(part.trim(), page_count))
            .collect()
    }

    /// Groupes de `n` pages consécutives ; `n = 0` ou `n >= page_count` → un seul groupe.
    pub fn every(n: u16, page_count: u16) -> Vec<Vec<u16>> {
        let all: Vec<u16> = (0..page_count).collect();
        if n == 0 || n >= page_count {
            return vec![all];
        }
        all.chunks(n as usize).map(|c| c.to_vec()).collect()
    }

    /// Découpe `0..page_count` en parties commençant aux index `starts` (ex. pages visées par
    /// les signets de premier niveau). Les pages avant le premier début forment une partie ;
    /// doublons et index hors bornes sont ignorés.
    pub fn split_at(starts: &[u16], page_count: u16) -> Vec<Vec<u16>> {
        let mut bounds: BTreeSet<u16> =
            starts.iter().copied().filter(|&s| s < page_count).collect();
        bounds.insert(0);
        let bounds: Vec<u16> = bounds.into_iter().collect();
        bounds
            .iter()
            .enumerate()
            .map(|(i, &start)| {
                let end = bounds.get(i + 1).copied().unwrap_or(page_count);
                (start..end).collect()
            })
            .filter(|part: &Vec<u16>| !part.is_empty())
            .collect()
    }

    /// Une partie : `"5"`, `"1-3"`, `"8-"` (jusqu'à la fin) ou `"-2"` (depuis le début).
    fn parse_part(part: &str, page_count: u16) -> CoreResult<Vec<u16>> {
        let invalid = |why: &str| CoreError::InvalidPageSpec(format!("« {part} » : {why}"));
        if part.is_empty() {
            return Err(invalid("vide"));
        }
        let number = |text: &str| -> CoreResult<u16> {
            let n: u16 = text.trim().parse().map_err(|_| invalid("nombre attendu"))?;
            if n == 0 || n > page_count {
                return Err(invalid(&format!("les pages vont de 1 à {page_count}")));
            }
            Ok(n)
        };
        let mut pieces = part.splitn(3, '-');
        let (first, second, extra) = (pieces.next(), pieces.next(), pieces.next());
        if extra.is_some() {
            return Err(invalid("un seul tiret par plage"));
        }
        let (start, end) = match (first.map(str::trim), second.map(str::trim)) {
            (Some(single), None) => {
                let n = number(single)?;
                (n, n)
            }
            (Some(""), Some("")) => return Err(invalid("plage vide")),
            (Some(""), Some(to)) => (1, number(to)?),
            (Some(from), Some("")) => (number(from)?, page_count),
            (Some(from), Some(to)) => (number(from)?, number(to)?),
            (None, _) => return Err(invalid("vide")),
        };
        if start > end {
            return Err(invalid("le début dépasse la fin"));
        }
        Ok((start - 1..end).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoreError;

    #[test]
    fn parses_ranges_and_singletons() {
        assert_eq!(PageSpec::parse("1-3,5", 10).unwrap(), vec![0, 1, 2, 4]);
        assert_eq!(PageSpec::parse("5, 1-2, 2", 10).unwrap(), vec![0, 1, 4]);
        assert_eq!(PageSpec::parse("3", 3).unwrap(), vec![2]);
    }

    #[test]
    fn open_range_to_end_or_from_start() {
        assert_eq!(PageSpec::parse("8-", 10).unwrap(), vec![7, 8, 9]);
        assert_eq!(PageSpec::parse("-2", 10).unwrap(), vec![0, 1]);
    }

    #[test]
    fn rejects_out_of_bounds_and_garbage() {
        for bad in ["0", "4", "a-b", "", "3-1", "1--2", ",", "1-2-3"] {
            assert!(
                matches!(PageSpec::parse(bad, 3), Err(CoreError::InvalidPageSpec(_))),
                "« {bad} » aurait dû être refusé"
            );
        }
    }

    #[test]
    fn every_n_groups() {
        assert_eq!(PageSpec::every(2, 5), vec![vec![0, 1], vec![2, 3], vec![4]]);
        assert_eq!(PageSpec::every(0, 3), vec![vec![0, 1, 2]]);
        assert_eq!(PageSpec::every(10, 3), vec![vec![0, 1, 2]]);
    }

    #[test]
    fn ranges_for_split() {
        assert_eq!(
            PageSpec::ranges("1-2, 3-5", 5).unwrap(),
            vec![vec![0, 1], vec![2, 3, 4]]
        );
        assert!(PageSpec::ranges("1-9", 5).is_err());
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn split_at_boundaries() {
        // Débuts de parties en pages 0 et 2 (ex. signets) sur 5 pages → [0,1] [2,3,4].
        assert_eq!(
            PageSpec::split_at(&[0, 2], 5),
            vec![vec![0, 1], vec![2, 3, 4]]
        );
        // Une première partie « avant le premier signet » si celui-ci n'est pas en page 0.
        assert_eq!(
            PageSpec::split_at(&[2, 4], 5),
            vec![vec![0, 1], vec![2, 3], vec![4]]
        );
        // Doublons et hors bornes ignorés ; aucun début valide → tout le document.
        assert_eq!(PageSpec::split_at(&[9, 9], 3), vec![vec![0, 1, 2]]);
    }
}
