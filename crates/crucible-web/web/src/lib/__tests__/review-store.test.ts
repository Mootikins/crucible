import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { FakeEventSource, installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { sessionEvents, resetSseForTests } from '@/lib/query/sse';
import type { ComposedHunk } from '@/lib/review-types';

const listReviewHunks = vi.fn();
const setHunkState = vi.fn(async () => ({ hunk_id: 'h1', state: 'accepted' as const }));
const setHunkStates = vi.fn(async () => ({ applied: [] as string[], failed: [] }));
const undoReject = vi.fn(async () => ({ applied: [] as string[], failed: [] }));
const addReviewComment = vi.fn(async () => ({ comment: {} }));
const resolveReviewComment = vi.fn(async () => ({ comment_id: 'c1' }));
vi.mock('@/lib/review-api', () => ({
  listReviewHunks: (...a: unknown[]) => listReviewHunks(...a),
  setHunkState: (...a: unknown[]) => setHunkState(...(a as [])),
  setHunkStates: (...a: unknown[]) => setHunkStates(...(a as [])),
  undoReject: (...a: unknown[]) => undoReject(...(a as [])),
  addReviewComment: (...a: unknown[]) => addReviewComment(...(a as [])),
  resolveReviewComment: (...a: unknown[]) => resolveReviewComment(...(a as [])),
}));

const {
  __resetReviewStore,
  REVIEW_REFRESH_DEBOUNCE_MS,
  indexToolCall,
  pendingReveal,
  reviewActions,
  reviewStore,
  toolCallLabel,
  useReviewSession,
} = await import('../review-store');

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
 * Answer every list with a FRESH copy. The store mutates hunk state
 * optimistically, and a mock resolving the same object twice would hand the
 * refresh back the optimistic value it was supposed to correct.
 */
const answer = (hunks: ComposedHunk[], comments: unknown[] = []) =>
  listReviewHunks.mockImplementation(async () => ({
    session_id: 's1',
    hunks: structuredClone(hunks),
    comments: structuredClone(comments),
  }));

const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  installFakeEventSource();
  answer([]);
  // clearMocks wipes call history, not implementations — a test that installs
  // a never-resolving one would otherwise hang every test after it.
  setHunkState.mockReset();
  setHunkState.mockResolvedValue({ hunk_id: 'h1', state: 'accepted' });
});

afterEach(() => {
  // The store first, because dropping a binding unsubscribes it; the roots
  // after, so a source of this case cannot answer the next one.
  __resetReviewStore();
  resetSseForTests();
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
    answer([
      hunk({ id: 'mine', state: 'unreviewed' }),
      hunk({ id: 'theirs', state: 'unreviewed', tool_call_ids: [] }),
      hunk({ id: 'done', state: 'accepted' }),
    ]);
    await reviewActions.refresh('s1');
    // An external hunk is the user's own edit; counting it as work owed would
    // make the queue argue for reviewing yourself.
    expect(reviewStore.unreviewedCount('s1')).toBe(1);
  });

  it('collects hunks for a path across every session under review', async () => {
    answer([hunk({ id: 'a' })]);
    await reviewActions.refresh('s1');
    answer([hunk({ id: 'b' })]);
    await reviewActions.refresh('s2');

    const found = reviewStore.hunksForOpenPath('/repo/src/a.rs');
    expect(found.map((f) => `${f.sessionId}:${f.hunk.id}`).sort()).toEqual(['s1:a', 's2:b']);
    expect(reviewStore.hunksForOpenPath('/repo/src/other.rs')).toEqual([]);
  });

  it('hunksForToolCall matches on the daemon call id', async () => {
    answer([hunk({ id: 'a', tool_call_ids: ['call-1', 'call-2'] })]);
    await reviewActions.refresh('s1');
    expect(reviewStore.hunksForToolCall('s1', 'call-2')).toHaveLength(1);
    expect(reviewStore.hunksForToolCall('s1', 'call-9')).toHaveLength(0);
  });
});

describe('scope', () => {
  it('a session lists under the session scope until asked for the turn', async () => {
    await reviewActions.refresh('s1');
    expect(listReviewHunks).toHaveBeenLastCalledWith('s1', 'session');
    expect(reviewStore.scope('s1')).toBe('session');

    await reviewActions.setScope('s1', 'turn');
    expect(reviewStore.scope('s1')).toBe('turn');
    expect(listReviewHunks).toHaveBeenLastCalledWith('s1', 'turn');
    expect(listReviewHunks).toHaveBeenCalledTimes(2);

    // The same scope again is not a round trip.
    await reviewActions.setScope('s1', 'turn');
    expect(listReviewHunks).toHaveBeenCalledTimes(2);
  });

  it('an answer for a scope the session has left is dropped', async () => {
    // The daemon echoes the scope it answered. A session-wide listing that
    // lands after the user switched to the turn would show the whole diff
    // under a control that says "Turn".
    listReviewHunks.mockImplementation(async (_id: string, scope: string) => ({
      session_id: 's1',
      scope: 'session',
      hunks: scope === 'session' ? [hunk({ id: 'whole' })] : [],
      comments: [],
    }));
    await reviewActions.setScope('s1', 'turn');
    expect(reviewStore.session('s1').hunks).toEqual([]);
    // ...and it does not count as a load either: nothing is known about the turn.
    expect(reviewStore.session('s1').loaded).toBe(false);
  });
});

describe('refresh', () => {
  it('a failed list does NOT mark the session loaded', async () => {
    listReviewHunks.mockRejectedValue(new Error('HTTP 500'));
    await reviewActions.refresh('s1');
    expect(reviewStore.session('s1').error).toContain('500');
    // Claiming "loaded" here would let every ToolCard announce that its edit
    // had been superseded, on nothing but a failed request.
    expect(reviewStore.session('s1').loaded).toBe(false);
  });
});

describe('mutations', () => {
  it('accept applies optimistically, then re-lists', async () => {
    answer([hunk({ id: 'h1' })]);
    await reviewActions.refresh('s1');

    let resolveCall: (v: unknown) => void = () => {};
    setHunkState.mockImplementation(
      () => new Promise((r) => (resolveCall = r as (v: unknown) => void)),
    );
    const pending = reviewActions.setState('s1', 'h1', 'accepted');

    expect(reviewStore.session('s1').hunks[0].state).toBe('accepted');
    answer([hunk({ id: 'h1', state: 'accepted' })]);
    resolveCall(undefined);
    await pending;
    expect(setHunkState).toHaveBeenCalledWith('s1', 'h1', 'accepted');
    expect(listReviewHunks).toHaveBeenCalledTimes(2);
  });

  it('reject IS set-state rejected — one operation, one disk write', async () => {
    answer([hunk({ id: 'h1' })]);
    await reviewActions.refresh('s1');
    await reviewActions.reject('s1', 'h1');
    expect(setHunkState).toHaveBeenCalledWith('s1', 'h1', 'rejected');
  });

  it('a failed accept still re-lists, so the optimistic mark cannot stick', async () => {
    answer([hunk({ id: 'h1' })]);
    await reviewActions.refresh('s1');
    setHunkState.mockRejectedValue(new Error('stale'));
    await expect(reviewActions.setState('s1', 'h1', 'accepted')).rejects.toThrow('stale');
    expect(listReviewHunks).toHaveBeenCalledTimes(2);
    expect(reviewStore.session('s1').hunks[0].state).toBe('unreviewed');
  });

  it('a bulk reject is ONE call with the ids in the order given, then a re-list', async () => {
    answer([hunk({ id: 'h1' }), hunk({ id: 'h2' })]);
    await reviewActions.refresh('s1');
    setHunkStates.mockResolvedValue({ applied: ['h2', 'h1'], failed: [] });

    const outcome = await reviewActions.rejectMany('s1', ['h2', 'h1']);

    expect(setHunkStates).toHaveBeenCalledTimes(1);
    expect(setHunkStates).toHaveBeenCalledWith('s1', ['h2', 'h1'], 'rejected');
    expect(setHunkState).not.toHaveBeenCalled();
    expect(outcome.applied).toEqual(['h2', 'h1']);
    expect(listReviewHunks).toHaveBeenCalledTimes(2);
  });

  it('a bulk decision marks every named hunk optimistically and a failed call re-lists', async () => {
    answer([hunk({ id: 'h1' }), hunk({ id: 'h2' }), hunk({ id: 'h3' })]);
    await reviewActions.refresh('s1');
    let resolveCall: (v: unknown) => void = () => {};
    setHunkStates.mockImplementation(
      () => new Promise((r) => (resolveCall = r as (v: unknown) => void)),
    );
    const pending = reviewActions.setStates('s1', ['h1', 'h3'], 'accepted');

    const states = reviewStore.session('s1').hunks.map((h) => h.state);
    expect(states).toEqual(['accepted', 'unreviewed', 'accepted']);
    resolveCall({ applied: ['h1', 'h3'], failed: [] });
    await pending;

    setHunkStates.mockRejectedValue(new Error('stale'));
    await expect(reviewActions.setStates('s1', ['h2'], 'accepted')).rejects.toThrow('stale');
    expect(listReviewHunks).toHaveBeenCalledTimes(3);
    expect(reviewStore.session('s1').hunks[1].state).toBe('unreviewed');
  });

  it('undo names no hunk, returns what the daemon restored, and re-lists', async () => {
    answer([hunk({ id: 'h1', state: 'rejected' })]);
    await reviewActions.refresh('s1');
    undoReject.mockResolvedValue({ applied: ['h1'], failed: [] });

    const outcome = await reviewActions.undoReject('s1');

    expect(undoReject).toHaveBeenCalledWith('s1');
    expect(outcome.applied).toEqual(['h1']);
    expect(listReviewHunks).toHaveBeenCalledTimes(2);
  });

  it('commenting and resolving both re-list', async () => {
    await reviewActions.comment('s1', { path: 'src/a.rs', line_start: 3, body: 'change this' });
    expect(addReviewComment).toHaveBeenCalled();
    await reviewActions.resolveComment('s1', 'c1');
    expect(resolveReviewComment).toHaveBeenCalledWith('s1', 'c1');
  });
});

describe('subscription lifecycle', () => {
  const bind = (id: () => string | undefined) => {
    let dispose = () => {};
    createRoot((d) => {
      dispose = d;
      useReviewSession(id);
    });
    return dispose;
  };

  /**
   * Stands in for `ChatProvider`, which reads the same stream for the
   * transcript. It is the other half of the fault this suite exists to find:
   * a chat pane and a changes panel on one session used to hold one
   * `EventSource` each.
   */
  const bindChatPane = (id: string) => sessionEvents(id).subscribe(() => {});

  it('a chat pane and the changes panel share ONE EventSource', async () => {
    const stopChat = bindChatPane('s1');
    const dispose = bind(() => 's1');
    await settle();

    // `onlyEventSource` fails with the count, which is the number that names
    // the fault: two sources means the panel went around the shared root.
    expect(onlyEventSource().url).toBe('/api/chat/events/s1');

    dispose();
    stopChat();
    await settle();
    expect(onlyEventSource().closed).toBe(true);
  });

  it('a second panel on the same session opens no second source', async () => {
    const d1 = bind(() => 's1');
    await settle();
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
    await settle();
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(listReviewHunks).toHaveBeenCalledTimes(1);

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

  it('a review_changed event re-lists, debounced across a burst', async () => {
    const dispose = bind(() => 's1');
    await settle();
    listReviewHunks.mockClear();
    const source = onlyEventSource();

    for (let i = 0; i < 5; i++) {
      source.emit('session_event', { type: 'session_event', event: 'review_changed', data: {} });
    }
    expect(listReviewHunks).not.toHaveBeenCalled();
    await new Promise((r) => setTimeout(r, REVIEW_REFRESH_DEBOUNCE_MS + 20));
    expect(listReviewHunks).toHaveBeenCalledTimes(1);
    dispose();
  });

  it('tool results and turn ends re-list too — nothing else announces new hunks', async () => {
    const dispose = bind(() => 's1');
    await settle();
    listReviewHunks.mockClear();
    const source = onlyEventSource();

    source.emit('tool_result', { type: 'tool_result', id: 'x', result: '' });
    await new Promise((r) => setTimeout(r, REVIEW_REFRESH_DEBOUNCE_MS + 20));
    expect(listReviewHunks).toHaveBeenCalledTimes(1);

    source.emit('message_complete', { type: 'message_complete' });
    await new Promise((r) => setTimeout(r, REVIEW_REFRESH_DEBOUNCE_MS + 20));
    expect(listReviewHunks).toHaveBeenCalledTimes(2);
    dispose();
  });

  it('a review_gate event records the block and refreshes immediately', async () => {
    const dispose = bind(() => 's1');
    await settle();
    listReviewHunks.mockClear();
    const source = onlyEventSource();

    source.emit('session_event', {
      type: 'session_event',
      event: 'review_gate',
      data: { blocked: true, tool: 'edit_file', path: '/repo/src/a.rs' },
    });
    expect(reviewStore.session('s1').gate).toEqual({
      blocked: true,
      tool: 'edit_file',
      path: '/repo/src/a.rs',
    });
    // Not debounced: a held agent is exactly when the user needs the queue.
    expect(listReviewHunks).toHaveBeenCalledTimes(1);

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
  it('restores a block a reload never saw the event for', async () => {
    listReviewHunks.mockResolvedValueOnce({
      session_id: 's1',
      hunks: [],
      comments: [],
      gate: { tool: 'edit_file', path: '/repo/src/a.rs' },
    });
    await reviewActions.refresh('s1');

    expect(reviewStore.session('s1').gate).toEqual({
      blocked: true,
      tool: 'edit_file',
      path: '/repo/src/a.rs',
    });
  });

  it('an explicit null clears a block that has since been released', async () => {
    listReviewHunks.mockResolvedValueOnce({
      session_id: 's1',
      hunks: [],
      comments: [],
      gate: { tool: 'edit_file', path: '/repo/src/a.rs' },
    });
    await reviewActions.refresh('s1');

    listReviewHunks.mockResolvedValueOnce({
      session_id: 's1',
      hunks: [],
      comments: [],
      gate: null,
    });
    await reviewActions.refresh('s1');

    expect(reviewStore.session('s1').gate).toBeNull();
  });

  it('a daemon that reports no gate key leaves the event-established block alone', async () => {
    listReviewHunks.mockResolvedValueOnce({
      session_id: 's1',
      hunks: [],
      comments: [],
      gate: { tool: 'edit_file', path: '/repo/src/a.rs' },
    });
    await reviewActions.refresh('s1');

    // An older daemon omits the key entirely. Treating that as "not blocked"
    // would erase a live block on every refresh.
    answer([]);
    await reviewActions.refresh('s1');

    expect(reviewStore.session('s1').gate?.blocked).toBe(true);
  });
});
