// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import type { RedactReport } from '$lib/api';
import { fr } from '$lib/strings';
import { redactNotice } from './redact';

function report(patch: Partial<RedactReport>): RedactReport {
  return {
    texts: 0,
    images_removed: 0,
    images_redacted: 0,
    paths_removed: 0,
    paths_kept: 0,
    annotations: 0,
    ...patch,
  };
}

describe('redactNotice', () => {
  it('sums what was removed and blacked out', () => {
    const n = redactNotice(
      report({ texts: 2, images_removed: 0, images_redacted: 2, paths_removed: 1, annotations: 1 }),
    );
    expect(n).toEqual({ message: fr.redactDone(2, 2, 1, 1), warn: false });
  });
  it('warns when paths crossing the area were only covered', () => {
    const n = redactNotice(report({ texts: 1, paths_kept: 3 }));
    expect(n.warn).toBe(true);
    expect(n.message).toBe(`${fr.redactDone(1, 0, 0, 0)} ${fr.redactPathsKept(3)}`);
  });
  it('warns when whole images were removed (format that cannot be rewritten)', () => {
    const n = redactNotice(report({ images_removed: 2 }));
    expect(n.warn).toBe(true);
    expect(n.message).toBe(`${fr.redactDone(0, 2, 0, 0)} ${fr.redactImagesRemoved(2)}`);
  });
  it('falls back to the short message without a report', () => {
    expect(redactNotice(null)).toEqual({ message: fr.redacted, warn: false });
  });
});
