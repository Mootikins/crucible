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
import type { ComposedHunk } from '@/lib/review-types';
import {
  __resetReviewStore,
  REVIEW_REFRESH_DEBOUNCE_MS,
  indexToolCall,
  pendingReveal,
  reviewActions,
  reviewStore,
  toolCallLabel,
  useReviewSession,
} from '../review-store';

/**
 * The review slot, over the cache and the stream that feed it.
 *
 * Nothing in `@/lib/review-api` is stubbed. The listing is a cache entry now,
 * so a mocked module would count the calls that reach IT rather than the ones
 * that reach the daemon — and the two facts this suite exists for, that three
 * surfaces share one listing and that a burst of events costs one, are both
 * about what reaches the daemon.
 *
 * A slot exists only while a session is BOUND. That is not new — a slot per
 * session ever visited is a leak — but it is now the only way to fill one:
 * `refresh` marks the listing wrong, and it is the binding's observer that
 * asks again.
 */

let env: TestQueryEnv;
/** Every listing the daemon answered: the session, and the scope asked for. */
let listed: { session: string; scope: string }[] = [];
/** Every write the daemon answered: its name, and the body it was sent. */
let wrote: { name: string; body: Record<string, unknown> }[] = [];
/** What each session's listing answers next. */
let answers = new Map<string, () => unknown>();
/** What each write answers next; a `Response` refuses. */
let writeAnswers = new Map<string, () => unknown>();

function hunk(over: Partial<ComposedHunk> = {}): ComposedHunk {
  return {
    id: 'h1',
    root: '/repo',
    path: 'src/a.rs',
    base_range: { start: 1, end: 3 },
    current_range: { start: 1, end: 3 },
    before_content: 'old\n',
    after_content: 'new\n',
    tool_call_ids: ['call-1'],
    state: 'unreviewed',
    reapplied: false,
    ...over,
  };
}

/**
 * Answer one session's listing with a FRESH copy each time.
 *
 * The store marks hunk state optimistically, and a route answering the same
 * object twice would hand the refetch back the optimistic value it is supposed
 * to correct.
 */
function answer(session: string, hunks: ComposedHunk[], comments: unknown[] = []): void {
  answers.set(session, () => ({
    session_id: session,
    hunks: structuredClone(hunks),
    comments: structuredClone(comments),
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
/** The scope of the last listing of one session. */
const lastScopeOf = (session: string) =>
  listed.filter((seen) => seen.session === session).at(-1)?.scope;
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

  const listing = (session: string) => (request: Request) => {
    listed.push({
      session,
      scope: new URL(request.url).searchParams.get('scope') ?? '',
    });
    return answers.get(session)!();
  };
  const write = (name: string) => async (request: Request) => {
    wrote.push({ name, body: (await request.json()) as Record<string, unknown> });
    return writeAnswers.get(name)?.() ?? { applied: [], failed: [] };
  };

  const routes: Record<string, unknown> = {};
  for (const session of ['s1', 's2']) {
    const base = `/api/session/${session}/review`;
    routes[`GET ${base}/hunks`] = listing(session);
    routes[`POST ${base}/state`] = write('state');
    routes[`POST ${base}/states`] = write('states');
    routes[`POST ${base}/undo-reject`] = write('undo');
    routes[`POST ${base}/rebase`] = write('rebase');
    routes[`POST ${base}/comment`] = write('comment');
    routes[`POST ${base}/comment/c1/resolve`] = write('resolve');
  }
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
    expect(s.hunks).toEqual([]);
    // The difference matters: "unloaded" is what stops ToolCard claiming a
    // call was superseded before anyone has looked.
    expect(s.loaded).toBe(false);
  });

  it('unreviewed count excludes external hunks', async () => {
    answer('s1', [
      hunk({ id: 'mine', state: 'unreviewed' }),
      hunk({ id: 'theirs', state: 'unreviewed', tool_call_ids: [] }),
      hunk({ id: 'done', state: 'accepted' }),
    ]);
    const dispose = await bound('s1');
    // An external hunk is the user's own edit; counting it as work owed would
    // make the queue argue for reviewing yourself.
    expect(reviewStore.unreviewedCount('s1')).toBe(1);
    dispose();
  });

  it('collects hunks for a path across every session under review', async () => {
    answer('s1', [hunk({ id: 'a' })]);
    answer('s2', [hunk({ id: 'b' })]);
    const d1 = await bound('s1');
    const d2 = await bound('s2');

    const found = reviewStore.hunksForOpenPath('/repo/src/a.rs');
    expect(found.map((f) => `${f.sessionId}:${f.hunk.id}`).sort()).toEqual(['s1:a', 's2:b']);
    expect(reviewStore.hunksForOpenPath('/repo/src/other.rs')).toEqual([]);
    d1();
    d2();
  });

  it('hunksForToolCall matches on the daemon call id', async () => {
    answer('s1', [hunk({ id: 'a', tool_call_ids: ['call-1', 'call-2'] })]);
    const dispose = await bound('s1');
    expect(reviewStore.hunksForToolCall('s1', 'call-2')).toHaveLength(1);
    expect(reviewStore.hunksForToolCall('s1', 'call-9')).toHaveLength(0);
    dispose();
  });
});

describe('scope', () => {
  it('a session lists under the session scope until asked for the turn', async () => {
    const dispose = await bound('s1');
    expect(lastScopeOf('s1')).toBe('session');
    expect(reviewStore.scope('s1')).toBe('session');

    await reviewActions.setScope('s1', 'turn');
    await waitFor(() => expect(countOf('s1')).toBe(2));
    expect(reviewStore.scope('s1')).toBe('turn');
    expect(lastScopeOf('s1')).toBe('turn');

    // The same scope again is not a round trip.
    await reviewActions.setScope('s1', 'turn');
    await settle();
    expect(countOf('s1')).toBe(2);
    dispose();
  });

  it('an answer for a scope the session has left is dropped', async () => {
    // The daemon echoes the scope it answered. A session-wide listing that
    // lands after the user switched to the turn would show the whole diff
    // under a control that says "Turn".
    // The daemon answers the whole session's diff whatever it is asked.
    answerWith('s1', () => ({
      session_id: 's1',
      scope: 'session',
      hunks: [hunk({ id: 'whole' })],
      comments: [],
    }));
    await reviewActions.setScope('s1', 'turn');
    const dispose = bind(() => 's1');
    await waitFor(() => expect(countOf('s1')).toBe(1));
    expect(lastScopeOf('s1')).toBe('turn');

    expect(reviewStore.session('s1').hunks).toEqual([]);
    // ...and it does not count as a load either: nothing is known about the turn.
    expect(reviewStore.session('s1').loaded).toBe(false);
    dispose();
  });
});

describe('refresh', () => {
  it('a failed list does NOT mark the session loaded', async () => {
    answerWith('s1', () => refuse(500, 'HTTP 500'));
    const dispose = bind(() => 's1');

    await waitFor(() => expect(reviewStore.session('s1').error).toContain('500'));
    // Claiming "loaded" here would let every ToolCard announce that its edit
    // had been superseded, on nothing but a failed request.
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
  it('accept applies optimistically, then re-lists', async () => {
    answer('s1', [hunk({ id: 'h1' })]);
    const dispose = await bound('s1');

    let release: () => void = () => {};
    const held = new Promise<void>((r) => (release = r));
    writeAnswers.set('state', () => held.then(() => ({ hunk_id: 'h1', state: 'accepted' })));
    const pending = reviewActions.setState('s1', 'h1', 'accepted');

    await waitFor(() => expect(reviewStore.session('s1').hunks[0].state).toBe('accepted'));
    answer('s1', [hunk({ id: 'h1', state: 'accepted' })]);
    release();
    await pending;

    expect(wrote.filter((w) => w.name === 'state')[0].body).toEqual({
      hunk_id: 'h1',
      state: 'accepted',
    });
    await waitFor(() => expect(countOf('s1')).toBe(2));
    dispose();
  });

  it('reject IS set-state rejected — one operation, one disk write', async () => {
    answer('s1', [hunk({ id: 'h1' })]);
    const dispose = await bound('s1');

    await reviewActions.reject('s1', 'h1');

    expect(wrote.filter((w) => w.name === 'state')[0].body).toEqual({
      hunk_id: 'h1',
      state: 'rejected',
    });
    dispose();
  });

  it('a failed accept still re-lists, so the optimistic mark cannot stick', async () => {
    answer('s1', [hunk({ id: 'h1' })]);
    const dispose = await bound('s1');
    writeAnswers.set('state', () => refuse(409, 'stale'));

    await expect(reviewActions.setState('s1', 'h1', 'accepted')).rejects.toThrow('stale');

    await waitFor(() => expect(countOf('s1')).toBe(2));
    await waitFor(() => expect(reviewStore.session('s1').hunks[0].state).toBe('unreviewed'));
    dispose();
  });

  it('a bulk reject is ONE call with the ids in the order given, then a re-list', async () => {
    answer('s1', [hunk({ id: 'h1' }), hunk({ id: 'h2' })]);
    const dispose = await bound('s1');
    writeAnswers.set('states', () => ({ applied: ['h2', 'h1'], failed: [] }));

    const outcome = await reviewActions.rejectMany('s1', ['h2', 'h1']);

    expect(writeCount('states')).toBe(1);
    expect(wrote.filter((w) => w.name === 'states')[0].body).toEqual({
      hunk_ids: ['h2', 'h1'],
      state: 'rejected',
    });
    expect(writeCount('state')).toBe(0);
    expect(outcome.applied).toEqual(['h2', 'h1']);
    await waitFor(() => expect(countOf('s1')).toBe(2));
    dispose();
  });

  it('a bulk decision marks every named hunk optimistically and a failed call re-lists', async () => {
    answer('s1', [hunk({ id: 'h1' }), hunk({ id: 'h2' }), hunk({ id: 'h3' })]);
    const dispose = await bound('s1');

    let release: () => void = () => {};
    const held = new Promise<void>((r) => (release = r));
    writeAnswers.set('states', () => held.then(() => ({ applied: ['h1', 'h3'], failed: [] })));
    const pending = reviewActions.setStates('s1', ['h1', 'h3'], 'accepted');

    await waitFor(() =>
      expect(reviewStore.session('s1').hunks.map((h) => h.state)).toEqual([
        'accepted',
        'unreviewed',
        'accepted',
      ]),
    );
    release();
    await pending;

    writeAnswers.set('states', () => refuse(409, 'stale'));
    await expect(reviewActions.setStates('s1', ['h2'], 'accepted')).rejects.toThrow('stale');

    await waitFor(() => expect(countOf('s1')).toBe(3));
    await waitFor(() => expect(reviewStore.session('s1').hunks[1].state).toBe('unreviewed'));
    dispose();
  });

  it('undo names no hunk, returns what the daemon restored, and re-lists', async () => {
    answer('s1', [hunk({ id: 'h1', state: 'rejected' })]);
    const dispose = await bound('s1');
    writeAnswers.set('undo', () => ({ applied: ['h1'], failed: [] }));

    const outcome = await reviewActions.undoReject('s1');

    expect(wrote.filter((w) => w.name === 'undo')[0].body).toEqual({});
    expect(outcome.applied).toEqual(['h1']);
    await waitFor(() => expect(countOf('s1')).toBe(2));
    dispose();
  });

  it('commenting and resolving both re-list', async () => {
    const dispose = await bound('s1');
    writeAnswers.set('comment', () => ({ comment: {} }));
    writeAnswers.set('resolve', () => ({ comment_id: 'c1' }));

    await reviewActions.comment('s1', { path: 'src/a.rs', line_start: 3, body: 'change this' });
    await waitFor(() => expect(countOf('s1')).toBe(2));

    await reviewActions.resolveComment('s1', 'c1');
    await waitFor(() => expect(countOf('s1')).toBe(3));
    expect(writeCount('resolve')).toBe(1);
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

  it('tool results and turn ends re-list too — nothing else announces new hunks', async () => {
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

  it('a review_gate event records the block and re-lists', async () => {
    const dispose = await bound('s1');
    const source = onlyEventSource();

    source.emit('session_event', {
      type: 'session_event',
      event: 'review_gate',
      data: { blocked: true, tool: 'edit_file', path: '/repo/src/a.rs' },
    });

    // The block is state only this slot holds, so it lands at once — the chip
    // must not wait on a round trip.
    expect(reviewStore.session('s1').gate).toEqual({
      blocked: true,
      tool: 'edit_file',
      path: '/repo/src/a.rs',
    });
    await afterDebounce();
    expect(countOf('s1')).toBe(2);

    source.emit('session_event', {
      type: 'session_event',
      event: 'review_gate',
      data: { blocked: false },
    });
    expect(reviewStore.session('s1').gate?.blocked).toBe(false);
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

describe('attribution labels', () => {
  it('falls back to a short id until the transcript publishes the name', () => {
    expect(toolCallLabel('call_abcdef0123456')).toBe('call_abc…');
    indexToolCall('call_abcdef0123456', 'edit_file');
    expect(toolCallLabel('call_abcdef0123456')).toBe('edit_file');
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

/**
 * The gate chip has to survive a reload. `review_gate` is an event, and events
 * are dropped rather than replayed across a reconnect, so without the listing
 * carrying the block a tab opened mid-turn shows nothing while the agent sits
 * parked — the exact "agent looks hung" failure the chip exists to prevent.
 */
describe('gate state from a listing', () => {
  const blocked = () => ({
    session_id: 's1',
    hunks: [],
    comments: [],
    gate: { tool: 'edit_file', path: '/repo/src/a.rs' },
  });

  it('restores a block a reload never saw the event for', async () => {
    answerWith('s1', blocked);
    const dispose = await bound('s1');

    expect(reviewStore.session('s1').gate).toEqual({
      blocked: true,
      tool: 'edit_file',
      path: '/repo/src/a.rs',
    });
    dispose();
  });

  it('an explicit null clears a block that has since been released', async () => {
    answerWith('s1', blocked);
    const dispose = await bound('s1');

    answerWith('s1', () => ({ session_id: 's1', hunks: [], comments: [], gate: null }));
    await reviewActions.refresh('s1');

    await waitFor(() => expect(reviewStore.session('s1').gate).toBeNull());
    dispose();
  });

  it('a daemon that reports no gate key leaves the event-established block alone', async () => {
    answerWith('s1', blocked);
    const dispose = await bound('s1');

    // An older daemon omits the key entirely. Treating that as "not blocked"
    // would erase a live block on every refresh.
    answer('s1', []);
    await reviewActions.refresh('s1');

    await waitFor(() => expect(countOf('s1')).toBe(2));
    expect(reviewStore.session('s1').gate?.blocked).toBe(true);
    dispose();
  });
});
