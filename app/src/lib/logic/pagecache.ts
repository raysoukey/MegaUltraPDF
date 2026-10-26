// SPDX-License-Identifier: GPL-3.0-or-later
// Bornage des caches par page (couche de texte, liens) : on ne garde que les pages les
// plus proches de la page courante, pour que la mémoire ne croisse pas avec la lecture.

/** Pages à retirer pour n'en garder que `keep`, les plus proches de `current` d'abord. */
export function pagesToEvict(cached: number[], current: number, keep: number): number[] {
  if (cached.length <= keep) return [];
  const byDistance = [...cached].sort((a, b) => Math.abs(a - current) - Math.abs(b - current));
  return byDistance.slice(keep);
}
