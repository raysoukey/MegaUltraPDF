// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { inGroup } from './group';

describe('inGroup', () => {
  it('opens, runs and closes in order', async () => {
    const log: string[] = [];
    const result = await inGroup(
      async () => void log.push('begin'),
      async () => {
        log.push('body');
        return 3;
      },
      async () => void log.push('end'),
    );
    expect(result).toBe(3);
    expect(log).toEqual(['begin', 'body', 'end']);
  });
  it('still closes the group when the body stops on an error', async () => {
    const log: string[] = [];
    await expect(
      inGroup(
        async () => void log.push('begin'),
        async () => {
          throw new Error('arrêt');
        },
        async () => void log.push('end'),
      ),
    ).rejects.toThrow('arrêt');
    expect(log).toEqual(['begin', 'end']);
  });
});
