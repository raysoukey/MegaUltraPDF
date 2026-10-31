// SPDX-License-Identifier: GPL-3.0-or-later
// Réordonner les pages en glissant leurs miniatures (Lot 13, audit P1) : par événements
// **pointeur**, pas par glisser-déposer HTML5. Sous Windows, Tauri intercepte le
// glisser-déposer natif de la fenêtre (pour ouvrir un fichier déposé), et le glisser-déposer
// HTML5 ne reçoit alors jamais ses événements ; les événements pointeur, eux, arrivent
// toujours, avec la souris, le stylet ou le doigt.

/** Distance (pixels CSS) à parcourir, bouton enfoncé, avant qu'un clic devienne un glisser. */
export const DRAG_THRESHOLD = 5;

/** Vrai si le pointeur, enfoncé en (`x0`, `y0`), est allé assez loin pour glisser. */
export function isDrag(x0: number, y0: number, x: number, y: number): boolean {
  return Math.hypot(x - x0, y - y0) > DRAG_THRESHOLD;
}
