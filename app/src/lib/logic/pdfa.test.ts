// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { saveNotice } from './pdfa';
import { t } from '$lib/strings';

describe('saveNotice', () => {
  it('warns when a modified PDF/A document is saved', () => {
    expect(saveNotice(true, 'PDF/A-2B')).toBe(t.pdfaNoLongerGuaranteed('PDF/A-2B'));
  });
  it('just says saved otherwise', () => {
    expect(saveNotice(false, 'PDF/A-2B')).toBe(t.saved);
    expect(saveNotice(true, null)).toBe(t.saved);
  });
});
