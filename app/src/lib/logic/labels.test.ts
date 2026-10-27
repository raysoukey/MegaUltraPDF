// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { fileName, formatPdfDate, tabLabel } from './labels';

describe('fileName', () => {
  it('keeps the last segment of a Windows or POSIX path', () => {
    expect(fileName('C:\\docs\\rapport.pdf')).toBe('rapport.pdf');
    expect(fileName('/home/r/rapport.pdf')).toBe('rapport.pdf');
    expect(fileName('rapport.pdf')).toBe('rapport.pdf');
  });
});

describe('tabLabel', () => {
  it('prefers the title, then the file name, then a placeholder', () => {
    expect(tabLabel({ path: '/x/a.pdf', title: 'Titre', modified: false })).toBe('Titre');
    expect(tabLabel({ path: '/x/a.pdf', title: null, modified: false })).toBe('a.pdf');
    expect(tabLabel({ path: null, title: null, modified: false })).toBe('(sans titre)');
  });
  it('appends the modified marker', () => {
    expect(tabLabel({ path: '/x/a.pdf', title: null, modified: true })).toBe('a.pdf •');
    expect(tabLabel({ path: null, title: null, modified: true })).toBe('(sans titre) •');
  });
});

describe('formatPdfDate', () => {
  it('formats D: dates and tolerates partial or foreign values', () => {
    expect(formatPdfDate("D:20261026143000+00'00'")).toBe('2026-10-26 14:30');
    expect(formatPdfDate('D:202610')).toBe('2026-10-01 00:00');
    expect(formatPdfDate('hier')).toBe('hier');
    expect(formatPdfDate(null)).toBeNull();
  });
});
