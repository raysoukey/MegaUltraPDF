// SPDX-License-Identifier: GPL-3.0-or-later
// Construction des URL du protocole `mega://` qui sert les pages rendues.

import { convertFileSrc } from '@tauri-apps/api/core';
import type { Priority, Rotation } from './api';

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
