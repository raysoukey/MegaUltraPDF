// SPDX-License-Identifier: GPL-3.0-or-later
// Construction des URL du protocole `mega://` qui sert les pages rendues.

import { convertFileSrc } from '@tauri-apps/api/core';
import type { Priority, Rotation } from './api';
import { MAX_SCALE } from './logic/zoom';

/** Densité d'écran au-delà de laquelle on ne rend pas plus fin (coût mémoire × densité²). */
const MAX_DENSITY = 3;

/**
 * Échelle de **rendu** d'une page affichée à l'échelle `scale` sur un écran de densité `dpr`
 * (`window.devicePixelRatio` : 1,5 à 150 %, 2 à 200 %) (Lot 13, audit P3). L'image est demandée
 * en pixels physiques puis affichée à sa taille CSS : nette au lieu d'être agrandie et floue.
 * Jamais moins que la taille CSS, jamais plus que la borne du backend.
 */
export function renderScale(scale: number, dpr: number): number {
  const density = Math.min(MAX_DENSITY, Math.max(1, dpr));
  return Math.min(MAX_SCALE, Math.round(scale * density * 1000) / 1000);
}

const ROTATION_DEGREES: Record<Rotation, number> = { none: 0, cw90: 90, cw180: 180, cw270: 270 };

/** Chemin attendu par le backend : `page/<id>/<index>` (index à partir de 0). */
export function pagePath(docId: number, index: number): string {
  return `page/${docId}/${index}`;
}

/**
 * URL complète d'une page rendue.
 *
 * `convertFileSrc` produit la forme propre à chaque plateforme
 * (`http://mega.localhost/…` sous Windows, `mega://localhost/…` ailleurs)
 * et encode le chemin ; le backend le décode. La génération permet au backend
 * d'ignorer les demandes périmées (409) après un changement de zoom.
 */
export function pageUrl(
  docId: number,
  index: number,
  scale: number,
  rotation: Rotation = 'none',
  generation = 0,
  priority: Priority = 'visible',
): string {
  const query = `scale=${scale}&rot=${ROTATION_DEGREES[rotation]}&gen=${generation}&prio=${priority}`;
  return `${convertFileSrc(pagePath(docId, index), 'mega')}?${query}`;
}
