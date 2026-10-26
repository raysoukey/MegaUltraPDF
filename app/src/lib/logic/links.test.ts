// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { externalLinkPolicy } from './links';

describe('externalLinkPolicy', () => {
  it('opens web and mail links', () => {
    expect(externalLinkPolicy('https://example.org')).toBe('open');
    expect(externalLinkPolicy('HTTP://example.org/a?b=c')).toBe('open');
    expect(externalLinkPolicy('mailto:someone@example.org')).toBe('open');
  });
  it('blocks everything else', () => {
    expect(externalLinkPolicy('file:///C:/Windows/system.ini')).toBe('blocked');
    expect(externalLinkPolicy('javascript:alert(1)')).toBe('blocked');
    expect(externalLinkPolicy('data:text/html,hi')).toBe('blocked');
    expect(externalLinkPolicy('')).toBe('blocked');
    expect(externalLinkPolicy('  https://x.org')).toBe('blocked');
  });
});
