// SPDX-License-Identifier: GPL-3.0-or-later
// Choix de la langue de l'interface : logique pure, sans DOM.

/** Langues disponibles. */
export type Language = 'fr' | 'en';

/** Préférence de l'utilisateur : une langue, ou celle du système. */
export type LanguagePreference = 'system' | Language;

/**
 * Langue à afficher : le choix explicite l'emporte ; « système » donne le français pour toute
 * variante française (`fr`, `fr-CA`, `fr-BE`…) et l'anglais pour tout le reste.
 */
export function resolveLanguage(pref: LanguagePreference, systemLanguage: string): Language {
  if (pref === 'fr' || pref === 'en') return pref;
  return systemLanguage.toLowerCase().startsWith('fr') ? 'fr' : 'en';
}
