// SPDX-License-Identifier: GPL-3.0-or-later
// Densité de l'écran (`devicePixelRatio`), réactive (Lot 13, audit P3) : elle change quand la
// fenêtre passe sur un autre écran ou quand l'utilisateur modifie la mise à l'échelle de
// Windows. Les pages se redemandent alors à la nouvelle résolution.

export const screen = $state({ dpr: 1 });

/**
 * Lit la densité et la suit : `matchMedia('(resolution: …dppx)')` prévient quand elle quitte
 * la valeur courante ; on se réabonne alors pour la nouvelle valeur.
 */
export function watchScreenDensity(): void {
  if (typeof window === 'undefined') return;
  const update = () => {
    screen.dpr = window.devicePixelRatio || 1;
    window
      .matchMedia(`(resolution: ${screen.dpr}dppx)`)
      .addEventListener('change', update, { once: true });
  };
  update();
}
