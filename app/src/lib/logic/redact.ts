// SPDX-License-Identifier: GPL-3.0-or-later
// Compte rendu d'un caviardage pour le bandeau de notification (logique pure, testée).
import type { RedactReport } from '$lib/api';
import { t } from '$lib/strings';

/**
 * Message affiché après un caviardage. `warn` est vrai quand des tracés qui traversent la
 * zone n'ont été que recouverts : le message reste alors affiché plus longtemps.
 */
export function redactNotice(report: RedactReport | null): { message: string; warn: boolean } {
  if (!report) return { message: t.redacted, warn: false };
  const images = report.images_removed + report.images_redacted;
  const done = t.redactDone(report.texts, images, report.paths_removed, report.annotations);
  if (report.paths_kept > 0) {
    return { message: `${done} ${t.redactPathsKept(report.paths_kept)}`, warn: true };
  }
  return { message: done, warn: false };
}
