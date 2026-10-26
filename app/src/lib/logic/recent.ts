// SPDX-License-Identifier: GPL-3.0-or-later
// Liste des fichiers récents : le plus récent d'abord, sans doublon, bornée.

export const MAX_RECENT = 10;

/** Nouvelle liste avec `path` en tête ; l'entrée d'origine n'est pas modifiée. */
export function pushRecent(list: readonly string[], path: string, max = MAX_RECENT): string[] {
  return [path, ...list.filter((p) => p !== path)].slice(0, max);
}
