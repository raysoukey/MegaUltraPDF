// SPDX-License-Identifier: GPL-3.0-or-later
// Conformité PDF/A déclarée (Lot 13, audit C6) : MegaUltraPDF garde la déclaration d'un
// document qu'il modifie, mais ne vérifie pas que le résultat respecte encore la norme
// (polices incorporées, couleurs, transparence…). L'enregistrement le dit.
import { t } from '$lib/strings';

/**
 * Message affiché après un enregistrement réussi : un avertissement si le document a été
 * modifié et déclare une conformité PDF/A (`pdfa`, ex. « PDF/A-2B »), sinon « Enregistré ».
 */
export function saveNotice(modified: boolean, pdfa: string | null): string {
  return modified && pdfa ? t.pdfaNoLongerGuaranteed(pdfa) : t.saved;
}
