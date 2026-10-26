// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { pushRecent } from './recent';

describe('pushRecent', () => {
  it('puts the newest first and removes duplicates', () => {
    expect(pushRecent(['a', 'b'], 'b')).toEqual(['b', 'a']);
    expect(pushRecent(['a', 'b'], 'c')).toEqual(['c', 'a', 'b']);
  });
  it('truncates to the maximum', () => {
    expect(pushRecent(['1', '2', '3'], '0', 3)).toEqual(['0', '1', '2']);
  });
  it('does not mutate the input', () => {
    const input = ['a'];
    pushRecent(input, 'b');
    expect(input).toEqual(['a']);
  });
});
