// SPDX-License-Identifier: GPL-3.0-or-later
// Construction des URL du protocole `mega://` qui sert les pages rendues.

import { convertFileSrc } from '@tauri-apps/api/core';

/** Chemin attendu par le backend : `page/<id>/<index>` (index à partir de 0). */
export function pagePath(docId: number, index: number): string {
  return `page/${docId}/${index}`;
}

/**
 * URL complète d'une page rendue à l'échelle `scale`.
 *
 * `convertFileSrc` produit la forme propre à chaque plateforme
 * (`http://mega.localhost/…` sous Windows, `mega://localhost/…` ailleurs)
 * et encode le chemin ; le backend le décode.
 */
export function pageUrl(docId: number, index: number, scale: number): string {
  return `${convertFileSrc(pagePath(docId, index), 'mega')}?scale=${scale}`;
}
