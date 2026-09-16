import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

import { recentFiles, recordRecentFile, syncRecentsFromServer } from '../recent-files';

/**
 * Nothing in `@/lib/api` is stubbed.
 *
 * The server's list is a cache entry now, so a mocked module would count the
 * calls that reach it rather than the ones that reach the daemon — and what
 * the last case is about is the request the write makes the next read send.
 */
let env: TestQueryEnv;
/** How many times the daemon was asked for the list. */
let reads = 0;
/** Every record the daemon was sent, in order. */
let wrote: { abs_path: string; name: string }[] = [];
/** What the daemon answers next. */
let onServer: { abs_path: string; name: string; opened_at: number }[] = [];

describe('recent-files', () => {
  // The store is a module-level signal (no reset API) — tests below use
  // saturating writes / relative assertions so shared state can't skew them.
  beforeEach(() => {
    localStorage.clear();
    reads = 0;
    wrote = [];
    onServer = [];
    env = createTestQueryEnv({
      'GET /api/recents': () => {
        reads += 1;
        return { recents: onServer };
      },
      'POST /api/recents': async (request: Request) => {
        const body = (await request.json()) as { abs_path: string; name: string };
        wrote.push(body);
        onServer = [{ ...body, opened_at: wrote.length }, ...onServer];
        return {};
      },
    });
  });

  afterEach(() => env?.restore());

  /**
   * Keeps an entry the last reader left, the way the app does.
   *
   * The test client drops one the moment nobody observes it, and this module
   * reads the list as a promise rather than as an observer — so a case about
   * reading it TWICE has to hold it the app's way or it asserts the harness.
   */
  function holdUnobservedEntries(): void {
    env.client.setDefaultOptions({
      queries: { gcTime: 60_000, staleTime: 5 * 60 * 1000, retry: false },
    });
  }

  it('records most-recent-first, dedupes by path, and caps the ring', () => {
    for (let i = 0; i < 25; i++) {
      recordRecentFile(`/k/f${i}.md`, `f${i}.md`);
    }
    // Capped at 20 (matches the server-side MAX_RECENTS), newest first.
    expect(recentFiles().length).toBe(20);
    expect(recentFiles()[0].absPath).toBe('/k/f24.md');

    // Re-opening an older file moves it to the front without duplication.
    recordRecentFile('/k/f10.md', 'f10.md');
    expect(recentFiles()[0].absPath).toBe('/k/f10.md');
    expect(recentFiles().filter((r) => r.absPath === '/k/f10.md')).toHaveLength(1);
    expect(recentFiles().length).toBe(20);
  });

  it('persists to localStorage', () => {
    recordRecentFile('/k/persisted.md', 'persisted.md');
    const raw = JSON.parse(localStorage.getItem('crucible:recentFiles') ?? '[]');
    expect(raw[0]).toEqual({ absPath: '/k/persisted.md', name: 'persisted.md' });
  });

  /**
   * The write is what the key is for.
   *
   * The list on the empty centre is wrong the moment an open lands, and it
   * used to be right only because this module patched its own copy beside the
   * write — which left the server's order and the screen's order to agree by
   * coincidence.
   */
  it('makes the held list wrong, so the next sync sees the new entry', async () => {
    holdUnobservedEntries();
    syncRecentsFromServer();
    await waitFor(() => expect(reads).toBe(1));

    recordRecentFile('/k/fresh.md', 'fresh.md');
    await waitFor(() => expect(wrote).toHaveLength(1));

    syncRecentsFromServer();
    await waitFor(() => expect(recentFiles()[0].absPath).toBe('/k/fresh.md'));
    expect(reads).toBe(2);
  });

  // The negative: a list nothing changed is answered from what is held.
  it('asks the daemon once for two syncs with no open between them', async () => {
    holdUnobservedEntries();
    onServer = [{ abs_path: '/k/held.md', name: 'held.md', opened_at: 1 }];
    syncRecentsFromServer();
    await waitFor(() => expect(recentFiles()[0].absPath).toBe('/k/held.md'));

    syncRecentsFromServer();
    await new Promise((resolve) => setTimeout(resolve, 20));

    expect(reads).toBe(1);
  });
});
