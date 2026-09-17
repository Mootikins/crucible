import { render, screen, waitFor } from '@solidjs/testing-library';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createEffect, createSignal } from 'solid-js';
import { resetTranscriptsForTests } from './transcriptStore';
import { ChatProvider, useChat, useChatSafe } from './ChatContext';
import { resetSseForTests } from '@/lib/query/sse';
import { queryClientOptions } from '@/lib/query/client';
import { keys } from '@/lib/query/keys';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';
import type { Session } from '@/lib/types';

// The turn helpers now live in `lib/turn.ts`; the deterministic ids they
// are mocked for live there too.
vi.mock('@/lib/turn', () => ({
  // Monotonic — sendMessage mints two temp ids back-to-back, and a
  // Date.now()-based id would collide within one millisecond.
  generateMessageId: (() => {
    let n = 0;
    return () => `msg_${++n}_test`;
  })(),
  turnResponseId: (id: string) => `${id}-response`,
  turnSegmentId: (id: string, index: number) => `${id}-seg-${index}`,
  turnThinkingId: (id: string) => `${id}-thinking`,
  stripFrozenPrefix: (full: string, segs: string[]) => {
    let rest = full;
    for (const seg of segs) {
      if (rest.startsWith(seg)) {
        rest = rest.slice(seg.length);
        continue;
      }
      const trimmed = seg.replace(/\s+$/, '');
      if (trimmed !== '' && rest.startsWith(trimmed)) {
        rest = rest.slice(trimmed.length);
        continue;
      }
      return full;
    }
    return rest;
  },
}));

// No `vi.mock('@/lib/api')`. The provider reads the daemon through the query
// layer and the shared stream root, so the ROUTES answer: each case counts
// requests and reads bodies off the wire, which a module double cannot prove.
// The stream is the FakeEventSource of the outer beforeEach; a case that means
// the daemon to speak emits a frame on it directly.

const ID = 'test-session-1';
const HISTORY_ROUTE = `GET /api/session/${ID}/history`;
const SEND_ROUTE = 'POST /api/chat/send';
/** Every session id some case binds a pane to. */
const IDS = [ID, 'test-session-2', 'session-a', 'session-b'] as const;

const mockSession: Session = {
  session_id: 'test-session-1',
  type: 'chat',
  kilns: ['/tmp/test-kiln'],
  workspace: '/tmp/test-workspace',
  state: 'active',
  title: 'Test Session',
  agent_model: 'test-model',
  started_at: new Date().toISOString(),
  event_count: 0,
};

// ---- What each route answers this case. The outer beforeEach sets the
// standing answers; a case replaces what it means before it renders. ----

/** `GET /api/session/{id}` for the main session. */
let sessionAnswer: () => unknown;
/** `GET /api/session/{id}/history` for the main session. */
let historyAnswer: () => unknown;
/** The answers the modes route owes in order; the standing one follows them. */
let modesOnce: Array<() => unknown>;
let modesAnswer: () => unknown;
/** What `POST /api/chat/send` answers: the turn id the daemon minted. */
let sendAnswer: () => unknown;
/** What `GET /api/session/list` answers. */
let listAnswer: () => unknown;
/** What `POST /api/session/{id}/mode` answers. */
let setModeAnswer: () => unknown;
/** The session each history read asked for, in order. */
const historyAsked: string[] = [];
/** The `limit` each history read asked with, in order. */
const historyLimits: (string | null)[] = [];
/** The bodies that reached the send route, in order. */
const sentTurns: { session_id: string; content: string }[] = [];

/** The daemon's refusal, as its routes serialise it. */
const refusal = (status: number, message: string): Response =>
  new Response(JSON.stringify({ error: { code: status, message } }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

/**
 * A promise a case holds and releases by hand. The tsconfig targets ES2022,
 * one lib short of `Promise.withResolvers`, so the pair is built once here.
 */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

let env: TestQueryEnv;

/** Installs the query env with every route a pane's mount reaches. */
function serve(): void {
  const routes: Record<string, (request: Request) => unknown> = {
    'GET /api/interactions/pending': () => ({ pending: [] }),
    'GET /api/kilns': () => ({ kilns: [] }),
    'GET /api/session/list': () => listAnswer(),
    [SEND_ROUTE]: async (request) => {
      sentTurns.push((await request.clone().json()) as { session_id: string; content: string });
      return sendAnswer();
    },
  };
  for (const id of IDS) {
    // A pane that binds reads the session it named: the record answers under
    // the id it was asked for, so a fixed record would hand every pane one
    // session whatever it bound to.
    routes[`GET /api/session/${id}`] = () =>
      id === ID ? sessionAnswer() : { ...mockSession, session_id: id };
    routes[`GET /api/session/${id}/history`] = (request) => {
      historyAsked.push(id);
      historyLimits.push(new URL(request.url).searchParams.get('limit'));
      return id === ID
        ? historyAnswer()
        : { session_id: id, history: [], total_events: 0 };
    };
    routes[`GET /api/session/${id}/modes`] = () =>
      modesOnce.length ? modesOnce.shift()!() : modesAnswer();
    routes[`POST /api/session/${id}/mode`] = () => setModeAnswer();
  }
  env = createTestQueryEnv(routes);
}

beforeEach(() => {
  installFakeEventSource();
  sessionAnswer = () => mockSession;
  historyAnswer = () => ({ session_id: ID, history: [], total_events: 0 });
  modesOnce = [];
  modesAnswer = () => ({
    current_mode_id: 'ask',
    modes: [
      { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
      { id: 'review', name: 'Review', description: null, icon: null, color: null },
    ],
  });
  sendAnswer = () => ({ message_id: 'msg-turn-1' });
  listAnswer = () => ({ sessions: [], total: 0 });
  setModeAnswer = () => new Response(null, { status: 204 });
  historyAsked.length = 0;
  historyLimits.length = 0;
  sentTurns.length = 0;
  serve();
});

afterEach(() => {
  resetSseForTests();
  // The transcript store is a module singleton keyed by session; forget it
  // so one case cannot answer the next one.
  resetTranscriptsForTests();
  env?.restore();
});

function TestConsumer() {
  const { messages, isLoading, sendMessage } = useChat();

  return (
    <div>
      <span data-testid="loading">{isLoading() ? 'loading' : 'idle'}</span>
      <span data-testid="count">{messages().length}</span>
      <button onClick={() => sendMessage('test message')}>Send</button>
      <ul>
        {messages().map((m) => (
          <li data-testid={`msg-${m.id}`} data-role={m.role}>
            {m.content}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Reports this pane's transcript, so a test can prove the pane still hears. */
function TokenConsumer(props: { onMessages: (messages: { content: string }[]) => void }) {
  const { messages } = useChat();
  createEffect(() => {
    const held = messages().filter((m) => m.content !== '');
    if (held.length > 0) props.onMessages(held.map((m) => ({ content: m.content })));
  });
  return <span />;
}

function TestWrapper(props: { children: any; session?: Session | null }) {
  const [session] = createSignal(props.session !== undefined ? props.session : mockSession);
  return <ChatProvider sessionId={session()?.session_id ?? ''}>{props.children}</ChatProvider>;
}

describe('ChatContext', () => {
  it('starts with empty messages', () => {
    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    expect(screen.getByTestId('count').textContent).toBe('0');
    expect(screen.getByTestId('loading').textContent).toBe('idle');
  });

  it('adds user message when sending', async () => {
    sendAnswer = () => ({ message_id: 'msg_server_1' });

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    // Let the mount-time bootstrap (empty history) fire its load first — the
    // merge in loadHistory must not clobber the optimistic messages. Anchor on
    // the actual history read rather than an arbitrary sleep.
    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBeGreaterThan(0));

    const sendButton = screen.getByText('Send');
    sendButton.click();

    await waitFor(() => {
      expect(screen.getByTestId('count').textContent).toBe('2');
    });

    const items = screen.getAllByRole('listitem');
    expect(items[0].getAttribute('data-role')).toBe('user');
    expect(items[0].textContent).toBe('test message');
  });

  it('does not send without session', async () => {
    sendAnswer = () => ({ message_id: 'msg_server_1' });

    render(() => (
      <TestWrapper session={null}>
        <TestConsumer />
      </TestWrapper>
    ));

    const sendButton = screen.getByText('Send');
    sendButton.click();

    // A null session makes sendMessage bail synchronously before any await;
    // flush the microtask queue (deterministic, no arbitrary delay) and assert
    // nothing was sent.
    await Promise.resolve();

    expect(screen.getByTestId('count').textContent).toBe('0');
    expect(env.fetch.calls(SEND_ROUTE)).toBe(0);
  });

  it('shows loading state while sending', async () => {
    sendAnswer = () => ({ message_id: 'msg_server_1' });

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    screen.getByText('Send').click();

    await waitFor(() => {
      expect(screen.getByTestId('loading').textContent).toBe('loading');
    });

    FakeEventSource.instances[0]!.emit('message_complete', {
      type: 'message_complete',
      id: 'msg_server_1',
      content: 'Response from assistant',
    });

    await waitFor(() => {
      expect(screen.getByTestId('loading').textContent).toBe('idle');
    });
  });
});

describe('streaming reconciliation', () => {
  it('reconciles a message minted by a token that beat the send POST (no orphan bubble)', async () => {
    // Hold the POST open so a token can arrive mid-flight.
    const held = deferred<{ message_id: string }>();
    sendAnswer = () => held.promise;

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    screen.getByText('Send').click();
    await waitFor(() => expect(env.fetch.calls(SEND_ROUTE)).toBe(1));

    // Token arrives before the POST resolves → reducer mints a random-id
    // assistant and streams into it.
    FakeEventSource.instances[0]!.emit('token', { type: 'token', content: 'partial ' });

    // POST resolves with the canonical turn id. The early streaming message
    // must be reconciled into `${id}-response`, not left orphaned beside a new
    // empty placeholder.
    held.resolve({ message_id: 'msg-turn-1' });
    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('2'));

    FakeEventSource.instances[0]!.emit('message_complete', {
      type: 'message_complete',
      id: 'msg-turn-1',
      content: 'partial answer',
    });

    await waitFor(() => {
      const items = screen.getAllByRole('listitem');
      const assistant = items.find((i) => i.getAttribute('data-role') === 'assistant');
      expect(assistant?.textContent).toBe('partial answer');
    });
    // Still exactly user + one assistant — the orphan bug would make it three.
    expect(screen.getByTestId('count').textContent).toBe('2');
  });
});

describe('a reply the provider cut off', () => {
  // The daemon names the reason on `message_complete` and WORDS the note
  // beside it. This is where a reader meets it: a system line under the reply.
  //
  // The text below is deliberately not a wording the daemon ships. The page
  // must draw the string it received, so a test that used the real wording
  // could pass while the page derived the words itself.
  it('draws the note the daemon worded', async () => {
    sendAnswer = () => ({ message_id: 'msg-turn-1' });

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    screen.getByText('Send').click();
    await waitFor(() => expect(env.fetch.calls(SEND_ROUTE)).toBe(1));

    FakeEventSource.instances[0]!.emit('message_complete', {
      type: 'message_complete',
      id: 'msg-turn-1',
      content: 'Half an ans',
      stop_reason: 'max_tokens',
      stop_notice: 'a note only the daemon can word',
    });

    await waitFor(() => {
      const items = screen.getAllByRole('listitem');
      const system = items.find((i) => i.getAttribute('data-role') === 'system');
      expect(system?.textContent).toContain('a note only the daemon can word');
    });
  });

  // No `stop_notice`, so no note — even though the reason is one the page used
  // to keep a wording for. The page derives nothing.
  it('draws nothing extra when the reply finished', async () => {
    sendAnswer = () => ({ message_id: 'msg-turn-2' });

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    screen.getByText('Send').click();
    await waitFor(() => expect(env.fetch.calls(SEND_ROUTE)).toBe(1));

    FakeEventSource.instances[0]!.emit('message_complete', {
      type: 'message_complete',
      id: 'msg-turn-2',
      content: 'A whole answer',
      stop_reason: 'end_turn',
    });

    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('2'));
    const items = screen.getAllByRole('listitem');
    expect(items.some((i) => i.getAttribute('data-role') === 'system')).toBe(false);
  });
});

describe('draft first-message handoff', () => {
  afterEach(async () => {
    // The staged message survives rendering now (peek, not consume — the
    // destructive read happens only at dispatch, which these tests hold
    // open). Drain it so it can't leak into later tests as a phantom
    // optimistic turn.
    const { consumePendingFirstMessage } = await import('@/lib/draft-session');
    consumePendingFirstMessage(mockSession.session_id);
  });

  it('renders the user message and working indicator immediately, before bootstrap and SSE resolve', async () => {
    // Neither gate ever resolves: bootstrap hangs, SSE never opens. The
    // optimistic turn must render anyway — the user should never stare at an
    // empty transcript after sending their first draft message.
    sessionAnswer = () => new Promise(() => {});
    historyAnswer = () => new Promise(() => {});
    sendAnswer = () => ({ message_id: 'msg-turn-1' });

    const { setPendingFirstMessage } = await import('@/lib/draft-session');
    setPendingFirstMessage(mockSession.session_id, 'first message from draft');

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('2'));
    const items = screen.getAllByRole('listitem');
    expect(items[0].getAttribute('data-role')).toBe('user');
    expect(items[0].textContent).toBe('first message from draft');
    expect(items[1].getAttribute('data-role')).toBe('assistant');
    expect(items[1].textContent).toBe('');
    expect(screen.getByTestId('loading').textContent).toBe('loading');
    // The POST is still gated — only the rendering is immediate.
    expect(env.fetch.calls(SEND_ROUTE)).toBe(0);
  });

  /**
   * Regression. `useCreateSession` seeds `['session', id]` with the daemon's
   * create reply, and that reply carries no title for a session nobody named
   * yet. The bootstrap reads that seeded record, so it wrote `undefined` over
   * a title signal holding `null`. The two values differ, so the signal
   * notified — and the bind effect read that signal through the attention
   * mirror, so it re-ran for the SAME session and staged the first message a
   * second time. The user saw their own turn twice.
   */
  it('draws the staged turn once when the seeded record carries no title', async () => {
    const seeded = { ...mockSession, title: undefined } as unknown as Session;
    env.client.setQueryData(keys.session(mockSession.session_id), seeded);
    sessionAnswer = () => seeded;
    sendAnswer = () => ({ message_id: 'msg-turn-1' });

    const { setPendingFirstMessage } = await import('@/lib/draft-session');
    setPendingFirstMessage(mockSession.session_id, 'first message from draft');

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    // The staged turn goes up at once, as the case above proves.
    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('2'));
    // The bootstrap writes the title before it reads the history, so a history
    // read is the mark that the write has happened. Then let every effect the
    // write queued run.
    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBeGreaterThan(0));
    for (let flush = 0; flush < 5; flush += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }

    const userTurns = screen
      .getAllByRole('listitem')
      .filter((item) => item.getAttribute('data-role') === 'user');
    expect(userTurns.length).toBe(1);
    expect(screen.getByTestId('count').textContent).toBe('2');
  });
});

describe('session switching', () => {
  const mockSession2: Session = {
    session_id: 'test-session-2',
    type: 'chat',
    kilns: ['/tmp/test-kiln'],
    workspace: '/tmp/test-workspace',
    state: 'active',
    title: 'Test Session 2',
    agent_model: 'test-model',
    started_at: new Date().toISOString(),
    event_count: 0,
  };

  function DynamicTestWrapper(props: { children: any }) {
    const [session, setSession] = createSignal<Session | null>(mockSession);
    return (
      <ChatProvider sessionId={session()?.session_id ?? ''}>
        {props.children}
        <button data-testid="switch-session" onClick={() => setSession(mockSession2)}>Switch</button>
        <button data-testid="clear-session" onClick={() => setSession(null)}>Clear</button>
      </ChatProvider>
    );
  }

  it('does not clear messages on initial mount', async () => {
    sendAnswer = () => ({ message_id: 'msg_server_1' });

    render(() => (
      <DynamicTestWrapper>
        <TestConsumer />
      </DynamicTestWrapper>
    ));

    screen.getByText('Send').click();

    await waitFor(() => {
      expect(screen.getByTestId('count').textContent).toBe('2');
    });
  });

  it('clears messages when switching to different session', async () => {
    sendAnswer = () => ({ message_id: 'msg_server_1' });

    render(() => (
      <DynamicTestWrapper>
        <TestConsumer />
      </DynamicTestWrapper>
    ));

    screen.getByText('Send').click();

    await waitFor(() => {
      expect(screen.getByTestId('count').textContent).toBe('2');
    });

    screen.getByTestId('switch-session').click();

    await waitFor(() => {
      expect(screen.getByTestId('count').textContent).toBe('0');
    });
  });
});

describe('useChatSafe', () => {
  function SafeTestConsumer() {
    const { messages, isLoading, isStreaming, sendMessage } = useChatSafe();

    return (
      <div>
        <span data-testid="loading">{isLoading() ? 'loading' : 'idle'}</span>
        <span data-testid="streaming">{isStreaming() ? 'yes' : 'no'}</span>
        <span data-testid="count">{messages().length}</span>
        <button onClick={() => sendMessage('test')}>Send</button>
      </div>
    );
  }

  it('returns fallback values when used outside provider', () => {
    // This simulates dockview rendering panels outside the context tree
    render(() => <SafeTestConsumer />);

    expect(screen.getByTestId('loading').textContent).toBe('idle');
    expect(screen.getByTestId('streaming').textContent).toBe('no');
    expect(screen.getByTestId('count').textContent).toBe('0');
  });

  it('does not throw when sendMessage called outside provider', async () => {
    render(() => <SafeTestConsumer />);

    // Should not throw - fallback is a noop
    const sendButton = screen.getByText('Send');
    expect(() => sendButton.click()).not.toThrow();
  });

  it('uses real context when inside provider', () => {
    render(() => (
      <TestWrapper>
        <SafeTestConsumer />
      </TestWrapper>
    ));

    expect(screen.getByTestId('count').textContent).toBe('0');
    expect(screen.getByTestId('loading').textContent).toBe('idle');
  });
});

describe('isLoadingHistory', () => {
  function HistoryTestConsumer() {
    const { isLoadingHistory, messages } = useChat();

    return (
      <div>
        <span data-testid="history-loading">{isLoadingHistory() ? 'loading' : 'idle'}</span>
        <span data-testid="msg-count">{messages().length}</span>
        <span data-testid="precog">
          {messages()
            .filter((m) => m.precognition)
            .map(
              (m) =>
                `${m.id}:${m.precognition!.notesCount}:${m.precognition!.notes
                  .map((n) => n.name)
                  .join('|')}`,
            )
            .join(',')}
        </span>
        <ul>
          {messages().map((m) => (
            <li data-testid={`hist-msg-${m.id}`} data-role={m.role}>
              {m.content}
            </li>
          ))}
        </ul>
      </div>
    );
  }

  it('is true during history load and false after', async () => {
    const held = deferred<unknown>();
    historyAnswer = () => held.promise;

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('history-loading').textContent).toBe('loading');
    });

    held.resolve({ session_id: ID, history: [], total_events: 0 });

    await waitFor(() => {
      expect(screen.getByTestId('history-loading').textContent).toBe('idle');
    });
  });

  it('resets to false on error', async () => {
    historyAnswer = () => refusal(500, 'Network error');

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('history-loading').textContent).toBe('idle');
    });
  });

  it('populates messages from session history events', async () => {
    historyAnswer = () => ({
      session_id: ID,
      history: [
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'user_message',
          data: { content: 'hello', message_id: 'msg1' },
        },
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'message_complete',
          data: { full_response: 'hi there', message_id: 'msg2' },
        },
      ],
      total_events: 2,
    });


    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('msg-count').textContent).toBe('2');
    });

    const items = screen.getAllByRole('listitem');
    expect(items[0].getAttribute('data-role')).toBe('user');
    expect(items[0].textContent?.trim()).toBe('hello');
    expect(items[1].getAttribute('data-role')).toBe('assistant');
    expect(items[1].textContent?.trim()).toBe('hi there');
  });

  it('restores the precognition badge from history', async () => {
    // The badge used to vanish on reload because precognition was a live-only
    // event. Now that the daemon persists it, replay must reattach it to the
    // user message that triggered it — same target the live reducer picks.
    historyAnswer = () => ({
      session_id: ID,
      history: [
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'user_message',
          data: { content: 'tell me about the kiln', message_id: 'msg1' },
        },
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'precognition_complete',
          data: {
            notes_count: 2,
            notes: [
              { title: 'Kilns', kiln: 'docs', score: 0.91 },
              { title: 'Wikilinks', kiln: 'docs', score: 0.72 },
            ],
          },
        },
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'message_complete',
          data: { full_response: 'A kiln is…', message_id: 'msg1' },
        },
      ],
      total_events: 3,
    });

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('precog').textContent).toBe('msg1:2:Kilns|Wikilinks');
    });
    // The event itself is metadata, not a transcript bubble.
    expect(screen.getByTestId('msg-count').textContent).toBe('2');
  });

  it('ignores a precognition event with no preceding user message', async () => {
    // Truncated/paginated history can start mid-turn; attaching to nothing
    // must not throw or invent a message.
    historyAnswer = () => ({
      session_id: ID,
      history: [
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'precognition_complete',
          data: { notes_count: 1, notes: [{ title: 'Orphan', score: 0.5 }] },
        },
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'message_complete',
          data: { full_response: 'hi', message_id: 'msg9' },
        },
      ],
      total_events: 2,
    });

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('msg-count').textContent).toBe('1');
    });
    expect(screen.getByTestId('precog').textContent).toBe('');
  });

  it('attaches precognition to its own turn when the history holds several', async () => {
    historyAnswer = () => ({
      session_id: ID,
      history: [
        { type: 'event', session_id: 'test-session-1', event: 'user_message', data: { content: 'first', message_id: 'u1' } },
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'precognition_complete',
          data: { notes_count: 1, notes: [{ title: 'Alpha', score: 0.9 }] },
        },
        { type: 'event', session_id: 'test-session-1', event: 'message_complete', data: { full_response: 'a', message_id: 'u1' } },
        { type: 'event', session_id: 'test-session-1', event: 'user_message', data: { content: 'second', message_id: 'u2' } },
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'precognition_complete',
          data: { notes_count: 1, notes: [{ title: 'Beta', score: 0.8 }] },
        },
      ],
      total_events: 5,
    });

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('precog').textContent).toBe('u1:1:Alpha,u2:1:Beta');
    });
  });

  it('reconstructs a segmented turn into canonical segment + final bubbles matching the live reducer', async () => {
    // A text → tool → text turn persists a segment_complete plus a
    // message_complete carrying the WHOLE turn. Reconstruction must split it
    // into a segment bubble + a trailing bubble with the SAME canonical ids
    // the live reducer streams (turnSegmentId / turnResponseId) — that identity
    // is what makes live and reloaded transcripts converge.
    historyAnswer = () => ({
      session_id: ID,
      history: [
        { type: 'event', session_id: 'test-session-1', event: 'user_message', data: { content: 'find it', message_id: 'msg1' } },
        { type: 'event', session_id: 'test-session-1', event: 'segment_complete', data: { message_id: 'msg1', index: 0, content: 'Let me look. ' } },
        { type: 'event', session_id: 'test-session-1', event: 'tool_call', data: { call_id: 'tc-1', tool: 'search', args: {} } },
        { type: 'event', session_id: 'test-session-1', event: 'tool_result', data: { call_id: 'tc-1', result: 'notes' } },
        { type: 'event', session_id: 'test-session-1', event: 'message_complete', data: { full_response: 'Let me look. Here it is.', message_id: 'msg1' } },
      ],
      total_events: 5,
    });

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('msg-count').textContent).toBe('4');
    });

    const items = screen.getAllByRole('listitem');
    expect(items.map((el) => el.getAttribute('data-role'))).toEqual([
      'user',
      'assistant',
      'tool',
      'assistant',
    ]);
    // Canonical ids identical to the live-streamed transcript.
    expect(screen.getByTestId('hist-msg-msg1-seg-0').textContent?.trim()).toBe('Let me look.');
    expect(screen.getByTestId('hist-msg-msg1-response').textContent?.trim()).toBe('Here it is.');
  });

  it('omits the trailing bubble when segments cover the whole turn (matches the live reducer)', async () => {
    // text → tool with no trailing narration: the whole turn is the single
    // segment, so message_complete's stripped content is empty and no final
    // bubble is added — the same shape the live reducer produces.
    historyAnswer = () => ({
      session_id: ID,
      history: [
        { type: 'event', session_id: 'test-session-1', event: 'user_message', data: { content: 'go', message_id: 'msg1' } },
        { type: 'event', session_id: 'test-session-1', event: 'segment_complete', data: { message_id: 'msg1', index: 0, content: 'All done via tool.' } },
        { type: 'event', session_id: 'test-session-1', event: 'tool_call', data: { call_id: 'tc-1', tool: 'search', args: {} } },
        { type: 'event', session_id: 'test-session-1', event: 'tool_result', data: { call_id: 'tc-1', result: 'notes' } },
        { type: 'event', session_id: 'test-session-1', event: 'message_complete', data: { full_response: 'All done via tool.', message_id: 'msg1' } },
      ],
      total_events: 5,
    });

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('msg-count').textContent).toBe('3');
    });

    const items = screen.getAllByRole('listitem');
    expect(items.map((el) => el.getAttribute('data-role'))).toEqual(['user', 'assistant', 'tool']);
    expect(screen.getByTestId('hist-msg-msg1-seg-0').textContent?.trim()).toBe('All done via tool.');
    // No canonical response bubble was added.
    expect(screen.queryByTestId('hist-msg-msg1-response')).toBeNull();
  });

  it('falls back to persisted history when getSession fails', async () => {
    sessionAnswer = () => refusal(404, 'no such session');
    historyAnswer = () => ({
      session_id: ID,
      history: [
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'user_message',
          data: { content: 'persisted user', message_id: 'msg-user' },
        },
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'message_complete',
          data: { full_response: 'persisted assistant', message_id: 'msg-assistant' },
        },
      ],
      total_events: 2,
    });

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('msg-count').textContent).toBe('2');
    });

    // The transcript read the daemon's own store — the whole of it, the
    // `HISTORY_LIMIT` the query layer always asks with.
    expect(historyAsked).toContain('test-session-1');
    expect(historyLimits).toContain('10000');
  });

  it('dedups a live canonical user message against a pre-canonical reconstructed one', async () => {
    // Hold history open so a live SSE echo lands in the message list first.
    const held = deferred<unknown>();
    historyAnswer = () => held.promise;

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    // Live echo adds the prompt under its canonical id.
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    FakeEventSource.instances[0]!.emit('session_event', {
      type: 'session_event',
      event: 'user_message',
      data: { message_id: 'msg-live', content: 'hello' },
    });
    await waitFor(() => expect(screen.getByTestId('msg-count').textContent).toBe('1'));
    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBeGreaterThan(0));

    // History replays the SAME prompt from an old event that predates canonical
    // message_ids → reconstructed under a fallback id (user-0).
    held.resolve({
      session_id: ID,
      history: [
        {
          type: 'event',
          session_id: 'test-session-1',
          event: 'user_message',
          data: { content: 'hello' },
        },
      ],
      total_events: 1,
    });

    // Wait for the merge to complete (history-loading flips to idle AFTER the
    // setMessages merge), then assert the prompt did not render twice.
    await waitFor(() => expect(screen.getByTestId('history-loading').textContent).toBe('idle'));
    expect(screen.getByTestId('msg-count').textContent).toBe('1');
  });
});

describe('mode hydration', () => {
  it('restores a Lua-declared mode the frontend has no constant for', async () => {
    // The old hydrateMode checked the id against 'ask' | 'plan' | 'auto'
    // and dropped anything else, so a session persisted in `review` came back
    // showing Normal while the agent kept running review.
    //
    // The mode list is the authority when it answers (the case below), so it
    // is kept out of the way here: the persisted string is then the only
    // source the chip has, which is the path this case is about.
    modesOnce.push(() => refusal(503, 'daemon down'));
    // `session.get` nests the persisted mode under `agent`; nothing sends a
    // top-level `agent_mode`.
    sessionAnswer = () => ({
      ...mockSession,
      agent: { model: 'ollama:neural-chat', mode: 'review' },
    });

    let mode: () => string = () => '';
    const Probe = () => {
      mode = useChat().chatMode;
      return null;
    };
    render(() => (
      <ChatProvider sessionId="test-session-1">
        <Probe />
      </ChatProvider>
    ));

    await waitFor(() => expect(mode()).toBe('review'));
  });

  it('offers the daemon modes, not a hardcoded three', async () => {
    sessionAnswer = () => mockSession;

    let modes: () => { id: string }[] = () => [];
    const Probe = () => {
      modes = useChat().availableModes;
      return null;
    };
    render(() => (
      <ChatProvider sessionId="test-session-1">
        <Probe />
      </ChatProvider>
    ));

    await waitFor(() => expect(modes().map((m) => m.id)).toEqual(['ask', 'review']));
  });

  it("takes the daemon's current_mode_id over the persisted string", async () => {
    // `session.get` returns whatever was last written; `session.list_modes`
    // clamps to a mode that still exists. When they disagree — a `review`
    // session whose declaration was removed — the daemon's answer is the one
    // that describes what will actually run.
    // `session.get` nests the persisted mode under `agent`; nothing sends a
    // top-level `agent_mode`.
    sessionAnswer = () => ({
      ...mockSession,
      agent: { model: 'ollama:neural-chat', mode: 'review' },
    });
    modesAnswer = () => ({
      current_mode_id: 'ask',
      modes: [
        { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
        { id: 'plan', name: 'Plan', description: null, icon: null, color: null },
      ],
    });

    let mode: () => string = () => '';
    const Probe = () => {
      mode = useChat().chatMode;
      return null;
    };
    render(() => (
      <ChatProvider sessionId="test-session-1">
        <Probe />
      </ChatProvider>
    ));

    await waitFor(() => expect(mode()).toBe('ask'));
  });

  it('re-fetches the mode list when the daemon rejects a switch', async () => {
    // The mount-time fetch fails, so the chip falls back to the built-in three
    // and offers `plan` in a session that may not declare it. Clicking it
    // POSTs a mode the daemon rejects; the list it came from must not survive.
    modesOnce.push(() => refusal(503, 'daemon down'));
    setModeAnswer = () => refusal(422, "unknown mode 'plan'");

    let ctx: { switchMode: (m: string) => void; availableModes: () => { id: string }[] } | null =
      null;
    const Probe = () => {
      const c = useChat();
      ctx = { switchMode: c.switchMode, availableModes: c.availableModes };
      return null;
    };
    render(() => (
      <ChatProvider sessionId="test-session-1">
        <Probe />
      </ChatProvider>
    ));

    await waitFor(() => expect(ctx).not.toBeNull());
    ctx!.switchMode('plan');

    await waitFor(() =>
      expect(ctx!.availableModes().map((m) => m.id)).toEqual(['ask', 'review'])
    );
  });
});

// A history load that lands ON TOP of a finished live turn is the shape that
// doubled the transcript: `loadHistory` merges by id alone, so the live answer
// and the reconstructed answer collapse into one bubble only while the live
// one carries `turnResponseId`. A turn that opens with reasoning and goes
// straight to a tool used to leave that id on the retired placeholder.
describe('a slow history load cannot duplicate a finished turn', () => {
  const ANSWER = 'Here is what a new user does first.';

  it('merges the reconstructed answer onto the live one', async () => {
    // Held open until the live turn has finished.
    const held = deferred<unknown>();
    historyAnswer = () => held.promise;
    sendAnswer = () => ({ message_id: 'msg-turn-1' });

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const stream = () => FakeEventSource.instances[0]!;
    screen.getByText('Send').click();
    // The send POST returns the turn id, which renames the optimistic
    // placeholder to the canonical response id.
    await waitFor(() => expect(screen.queryByTestId('msg-msg-turn-1-response')).not.toBeNull());

    // Live turn: reason, call a tool, reason again, then answer.
    stream().emit('thinking', { type: 'thinking', content: 'The user wants the guide. ' });
    stream().emit('tool_call', { type: 'tool_call', id: 'call-a', title: 'read_note', arguments: { path: 'g.md' } });
    stream().emit('tool_result', { type: 'tool_result', id: 'call-a', result: '{}' });
    stream().emit('thinking', { type: 'thinking', content: 'Now I can answer. ' });
    stream().emit('token', { type: 'token', content: ANSWER });
    stream().emit('message_complete', { type: 'message_complete', id: 'msg-turn-1', content: ANSWER, total_tokens: 6707 });

    await waitFor(() =>
      expect(screen.getAllByRole('listitem').some((li) => li.textContent === ANSWER)).toBe(true)
    );

    // The daemon's own record of the same turn arrives now.
    held.resolve({
      session_id: ID,
      history: [
        { event: 'user_message', data: { message_id: 'msg-turn-1', content: 'test message' } },
        { event: 'tool_call', data: { call_id: 'call-a', tool: 'read_note', args: { path: 'g.md' } } },
        { event: 'tool_result', data: { call_id: 'call-a', result: '{}' } },
        { event: 'message_complete', data: { message_id: 'msg-turn-1', full_response: ANSWER } },
      ],
      total_events: 4,
    });

    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBeGreaterThan(0));
    await waitFor(() => {
      const answers = screen.getAllByRole('listitem').filter((li) => li.textContent === ANSWER);
      expect(answers).toHaveLength(1);
    });
  });
});

// The stream itself, not a double of it. `lib/query/sse.ts` owns one source
// per session and every pane subscribes to it; these cases prove that the
// provider reaches the stream through that root, so the count of EventSources
// is the count of sessions on screen and not the count of panes.
describe('the shared session stream', () => {
  afterEach(async () => {
    const { consumePendingFirstMessage } = await import('@/lib/draft-session');
    consumePendingFirstMessage(mockSession.session_id);
  });

  function RetryConsumer() {
    const { retryConnection, connectionStatus } = useChat();
    return (
      <button data-testid="retry" data-status={connectionStatus()} onClick={retryConnection}>
        Retry
      </button>
    );
  }

  it('opens one EventSource for two panes on one session', async () => {
    render(() => (
      <>
        <ChatProvider sessionId={mockSession.session_id}>
          <span />
        </ChatProvider>
        <ChatProvider sessionId={mockSession.session_id}>
          <span />
        </ChatProvider>
      </>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    expect(FakeEventSource.instances[0]!.url).toBe(`/api/chat/events/${mockSession.session_id}`);
  });

  it('opens one EventSource per session when the panes differ', async () => {
    render(() => (
      <>
        <ChatProvider sessionId="session-a">
          <span />
        </ChatProvider>
        <ChatProvider sessionId="session-b">
          <span />
        </ChatProvider>
      </>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(2));
    expect(FakeEventSource.instances.map((source) => source.url)).toEqual([
      '/api/chat/events/session-a',
      '/api/chat/events/session-b',
    ]);
  });

  it('re-issues the source on a manual retry, and closes the dead one', async () => {
    render(() => (
      <ChatProvider sessionId={mockSession.session_id}>
        <RetryConsumer />
      </ChatProvider>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const dead = FakeEventSource.instances[0]!;

    screen.getByTestId('retry').click();

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(2));
    expect(dead.closed).toBe(true);
    expect(FakeEventSource.instances[1]!.closed).toBe(false);
  });

  it('keeps the retry of one pane from dropping the other pane off the stream', async () => {
    const seen: unknown[] = [];
    render(() => (
      <>
        <ChatProvider sessionId={mockSession.session_id}>
          <RetryConsumer />
        </ChatProvider>
        <ChatProvider sessionId={mockSession.session_id}>
          <TokenConsumer onMessages={(messages) => seen.push(...messages)} />
        </ChatProvider>
      </>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    screen.getByTestId('retry').click();
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(2));

    FakeEventSource.instances[1]!.emit('token', { type: 'token', content: 'still here' });

    await waitFor(() => expect(seen.length).toBeGreaterThan(0));
  });

  it('gives a pane that joins an open stream its open gate at once', async () => {
    const { setPendingFirstMessage } = await import('@/lib/draft-session');
    sendAnswer = () => ({ message_id: 'msg-turn-1' });

    render(() => (
      <ChatProvider sessionId={mockSession.session_id}>
        <span />
      </ChatProvider>
    ));
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    FakeEventSource.instances[0]!.open();

    // The message is staged after the first pane bound, so only the pane below
    // has one to send. Its gate is the stream's open, which happened already.
    setPendingFirstMessage(mockSession.session_id, 'first message from draft');
    render(() => (
      <ChatProvider sessionId={mockSession.session_id}>
        <span />
      </ChatProvider>
    ));

    // The wire took the staged turn, for the session the pane had bound to.
    await waitFor(() =>
      expect(sentTurns).toEqual([
        { session_id: mockSession.session_id, content: 'first message from draft' },
      ]),
    );
    expect(FakeEventSource.instances).toHaveLength(1);
  });
});

// The transcript itself, not a double of it. `lib/query/history.ts` owns one
// document per session, and every pane reads that one; these cases prove the
// provider reaches it through that key, so the count of history requests is
// the count of sessions read and not the count of binds onto them.
describe('the shared session transcript', () => {
  /** The session of each history read, in order. */
  const asked = () => [...historyAsked];

  /** Lets every pending answer land, so a second request would be counted. */
  const flush = async () => {
    for (let tick = 0; tick < 3; tick += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
  };


  it('reads the transcript once for two panes on one session', async () => {
    render(() => (
      <>
        <ChatProvider sessionId={mockSession.session_id}>
          <span />
        </ChatProvider>
        <ChatProvider sessionId={mockSession.session_id}>
          <span />
        </ChatProvider>
      </>
    ));

    await waitFor(() => expect(asked()).toEqual([mockSession.session_id]));
    // Both panes have bound, and a second request would have been made by the
    // time the first one's answer has been folded twice over.
    await flush();
    expect(asked()).toEqual([mockSession.session_id]);
  });

  it('a bind onto another session asks for that one, and not again for the first', async () => {
    // The pane used to abort the load in flight and start over on every bind,
    // so coming back to a session it had read asked for the whole transcript
    // again. The test client drops an unobserved entry at once (`gcTime: 0`);
    // this case is about the lifetime the app gives an entry, so it asks for
    // that lifetime.
    env.client.setDefaultOptions({ queries: { ...queryClientOptions.defaultOptions?.queries } });
    const [id, setId] = createSignal('session-a');

    // A read that has landed, not merely started: the wire answers over a
    // real fetch, and switching mid-read aborts it — a different claim than
    // the one this case exists for.
    const read = (session: string) =>
      waitFor(() => expect(env.client.getQueryData(keys.sessionHistory(session))).toBeDefined());

    render(() => (
      <ChatProvider sessionId={id()}>
        <span />
      </ChatProvider>
    ));
    await read('session-a');
    expect(asked()).toEqual(['session-a']);

    setId('session-b');
    await read('session-b');
    expect(asked()).toEqual(['session-a', 'session-b']);

    // Back to the first: the app's cache holds its transcript, so the bind
    // reads it where it sits instead of asking the daemon again.
    setId('session-a');
    await flush();
    expect(asked()).toEqual(['session-a', 'session-b']);
  });
});
