// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { pageAtScroll, pageLayout, visiblePages } from './virtualization';

const pages = [
  { width_pt: 595, height_pt: 842 },
  { width_pt: 595, height_pt: 842 },
  { width_pt: 842, height_pt: 595 },
];

describe('pageLayout', () => {
  it('stacks pages vertically with a gap', () => {
    const layout = pageLayout(pages, 1, 'none', 10);
    expect(layout.map((p) => p.top)).toEqual([0, 852, 1704]);
    expect(layout[2]).toEqual({ top: 1704, width: 842, height: 595 });
    expect(pageLayout(pages, 1, 'cw90', 10)[0]).toEqual({ top: 0, width: 842, height: 595 });
  });
  it('scales', () => {
    expect(pageLayout(pages, 0.5, 'none', 0)[1].top).toBe(421);
  });
});

describe('visiblePages', () => {
  const layout = pageLayout(pages, 1, 'none', 10);
  it('returns the pages intersecting the viewport', () => {
    expect(visiblePages(layout, 0, 600, 0)).toEqual([0]);
    expect(visiblePages(layout, 800, 600, 0)).toEqual([0, 1]);
  });
  it('adds overscan pages around', () => {
    expect(visiblePages(layout, 0, 600, 1)).toEqual([0, 1]);
    expect(visiblePages(layout, 1800, 100, 2)).toEqual([0, 1, 2]);
  });
  it('handles an empty layout', () => {
    expect(visiblePages([], 0, 600, 1)).toEqual([]);
  });
});

describe('pageAtScroll', () => {
  const layout = pageLayout(pages, 1, 'none', 10);
  it('is the page covering the reading point (top third of the viewport)', () => {
    expect(pageAtScroll(layout, 0, 600)).toBe(0);
    expect(pageAtScroll(layout, 700, 600)).toBe(1);
    expect(pageAtScroll(layout, 99999, 600)).toBe(2);
  });
});
