// SPDX-License-Identifier: GPL-3.0-or-later
// Verdict lisible d'une signature numérique (logique pure, testée).
import type { SignatureInfo } from '$lib/api';
import { t } from '$lib/strings';

/** Ton du verdict : vert, orange ou rouge dans l'interface. */
export type VerdictTone = 'ok' | 'warn' | 'bad';

/**
 * Ce que l'interface dit d'une signature. « Intègre » ne veut jamais dire « identité
 * certifiée » : le panneau le rappelle toujours (aucune chaîne de confiance n'est vérifiée).
 */
export function signatureVerdict(info: SignatureInfo): { tone: VerdictTone; text: string } {
  switch (info.status.kind) {
    case 'intact':
      return info.covers_whole_document
        ? { tone: 'ok', text: t.sigIntact }
        : { tone: 'warn', text: t.sigModifiedAfter };
    case 'invalid':
      return { tone: 'bad', text: t.sigInvalid(info.status.detail) };
    case 'unsupported':
      return { tone: 'warn', text: t.sigUnsupported(info.status.detail) };
  }
}
