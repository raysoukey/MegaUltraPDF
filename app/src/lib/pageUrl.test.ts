// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it, vi } from 'vitest';

// `convertFileSrc` dépend du runtime Tauri (absent sous Vitest) : on le remplace par
// une imitation de son comportement Windows.
vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (p: string, s: string) => `http://${s}.localhost/${encodeURIComponent(p)}`,
}));

import { pagePath, pageUrl, renderScale } from './pageUrl';
import { MAX_SCALE } from './logic/zoom';

describe('pageUrl', () => {
  it('builds the backend path', () => {
    expect(pagePath(3, 0)).toBe('page/3/0');
  });

  it('appends scale, rotation, generation and priority', () => {
    expect(pageUrl(3, 0, 2, 'cw90', 5, 'thumbnail')).toBe(
      'http://mega.localhost/page%2F3%2F0?scale=2&rot=90&gen=5&prio=thumbnail',
    );
  });

  it('defaults to upright, generation 0, visible', () => {
    expect(pageUrl(3, 0, 1.5)).toBe(
      'http://mega.localhost/page%2F3%2F0?scale=1.5&rot=0&gen=0&prio=visible',
    );
  });
});

describe('renderScale', () => {
  it('renders at the screen resolution on a high-DPI display (Lot 13)', () => {
    expect(renderScale(1.5, 2)).toBe(3);
    expect(renderScale(1, 1.25)).toBe(1.25);
  });
  it('never asks for less than the CSS size nor more than the backend cap', () => {
    expect(renderScale(1, 0.5)).toBe(1);
    expect(renderScale(6, 2)).toBe(MAX_SCALE);
    expect(renderScale(1, 10)).toBe(3);
  });
});
