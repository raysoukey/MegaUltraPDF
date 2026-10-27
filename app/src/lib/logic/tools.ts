// SPDX-License-Identifier: GPL-3.0-or-later
// Machine à états des outils d'annotation : un « brouillon » (points en coordonnées PDF)
// commence au premier geste, s'étend au fil du pointeur et se termine en une demande de
// création que le backend applique. Logique pure, sans DOM.

import type { Geometry, NewAnnotation, Style } from '$lib/api';

export type Tool =
  | 'select'
  | 'note'
  | 'highlight'
  | 'underline'
  | 'strikeout'
  | 'text'
  | 'ink'
  | 'rect'
  | 'ellipse'
  | 'line'
  | 'arrow'
  | 'stamp'
  | 'signature';

export interface Pt {
  x: number;
  y: number;
}

/** Un tracé en cours : l'outil et ses points (PDF). */
export interface Draft {
  tool: Tool;
  points: Pt[];
}

/** Outils qui dessinent sur le calque (les autres agissent sur la sélection de texte). */
export function isDrawingTool(tool: Tool): boolean {
  return !(
    tool === 'select' ||
    tool === 'highlight' ||
    tool === 'underline' ||
    tool === 'strikeout'
  );
}

export function beginDraft(tool: Tool, p: Pt): Draft | null {
  if (!isDrawingTool(tool)) return null;
  return { tool, points: [{ ...p }] };
}

/** Encre : ajoute le point s'il est à au moins 1 pt du précédent ; sinon remplace le 2ᵉ point. */
export function extendDraft(draft: Draft, p: Pt): Draft {
  if (draft.tool === 'ink') {
    const last = draft.points[draft.points.length - 1];
    if (Math.hypot(p.x - last.x, p.y - last.y) < 1) return draft;
    return { tool: draft.tool, points: [...draft.points, { ...p }] };
  }
  return { tool: draft.tool, points: [draft.points[0], { ...p }] };
}

function cornersToRect(a: Pt, b: Pt) {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.abs(a.x - b.x), h: Math.abs(a.y - b.y) };
}

const MIN_SIZE = 2;

/** Termine le brouillon ; `null` s'il est trop petit ou incomplet. */
export function finishDraft(
  draft: Draft,
  style: Style,
  contents: string,
  author: string,
): NewAnnotation | null {
  const [first, second] = draft.points;
  let geometry: Geometry;
  switch (draft.tool) {
    case 'note':
      if (contents.trim() === '') return null;
      geometry = { kind: 'rect', rect: { x: first.x, y: first.y, w: 0, h: 0 } };
      break;
    case 'stamp': {
      if (contents.trim() === '') return null;
      const w = Math.max(120, contents.trim().length * style.font_size * 0.7 + 20);
      const h = Math.max(36, style.font_size * 2.2);
      geometry = { kind: 'rect', rect: { x: first.x - w / 2, y: first.y - h / 2, w, h } };
      break;
    }
    case 'text':
    case 'rect':
    case 'ellipse': {
      if (!second) return null;
      const rect = cornersToRect(first, second);
      if (Math.max(rect.w, rect.h) < MIN_SIZE) return null;
      if (draft.tool === 'text' && contents.trim() === '') return null;
      geometry = { kind: 'rect', rect };
      break;
    }
    case 'line':
    case 'arrow': {
      if (!second) return null;
      if (Math.hypot(second.x - first.x, second.y - first.y) < MIN_SIZE) return null;
      geometry = { kind: 'line', from: [first.x, first.y], to: [second.x, second.y] };
      break;
    }
    case 'ink': {
      if (draft.points.length < 2) return null;
      geometry = { kind: 'strokes', strokes: [draft.points.map((p) => [p.x, p.y])] };
      break;
    }
    default:
      return null;
  }
  return { kind: draft.tool, geometry, style, contents, author };
}
