// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { promptAfterOpen } from './open';

describe('promptAfterOpen', () => {
  it('keeps the password prompt of another file (several files opened from the system)', () => {
    const prompt = { path: '/docs/chiffre.pdf', error: null };
    expect(promptAfterOpen(prompt, '/docs/simple.pdf')).toBe(prompt);
  });
  it('closes the prompt of the file that just opened', () => {
    const prompt = { path: '/docs/chiffre.pdf', error: null };
    expect(promptAfterOpen(prompt, '/docs/chiffre.pdf')).toBe(null);
    expect(promptAfterOpen(null, '/docs/simple.pdf')).toBe(null);
  });
});
