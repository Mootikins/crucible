import { describe, it, expect, vi } from 'vitest';
import { memoryStore } from '../store';
import { queueWrite, drainOutbox, readQueued, recordConflict, type OutboxEntry } from '../outbox';

const first = { kind: 'whole' as const, path: '/k/a.md', kiln: '/k', daemon: 'A',
  body: '- [ ] milk\n- [ ] eggs\n', base: 'h0', baseText: '- [ ] milk\n- [ ] eggs\n' };
const tick = { expect: '- [ ] milk', replace: '- [x] milk' };

describe('outbox write lifecycle', () => {
  it('merges a refused anchored edit when its base text can reconstruct the note', async () => {
    const store = memoryStore();
    await queueWrite(store, { ...first, kind: 'anchored', edits: [tick] });
    const write = vi.fn(async (_entry: OutboxEntry, opts?: { baseText: string }) =>
      opts ? { ok: true as const, hash: 'merged', merged: true as const, content: 'merged text' }
        : { ok: false as const, refused: true as const, current: 'remote' });
    const result = await drainOutbox(store, { write }, 'A');
    expect(write).toHaveBeenCalledTimes(2);
    expect(result.sent).toBe(1);
    expect(result.refusedEdits).toEqual([]);
  });

  it('folds concurrent edits without losing either change', async () => {
    const store = memoryStore();
    await queueWrite(store, first);
    await Promise.all([
      queueWrite(store, { ...first, kind: 'anchored', edits: [tick] }),
      queueWrite(store, { ...first, kind: 'anchored', edits: [{ expect: '- [ ] eggs', replace: '- [x] eggs' }] }),
    ]);
    expect(await readQueued(store, first.path)).toMatchObject({ body: '- [x] milk\n- [x] eggs\n' });
  });

  it('never folds writing from a different daemon into this note', async () => {
    const store = memoryStore();
    await queueWrite(store, first);
    await expect(queueWrite(store, { ...first, daemon: 'B', body: 'B writing' })).rejects.toThrow(/daemon/i);
    expect(await readQueued(store, first.path)).toMatchObject({ daemon: 'A', body: first.body });
  });
});

it('does not replace a newer save with an older in-flight conflict', async () => {
  const store = memoryStore();
  await queueWrite(store, first);
  await expect(recordConflict(store, { ...first, body: 'old conflicting text' },
    { ok: false, current: 'h1', currentContent: 'theirs' }, null)).rejects.toThrow(/changed locally/);
  expect(await readQueued(store, first.path)).toMatchObject({ body: first.body });
});
