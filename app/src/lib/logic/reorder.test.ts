// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { DRAG_THRESHOLD, EDGE, edgeScroll, isDrag } from './reorder';

describe('isDrag', () => {
  it('treats a small jitter during a click as a click', () => {
    expect(isDrag(100, 200, 102, 203)).toBe(false);
  });
  it('starts a drag once the pointer has moved far enough', () => {
    expect(isDrag(100, 200, 100, 200 + DRAG_THRESHOLD + 1)).toBe(true);
    expect(isDrag(100, 200, 100 - DRAG_THRESHOLD - 1, 200)).toBe(true);
  });
});

describe('edgeScroll', () => {
  // Liste visible de y = 100 à y = 500.
  it('does not scroll in the middle of the list', () => {
    expect(edgeScroll(300, 100, 500)).toBe(0);
  });
  it('scrolls up near the top edge and down near the bottom edge, faster closer to it', () => {
    expect(edgeScroll(100 + EDGE / 2, 100, 500)).toBeLessThan(0);
    expect(edgeScroll(500 - EDGE / 2, 100, 500)).toBeGreaterThan(0);
    expect(edgeScroll(505, 100, 500)).toBeGreaterThan(edgeScroll(500 - EDGE / 2, 100, 500));
  });
});
