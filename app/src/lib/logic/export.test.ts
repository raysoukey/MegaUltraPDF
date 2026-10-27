// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { defaultPrefix, exportProblems, parseCounters, printScale } from './export';

describe('defaultPrefix', () => {
  it('uses the file stem, then the title, then a fallback', () => {
    expect(defaultPrefix('C:\\docs\\Rapport final.pdf', 'Titre')).toBe('Rapport final');
    expect(defaultPrefix('/tmp/a.PDF', null)).toBe('a');
    expect(defaultPrefix(null, 'Mon titre')).toBe('Mon titre');
    expect(defaultPrefix(null, null)).toBe('page');
  });
  it('strips characters that cannot appear in a file name', () => {
    expect(defaultPrefix(null, 'a/b:c*d?e')).toBe('a-b-c-d-e');
  });
});

describe('exportProblems', () => {
  it('accepts the documented ranges and rejects the rest', () => {
    expect(exportProblems({ dpi: 150, quality: 85, prefix: 'p' })).toEqual([]);
    expect(exportProblems({ dpi: 20, quality: 85, prefix: 'p' })).toContain('dpi');
    expect(exportProblems({ dpi: 700, quality: 85, prefix: 'p' })).toContain('dpi');
    expect(exportProblems({ dpi: 150, quality: 5, prefix: 'p' })).toContain('quality');
    expect(exportProblems({ dpi: 150, quality: 85, prefix: '  ' })).toContain('prefix');
  });
});

describe('printScale', () => {
  it('converts DPI to a render scale (72 dpi = 1)', () => {
    expect(printScale(72)).toBe(1);
    expect(printScale(150)).toBeCloseTo(2.0833, 3);
    expect(printScale(300)).toBeCloseTo(4.1667, 3);
  });
});

describe('parseCounters', () => {
  it('reads the 24-byte header of a compare response', () => {
    const buf = new ArrayBuffer(24 + 3);
    const view = new DataView(buf);
    view.setBigUint64(0, 1234n, true);
    view.setBigUint64(8, 500000n, true);
    view.setUint32(16, 595, true);
    view.setUint32(20, 842, true);
    new Uint8Array(buf).set([1, 2, 3], 24);
    const parsed = parseCounters(buf);
    expect(parsed.differing_pixels).toBe(1234);
    expect(parsed.total_pixels).toBe(500000);
    expect(parsed.width_px).toBe(595);
    expect(parsed.height_px).toBe(842);
    expect(Array.from(parsed.png)).toEqual([1, 2, 3]);
  });
});
