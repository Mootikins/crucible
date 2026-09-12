import { describe, it, expect } from 'vitest';
import { approximateBytes, memoryStore } from '@/lib/offline/store';

describe('the offline store contract', () => {
  it('keeps, reads back, lists by prefix and removes', async () => {
    const store = memoryStore();
    await store.put('mirror', '/kiln/a.md', { body: 'A' });
    await store.put('mirror', '/kiln/b.md', { body: 'B' });
    await store.put('mirror', '/other/c.md', { body: 'C' });

    expect(await store.get('mirror', '/kiln/a.md')).toEqual({ body: 'A' });
    expect((await store.list('mirror', '/kiln/')).map((e) => e.key)).toEqual([
      '/kiln/a.md',
      '/kiln/b.md',
    ]);

    await store.remove('mirror', '/kiln/a.md');
    expect(await store.get('mirror', '/kiln/a.md')).toBeNull();
  });

  it('answers null for what it never kept', async () => {
    expect(await memoryStore().get('mirror', 'nothing')).toBeNull();
  });

  it('keeps its tables apart', async () => {
    const store = memoryStore();
    await store.put('mirror', 'k', 'in mirror');
    expect(await store.get('outbox', 'k')).toBeNull();
  });

  // The settings group shows a kiln's size, so a user can reclaim it.
  it('sizes a table, and a prefix of one', async () => {
    const store = memoryStore();
    await store.put('mirror', '/kiln/a.md', { body: 'x'.repeat(100) });
    await store.put('mirror', '/other/b.md', { body: 'y'.repeat(1000) });
    const kiln = await store.size('mirror', '/kiln/');
    expect(kiln).toBeGreaterThan(190);
    expect(kiln).toBeLessThan(await store.size('mirror'));
  });

  it('counts a blob by its bytes, not its shape', () => {
    expect(approximateBytes(new Blob([new Uint8Array(2048)]))).toBe(2048);
  });
});
