// SPDX-License-Identifier: GPL-3.0-or-later
// Construction des URL du protocole `mega://` qui sert les pages rendues.

import { convertFileSrc } from '@tauri-apps/api/core';
import type { PageSize, Priority, Rotation } from './api';
import { MAX_SCALE } from './logic/zoom';

/** Densité d'écran au-delà de laquelle on ne rend pas plus fin (coût mémoire × densité²). */
const MAX_DENSITY = 3;

/** Plafond de pixels d'un rendu, le même que le backend (`MAX_RENDER_PIXELS`, 50 Mpx). */
const MAX_RENDER_PIXELS = 50_000_000;

/**
 * Échelle de **rendu** d'une page affichée à l'échelle `scale` sur un écran de densité `dpr`
 * (`window.devicePixelRatio` : 1,5 à 150 %, 2 à 200 %) (Lot 13, audit P3). L'image est demandée
 * en pixels physiques puis affichée à sa taille CSS : nette au lieu d'être agrandie et floue.
 * Jamais moins que la taille CSS, jamais plus que la borne du backend ; avec la taille de la
 * page (`size`, en points), la densité baisse pour rester sous le plafond de pixels du backend,
 * qui refuserait sinon un grand plan ou une affiche à un zoom ordinaire (relecture de la PR C).
 */
export function renderScale(scale: number, dpr: number, size?: PageSize): number {
  const density = Math.min(MAX_DENSITY, Math.max(1, dpr));
  let wanted = Math.min(MAX_SCALE, scale * density);
  if (size && size.width_pt > 0 && size.height_pt > 0) {
    wanted = Math.min(wanted, Math.sqrt(MAX_RENDER_PIXELS / (size.width_pt * size.height_pt)));
  }
  // Arrondi vers le bas : un millième de trop suffirait à dépasser le plafond.
  return Math.max(scale, Math.floor(wanted * 1000) / 1000);
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
