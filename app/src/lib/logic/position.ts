// SPDX-License-Identifier: GPL-3.0-or-later
// Où rouvrir un fichier : à la position mémorisée ou avec le zoom par défaut (logique pure).

import type { ZoomMode } from '$lib/state/settings.svelte';

/** Page et zoom d'ouverture d'un document. */
export interface StartPosition {
  page: number;
  scale: number;
  zoomMode: ZoomMode;
}

/** La partie des préférences qui décide de la position d'ouverture. */
export interface PositionSettings {
  /** Dernier zoom utilisé (mis à jour à chaque zoom). */
  scale: number;
  /** Zoom choisi dans les préférences pour ouvrir un fichier inconnu. */
  defaultZoomMode: ZoomMode;
  restorePositions: boolean;
  positions: Record<string, StartPosition>;
}

/**
 * Position d'ouverture de `path` : la dernière position mémorisée si la préférence
 * « rouvrir à la dernière position » est active et que le fichier est connu, sinon la
 * première page avec le zoom choisi pour l'ouverture (« dernier zoom utilisé » : `custom`
 * avec la dernière échelle).
 */
export function initialPosition(s: PositionSettings, path: string | null): StartPosition {
  const remembered = s.restorePositions && path ? s.positions[path] : undefined;
  if (remembered)
    return { page: remembered.page, scale: remembered.scale, zoomMode: remembered.zoomMode };
  return { page: 0, scale: s.scale, zoomMode: s.defaultZoomMode };
}

/**
 * Garde les `keep` positions les plus récemment mémorisées (ordre d'insertion des clés :
 * mémoriser une position la replace en dernier). Sans borne, les préférences grossiraient
 * à chaque fichier ouvert (A20).
 */
export function boundPositions<T>(positions: Record<string, T>, keep = 100): Record<string, T> {
  const keys = Object.keys(positions);
  if (keys.length <= keep) return positions;
  return Object.fromEntries(keys.slice(keys.length - keep).map((k) => [k, positions[k]]));
}
