import { describe, it, expect, afterEach, beforeEach } from 'vitest';
import { createRoot } from 'solid-js';
import { waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { keys } from '../keys';
import {
  addReviewCommentOnce,
  resolveReviewCommentOnce,
  useSessionRecord,
} from '../review';

/**
 * The session record of a session, held under that session.
 *
 * The key is what lets the daemon's own event reach every reader. The session
 * stream says `review_changed`, and the route of that stream invalidates this
 * entry.
 *
 * Every write invalidates the same entry, because the answer to "what is
 * listed" is the daemon's and not a patch the browser can compute.
 */

const SESSION = 's1';

let env: TestQueryEnv;
let dispose: (() => void) | null = null;
/** The session of every record the daemon answered, in order. */
let listed: { session: string }[] = [];
/** The session of every comment listing the daemon answered, in order. */
let commentsListed: string[] = [];
/** The path of every write the daemon answered, in order. */
let wrote: string[] = [];

const FILE = {
  root: '/repo',
  path: 'a.md',
  status: { kind: 'modified' },
  added: 1,
  removed: 0,
  binary: false,
  too_large: false,
};

function reviewRoutes() {
  listed = [];
  commentsListed = [];
  wrote = [];
  const record = (name: string) => () => {
    wrote.push(name);
    return {};
  };
  return {
    'GET /api/diff': (request: Request) => {
      const session = new URL(request.url).searchParams.get('session') ?? '';
      listed.push({ session });
      return {
        id: `session-${session}`,
        source: { kind: 'session_record', session },
        files: session === SESSION ? [FILE] : [],
      };
    },
    'GET /api/diff/comments': (request: Request) => {
      const session = new URL(request.url).searchParams.get('session') ?? '';
      commentsListed.push(session);
      return { diffset: `session-${session}`, comments: [] };
    },
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

describe('useSessionRecord', () => {
  it('lists once for two readers of one session', async () => {
    const both = inRoot(() => ({
      panel: useSessionRecord(() => SESSION),
      other: useSessionRecord(() => SESSION),
    }));

    await waitFor(() => expect(both.panel.data).toBeDefined());
    await waitFor(() => expect(both.other.data).toBeDefined());
    expect(countOf('s1')).toBe(1);
    expect(env.client.getQueryData(keys.review(SESSION))).toEqual(both.panel.data);
  });

  it('reads the files of the session record and its comments', async () => {
    const query = inRoot(() => useSessionRecord(() => SESSION));

    await waitFor(() => expect(query.data).toBeDefined());
    expect(query.data?.files).toEqual([FILE]);
    expect(query.data?.comments).toEqual([]);
    expect(commentsListed).toEqual([SESSION]);
  });

  // The negative: the session is the key, so a delegated child's record is its
  // own entry and never the parent's.
  it('holds a second session apart from the first', async () => {
    const both = inRoot(() => ({
      parent: useSessionRecord(() => SESSION),
      child: useSessionRecord(() => 's2'),
    }));

    await waitFor(() => expect(both.parent.data).toBeDefined());
    await waitFor(() => expect(both.child.data).toBeDefined());
    expect(countOf('s1')).toBe(1);
    expect(countOf('s2')).toBe(1);
    expect(both.child.data?.files).toEqual([]);
  });

  it('lists nothing for a session nobody named', async () => {
    const query = inRoot(() => useSessionRecord(() => null));

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(query.data).toBeUndefined();
    expect(listed).toEqual([]);
  });
});

describe('the writes', () => {
  /**
   * Every one of them re-asks, because the listing is the daemon's answer
   * and not a patch the browser can compute from the reply.
   */
  const writes: [string, (id: string) => Promise<unknown>][] = [
    [
      'addReviewCommentOnce',
      (id) => addReviewCommentOnce(id, { path: 'a.rs', line_start: 1, body: 'x' }),
    ],
    ['resolveReviewCommentOnce', (id) => resolveReviewCommentOnce(id, 'c1')],
  ];

  for (const [name, run] of writes) {
    it(`${name} makes the listing wrong`, async () => {
      const query = inRoot(() => useSessionRecord(() => SESSION));
      await waitFor(() => expect(query.data).toBeDefined());
      expect(countOf('s1')).toBe(1);

      await run(SESSION);

      await waitFor(() => expect(countOf('s1')).toBe(2));
    });
  }

  it('leaves another session’s listing alone', async () => {
    const both = inRoot(() => ({
      parent: useSessionRecord(() => SESSION),
      child: useSessionRecord(() => 's2'),
    }));
    await waitFor(() => expect(both.parent.data).toBeDefined());
    await waitFor(() => expect(both.child.data).toBeDefined());

    await resolveReviewCommentOnce(SESSION, 'c1');

    await waitFor(() => expect(countOf('s1')).toBe(2));
    expect(countOf('s2')).toBe(1);
  });
});
