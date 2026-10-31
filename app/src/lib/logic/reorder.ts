// SPDX-License-Identifier: GPL-3.0-or-later
// Réordonner les pages en glissant leurs miniatures (Lot 13, audit P1) : par événements
// **pointeur**, pas par glisser-déposer HTML5. Sous Windows, Tauri intercepte le
// glisser-déposer natif de la fenêtre (pour ouvrir un fichier déposé), et le glisser-déposer
// HTML5 ne reçoit alors jamais ses événements ; les événements pointeur, eux, arrivent
// toujours. Souris et stylet glissent ; au doigt, le geste fait défiler la liste (le
// navigateur garde le défilement tactile), comme dans la plupart des visionneuses.

/** Distance (pixels CSS) à parcourir, bouton enfoncé, avant qu'un clic devienne un glisser. */
export const DRAG_THRESHOLD = 5;

/** Largeur (pixels CSS) des bandes, en haut et en bas de la liste, qui la font défiler. */
export const EDGE = 30;

/** Défilement maximal par mouvement du pointeur, en pixels CSS. */
const MAX_STEP = 24;

/** Vrai si le pointeur, enfoncé en (`x0`, `y0`), est allé assez loin pour glisser. */
export function isDrag(x0: number, y0: number, x: number, y: number): boolean {
  return Math.hypot(x - x0, y - y0) > DRAG_THRESHOLD;
}

/**
 * Défilement à appliquer à la liste (pixels, négatif vers le haut) quand le pointeur, à
 * l'ordonnée `y`, glisse près du bord haut (`top`) ou bas (`bottom`) de la partie visible :
 * d'autant plus vite qu'il en est près, ou au-delà. Sans cela, une page lointaine d'un long
 * document ne pourrait pas être atteinte en glissant.
 */
export function edgeScroll(y: number, top: number, bottom: number): number {
  const speed = (depth: number) => Math.round(Math.min(1, depth / EDGE) * MAX_STEP);
  if (y < top + EDGE) return -Math.max(1, speed(top + EDGE - y));
  if (y > bottom - EDGE) return Math.max(1, speed(y - (bottom - EDGE)));
  return 0;
}
