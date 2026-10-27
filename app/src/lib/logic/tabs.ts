// SPDX-License-Identifier: GPL-3.0-or-later
// Navigation au clavier dans une liste d'onglets (motif ARIA « tabs ») : logique pure.

/**
 * Onglet visé par une touche : ← → (en bouclant), Début, Fin ; `null` pour toute autre
 * touche (C28, Lot 9).
 */
export function tabKey(key: string, index: number, count: number): number | null {
  if (count === 0) return null;
  switch (key) {
    case 'ArrowRight':
      return (index + 1) % count;
    case 'ArrowLeft':
      return (index - 1 + count) % count;
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    default:
      return null;
  }
}
