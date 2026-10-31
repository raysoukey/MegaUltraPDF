// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { physicalSize } from './physicalSize';

describe('physicalSize', () => {
  it('multiplies by the user unit and groups thousands', () => {
    expect(physicalSize(595.3, 841.9, 10)).toBe('2 100 × 2 970 mm');
  });
  it('stays plain below a thousand millimetres', () => {
    expect(physicalSize(595.3, 841.9, 1)).toBe('210 × 297 mm');
  });
  it('rounds to whole millimetres', () => {
    expect(physicalSize(100, 100, 10)).toBe('353 × 353 mm');
  });
});
