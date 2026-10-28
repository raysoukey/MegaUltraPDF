// SPDX-License-Identifier: GPL-3.0-or-later
// Choix des pages à reconnaître (logique pure) : le cœur refuse une page qui a déjà du texte,
// on ne lui envoie donc que des pages sans texte.

/** Pages à reconnaître : toutes celles sans texte, ou la page courante. */
export type OcrScope = 'untexted' | 'current';

/**
 * Bilan d'une reconnaissance : parmi les pages traitées, celles qui ont maintenant du texte
 * (`after[i]`) et celles où rien n'a été reconnu (le cœur n'y ajoute alors aucune étape).
 */
export function ocrOutcome(
  processed: number[],
  after: boolean[],
): { recognized: number; empty: number } {
  const recognized = processed.filter((page) => after[page] === true).length;
  return { recognized, empty: processed.length - recognized };
}

/** Indices des pages à envoyer à l'OCR, d'après `pagesWithText[i]` (la page a du texte). */
export function ocrPages(scope: OcrScope, current: number, pagesWithText: boolean[]): number[] {
  if (scope === 'current') {
    return current < pagesWithText.length && !pagesWithText[current] ? [current] : [];
  }
  return pagesWithText.flatMap((hasText, i) => (hasText ? [] : [i]));
}
