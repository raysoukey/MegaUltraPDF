// SPDX-License-Identifier: GPL-3.0-or-later
// Toutes les chaînes visibles de l'interface, en un seul endroit.
// Aucun composant ne contient de texte en dur : le Lot 7 branchera ici la traduction FR/EN.

export const fr = {
  appName: 'MegaUltraPDF',
  openFile: 'Ouvrir un PDF…',
  noDocument: 'Aucun document ouvert.',
  untitled: '(sans titre)',
  pages: (n: number) => `${n} page${n > 1 ? 's' : ''}`,
  errorPrefix: 'Erreur : ',
} as const;

export const t = fr;
