// SPDX-License-Identifier: GPL-3.0-or-later
// File d'exécution en série (logique pure) : les éditions, annulations et rétablissements
// d'un document passent l'un après l'autre (A23, Lot 9).

/**
 * Une file : chaque tâche commence quand la précédente est finie (réussie ou non). Deux
 * Ctrl+Z rapides annulent ainsi deux fois, le second voyant l'état laissé par le premier.
 */
export function serialQueue(): <T>(task: () => Promise<T>) => Promise<T> {
  let tail: Promise<unknown> = Promise.resolve();
  return <T>(task: () => Promise<T>): Promise<T> => {
    const run = tail.then(() => task());
    tail = run.catch(() => undefined);
    return run;
  };
}
