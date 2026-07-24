import { describe, it, expect, afterEach } from 'vitest';
import { createRoot, createEffect } from 'solid-js';
import { pendingDiffStore, pendingDiffActions } from '../pendingDiffStore';

const A = '/proj/a.ts';
const B = '/proj/b.ts';
const diff = (proposed: string) => ({ original: 'orig\n', proposed });

afterEach(() => {
  pendingDiffActions.clear(A);
  pendingDiffActions.clear(B);
});

describe('pendingDiffStore', () => {
  it('returns undefined for a path with no pending diff', () => {
    expect(pendingDiffStore.get(A)).toBeUndefined();
  });

  it('stores diffs per path independently', () => {
    pendingDiffActions.set(A, diff('a-new\n'));
    pendingDiffActions.set(B, diff('b-new\n'));
    expect(pendingDiffStore.get(A)?.proposed).toBe('a-new\n');
    expect(pendingDiffStore.get(B)?.proposed).toBe('b-new\n');
  });

  it('clear drops only the cleared path', () => {
    pendingDiffActions.set(A, diff('a-new\n'));
    pendingDiffActions.set(B, diff('b-new\n'));
    pendingDiffActions.clear(A);
    expect(pendingDiffStore.get(A)).toBeUndefined();
    expect(pendingDiffStore.get(B)?.proposed).toBe('b-new\n');
  });

  it('notifies a subscriber on set and on clear', async () => {
    const seen: (string | undefined)[] = [];
    const dispose = createRoot((d) => {
      createEffect(() => seen.push(pendingDiffStore.get(A)?.proposed));
      return d;
    });
    await Promise.resolve();
    pendingDiffActions.set(A, diff('a-new\n'));
    await Promise.resolve();
    pendingDiffActions.clear(A);
    await Promise.resolve();
    dispose();
    expect(seen).toEqual([undefined, 'a-new\n', undefined]);
  });
});
