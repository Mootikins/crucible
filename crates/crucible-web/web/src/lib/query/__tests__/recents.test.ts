import { describe, it, expect, afterEach, beforeEach } from 'vitest';
import { createRoot } from 'solid-js';
import { waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { keys } from '../keys';
import { fetchRecentsOnce, recordRecentOnce, useFetchRecents } from '../recents';

/**
 * The recently opened files, as one list.
 *
 * It is short, it is never indexed by path, and every reader wants the same
 * twenty entries, so it is one entry under one key. The reason it needs a key
 * at all is the WRITE: opening a file records it, and the list the composer is
 * showing is wrong the moment that lands.
 */

let env: TestQueryEnv;
let dispose: (() => void) | null = null;
/** How many times the daemon was asked for the list. */
let reads = 0;
/** Every record the daemon was sent, in order. */
let wrote: { abs_path: string; name: string }[] = [];
/** What the daemon answers next. */
let onServer: { abs_path: string; name: string; opened_at: number }[] = [];

function recentRoutes() {
  reads = 0;
  wrote = [];
  onServer = [{ abs_path: '/k/a.md', name: 'a.md', opened_at: 1 }];
  return {
    'GET /api/recents': () => {
      reads += 1;
      return { recents: onServer };
    },
    'POST /api/recents': async (request: Request) => {
      const body = (await request.json()) as { abs_path: string; name: string };
      wrote.push(body);
      onServer = [{ ...body, opened_at: 2 }, ...onServer];
      return {};
    },
  };
}

function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

beforeEach(() => {
  env = createTestQueryEnv(recentRoutes());
});

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
});

describe('useFetchRecents', () => {
  it('reads the list once for two readers of it', async () => {
    const both = inRoot(() => ({
      composer: useFetchRecents(),
      other: useFetchRecents(),
    }));

    await waitFor(() => expect(both.composer.data).toBeDefined());
    await waitFor(() => expect(both.other.data).toBeDefined());
    expect(reads).toBe(1);
    expect(both.composer.data).toEqual([{ absPath: '/k/a.md', name: 'a.md' }]);
    expect(env.client.getQueryData(keys.recents())).toEqual(both.composer.data);
  });

  it('answers a held list without asking again', async () => {
    await fetchRecentsOnce();
    await fetchRecentsOnce();

    expect(reads).toBe(1);
  });
});

describe('recordRecentOnce', () => {
  /**
   * The write is what the key is for.
   *
   * Opening a file records it, and the list on the empty centre is wrong the
   * moment that lands. It used to be right only because the caller patched its
   * own copy beside the write, which left the server's order and the screen's
   * order to agree by coincidence.
   */
  it('makes the held list wrong, so the next read sees the new entry', async () => {
    const list = inRoot(() => useFetchRecents());
    await waitFor(() => expect(list.data).toHaveLength(1));

    await recordRecentOnce('/k/b.md', 'b.md');

    await waitFor(() => expect(list.data).toHaveLength(2));
    expect(list.data?.[0]).toEqual({ absPath: '/k/b.md', name: 'b.md' });
    expect(wrote).toEqual([{ abs_path: '/k/b.md', name: 'b.md' }]);
    expect(reads).toBe(2);
  });

  /**
   * A refused record is not a failed open.
   *
   * The daemon may be an older one with no `/api/recents`, and the file is
   * open on screen either way. The caller must not see an error it has no
   * answer for.
   */
  it('swallows a refusal, because the file is open regardless', async () => {
    env.restore();
    env = createTestQueryEnv({
      'GET /api/recents': () => ({ recents: [] }),
      'POST /api/recents': () =>
        new Response(JSON.stringify({ error: { code: 404, message: 'no such route' } }), {
          status: 404,
          headers: { 'Content-Type': 'application/json' },
        }),
    });

    await expect(recordRecentOnce('/k/b.md', 'b.md')).resolves.toBeUndefined();
  });
});
