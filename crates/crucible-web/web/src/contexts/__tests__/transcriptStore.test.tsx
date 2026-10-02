import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { waitFor } from '@solidjs/testing-library';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { advanceSessionCursor, sessionCursor } from '@/lib/query/sse';
import {
  addOptimisticTurn,
  confirmOptimisticTurn,
  daemonTranscriptOf,
  renderTranscript,
  resetTranscriptsForTests,
  retainTranscript,
  seedTranscript,
  patchTranscript,
  setTranscriptStreaming,
  transcriptOf,
} from '../transcriptStore';
import type { Transcript } from '@/lib/transcript';
import { append, emitOps, historyOf, segment, upsert, userTurn } from '@/test-utils/transcript';

// No `vi.mock('@/lib/api')`. The stream is the FakeEventSource, the history
// route answers over the mock fetch, and the store runs through the REAL
// sessionEvents root.

const ID = 'store-session';
// `session.history` is an RPC method now ([[Simplification Plan#Step 19]]
// item 9); the browser calls `rpc('session.history', ...)` now.
const HISTORY_ROUTE = 'POST /api/rpc/session.history';

/** What the history route answers next. */
let snapshot: Transcript;
let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  snapshot = { as_of_seq: 0, items: [] };
  env = createTestQueryEnv({
    [HISTORY_ROUTE]: () => ({ session_id: ID, history: [], total_events: 0, transcript: snapshot }),
  });
});

afterEach(() => {
  resetTranscriptsForTests();
  env.restore();
});

function stream(): FakeEventSource {
  return FakeEventSource.instances[0]!;
}

/** The rows the pane draws, as `role:content`. */
function rows(): string[] {
  return renderTranscript(daemonTranscriptOf(ID), transcriptOf(ID)).map(
    (message) => `${message.role}:${message.content}`,
  );
}

function start(seed: Transcript = { as_of_seq: 0, items: [] }): void {
  retainTranscript(ID);
  seedTranscript(ID, seed, Date.now());
}

describe('the transcript store applies the daemon ops', () => {
  it('applies each frame in order onto the snapshot', () => {
    start(historyOf(ID, [userTurn('t1', 'hi')], 4).transcript);

    emitOps(stream(), 5, [upsert(segment('t1', 0, '', { streaming: true })), append('t1-seg-0', 0, 'Hel')]);
    emitOps(stream(), 6, [append('t1-seg-0', 3, 'lo')]);

    expect(rows()).toEqual(['user:hi', 'assistant:Hello']);
    expect(daemonTranscriptOf(ID)?.as_of_seq).toBe(6);
  });

  it('drops a frame at or below the seq of the snapshot', () => {
    start(historyOf(ID, [userTurn('t1', 'hi'), segment('t1', 0, 'Hello')], 6).transcript);

    // A replay of an op that the snapshot holds would append twice.
    emitOps(stream(), 6, [append('t1-seg-0', 3, 'lo')]);

    expect(rows()).toEqual(['user:hi', 'assistant:Hello']);
    expect(env.fetch.calls(HISTORY_ROUTE)).toBe(0);
  });

  it('keeps a frame that arrives before the snapshot, and applies it after', () => {
    retainTranscript(ID);
    emitOps(stream(), 3, [upsert(userTurn('t2', 'second'))]);
    expect(rows()).toEqual([]);

    seedTranscript(ID, historyOf(ID, [userTurn('t1', 'first')], 2).transcript, Date.now());

    expect(rows()).toEqual(['user:first', 'user:second']);
  });

  it('ignores an older snapshot', () => {
    start(historyOf(ID, [userTurn('t1', 'hi')], 1).transcript);
    emitOps(stream(), 2, [upsert(segment('t1', 0, 'live'))]);

    seedTranscript(ID, historyOf(ID, [userTurn('t1', 'hi')], 1).transcript, Date.now());

    expect(rows()).toEqual(['user:hi', 'assistant:live']);
  });

  it('records the seq of the snapshot as the resume cursor', () => {
    start(historyOf(ID, [userTurn('t1', 'hi')], 9).transcript);
    expect(sessionCursor(ID)).toBe(9);
  });
});

describe('the transcript store reads a new snapshot', () => {
  it('when an op does not fit', async () => {
    start(historyOf(ID, [userTurn('t1', 'hi'), segment('t1', 0, 'Hel', { streaming: true })], 2).transcript);
    // The daemon's fold holds the whole segment; this browser missed an op.
    snapshot = historyOf(ID, [userTurn('t1', 'hi'), segment('t1', 0, 'Hello!', { streaming: true })], 4).transcript;

    emitOps(stream(), 4, [append('t1-seg-0', 5, '!')]);

    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBe(1));
    await waitFor(() => expect(rows()).toEqual(['user:hi', 'assistant:Hello!']));
  });

  it('applies the frames above the new snapshot after it', async () => {
    start(historyOf(ID, [userTurn('t1', 'hi')], 1).transcript);
    // The daemon stores no text delta: the new snapshot has the turn, and
    // the segment comes only from the frames.
    snapshot = historyOf(ID, [userTurn('t1', 'hi')], 1).transcript;
    emitOps(stream(), 2, [upsert(segment('t1', 0, '', { streaming: true })), append('t1-seg-0', 0, 'ab')]);
    // A frame for an item that is not here does not fit.
    emitOps(stream(), 3, [append('t9-seg-0', 0, 'x')]);
    emitOps(stream(), 4, [append('t1-seg-0', 2, 'c')]);

    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBe(1));
    await waitFor(() => expect(rows()).toEqual(['user:hi', 'assistant:abc']));
  });

  it('on a stream gap', async () => {
    start();
    snapshot = historyOf(ID, [userTurn('t1', 'lost in the gap')], 3).transcript;

    stream().emit('stream_gap', { event: 'stream_gap', data: { dropped: 2 } });

    await waitFor(() => expect(rows()).toEqual(['user:lost in the gap']));
  });

  it('on a reconnect', async () => {
    start();
    stream().open();
    snapshot = historyOf(ID, [userTurn('t1', 'sent while away')], 3).transcript;

    stream().open();

    await waitFor(() => expect(rows()).toEqual(['user:sent while away']));
  });

  it('does not let a reconnect snapshot suppress the replayed turn completion', async () => {
    start(historyOf(ID, [userTurn('t1', 'check tools')], 1).transcript);
    stream().open();
    patchTranscript(ID, { isLoading: true });
    setTranscriptStreaming(ID, true);
    stream().emit('tool_call', { event: 'tool_call', data: { call_id: 'call', tool: 'get_kiln_info', args: {} } }, { lastEventId: `${ID}:2` });
    snapshot = historyOf(ID, [userTurn('t1', 'check tools'), segment('t1', 0, 'The tools work.')], 5).transcript;

    // The history request beats the replay of turn_finished on reconnect.
    stream().open();
    await waitFor(() => expect(rows()).toEqual(['user:check tools', 'assistant:The tools work.']));
    expect(transcriptOf(ID).isStreaming).toBe(true);
    stream().emit('turn_finished', { event: 'turn_finished', data: { status: 'completed', stop_reason: 'end_turn' } }, { lastEventId: `${ID}:5` });

    expect(transcriptOf(ID).isLoading).toBe(false);
    expect(transcriptOf(ID).isStreaming).toBe(false);
    expect(sessionCursor(ID)).toBe(5);
  });

  it('keeps turn completion eligible when a bind snapshot arrives during a send', () => {
    start(historyOf(ID, [userTurn('t1', 'check tools')], 1).transcript);
    patchTranscript(ID, { isLoading: true });
    setTranscriptStreaming(ID, true);
    seedTranscript(ID, historyOf(ID, [userTurn('t1', 'check tools'), segment('t1', 0, 'Done')], 5).transcript, Date.now());

    stream().emit('turn_finished', { event: 'turn_finished', data: { status: 'completed' } }, { lastEventId: `${ID}:5` });

    expect(transcriptOf(ID).isLoading).toBe(false);
    expect(transcriptOf(ID).isStreaming).toBe(false);
  });

  it('on the first open of a resumed stream, when the snapshot is older than the subscription', async () => {
    // A pane bound this session before: the stream resumes at its cursor.
    advanceSessionCursor(ID, 1);
    retainTranscript(ID);
    // A cached read, from before this subscription.
    seedTranscript(ID, historyOf(ID, [], 0).transcript, 0);
    snapshot = historyOf(ID, [userTurn('t1', 'sent before the bind')], 3).transcript;

    stream().open();

    await waitFor(() => expect(rows()).toEqual(['user:sent before the bind']));
  });

  it('when the first live event of a fresh stream is not the next seq', async () => {
    // The snapshot holds seq 2; seqs 3 and 4 happened before the stream
    // subscribed, so their ops never arrive.
    start(historyOf(ID, [userTurn('t1', 'hi')], 2).transcript);
    snapshot = historyOf(ID, [userTurn('t1', 'hi'), userTurn('t2', 'lost')], 4).transcript;

    emitOps(stream(), 5, [upsert(userTurn('t3', 'live'))]);

    await waitFor(() => expect(rows()).toEqual(['user:hi', 'user:lost', 'user:live']));
    expect(env.fetch.calls(HISTORY_ROUTE)).toBe(1);
  });

  it('not when the first live event of a fresh stream is the next seq', async () => {
    start(historyOf(ID, [userTurn('t1', 'hi')], 2).transcript);

    emitOps(stream(), 3, [upsert(userTurn('t2', 'live'))]);
    await Promise.resolve();

    expect(rows()).toEqual(['user:hi', 'user:live']);
    expect(env.fetch.calls(HISTORY_ROUTE)).toBe(0);
  });

  it('not on the first open, when the snapshot is newer than the subscription', async () => {
    start(historyOf(ID, [userTurn('t1', 'hi')], 1).transcript);

    stream().open();
    await Promise.resolve();

    expect(env.fetch.calls(HISTORY_ROUTE)).toBe(0);
  });
});

describe('the optimistic entry', () => {
  it('shows at once, and the daemon user turn with its id replaces it', () => {
    start();
    const id = addOptimisticTurn(ID, 'hello');
    expect(rows()).toEqual(['user:hello']);

    confirmOptimisticTurn(ID, id, 'turn-1');
    expect(rows()).toEqual(['user:hello']);

    emitOps(stream(), 1, [upsert(userTurn('turn-1', 'hello'))]);
    expect(rows()).toEqual(['user:hello']);
    expect(transcriptOf(ID).optimistic).toEqual([]);
  });

  it('is replaced by an echo that beats the send answer', () => {
    start(historyOf(ID, [userTurn('t0', 'hello')], 1).transcript);
    addOptimisticTurn(ID, 'hello');
    // The older turn with the same text does not replace the new entry.
    expect(rows()).toEqual(['user:hello', 'user:hello']);

    emitOps(stream(), 2, [upsert(userTurn('t1', 'hello'))]);

    expect(rows()).toEqual(['user:hello', 'user:hello']);
    expect(daemonTranscriptOf(ID)?.items.map((item) => item.id)).toEqual(['t0', 't1']);
  });
});
