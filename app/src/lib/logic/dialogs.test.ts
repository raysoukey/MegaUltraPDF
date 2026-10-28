// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { afterEscape, canOpen, isShown, request } from './dialogs';

describe('dialogs', () => {
  it('opens only dialogs that work without a document when none is open', () => {
    expect(canOpen('help', false)).toBe(true);
    expect(canOpen('merge', false)).toBe(true);
    expect(canOpen('preferences', false)).toBe(true);
    expect(canOpen('sign', false)).toBe(false);
    expect(canOpen('sign', true)).toBe(true);
  });
  it('keeps a single dialog: a request while another is open is ignored', () => {
    expect(request(null, 'watermark', true)).toBe('watermark');
    expect(request('watermark', 'crop', true)).toBe('watermark');
    expect(request(null, 'crop', false)).toBe(null);
  });
  it('escape closes the dialog unless it is busy', () => {
    expect(afterEscape('ocr', false)).toBe(null);
    expect(afterEscape('ocr', true)).toBe('ocr');
    expect(afterEscape(null, false)).toBe(null);
  });
  it('a document dialog is not shown once the document is closed', () => {
    expect(isShown('export', 'export', true)).toBe(true);
    expect(isShown('export', 'export', false)).toBe(false);
    expect(isShown('help', 'help', false)).toBe(true);
    expect(isShown('help', 'export', true)).toBe(false);
  });
});
