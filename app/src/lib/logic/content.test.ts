// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import type { PageObject } from '$lib/api';
import {
  cropMarginsValid,
  fillTemplate,
  hitTestObjects,
  isContentTool,
  nearCorner,
  oppositeCorner,
  rectFromCorners,
  scopePages,
} from './content';

function obj(index: number, x: number, y: number, w: number, h: number): PageObject {
  return { index, kind: 'text', bounds: { x, y, w, h }, text: null, font_size: null, font: null };
}

describe('isContentTool', () => {
  it('recognises the four content tools and nothing else', () => {
    expect(isContentTool('objects')).toBe(true);
    expect(isContentTool('text-content')).toBe(true);
    expect(isContentTool('image')).toBe(true);
    expect(isContentTool('redact')).toBe(true);
    expect(isContentTool('select')).toBe(false);
    expect(isContentTool('text')).toBe(false);
    expect(isContentTool('rect')).toBe(false);
  });
});

describe('hitTestObjects', () => {
  const objects = [obj(0, 0, 0, 100, 100), obj(1, 50, 50, 20, 20), obj(2, 200, 200, 10, 10)];
  it('returns the topmost (last drawn) object under the point', () => {
    expect(hitTestObjects(objects, { x: 60, y: 60 })).toBe(1);
    expect(hitTestObjects(objects, { x: 10, y: 10 })).toBe(0);
    expect(hitTestObjects(objects, { x: 205, y: 205 })).toBe(2);
  });
  it('returns null outside every object and tolerates a 2 pt margin for thin objects', () => {
    expect(hitTestObjects(objects, { x: 150, y: 150 })).toBeNull();
    expect(hitTestObjects([obj(0, 10, 10, 50, 0)], { x: 20, y: 11 })).toBe(0);
  });
});

describe('rectFromCorners', () => {
  it('normalises any two corners into a positive rectangle', () => {
    expect(rectFromCorners({ x: 10, y: 30 }, { x: 4, y: 5 })).toEqual({ x: 4, y: 5, w: 6, h: 25 });
  });
});

describe('fillTemplate', () => {
  it('substitutes {n}, {total} and {titre}', () => {
    expect(fillTemplate('{n} / {total} — {titre}', 3, 10, 'Rapport')).toBe('3 / 10 — Rapport');
    expect(fillTemplate('p. {n}', 7, 10, '')).toBe('p. 7');
  });
});

describe('scopePages', () => {
  it('maps a scope to the page list the backend expects (null = all)', () => {
    expect(scopePages('all', 2, [0, 4])).toBeNull();
    expect(scopePages('current', 2, [0, 4])).toEqual([2]);
    expect(scopePages('selected', 2, [4, 0])).toEqual([0, 4]);
    // Sans sélection, « pages sélectionnées » retombe sur la page courante.
    expect(scopePages('selected', 2, [])).toEqual([2]);
  });
});

describe('cropMarginsValid', () => {
  const a4 = { width_pt: 595.28, height_pt: 841.89 };
  it('requires non-negative margins that leave at least one point of page', () => {
    expect(cropMarginsValid([50, 50, 50, 50], a4)).toBe(true);
    expect(cropMarginsValid([0, 0, 0, 0], a4)).toBe(true);
    expect(cropMarginsValid([300, 0, 300, 0], a4)).toBe(false);
    expect(cropMarginsValid([-1, 0, 0, 0], a4)).toBe(false);
    expect(cropMarginsValid([Number.NaN, 0, 0, 0], a4)).toBe(false);
  });
});

describe('poignée de redimensionnement', () => {
  it('oppositeCorner returns the bounds corner farthest from the point', () => {
    const b = { x: 100, y: 100, w: 50, h: 20 };
    expect(oppositeCorner(b, { x: 151, y: 99 })).toEqual({ x: 100, y: 120 });
    expect(oppositeCorner(b, { x: 99, y: 121 })).toEqual({ x: 150, y: 100 });
  });
  it('nearCorner hits the bottom-right corner of a CSS box within the tolerance', () => {
    const c = { left: 10, top: 20, width: 100, height: 50 };
    expect(nearCorner(108, 66, c)).toBe(true);
    expect(nearCorner(100, 60, c)).toBe(false);
  });
});
