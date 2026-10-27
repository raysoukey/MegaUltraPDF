// SPDX-License-Identifier: GPL-3.0-or-later
// Outil d'annotation actif et style courant, par famille d'outils (les valeurs par défaut
// diffèrent : un surlignage est jaune et translucide, une forme rouge et opaque…).

import type { Color, Style } from '$lib/api';
import type { Tool } from '$lib/logic/tools';

export type StyleFamily = 'markup' | 'shape' | 'text' | 'note';

export function familyOf(tool: Tool): StyleFamily {
  switch (tool) {
    case 'highlight':
    case 'underline':
    case 'strikeout':
      return 'markup';
    case 'text':
    case 'stamp':
      return 'text';
    case 'note':
      return 'note';
    default:
      return 'shape';
  }
}

const YELLOW: Color = { r: 255, g: 212, b: 0 };
const RED: Color = { r: 229, g: 57, b: 53 };
const BLUE: Color = { r: 30, g: 136, b: 229 };

const DEFAULTS: Record<StyleFamily, Style> = {
  markup: { color: YELLOW, fill: null, opacity: 0.5, stroke_width: 1, font_size: 12 },
  shape: { color: RED, fill: null, opacity: 1, stroke_width: 2, font_size: 12 },
  text: { color: BLUE, fill: null, opacity: 1, stroke_width: 1, font_size: 14 },
  note: { color: YELLOW, fill: null, opacity: 1, stroke_width: 1, font_size: 12 },
};

/** Libellés des tampons prédéfinis. */
export const STAMPS = ['APPROUVÉ', 'BROUILLON', 'CONFIDENTIEL', 'REFUSÉ', 'VU'] as const;

interface ToolState {
  tool: Tool;
  styles: Record<StyleFamily, Style>;
  stamp: string;
}

export const toolState = $state<ToolState>({
  tool: 'select',
  styles: structuredClone(DEFAULTS),
  stamp: STAMPS[1],
});

export function setTool(tool: Tool): void {
  toolState.tool = tool;
}

/** Style de l'outil courant (ou de l'outil donné). */
export function currentStyle(tool: Tool = toolState.tool): Style {
  return toolState.styles[familyOf(tool)];
}

/** Modifie le style de la famille de l'outil courant. */
export function updateStyle(patch: Partial<Style>): void {
  const family = familyOf(toolState.tool);
  toolState.styles[family] = { ...toolState.styles[family], ...patch };
}

export function setStamp(label: string): void {
  toolState.stamp = label;
}
