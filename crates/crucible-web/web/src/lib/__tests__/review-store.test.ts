import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { waitFor } from '@solidjs/testing-library';
import { FakeEventSource, installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { sessionEvents } from '@/lib/query/sse';
import {
  installSessionEventRoute,
  resetReviewInvalidationForTests,
  REVIEW_INVALIDATE_DEBOUNCE_MS,
} from '@/lib/query/routes/session';
import type { DiffFileEntry } from '@/lib/diffset';
import {
  __resetReviewStore,
  REVIEW_REFRESH_DEBOUNCE_MS,
  pendingReveal,
  reviewActions,
  reviewStore,
  useReviewSession,
} from '../review-store';

/**
 * The review slot, over the cache and the stream that feed it.
 *
 * Nothing in `@/lib/review-api` is stubbed. The listing is a cache entry now,
 * so a mocked module would count the calls that reach IT rather than the ones
 * that reach the daemon — and the two facts this suite exists for, that three
 * surfaces share one listing and that a burst of events costs one, are both
 * about what reaches the daemon. The listing is the session record diffset
 * (`GET /api/diff?session=`) and its comments.
 *
 * A slot exists only while a session is BOUND. That is not new — a slot per
 * session ever visited is a leak — but it is now the only way to fill one:
 * `refresh` marks the listing wrong, and it is the binding's observer that
 * asks again.
 */

let env: TestQueryEnv;
/** Every session record the daemon answered, by session. */
let listed: { session: string }[] = [];
/** Every write the daemon answered: its name, and the body it was sent. */
let wrote: { name: string; body: Record<string, unknown> }[] = [];
/** What each session's listing answers next. */
let answers = new Map<string, () => unknown>();
/** What each write answers next; a `Response` refuses. */
let writeAnswers = new Map<string, () => unknown>();

function file(over: Partial<DiffFileEntry> = {}): DiffFileEntry {
  return {
    root: '/repo',
    path: 'src/a.rs',
    status: { kind: 'modified' },
    added: 1,
    removed: 1,
    binary: false,
    too_large: false,
    ...over,
  };
}

/** Answer one session's record with a FRESH copy each time. */
function answer(session: string, files: DiffFileEntry[]): void {
  answers.set(session, () => ({
    id: `session-${session}`,
    source: { kind: 'session_record', session },
    files: structuredClone(files),
  }));
}

/** Answer one session's listing with whatever the case wants, once or always. */
function answerWith(session: string, body: () => unknown): void {
  answers.set(session, body);
}

/** A refusal in the envelope `request()` unwraps, carrying the daemon's words. */
function refuse(status: number, message: string): Response {
  return new Response(JSON.stringify({ error: { code: status, message } }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

/** How many times one session was listed. */
const countOf = (session: string) => listed.filter((seen) => seen.session === session).length;
/** How many times one write was sent. */
const writeCount = (name: string) => wrote.filter((seen) => seen.name === name).length;

const settle = () => new Promise((r) => setTimeout(r, 0));
/** Waits out the route's coalescing window and lets the listing it starts land. */
const afterDebounce = () =>
  new Promise((r) => setTimeout(r, REVIEW_INVALIDATE_DEBOUNCE_MS + 30));

/** The review routes of two sessions, recording everything they are asked. */
function reviewRoutes() {
  listed = [];
  wrote = [];
  answers = new Map();
  writeAnswers = new Map();
  answer('s1', []);
  answer('s2', []);

  const listing = (request: Request) => {
    const session = new URL(request.url).searchParams.get('session') ?? '';
    listed.push({ session });
    return answers.get(session)!();
  };
  const comments = (request: Request) => {
    const session = new URL(request.url).searchParams.get('session') ?? '';
    return { diffset: `session-${session}`, comments: [] };
  };
  const write = (name: string) => async (request: Request) => {
    const text = await request.text();
    wrote.push({ name, body: text ? (JSON.parse(text) as Record<string, unknown>) : {} });
    return writeAnswers.get(name)?.() ?? {};
  };

  const routes: Record<string, unknown> = {
    'GET /api/diff': listing,
    'GET /api/diff/comments': comments,
  };
  routes['POST /api/diff/comment/resolve'] = write('resolve');
  return routes as Parameters<typeof createTestQueryEnv>[0];
}

/** Binds a session the way a panel does, and answers when to let go. */
function bind(id: () => string | undefined): () => void {
  let dispose = () => {};
  createRoot((d) => {
    dispose = d;
    useReviewSession(id);
  });
  return dispose;
}

/** Binds one session and waits for its first listing to land. */
async function bound(session: string): Promise<() => void> {
  const dispose = bind(() => session);
  await waitFor(() => expect(reviewStore.session(session).loaded).toBe(true));
  return dispose;
}

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv(reviewRoutes());
  // The app installs the routes of every stream at start (`src/index.tsx`);
  // `review_changed` reaches no listing without this one.
  installSessionEventRoute();
});

afterEach(() => {
  // The store first, because dropping a binding unsubscribes it; the client
  // after, so a stream of this case cannot answer the next one.
  __resetReviewStore();
  resetReviewInvalidationForTests();
  env.restore();
  vi.clearAllMocks();
});

describe('reviewStore reads', () => {
  it('an unbound session reads as empty and NOT loaded', () => {
    const s = reviewStore.session('nope');
    expect(s.files).toEqual([]);
    // "Unloaded" is what stops the panel claiming no changes before anyone
    // has looked.
    expect(s.loaded).toBe(false);
  });

  it('holds the files of each session record apart', async () => {
    answer('s1', [file({ path: 'a.md' })]);
    answer('s2', [file({ path: 'b.md' }), file({ path: 'c.md' })]);
    const d1 = await bound('s1');
    const d2 = await bound('s2');

    expect(reviewStore.session('s1').files.map((f) => f.path)).toEqual(['a.md']);
    expect(reviewStore.session('s2').files.map((f) => f.path)).toEqual(['b.md', 'c.md']);
    d1();
    d2();
  });
});

describe('refresh', () => {
  it('a failed list does NOT mark the session loaded', async () => {
    answerWith('s1', () => refuse(500, 'HTTP 500'));
    const dispose = bind(() => 's1');

    await waitFor(() => expect(reviewStore.session('s1').error).toContain('500'));
    // Claiming "loaded" here would let the panel announce that the session
    // changed nothing, on nothing but a failed request.
    expect(reviewStore.session('s1').loaded).toBe(false);
    dispose();
  });

  /**
   * `refresh` marks the listing wrong; the binding's observer asks again.
   *
   * It used to fetch by itself, which meant a caller could fill a slot for a
   * session nothing was bound to — a slot that then outlived every reader.
   */
  it('re-lists a bound session', async () => {
    const dispose = await bound('s1');

    await reviewActions.refresh('s1');

    await waitFor(() => expect(countOf('s1')).toBe(2));
    dispose();
  });
});

describe('mutations', () => {

  it('resolving a comment re-lists', async () => {
    const dispose = await bound('s1');
    writeAnswers.set('resolve', () => ({ comment_id: 'c1' }));

    await reviewActions.resolveComment('s1', 'c1');
    await waitFor(() => expect(countOf('s1')).toBe(2));
    expect(writeCount('resolve')).toBe(1);
    expect(wrote[0].body).toEqual({
      source: { kind: 'session_record', session: 's1' },
      comment_id: 'c1',
    });
    dispose();
  });
});

describe('subscription lifecycle', () => {
  /**
   * Stands in for `ChatProvider`, which reads the same stream for the
   * transcript. It is the other half of the fault this suite exists to find:
   * a chat pane and a changes panel on one session used to hold one
   * `EventSource` each.
   */
  const bindChatPane = (id: string) => sessionEvents(id).subscribe(() => {});

  it('a chat pane and the changes panel share ONE EventSource', async () => {
    const stopChat = bindChatPane('s1');
    const dispose = await bound('s1');

    // `onlyEventSource` fails with the count, which is the number that names
    // the fault: two sources means the panel went around the shared root.
    expect(onlyEventSource().url).toBe('/api/chat/events/s1');

    dispose();
    stopChat();
    await settle();
    expect(onlyEventSource().closed).toBe(true);
  });

  it('a second panel on the same session opens no second source', async () => {
    const d1 = await bound('s1');
    const opened = FakeEventSource.instances.length;

    const d2 = bind(() => 's1');
    await settle();
    expect(FakeEventSource.instances).toHaveLength(opened);

    d1();
    d2();
    await settle();
  });

  it('two consumers share ONE stream and one initial fetch', async () => {
    const d1 = bind(() => 's1');
    const d2 = bind(() => 's1');
    await waitFor(() => expect(reviewStore.session('s1').loaded).toBe(true));
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(countOf('s1')).toBe(1);

    // The stream outlives the first consumer — the panel closing must not
    // blind the editor.
    d1();
    await settle();
    expect(onlyEventSource().closed).toBe(false);
    d2();
    await settle();
    expect(onlyEventSource().closed).toBe(true);
    // ...and the slot is deleted, not left as an empty husk per session ever
    // visited.
    expect(reviewStore.session('s1').loaded).toBe(false);
  });

  /**
   * The burst of one turn costs ONE listing.
   *
   * The route of the stream owns this now, so every reader of the diff gets
   * it, not only this store. The coalescing is load-bearing: an invalidation
   * does not fold concurrent refetches of one key into one request, it cancels
   * the one in flight and starts another.
   */
  it('a review_changed burst re-lists once', async () => {
    const dispose = await bound('s1');
    const source = onlyEventSource();

    for (let i = 0; i < 5; i++) {
      source.emit('session_event', { type: 'session_event', event: 'review_changed', data: {} });
    }
    expect(countOf('s1')).toBe(1);

    await afterDebounce();

    expect(countOf('s1')).toBe(2);
    dispose();
  });

  it('tool results and turn ends re-list too — nothing else announces a changed file', async () => {
    const dispose = await bound('s1');
    const source = onlyEventSource();

    source.emit('tool_result', { type: 'tool_result', id: 'x', result: '' });
    await new Promise((r) => setTimeout(r, REVIEW_REFRESH_DEBOUNCE_MS + 30));
    await waitFor(() => expect(countOf('s1')).toBe(2));

    source.emit('message_complete', { type: 'message_complete' });
    await new Promise((r) => setTimeout(r, REVIEW_REFRESH_DEBOUNCE_MS + 30));
    await waitFor(() => expect(countOf('s1')).toBe(3));
    dispose();
  });

  it('following the active session releases the one it left', async () => {
    const [id, setId] = createSignal<string | undefined>('s1');
    const dispose = createRoot((d) => {
      useReviewSession(id);
      return d;
    });
    await settle();
    setId('s2');
    await settle();
    // Exactly one close: the old session's stream, not the new one's.
    expect(FakeEventSource.instances).toHaveLength(2);
    expect(FakeEventSource.instances[0].closed).toBe(true);
    expect(FakeEventSource.instances[1].closed).toBe(false);
    dispose();
  });
});

describe('reveal channel', () => {
  it('reveal sets a target the editor consumes and clears', () => {
    reviewActions.reveal('/repo/src/a.rs', 42);
    expect(pendingReveal()).toEqual({ path: '/repo/src/a.rs', line: 42 });
    reviewActions.clearReveal();
    expect(pendingReveal()).toBeNull();
  });
});
