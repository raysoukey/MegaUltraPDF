// SPDX-License-Identifier: GPL-3.0-or-later
// Logique pure autour de l'export, de l'impression et de la comparaison : préfixe de
// fichier, bornes des réglages, échelle d'impression, lecture du tampon de comparaison.

import type { ComparedPages } from '$lib/api';

/** Préfixe proposé pour les images exportées : nom du fichier, sinon titre, sinon « page ». */
export function defaultPrefix(path: string | null, title: string | null): string {
  const stem = path
    ?.split(/[\\/]/)
    .pop()
    ?.replace(/\.pdf$/i, '');
  const raw = (stem && stem.trim()) || (title && title.trim()) || 'page';
  return raw.replace(/[\\/:*?"<>|]+/g, '-').trim() || 'page';
}

/** Réglages d'un export d'images, tels que saisis. */
export interface ExportSettings {
  dpi: number;
  quality: number;
  prefix: string;
}

/** Les réglages hors bornes (`'dpi'`, `'quality'`, `'prefix'`) ; vide si tout va bien. */
export function exportProblems(s: ExportSettings): string[] {
  const problems: string[] = [];
  if (!Number.isFinite(s.dpi) || s.dpi < 36 || s.dpi > 600) problems.push('dpi');
  if (!Number.isFinite(s.quality) || s.quality < 10 || s.quality > 100) problems.push('quality');
  if (s.prefix.trim() === '' || /[\\/]/.test(s.prefix)) problems.push('prefix');
  return problems;
}

/** Échelle de rendu pour une résolution (72 dpi = 1). */
export function printScale(dpi: number): number {
  return dpi / 72;
}

/**
 * Décode la réponse binaire de `compare_pages` : 24 octets de compteurs (deux entiers
 * 64 bits puis deux entiers 32 bits, petit-boutiste) suivis de l'image PNG.
 */
export function parseCounters(raw: ArrayBuffer): ComparedPages {
  const view = new DataView(raw);
  return {
    differing_pixels: Number(view.getBigUint64(0, true)),
    total_pixels: Number(view.getBigUint64(8, true)),
    width_px: view.getUint32(16, true),
    height_px: view.getUint32(20, true),
    png: new Uint8Array(raw).slice(24),
  };
}
