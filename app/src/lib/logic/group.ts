// SPDX-License-Identifier: GPL-3.0-or-later
// Exécuter une suite d'éditions dans un groupe d'annulation (logique pure, testée).

/**
 * Ouvre le groupe (`begin`), exécute `body`, puis ferme le groupe (`end`) **quoi qu'il
 * arrive** : arrêt demandé, erreur d'une étape… Un groupe resté ouvert bloquerait annuler.
 */
export async function inGroup<T>(
  begin: () => Promise<void>,
  body: () => Promise<T>,
  end: () => Promise<void>,
): Promise<T> {
  await begin();
  try {
    return await body();
  } finally {
    await end();
  }
}
