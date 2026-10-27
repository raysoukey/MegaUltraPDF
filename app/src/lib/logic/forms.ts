// SPDX-License-Identifier: GPL-3.0-or-later
// Logique d'interface des formulaires : champs requis vides, type de contrôle HTML,
// ajustement d'une image et conversion des traits d'un canvas vers les points PDF.
// Les règles métier (validation, écriture) restent en Rust.

import type { FormField, Rect } from '$lib/api';

export type InputType = 'text' | 'password' | 'textarea' | 'checkbox' | 'radio' | 'select' | 'none';

/** Champs requis sans valeur (texte vide ou blanc, case non cochée). */
export function missingRequired(fields: FormField[]): FormField[] {
  const seenGroups = new Set<string>();
  return fields.filter((f) => {
    if (!f.required) return false;
    if (f.kind === 'radio') {
      // Un groupe requis compte une fois : il manque si aucun de ses boutons n'est coché.
      if (seenGroups.has(f.name)) return false;
      seenGroups.add(f.name);
      return !fields.some((g) => g.kind === 'radio' && g.name === f.name && g.checked === true);
    }
    if (f.kind === 'checkbox') return f.checked !== true;
    return (f.value ?? '').trim() === '';
  });
}

/** Contrôle HTML à utiliser pour un champ. */
export function fieldInputType(field: FormField): InputType {
  switch (field.kind) {
    case 'text':
      if (field.password) return 'password';
      return field.multiline ? 'textarea' : 'text';
    case 'checkbox':
      return 'checkbox';
    case 'radio':
      return 'radio';
    case 'combo':
    case 'list':
      return 'select';
    default:
      return 'none';
  }
}

/** Ajuste une image `w × h` dans `rect` en gardant les proportions, centrée (même règle que Rust). */
export function fitRect(imageW: number, imageH: number, rect: Rect): Rect {
  const scale = Math.min(rect.w / imageW, rect.h / imageH);
  const w = imageW * scale;
  const h = imageH * scale;
  return { x: rect.x + (rect.w - w) / 2, y: rect.y + (rect.h - h) / 2, w, h };
}

/**
 * Traits dessinés sur un canvas (pixels, origine en haut à gauche) → points PDF dans `rect`
 * (origine en bas à gauche), proportions gardées, dessin centré ; les traits de moins de
 * deux points sont ignorés.
 */
export function canvasStrokesToPdf(
  strokes: { x: number; y: number }[][],
  canvas: { w: number; h: number },
  rect: Rect,
): [number, number][][] {
  const target = fitRect(canvas.w, canvas.h, rect);
  const scale = target.w / canvas.w;
  return strokes
    .filter((s) => s.length >= 2)
    .map((s) =>
      s.map((p) => [target.x + p.x * scale, target.y + target.h - p.y * scale] as [number, number]),
    );
}
