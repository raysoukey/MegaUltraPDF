// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { pageAtScroll, pageLayout, pageSlots, visiblePages, wheelPageStep } from './virtualization';

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

describe('pageAtScroll at the end of the document', () => {
  it('reports the last page when the viewport reaches the bottom even if the page is short', () => {
    // Trois pages de 300 px, fenêtre de 600 px : la dernière page ne peut jamais atteindre le
    // haut de la fenêtre ; une fois en butée (scrollTop = total − 600), c'est bien la page 3.
    const short = pageLayout(
      [
        { width_pt: 100, height_pt: 300 },
        { width_pt: 100, height_pt: 300 },
        { width_pt: 100, height_pt: 300 },
      ],
      1,
      'none',
      0,
    );
    const total = 900;
    expect(pageAtScroll(short, total - 600, 600)).toBe(2);
    expect(pageAtScroll(short, 0, 600)).toBe(0);
  });
});

describe('pageSlots (A15)', () => {
  it('has one entry per page, visible first and interactive, prefetch after', () => {
    expect(pageSlots([2, 3], [1, 2, 3, 4])).toEqual([
      { index: 2, priority: 'visible', interactive: true },
      { index: 3, priority: 'visible', interactive: true },
      { index: 1, priority: 'prefetch', interactive: false },
      { index: 4, priority: 'prefetch', interactive: false },
    ]);
  });
});

describe('wheelPageStep (A16)', () => {
  it('turns the page only at the edges of a single page', () => {
    expect(wheelPageStep(100, 400, 1000, 600)).toBe(1); // en bas
    expect(wheelPageStep(-100, 0, 1000, 600)).toBe(-1); // en haut
    expect(wheelPageStep(100, 100, 1000, 600)).toBe(0); // au milieu : on défile
  });
  it('a page that fits turns at once', () => {
    expect(wheelPageStep(50, 0, 500, 600)).toBe(1);
    expect(wheelPageStep(-50, 0, 500, 600)).toBe(-1);
  });
});
