// SPDX-License-Identifier: GPL-3.0-or-later
// Politique d'ouverture des liens externes trouvés dans un PDF : un document est une
// donnée non fiable, seuls les schémas web et courriel sont ouverts (après confirmation).

const ALLOWED_SCHEMES = ['http:', 'https:', 'mailto:'];

export function externalLinkPolicy(uri: string): 'open' | 'blocked' {
  if (uri !== uri.trim() || uri === '') return 'blocked';
  const colon = uri.indexOf(':');
  if (colon <= 0) return 'blocked';
  const scheme = uri.slice(0, colon + 1).toLowerCase();
  return ALLOWED_SCHEMES.includes(scheme) ? 'open' : 'blocked';
}
