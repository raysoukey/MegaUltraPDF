// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { serialQueue } from './queue';

describe('serialQueue (A23)', () => {
  it('runs tasks one after the other, each seeing the state left by the previous one', async () => {
    const run = serialQueue();
    const log: string[] = [];
    let state = 2;
    const slow = (name: string) =>
      run(async () => {
        const seen = state;
        await new Promise((r) => setTimeout(r, 5));
        state = seen - 1;
        log.push(`${name}:${seen}`);
      });
    await Promise.all([slow('a'), slow('b')]);
    expect(log).toEqual(['a:2', 'b:1']);
    expect(state).toBe(0);
  });
  it('a failing task does not block the next ones', async () => {
    const run = serialQueue();
    await expect(run(async () => Promise.reject(new Error('non')))).rejects.toThrow('non');
    await expect(run(async () => 42)).resolves.toBe(42);
  });
});
