import { describe, it, expect, afterEach, beforeEach } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { ReviewScope } from '@/lib/review-types';
import { keys } from '../keys';
import {
  addReviewCommentOnce,
  invalidateReview,
  rebaseReviewOnce,
  resolveReviewCommentOnce,
  setHunkStateOnce,
  setHunkStatesOnce,
  undoRejectOnce,
  useReviewHunks,
} from '../review';

/**
 * The composed diff of a session, held under that session.
 *
 * Three surfaces read one listing — the changes panel, the editor gutter and
 * every tool card on screen — and the key is what lets the daemon's own event
 * reach all three. The session stream says `review_changed`, and the route of
 * that stream invalidates this entry; before it was held here, that
 * invalidation named a key nobody had.
 *
 * Every write invalidates the same entry, because the daemon decides what a
 * decision did: a reject reverts lines on disk and can renumber every hunk
 * after it, so the answer to "what is left" is the daemon's and not a patch
 * the browser can compute.
 */

const SESSION = 's1';

let env: TestQueryEnv;
let dispose: (() => void) | null = null;
/** The session and scope of every listing the daemon answered, in order. */
let listed: { session: string; scope: string }[] = [];
/** The path of every write the daemon answered, in order. */
let wrote: string[] = [];

function reviewRoutes() {
  listed = [];
  wrote = [];
  const record = (name: string) => () => {
    wrote.push(name);
    return { applied: [], failed: [] };
  };
  return {
    'GET /api/session/s1/review/hunks': (request: Request) => {
      const url = new URL(request.url);
      listed.push({ session: 's1', scope: url.searchParams.get('scope') ?? '' });
      return { session_id: 's1', hunks: [], comments: [] };
    },
    'GET /api/session/s2/review/hunks': (request: Request) => {
      const url = new URL(request.url);
      listed.push({ session: 's2', scope: url.searchParams.get('scope') ?? '' });
      return { session_id: 's2', hunks: [], comments: [] };
    },
    'POST /api/session/s1/review/state': record('state'),
    'POST /api/session/s1/review/states': record('states'),
    'POST /api/session/s1/review/undo-reject': record('undo'),
    'POST /api/session/s1/review/rebase': () => ({ roots: [] }),
    'POST /api/session/s1/review/comment': record('comment'),
    'POST /api/session/s1/review/comment/c1/resolve': record('resolve'),
  };
}

const countOf = (session: string) => listed.filter((seen) => seen.session === session).length;

function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

beforeEach(() => {
  env = createTestQueryEnv(reviewRoutes());
});

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
});

describe('useReviewHunks', () => {
  it('lists once for two readers of one session', async () => {
    const both = inRoot(() => ({
      panel: useReviewHunks(() => SESSION, () => 'session'),
      gutter: useReviewHunks(() => SESSION, () => 'session'),
    }));

    await waitFor(() => expect(both.panel.data).toBeDefined());
    await waitFor(() => expect(both.gutter.data).toBeDefined());
    expect(countOf('s1')).toBe(1);
    expect(env.client.getQueryData(keys.review(SESSION))).toEqual(both.panel.data);
  });

  // The negative: the session is the key, so a delegated child's ledger is its
  // own entry and never the parent's.
  it('holds a second session apart from the first', async () => {
    const both = inRoot(() => ({
      parent: useReviewHunks(() => SESSION, () => 'session'),
      child: useReviewHunks(() => 's2', () => 'session'),
    }));

    await waitFor(() => expect(both.parent.data).toBeDefined());
    await waitFor(() => expect(both.child.data).toBeDefined());
    expect(countOf('s1')).toBe(1);
    expect(countOf('s2')).toBe(1);
  });

  /**
   * The scope is asked, not keyed.
   *
   * The slot IS the listing: the panel, the gutter and the transcript read one
   * array, and a second array per scope would be a second listing and a second
   * reader of the same stream. Switching scope therefore re-asks under the
   * same key, with the other word.
   */
  it('asks under the scope its reader is on, and re-asks when that changes', async () => {
    const [scope, setScope] = createSignal<ReviewScope>('session');
    const query = inRoot(() => useReviewHunks(() => SESSION, scope));
    await waitFor(() => expect(query.data).toBeDefined());
    expect(listed[0].scope).toBe('session');

    setScope('turn');
    await invalidateReview(SESSION);

    await waitFor(() => expect(countOf('s1')).toBe(2));
    expect(listed[1].scope).toBe('turn');
  });

  it('lists nothing for a session nobody named', async () => {
    const query = inRoot(() => useReviewHunks(() => null, () => 'session'));

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(query.data).toBeUndefined();
    expect(listed).toEqual([]);
  });
});

describe('the writes', () => {
  /**
   * Every one of them re-asks.
   *
   * The daemon decides what a decision did. A reject reverts lines on disk and
   * renumbers every hunk after it, so "what is left" is the daemon's answer
   * and not a patch the browser can compute from the reply.
   */
  const writes: [string, (id: string) => Promise<unknown>][] = [
    ['setHunkStateOnce', (id) => setHunkStateOnce(id, 'h1', 'accepted')],
    ['setHunkStatesOnce', (id) => setHunkStatesOnce(id, ['h1'], 'rejected')],
    ['undoRejectOnce', (id) => undoRejectOnce(id)],
    ['rebaseReviewOnce', (id) => rebaseReviewOnce(id)],
    [
      'addReviewCommentOnce',
      (id) => addReviewCommentOnce(id, { path: 'a.rs', line_start: 1, body: 'x' }),
    ],
    ['resolveReviewCommentOnce', (id) => resolveReviewCommentOnce(id, 'c1')],
  ];

  for (const [name, run] of writes) {
    it(`${name} makes the listing wrong`, async () => {
      const query = inRoot(() => useReviewHunks(() => SESSION, () => 'session'));
      await waitFor(() => expect(query.data).toBeDefined());
      expect(countOf('s1')).toBe(1);

      await run(SESSION);

      await waitFor(() => expect(countOf('s1')).toBe(2));
    });
  }

  it('leaves another session’s listing alone', async () => {
    const both = inRoot(() => ({
      parent: useReviewHunks(() => SESSION, () => 'session'),
      child: useReviewHunks(() => 's2', () => 'session'),
    }));
    await waitFor(() => expect(both.parent.data).toBeDefined());
    await waitFor(() => expect(both.child.data).toBeDefined());

    await setHunkStateOnce(SESSION, 'h1', 'accepted');

    await waitFor(() => expect(countOf('s1')).toBe(2));
    expect(countOf('s2')).toBe(1);
  });
});
