// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it, vi } from 'vitest';

// `convertFileSrc` dépend du runtime Tauri (absent sous Vitest) : on le remplace par
// une imitation de son comportement Windows.
vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (p: string, s: string) => `http://${s}.localhost/${encodeURIComponent(p)}`,
}));

import { pagePath, pageUrl } from './pageUrl';

describe('pageUrl', () => {
  it('builds the backend path', () => {
    expect(pagePath(3, 0)).toBe('page/3/0');
  });

  it('appends the scale query', () => {
    expect(pageUrl(3, 0, 2)).toBe('http://mega.localhost/page%2F3%2F0?scale=2');
  });
});
