// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, describe, expect, it } from 'vitest';
import { setLanguage } from '$lib/strings';
import type { Style } from '$lib/api';
import {
  beginDraft,
  extendDraft,
  finishDraft,
  isDrawingTool,
  STAMP_IDS,
  stampLabel,
} from './tools';

const style: Style = {
  color: { r: 229, g: 57, b: 53 },
  fill: null,
  opacity: 1,
  stroke_width: 2,
  font_size: 14,
};

describe('isDrawingTool', () => {
  it('distinguishes tools that draw from those acting on the selection', () => {
    expect(isDrawingTool('rect')).toBe(true);
    expect(isDrawingTool('ink')).toBe(true);
    expect(isDrawingTool('note')).toBe(true);
    expect(isDrawingTool('signature')).toBe(true);
    expect(isDrawingTool('select')).toBe(false);
    expect(isDrawingTool('highlight')).toBe(false);
  });
});

describe('beginDraft / extendDraft', () => {
  it('returns null for tools that do not draw', () => {
    expect(beginDraft('select', { x: 1, y: 1 })).toBeNull();
    expect(beginDraft('underline', { x: 1, y: 1 })).toBeNull();
  });
  it('ink appends points at least 1 pt apart, other tools replace the second point', () => {
    let d = beginDraft('ink', { x: 10, y: 10 })!;
    d = extendDraft(d, { x: 10.5, y: 10.2 });
    expect(d.points).toHaveLength(1);
    d = extendDraft(d, { x: 12, y: 10 });
    d = extendDraft(d, { x: 14, y: 11 });
    expect(d.points).toHaveLength(3);

    let r = beginDraft('rect', { x: 0, y: 0 })!;
    r = extendDraft(r, { x: 5, y: 5 });
    r = extendDraft(r, { x: 20, y: 30 });
    expect(r.points).toEqual([
      { x: 0, y: 0 },
      { x: 20, y: 30 },
    ]);
  });
});

describe('finishDraft', () => {
  it('builds a rect annotation from two corners in any order', () => {
    let d = beginDraft('rect', { x: 50, y: 80 })!;
    d = extendDraft(d, { x: 10, y: 20 });
    const a = finishDraft(d, style, '', 'Moi')!;
    expect(a.kind).toBe('rect');
    expect(a.geometry).toEqual({ kind: 'rect', rect: { x: 10, y: 20, w: 40, h: 60 } });
    expect(a.author).toBe('Moi');
  });
  it('rejects drafts smaller than 2 pt', () => {
    let d = beginDraft('ellipse', { x: 10, y: 10 })!;
    d = extendDraft(d, { x: 11, y: 11.5 });
    expect(finishDraft(d, style, '', '')).toBeNull();
  });
  it('builds line, arrow, ink, note, text and stamp geometries', () => {
    let l = beginDraft('arrow', { x: 0, y: 0 })!;
    l = extendDraft(l, { x: 30, y: 40 });
    expect(finishDraft(l, style, '', '')!.geometry).toEqual({
      kind: 'line',
      from: [0, 0],
      to: [30, 40],
    });

    let i = beginDraft('ink', { x: 0, y: 0 })!;
    i = extendDraft(i, { x: 3, y: 4 });
    i = extendDraft(i, { x: 6, y: 2 });
    expect(finishDraft(i, style, '', '')!.geometry).toEqual({
      kind: 'strokes',
      strokes: [
        [
          [0, 0],
          [3, 4],
          [6, 2],
        ],
      ],
    });

    const n = finishDraft(beginDraft('note', { x: 100, y: 700 })!, style, 'Bonjour', 'Moi')!;
    expect(n.kind).toBe('note');
    expect(n.geometry).toEqual({ kind: 'rect', rect: { x: 100, y: 700, w: 0, h: 0 } });
    expect(n.contents).toBe('Bonjour');

    let t = beginDraft('text', { x: 10, y: 100 })!;
    t = extendDraft(t, { x: 110, y: 60 });
    const text = finishDraft(t, style, 'Essai', '')!;
    expect(text.kind).toBe('text');
    expect(text.geometry).toEqual({ kind: 'rect', rect: { x: 10, y: 60, w: 100, h: 40 } });

    // Un tampon posé d'un clic prend une taille par défaut autour du point.
    const s = finishDraft(beginDraft('stamp', { x: 200, y: 200 })!, style, 'BROUILLON', '')!;
    expect(s.kind).toBe('stamp');
    expect(s.geometry.kind).toBe('rect');
    if (s.geometry.kind === 'rect') {
      expect(s.geometry.rect.w).toBeGreaterThan(100);
      expect(s.geometry.rect.h).toBeGreaterThan(30);
    }
  });
  it('never turns a signature draft into an annotation (handled by the signature dialog)', () => {
    let s = beginDraft('signature', { x: 0, y: 0 })!;
    s = extendDraft(s, { x: 100, y: 40 });
    expect(finishDraft(s, style, '', '')).toBeNull();
  });
  it('requires text for note, text and stamp drafts', () => {
    let t = beginDraft('text', { x: 10, y: 100 })!;
    t = extendDraft(t, { x: 110, y: 60 });
    expect(finishDraft(t, style, '   ', '')).toBeNull();
    expect(finishDraft(beginDraft('note', { x: 1, y: 1 })!, style, '', '')).toBeNull();
    expect(finishDraft(beginDraft('stamp', { x: 1, y: 1 })!, style, '', '')).toBeNull();
  });
});

describe('stamps', () => {
  afterEach(() => setLanguage('fr'));
  it('stamp labels follow the interface language (the label is written into the PDF)', () => {
    expect(STAMP_IDS).toContain('approved');
    expect(stampLabel('approved')).toBe('APPROUVÉ');
    setLanguage('en');
    expect(stampLabel('approved')).toBe('APPROVED');
    expect(STAMP_IDS.map(stampLabel).every((l) => l.length > 0)).toBe(true);
  });
});
