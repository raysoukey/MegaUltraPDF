// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { DRAG_THRESHOLD, isDrag } from './reorder';

describe('isDrag', () => {
  it('treats a small jitter during a click as a click', () => {
    expect(isDrag(100, 200, 102, 203)).toBe(false);
  });
  it('starts a drag once the pointer has moved far enough', () => {
    expect(isDrag(100, 200, 100, 200 + DRAG_THRESHOLD + 1)).toBe(true);
    expect(isDrag(100, 200, 100 - DRAG_THRESHOLD - 1, 200)).toBe(true);
  });
});
