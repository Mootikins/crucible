import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { queryClientOptions } from '@/lib/query/client';
import type { SessionHistoryResponse } from '@/lib/types';
import { keys } from '../keys';
import {
  useSessionHistory,
  fetchSessionHistoryOnce,
  refetchSessionHistory,
  useSendChatMessage,
} from '../history';

// `session.history` and `session.send_message` are RPC methods now
// ([[Simplification Plan#Step 19]] item 9): every session's read shares the
// one `POST /api/rpc/session.history` key, so a fixture that must answer
// differently per session reads `session_id` off the request body instead of
// keying on a per-session URL.
const HISTORY = 'POST /api/rpc/session.history';
const SEND = 'POST /api/rpc/session.send_message';

/** One history document, as `session.history` answers it. */
function history(id: string, events: SessionHistoryResponse['history'] = []): SessionHistoryResponse {
  return {
    session_id: id,
    type: 'chat',
    state: 'active',
    kilns: [],
    history: events,
    total_events: events.length,
    transcript: { as_of_seq: 0, items: [] },
  };
}

/** One persisted user turn, as the daemon records it. */
function userTurn(id: string, messageId: string, content: string): SessionHistoryResponse['history'][number] {
  return {
    type: 'event',
    session_id: id,
    event: 'user_message',
    data: { message_id: messageId, content },
    timestamp: '2026-09-16T00:00:00Z',
  };
}

/** Answers `history(sessionId)` for whichever `session_id` the body names. */
function historyByBody(bodies: Record<string, SessionHistoryResponse>) {
  return async (request: Request) => {
    const body = (await request.clone().json()) as { session_id: string };
    return bodies[body.session_id];
  };
}

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useSessionHistory', () => {
  it('fetches once for two panes on one session', async () => {
    // The gate of this task: two `ChatProvider`s on one session held two
    // documents and asked twice. One key answers both.
    env = createTestQueryEnv({ [HISTORY]: () => history('s-1') });

    const panes = inRoot(() => ({
      left: useSessionHistory(() => 's-1'),
      right: useSessionHistory(() => 's-1'),
    }));

    await vi.waitFor(() => expect(panes.left.data).toEqual(history('s-1')));
    expect(panes.right.data).toEqual(history('s-1'));
    expect(env.fetch.calls(HISTORY)).toBe(1);
  });

  it('asks for the whole transcript, not the default page', async () => {
    // The server pages from the FRONT, and a long agentic turn logs hundreds
    // of events: the default page cuts off the tail, which holds the tool
    // results and the assistant's actual text.
    let asked: number | undefined;
    env = createTestQueryEnv({
      [HISTORY]: async (request) => {
        const body = (await request.clone().json()) as { limit?: number };
        asked = body.limit;
        return history('s-1');
      },
    });

    const query = inRoot(() => useSessionHistory(() => 's-1'));

    await vi.waitFor(() => expect(query.data).toBeDefined());
    expect(asked).toBe(10000);
  });

  it('asks nothing while the pane shows no session', async () => {
    env = createTestQueryEnv({ [HISTORY]: () => history('s-1') });

    const query = inRoot(() => useSessionHistory(() => null));

    await vi.waitFor(() => expect(query.fetchStatus).toBe('idle'));
    expect(env.fetch.calls(HISTORY)).toBe(0);
  });

  it('a session change is a key change, not an abort and a second ask', async () => {
    // The pane used to abort the load in flight and start again on every
    // bind, so going back to a session it had already read asked for the
    // whole transcript a second time.
    env = createTestQueryEnv({
      [HISTORY]: historyByBody({ 's-1': history('s-1'), 's-2': history('s-2') }),
    });
    // The test client drops an entry the moment nothing reads it (`gcTime: 0`),
    // so one case cannot answer the next. This case is about going BACK to a
    // session inside the lifetime the app gives an entry, so it asks for that
    // lifetime.
    env.client.setDefaultOptions({
      queries: { ...queryClientOptions.defaultOptions?.queries },
    });
    const [id, setId] = createSignal<string | null>('s-1');

    const query = inRoot(() => useSessionHistory(id));

    await vi.waitFor(() => expect(query.data).toEqual(history('s-1')));
    setId('s-2');
    await vi.waitFor(() => expect(query.data).toEqual(history('s-2')));
    setId('s-1');
    await vi.waitFor(() => expect(query.data).toEqual(history('s-1')));

    expect(env.fetch.calls(HISTORY)).toBe(2);
  });

});

describe('refetchSessionHistory', () => {
  it('reads the transcript again although the cache holds it', async () => {
    // The transcript store asks when its copy fell behind the daemon, so the
    // cached answer is the one that is wrong.
    env = createTestQueryEnv({ [HISTORY]: () => history('s-1') });
    env.client.setQueryData(keys.sessionHistory('s-1'), { ...history('s-1'), total_events: 99 });

    const answered = await refetchSessionHistory('s-1');

    expect(answered).toEqual(history('s-1'));
    expect(env.fetch.calls(HISTORY)).toBe(1);
  });
});

describe('fetchSessionHistoryOnce', () => {
  it('shares the one request the hook already made', async () => {
    // The bind awaits the document before it dispatches a staged first
    // message. It must not be a second GET of the same transcript.
    env = createTestQueryEnv({ [HISTORY]: () => history('s-1') });

    const query = inRoot(() => useSessionHistory(() => 's-1'));
    const answered = await fetchSessionHistoryOnce('s-1');

    expect(answered).toEqual(history('s-1'));
    await vi.waitFor(() => expect(query.data).toEqual(history('s-1')));
    expect(env.fetch.calls(HISTORY)).toBe(1);
  });
});

describe('useSendChatMessage', () => {
  it('answers the canonical id the daemon minted', async () => {
    env = createTestQueryEnv({ [SEND]: () => ({ outcome: 'turn', message_id: 'msg-turn-1' }) });

    const send = inRoot(() => useSendChatMessage());
    const outcome = await send.mutateAsync({ id: 's-1', message: 'hello' });

    expect(outcome).toEqual({ outcome: 'turn', message_id: 'msg-turn-1' });
    expect(env.fetch.calls(SEND)).toBe(1);
  });

  it('leaves a held transcript to the stream, which carries the canonical id', async () => {
    // The daemon echoes the turn over the stream under the id the send
    // answered, and the route appends it there under a message-id guard. A
    // second append here would be the same write in two modules.
    env = createTestQueryEnv({ [SEND]: () => ({ outcome: 'turn', message_id: 'msg-turn-1' }) });
    env.client.setQueryData(keys.sessionHistory('s-1'), history('s-1', [userTurn('s-1', 'msg-0', 'older')]));

    const send = inRoot(() => useSendChatMessage());
    await send.mutateAsync({ id: 's-1', message: 'hello' });

    const held = env.client.getQueryData<SessionHistoryResponse>(keys.sessionHistory('s-1'));
    expect(held?.history).toHaveLength(1);
  });
});
