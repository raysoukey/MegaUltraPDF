// SPDX-License-Identifier: GPL-3.0-or-later
// Navigation au clavier dans un menu déroulant (motif ARIA « menu ») : logique pure.

/** Ce qu'une touche fait dans un menu ouvert. */
export type MenuMove =
  { index: number } | { close: true; refocus: boolean } | { activate: true } | null;

/** Indice utilisable suivant (ou précédent) à partir de `from`, en bouclant ; -1 si aucun. */
function step(from: number, enabled: boolean[], dir: 1 | -1): number {
  const n = enabled.length;
  for (let k = 1; k <= n; k++) {
    const i = (((from + dir * k) % n) + n) % n;
    if (enabled[i]) return i;
  }
  return -1;
}

/**
 * Réaction à une touche : ↓ ↑ (en bouclant, éléments désactivés sautés), Début / Fin,
 * Échap ferme en rendant le focus au bouton, Tab ferme en laissant le focus suivre son cours,
 * Entrée et Espace activent l'élément courant, une lettre va à l'élément suivant qui commence
 * par elle (`labels`, C27).
 */
export function menuKey(
  key: string,
  index: number,
  enabled: boolean[],
  labels: string[] = [],
): MenuMove {
  const pick = (i: number): MenuMove => (i >= 0 ? { index: i } : null);
  switch (key) {
    case 'ArrowDown':
      return pick(step(index < 0 ? -1 : index, enabled, 1));
    case 'ArrowUp':
      return pick(step(index < 0 ? enabled.length : index, enabled, -1));
    case 'Home':
      return pick(step(-1, enabled, 1));
    case 'End':
      return pick(step(enabled.length, enabled, -1));
    case 'Escape':
      return { close: true, refocus: true };
    case 'Tab':
      return { close: true, refocus: false };
    case 'Enter':
    case ' ':
      return { activate: true };
    default:
      return key.length === 1 && key.trim() !== '' ? typeAhead(key, index, enabled, labels) : null;
  }
}

/** Élément actif suivant (en bouclant) dont le libellé commence par `letter`. */
function typeAhead(letter: string, index: number, enabled: boolean[], labels: string[]): MenuMove {
  const n = labels.length;
  const wanted = letter.toLocaleLowerCase();
  for (let k = 1; k <= n; k++) {
    const i = (((index + k) % n) + n) % n;
    if (enabled[i] && labels[i].trim().toLocaleLowerCase().startsWith(wanted)) return { index: i };
  }
  return null;
}
