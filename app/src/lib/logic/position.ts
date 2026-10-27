// SPDX-License-Identifier: GPL-3.0-or-later
// Où rouvrir un fichier : à la position mémorisée ou avec le zoom par défaut (logique pure).

/** Zoom de départ d'un document, tel que le stockent les préférences. */
export type ZoomModeLike = 'custom' | 'fitWidth' | 'fitPage';

/** Page et zoom d'ouverture d'un document. */
export interface StartPosition {
  page: number;
  scale: number;
  zoomMode: ZoomModeLike;
}

/** La partie des préférences qui décide de la position d'ouverture. */
export interface PositionSettings {
  scale: number;
  zoomMode: ZoomModeLike;
  restorePositions: boolean;
  positions: Record<string, StartPosition>;
}

/**
 * Position d'ouverture de `path` : la dernière position mémorisée si la préférence
 * « rouvrir à la dernière position » est active et que le fichier est connu, sinon la
 * première page avec le zoom par défaut.
 */
export function initialPosition(s: PositionSettings, path: string | null): StartPosition {
  const remembered = s.restorePositions && path ? s.positions[path] : undefined;
  if (remembered)
    return { page: remembered.page, scale: remembered.scale, zoomMode: remembered.zoomMode };
  return { page: 0, scale: s.scale, zoomMode: s.zoomMode };
}
