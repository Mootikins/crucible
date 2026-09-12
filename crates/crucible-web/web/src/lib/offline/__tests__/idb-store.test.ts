import { describe, it, expect, beforeEach } from 'vitest';
import 'fake-indexeddb/auto';
import { idbStore } from '@/lib/offline/store';

/**
 * The SHIPPED store, against a real IndexedDB implementation.
 *
 * `memoryStore` shares none of the failure modes that matter here —
 * transactions, commit-time aborts, cursors, key ranges — so a suite that
 * only exercises the twin proves nothing about what users run. jsdom has no
 * IndexedDB at all, which is why this file brings its own.
 */

let db: ReturnType<typeof idbStore>;
let n = 0;

beforeEach(() => {
  // A fresh database per test: the store caches its open promise forever.
  db = idbStore(`crucible-test-${(n += 1)}`);
});

describe('idbStore round trips', () => {
  it('stores a value and reads it back', async () => {
    await db.put('mirror', '/k/a.md', { body: 'hello' });
    expect(await db.get('mirror', '/k/a.md')).toEqual({ body: 'hello' });
  });

  it('answers null for a key it does not hold', async () => {
    expect(await db.get('mirror', '/k/missing.md')).toBeNull();
  });

  it('removes a key', async () => {
    await db.put('outbox', '/k/a.md', { body: 'x' });
    await db.remove('outbox', '/k/a.md');
    expect(await db.get('outbox', '/k/a.md')).toBeNull();
  });

  it('keeps tables apart', async () => {
    await db.put('mirror', 'same-key', 'in mirror');
    await db.put('outbox', 'same-key', 'in outbox');
    expect(await db.get('mirror', 'same-key')).toBe('in mirror');
    expect(await db.get('outbox', 'same-key')).toBe('in outbox');
  });

  // NOT tested here: that a Blob survives the round trip. `fake-indexeddb`
  // structured-clones through this jsdom, which does not preserve Blob, so
  // the value returns as `{}`. A real browser stores it. Asserting it here
  // would test the fake, and asserting the `{}` would pin the fake's defect.
  // The served tier is where an attachment is proved to paint.
});

describe('idbStore lists', () => {
  beforeEach(async () => {
    await db.put('mirror', '/k/one/a.md', { body: 'a' });
    await db.put('mirror', '/k/one/b.md', { body: 'b' });
    await db.put('mirror', '/k/two/c.md', { body: 'c' });
  });

  it('pairs every key with its OWN value', async () => {
    // The defect this replaces read keys and values in two transactions and
    // zipped them by index, so another writer between the two shifted one
    // array and paired every key with a neighbour's value.
    for (const { key, value } of await db.list<{ body: string }>('mirror')) {
      expect(value.body).toBe(key.slice(-4, -3));
    }
  });

  it('lists only the keys under a prefix', async () => {
    const keys = (await db.list('mirror', '/k/one/')).map((e) => e.key);
    expect(keys.sort()).toEqual(['/k/one/a.md', '/k/one/b.md']);
  });

  it('does not let a prefix reach a sibling that merely starts the same', async () => {
    await db.put('mirror', '/k/one-archive/d.md', { body: 'd' });
    expect((await db.list('mirror', '/k/one/')).map((e) => e.key).sort()).toEqual([
      '/k/one/a.md',
      '/k/one/b.md',
    ]);
  });

  it('answers an empty list for a prefix it holds nothing under', async () => {
    expect(await db.list('mirror', '/nowhere/')).toEqual([]);
  });

  it('clears a table without touching another', async () => {
    await db.put('outbox', '/k/one/a.md', { body: 'queued' });
    await db.clear('mirror');
    expect(await db.list('mirror')).toEqual([]);
    expect(await db.get('outbox', '/k/one/a.md')).toEqual({ body: 'queued' });
  });

  it('sizes a table, and a prefix of one', async () => {
    expect(await db.size('mirror')).toBeGreaterThan(0);
    expect(await db.size('mirror', '/k/two/')).toBeLessThan(await db.size('mirror'));
  });
});

describe('idbStore durability', () => {
  // A write must not be reported stored until the transaction COMMITS.
  // IndexedDB fails a quota or a forced close at commit, after every request
  // has already succeeded — and the editor clears the buffer on that report.
  it('resolves a write only once it is readable by a later transaction', async () => {
    await db.put('outbox', '/k/a.md', { body: 'committed' });
    // A separate handle, so this cannot be served from the writing one.
    expect(await idbStore(`crucible-test-${n}`).get('outbox', '/k/a.md')).toEqual({
      body: 'committed',
    });
  });
});
