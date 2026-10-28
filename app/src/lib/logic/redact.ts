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
  // Une image retirée entière (format qu'on ne sait pas réécrire : JPEG 2000, fax, 1 bit…)
  // peut faire disparaître toute une page numérisée : l'utilisateur doit le savoir.
  const warnings: string[] = [];
  if (report.images_removed > 0) warnings.push(t.redactImagesRemoved(report.images_removed));
  if (report.paths_kept > 0) warnings.push(t.redactPathsKept(report.paths_kept));
  if (warnings.length > 0) return { message: [done, ...warnings].join(' '), warn: true };
  return { message: done, warn: false };
}
