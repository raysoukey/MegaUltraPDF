// SPDX-License-Identifier: GPL-3.0-or-later
// Règles d'ouverture de plusieurs fichiers (logique pure, testée ; Lot 11).

/** L'invite de mot de passe en cours, si une ouverture en a demandé une. */
export interface PasswordPrompt {
  path: string;
  error: string | null;
}

/**
 * L'invite à garder après l'ouverture réussie de `opened` : seule celle **de ce fichier**
 * se ferme. Plusieurs fichiers ouverts depuis le système passent l'un après l'autre ; un
 * fichier chiffré suivi d'un fichier ordinaire ne doit pas perdre sa demande de mot de passe.
 */
export function promptAfterOpen(
  prompt: PasswordPrompt | null,
  opened: string,
): PasswordPrompt | null {
  return prompt && prompt.path !== opened ? prompt : null;
}
