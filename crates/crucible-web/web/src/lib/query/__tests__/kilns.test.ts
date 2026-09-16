import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { KilnListEntry } from '@/lib/types';
import { keys } from '../keys';
import { useKilns, fetchKilnsOnce, kilnsSnapshot, resetKilnsForTests } from '../kilns';

/** The storage key `swrLocal('kilns')` wrote, which the hook keeps. */
const STORAGE_KEY = 'crucible:cache:kilns';

const MAIN: KilnListEntry[] = [{ name: 'main', path: '/kilns/main', last_access_secs_ago: null, open: true, registered: true }];
const STORED: KilnListEntry[] = [{ name: 'stored', path: '/kilns/stored', last_access_secs_ago: null, open: true, registered: true }];

/** The envelope `GET /api/kilns` answers; `listKilns` unwraps `kilns`. */
function kilnsBody(kilns: KilnListEntry[]): { kilns: KilnListEntry[] } {
  return { kilns };
}

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

beforeEach(() => {
  localStorage.removeItem(STORAGE_KEY);
  resetKilnsForTests();
});

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
  localStorage.removeItem(STORAGE_KEY);
  resetKilnsForTests();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useKilns', () => {
  it('fetches once for two callers under one root', async () => {
    env = createTestQueryEnv({ 'GET /api/kilns': () => kilnsBody(MAIN) });

    const both = inRoot(() => ({ first: useKilns(), second: useKilns() }));

    await vi.waitFor(() => expect(both.first.data).toEqual(MAIN));
    expect(both.second.data).toEqual(MAIN);
    expect(env.fetch.calls('GET /api/kilns')).toBe(1);
  });

  it('surfaces a refusal as an error rather than as an empty list', async () => {
    env = createTestQueryEnv({
      'GET /api/kilns': apiError(422, 'the kiln registry is closed'),
    });

    const query = inRoot(() => useKilns());

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('Failed to list kilns');
    expect(query.data).toBeUndefined();
  });

  it('paints the stored list before the fetch answers', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(STORED));
    let release: (() => void) | undefined;
    const answered = new Promise<void>((resolve) => {
      release = resolve;
    });
    env = createTestQueryEnv({
      'GET /api/kilns': async () => {
        await answered;
        return kilnsBody(MAIN);
      },
    });

    const query = inRoot(() => useKilns());

    // The first read, before the fetch resolves, already has the last list.
    expect(query.data).toEqual(STORED);

    release?.();
    await vi.waitFor(() => expect(query.data).toEqual(MAIN));
    expect(env.fetch.calls('GET /api/kilns')).toBe(1);
  });

  it('writes the fetched list back to storage', async () => {
    env = createTestQueryEnv({ 'GET /api/kilns': () => kilnsBody(MAIN) });

    const query = inRoot(() => useKilns());

    await vi.waitFor(() => expect(query.data).toEqual(MAIN));
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(MAIN);
  });

  it('keeps the stored list out of the cache when a later read follows a failure', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(STORED));
    env = createTestQueryEnv({ 'GET /api/kilns': apiError(500, 'the daemon fell over') });

    const query = inRoot(() => useKilns());

    await vi.waitFor(() => expect(query.isError).toBe(true));
    // The stale list stays on screen; the failure is still reported.
    expect(query.data).toEqual(STORED);
    expect(query.error?.message).toContain('Failed to list kilns');
  });
});

describe('fetchKilnsOnce', () => {
  it('answers the same list the hook reads, from one fetch', async () => {
    env = createTestQueryEnv({ 'GET /api/kilns': () => kilnsBody(MAIN) });

    const query = inRoot(() => useKilns());
    await vi.waitFor(() => expect(query.data).toEqual(MAIN));

    await expect(fetchKilnsOnce()).resolves.toEqual(MAIN);
    expect(env.fetch.calls('GET /api/kilns')).toBe(1);
  });

  it('rejects when the daemon refuses, rather than answering an empty list', async () => {
    // No stored list: a seeded entry would be answered from the cache, and
    // this is about what the FETCH does when the daemon refuses it.
    env = createTestQueryEnv({ 'GET /api/kilns': apiError(500, 'the daemon fell over') });

    // The callers of this function cannot render a pending state and cannot
    // render a refusal either, so a resolved `[]` would read to them as "this
    // kiln does not exist". The rejection is what makes them stop instead.
    // `listKilns` does not ask for the body text, so the sentence carries the
    // attempt and the status, not the daemon's own words.
    await expect(fetchKilnsOnce()).rejects.toThrow('Failed to list kilns: HTTP 500');
  });
});

describe('kilnsSnapshot', () => {
  it('starts the one fetch and then answers what the cache holds', async () => {
    env = createTestQueryEnv({ 'GET /api/kilns': () => kilnsBody(MAIN) });

    expect(kilnsSnapshot()).toEqual([]);
    await vi.waitFor(() => expect(kilnsSnapshot()).toEqual(MAIN));
    expect(env.fetch.calls('GET /api/kilns')).toBe(1);
  });

  it('shares the fetch with the hook', async () => {
    env = createTestQueryEnv({ 'GET /api/kilns': () => kilnsBody(MAIN) });

    const query = inRoot(() => useKilns());
    kilnsSnapshot();

    await vi.waitFor(() => expect(query.data).toEqual(MAIN));
    await vi.waitFor(() => expect(kilnsSnapshot()).toEqual(MAIN));
    expect(env.fetch.calls('GET /api/kilns')).toBe(1);
  });

  it('answers the cache the test wrote, without a fetch of its own', async () => {
    env = createTestQueryEnv({ 'GET /api/kilns': () => kilnsBody(MAIN) });
    env.client.setQueryData(keys.kilns(), STORED);

    expect(kilnsSnapshot()).toEqual(STORED);
    expect(env.fetch.calls('GET /api/kilns')).toBe(0);
  });
});
