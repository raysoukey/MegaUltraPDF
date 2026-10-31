// SPDX-License-Identifier: GPL-3.0-or-later
/** Un point PDF vaut 1/72 de pouce, soit 25,4 / 72 millimètres. */
const MM_PER_POINT = 25.4 / 72;

/** Espace fine insécable : séparateur de milliers lisible dans toutes les langues. */
const THIN_NBSP = ' ';

function millimetres(points: number, userUnit: number): string {
  const mm = Math.round(points * userUnit * MM_PER_POINT);
  return String(mm).replace(/\B(?=(\d{3})+(?!\d))/g, THIN_NBSP);
}

/**
 * Taille physique d'une page en millimètres : sa taille en points multipliée par le
 * `/UserUnit` (§7.7.3.3). PDFium ignore cette unité, Acrobat l'applique ; l'afficher évite
 * de présenter un A4 agrandi dix fois comme une page minuscule.
 */
export function physicalSize(widthPt: number, heightPt: number, userUnit: number): string {
  return `${millimetres(widthPt, userUnit)} × ${millimetres(heightPt, userUnit)} mm`;
}
