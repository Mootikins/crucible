import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { queryClientOptions } from '@/lib/query/client';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import type { SessionHistoryResponse } from '@/lib/types';
import { keys } from '../keys';
import { sessionEvents } from '../sse';
import { installSessionEventRoute } from '../routes/session';
import { useSessionHistory, fetchSessionHistoryOnce, useSendChatMessage } from '../history';

const FIRST = 'GET /api/session/s-1/history';
const SECOND = 'GET /api/session/s-2/history';
const SEND = 'POST /api/chat/send';

/** One history document, as `GET /api/session/{id}/history` answers it. */
function history(id: string, events: SessionHistoryResponse['history'] = []): SessionHistoryResponse {
  return {
    session_id: id,
    type: 'chat',
    state: 'active',
    kilns: [],
    history: events,
    total_events: events.length,
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
    env = createTestQueryEnv({ [FIRST]: () => history('s-1') });

    const panes = inRoot(() => ({
      left: useSessionHistory(() => 's-1'),
      right: useSessionHistory(() => 's-1'),
    }));

    await vi.waitFor(() => expect(panes.left.data).toEqual(history('s-1')));
    expect(panes.right.data).toEqual(history('s-1'));
    expect(env.fetch.calls(FIRST)).toBe(1);
  });

  it('asks for the whole transcript, not the default page', async () => {
    // The server pages from the FRONT, and a long agentic turn logs hundreds
    // of events: the default page cuts off the tail, which holds the tool
    // results and the assistant's actual text.
    let asked: string | null = null;
    env = createTestQueryEnv({
      [FIRST]: (request) => {
        asked = new URL(request.url).searchParams.get('limit');
        return history('s-1');
      },
    });

    const query = inRoot(() => useSessionHistory(() => 's-1'));

    await vi.waitFor(() => expect(query.data).toBeDefined());
    expect(asked).toBe('10000');
  });

  it('asks nothing while the pane shows no session', async () => {
    env = createTestQueryEnv({ [FIRST]: () => history('s-1') });

    const query = inRoot(() => useSessionHistory(() => null));

    await vi.waitFor(() => expect(query.fetchStatus).toBe('idle'));
    expect(env.fetch.calls(FIRST)).toBe(0);
  });

  it('a session change is a key change, not an abort and a second ask', async () => {
    // The pane used to abort the load in flight and start again on every
    // bind, so going back to a session it had already read asked for the
    // whole transcript a second time.
    env = createTestQueryEnv({
      [FIRST]: () => history('s-1'),
      [SECOND]: () => history('s-2'),
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

    expect(env.fetch.calls(FIRST)).toBe(1);
    expect(env.fetch.calls(SECOND)).toBe(1);
  });

  it('shows the turn another pane sent, which the stream echoes into the cache', async () => {
    // The cross-pane patch: `routes/session.ts` appends the echoed user
    // message to this key, and the reader sees it without a fetch of its own.
    installFakeEventSource();
    env = createTestQueryEnv({ [FIRST]: () => history('s-1') });
    installSessionEventRoute();

    const query = inRoot(() => useSessionHistory(() => 's-1'));
    await vi.waitFor(() => expect(query.data).toEqual(history('s-1')));

    const stop = sessionEvents('s-1').subscribe(() => {});
    onlyEventSource().emit('session_event', {
      type: 'session_event',
      event: 'user_message',
      data: { message_id: 'msg-1', content: 'from the other pane' },
    });
    stop();

    await vi.waitFor(() => expect(query.data?.history).toHaveLength(1));
    expect((query.data?.history[0].data as { content?: string }).content).toBe(
      'from the other pane',
    );
    expect(env.fetch.calls(FIRST)).toBe(1);
  });
});

describe('fetchSessionHistoryOnce', () => {
  it('shares the one request the hook already made', async () => {
    // The bind awaits the document before it dispatches a staged first
    // message. It must not be a second GET of the same transcript.
    env = createTestQueryEnv({ [FIRST]: () => history('s-1') });

    const query = inRoot(() => useSessionHistory(() => 's-1'));
    const answered = await fetchSessionHistoryOnce('s-1');

    expect(answered).toEqual(history('s-1'));
    await vi.waitFor(() => expect(query.data).toEqual(history('s-1')));
    expect(env.fetch.calls(FIRST)).toBe(1);
  });
});

describe('useSendChatMessage', () => {
  it('answers the canonical id the daemon minted', async () => {
    env = createTestQueryEnv({ [SEND]: () => ({ message_id: 'msg-turn-1' }) });

    const send = inRoot(() => useSendChatMessage());
    const messageId = await send.mutateAsync({ id: 's-1', message: 'hello' });

    expect(messageId).toBe('msg-turn-1');
    expect(env.fetch.calls(SEND)).toBe(1);
  });

  it('leaves a held transcript to the stream, which carries the canonical id', async () => {
    // The daemon echoes the turn over the stream under the id the send
    // answered, and the route appends it there under a message-id guard. A
    // second append here would be the same write in two modules.
    env = createTestQueryEnv({ [SEND]: () => ({ message_id: 'msg-turn-1' }) });
    env.client.setQueryData(keys.sessionHistory('s-1'), history('s-1', [userTurn('s-1', 'msg-0', 'older')]));

    const send = inRoot(() => useSendChatMessage());
    await send.mutateAsync({ id: 's-1', message: 'hello' });

    const held = env.client.getQueryData<SessionHistoryResponse>(keys.sessionHistory('s-1'));
    expect(held?.history).toHaveLength(1);
  });
});
