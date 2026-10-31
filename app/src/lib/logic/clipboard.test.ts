// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { copyText } from './clipboard';

describe('copyText', () => {
  it('uses the asynchronous clipboard when it works', async () => {
    const written: string[] = [];
    const ok = await copyText('abc', {
      write: async (t) => void written.push(t),
      fallback: () => false,
    });
    expect(ok).toBe(true);
    expect(written).toEqual(['abc']);
  });
  it('falls back when the clipboard API is refused (Lot 13)', async () => {
    const copied: string[] = [];
    const ok = await copyText('abc', {
      write: () => Promise.reject(new Error('refusé')),
      fallback: (t) => (copied.push(t), true),
    });
    expect(ok).toBe(true);
    expect(copied).toEqual(['abc']);
  });
  it('reports a failure when both ways fail', async () => {
    const ok = await copyText('abc', {
      write: () => Promise.reject(new Error('refusé')),
      fallback: () => false,
    });
    expect(ok).toBe(false);
  });
});
