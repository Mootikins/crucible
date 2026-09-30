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
import { getBus } from '@/lib/bus';
import { statusBarActions } from '@/stores/statusBarStore';
import {
  emitOps,
  historyOf,
  notice,
  segment,
  toolCard,
  upsert,
  userTurn,
} from '@/test-utils/transcript';

// The optimistic entries take ids from `lib/turn.ts`; deterministic here.
vi.mock('@/lib/turn', async (original) => ({
  ...await original<object>(),
  generateMessageId: (() => {
    let n = 0;
    return () => `msg_${++n}_test`;
  })(),
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
  archived: false,
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
/** The slash commands that reached the command route of the main session. */
const sentCommands: string[] = [];

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
  routes[`POST /api/session/${ID}/command`] = async (request) => {
    sentCommands.push(((await request.clone().json()) as { command: string }).command);
    return { result: 'Context cleared', type: 'success' };
  };
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
  sendAnswer = () => ({ outcome: 'turn', message_id: 'msg-turn-1' });
  listAnswer = () => ({ sessions: [], total: 0 });
  setModeAnswer = () => new Response(null, { status: 204 });
  historyAsked.length = 0;
  historyLimits.length = 0;
  sentTurns.length = 0;
  sentCommands.length = 0;
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
    sendAnswer = () => ({ outcome: 'turn', message_id: 'msg_server_1' });

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    // Let the mount-time bootstrap (empty history) fire its load first — the
    // snapshot must not clobber the optimistic entry. Anchor on the actual
    // history read rather than an arbitrary sleep.
    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBeGreaterThan(0));

    const sendButton = screen.getByText('Send');
    sendButton.click();

    await waitFor(() => {
      expect(screen.getByTestId('count').textContent).toBe('1');
    });

    const items = screen.getAllByRole('listitem');
    expect(items[0].getAttribute('data-role')).toBe('user');
    expect(items[0].textContent).toBe('test message');
  });

  // A plugin command runs in the daemon and opens no turn. The optimistic
  // entry goes, and the result shows as a system line.
  it('shows a command result instead of a turn', async () => {
    sendAnswer = () => ({ outcome: 'command', command: 'reflect', result: 'reflected' });

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));
    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBeGreaterThan(0));

    screen.getByText('Send').click();

    await waitFor(() => {
      const items = screen.getAllByRole('listitem');
      expect(items).toHaveLength(1);
      expect(items[0].getAttribute('data-role')).toBe('system');
      expect(items[0].textContent).toBe('/reflect: reflected');
    });
    expect(screen.getByTestId('loading').textContent).toBe('idle');
  });

  it('does not send without session', async () => {
    sendAnswer = () => ({ outcome: 'turn', message_id: 'msg_server_1' });

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
    sendAnswer = () => ({ outcome: 'turn', message_id: 'msg_server_1' });

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

    FakeEventSource.instances[0]!.emit('turn_finished', { event: 'turn_finished', data: { status: 'completed' } });

    await waitFor(() => {
      expect(screen.getByTestId('loading').textContent).toBe('idle');
    });
  });
});

describe('the daemon transcript replaces the optimistic entry', () => {
  it('shows the echo once when it beats the send answer', async () => {
    // Hold the POST open so the echo arrives mid-flight.
    const held = deferred<{ outcome: 'turn'; message_id: string }>();
    sendAnswer = () => held.promise;
    historyAnswer = () => historyOf(ID, [], 0);

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBeGreaterThan(0));
    screen.getByText('Send').click();
    await waitFor(() => expect(env.fetch.calls(SEND_ROUTE)).toBe(1));
    expect(screen.getByTestId('count').textContent).toBe('1');

    const stream = FakeEventSource.instances[0]!;
    emitOps(stream, 1, [upsert(userTurn('msg-turn-1', 'test message'))]);
    await waitFor(() => expect(screen.queryByTestId('msg-msg-turn-1')).not.toBeNull());
    // The echo replaced the optimistic entry before the send answered.
    expect(screen.getByTestId('count').textContent).toBe('1');

    held.resolve({ outcome: 'turn', message_id: 'msg-turn-1' });
    emitOps(stream, 2, [upsert(segment('msg-turn-1', 0, 'partial answer', { streaming: true }))]);

    await waitFor(() => {
      const items = screen.getAllByRole('listitem');
      expect(items.map((i) => i.getAttribute('data-role'))).toEqual(['user', 'assistant']);
      expect(items[1].textContent).toBe('partial answer');
    });
  });
});

describe('a reply the provider cut off', () => {
  // The daemon words the note, and it is an item of the transcript. The text
  // below is deliberately not a wording the daemon ships: the page must draw
  // the string it received.
  it('draws the note the daemon worded', async () => {
    historyAnswer = () =>
      historyOf(ID, [
        userTurn('msg-turn-1', 'q'),
        segment('msg-turn-1', 0, 'Half an ans'),
        notice(
          'msg-turn-1-stop',
          { kind: 'stop_reason', reason: 'max_tokens', text: 'a note only the daemon can word' },
          'msg-turn-1',
        ),
      ]);

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      const items = screen.getAllByRole('listitem');
      const system = items.find((i) => i.getAttribute('data-role') === 'system');
      expect(system?.textContent).toBe('a note only the daemon can word');
    });
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
    sendAnswer = () => ({ outcome: 'turn', message_id: 'msg-turn-1' });

    const { setPendingFirstMessage } = await import('@/lib/draft-session');
    setPendingFirstMessage(mockSession.session_id, 'first message from draft');

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('1'));
    const items = screen.getAllByRole('listitem');
    expect(items[0].getAttribute('data-role')).toBe('user');
    expect(items[0].textContent).toBe('first message from draft');
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
    sendAnswer = () => ({ outcome: 'turn', message_id: 'msg-turn-1' });

    const { setPendingFirstMessage } = await import('@/lib/draft-session');
    setPendingFirstMessage(mockSession.session_id, 'first message from draft');

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    // The staged turn goes up at once, as the case above proves.
    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('1'));
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
    expect(screen.getByTestId('count').textContent).toBe('1');
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
    archived: false,
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
    sendAnswer = () => ({ outcome: 'turn', message_id: 'msg_server_1' });

    render(() => (
      <DynamicTestWrapper>
        <TestConsumer />
      </DynamicTestWrapper>
    ));

    screen.getByText('Send').click();

    await waitFor(() => {
      expect(screen.getByTestId('count').textContent).toBe('1');
    });
  });

  it('clears messages when switching to different session', async () => {
    sendAnswer = () => ({ outcome: 'turn', message_id: 'msg_server_1' });

    render(() => (
      <DynamicTestWrapper>
        <TestConsumer />
      </DynamicTestWrapper>
    ));

    screen.getByText('Send').click();

    await waitFor(() => {
      expect(screen.getByTestId('count').textContent).toBe('1');
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
            <li data-testid={`hist-msg-${m.id}`} data-role={m.role} data-plugin={m.plugin} data-via={m.via}>
              {m.content}
            </li>
          ))}
        </ul>
      </div>
    );
  }

  it('draws the clear marker the snapshot holds between two turns', async () => {
    historyAnswer = () =>
      historyOf(ID, [
        userTurn('before', 'before'),
        notice('clear-1', { kind: 'context_cleared', plugin: 'alpha' }),
        userTurn('after', 'after'),
      ]);
    render(() => <TestWrapper><HistoryTestConsumer /></TestWrapper>);
    await waitFor(() => expect(screen.getByTestId('msg-count').textContent).toBe('3'));
    expect(screen.getByTestId('hist-msg-before').textContent).toBe('before');
    expect(screen.getByTestId('hist-msg-clear-1').textContent).toContain('alpha cleared the context');
    expect(screen.getByTestId('hist-msg-after').textContent).toBe('after');
  });

  it('draws a plugin turn as a named system message', async () => {
    historyAnswer = () =>
      historyOf(ID, [
        userTurn('plugin-turn', 'continue with details', {
          origin: { kind: 'plugin', name: 'alpha' } as never,
        }),
      ]);
    render(() => <TestWrapper><HistoryTestConsumer /></TestWrapper>);
    await waitFor(() => expect(screen.getByTestId('hist-msg-plugin-turn')).toBeInTheDocument());
    expect(screen.getByTestId('hist-msg-plugin-turn')).toHaveAttribute('data-role', 'system');
    expect(screen.getByTestId('hist-msg-plugin-turn')).toHaveAttribute('data-plugin', 'alpha');
  });

  it('draws a relayed message as a user message that names its relay', async () => {
    historyAnswer = () =>
      historyOf(ID, [
        userTurn('relayed', 'hi', { origin: { kind: 'relay', name: 'discord' } as never }),
      ]);
    render(() => <TestWrapper><HistoryTestConsumer /></TestWrapper>);
    await waitFor(() => expect(screen.getByTestId('hist-msg-relayed')).toBeInTheDocument());
    expect(screen.getByTestId('hist-msg-relayed')).toHaveAttribute('data-role', 'user');
    expect(screen.getByTestId('hist-msg-relayed')).toHaveAttribute('data-via', 'discord');
  });

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

  it('draws the turns of the snapshot', async () => {
    historyAnswer = () => historyOf(ID, [userTurn('msg1', 'hello'), segment('msg1', 0, 'hi there')]);

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

  it('draws the precognition badge of each turn', async () => {
    const precognition = (title: string, score: number) => ({
      notes_count: 1,
      notes: [{ title, kiln: 'docs', score }] as never,
    });
    historyAnswer = () =>
      historyOf(ID, [
        userTurn('u1', 'first', { precognition: precognition('Alpha', 0.9) }),
        segment('u1', 0, 'a'),
        userTurn('u2', 'second', { precognition: precognition('Beta', 0.8) }),
      ]);

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('precog').textContent).toBe('u1:1:Alpha,u2:1:Beta');
    });
    expect(screen.getByTestId('msg-count').textContent).toBe('3');
  });

  it('draws the segments and the tool card of a turn in the order of the snapshot', async () => {
    historyAnswer = () =>
      historyOf(ID, [
        userTurn('msg1', 'find it'),
        segment('msg1', 0, 'Let me look. '),
        toolCard('msg1', 'tc-1', { name: 'search', result: 'notes' }),
        segment('msg1', 1, 'Here it is.'),
      ]);

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
    // The ids are the daemon's.
    expect(screen.getByTestId('hist-msg-msg1-seg-0').textContent?.trim()).toBe('Let me look.');
    expect(screen.getByTestId('hist-msg-msg1-seg-1').textContent?.trim()).toBe('Here it is.');
  });

  it('falls back to persisted history when getSession fails', async () => {
    sessionAnswer = () => refusal(404, 'no such session');
    historyAnswer = () =>
      historyOf(ID, [userTurn('msg-user', 'persisted user'), segment('msg-user', 0, 'persisted assistant')]);

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

// A history read that lands ON TOP of a live turn must not move the
// transcript back: the store keeps its copy when the snapshot is older.
describe('a slow history load cannot move the transcript back', () => {
  const ANSWER = 'Here is what a new user does first.';

  it('keeps the live ops when an older snapshot arrives', async () => {
    // Held open until the live turn has finished.
    const held = deferred<unknown>();
    historyAnswer = () => held.promise;

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const stream = FakeEventSource.instances[0]!;
    // The first snapshot arrives, then the live turn.
    held.resolve(historyOf(ID, [], 0));
    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBeGreaterThan(0));
    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('0'));
    emitOps(stream, 1, [upsert(userTurn('msg-turn-1', 'test message'))]);
    emitOps(stream, 2, [upsert(segment('msg-turn-1', 0, '', { streaming: true }))]);
    emitOps(stream, 3, [{ op: 'append', id: 'msg-turn-1-seg-0', field: 'text', at: 0, text: ANSWER }]);
    await waitFor(() =>
      expect(screen.getAllByRole('listitem').some((li) => li.textContent === ANSWER)).toBe(true),
    );

    // A refetch answers a snapshot from before the answer.
    env.client.setQueryData(keys.sessionHistory(ID), historyOf(ID, [userTurn('msg-turn-1', 'test message')], 1));

    await Promise.resolve();
    const answers = screen.getAllByRole('listitem').filter((li) => li.textContent === ANSWER);
    expect(answers).toHaveLength(1);
    expect(screen.getByTestId('count').textContent).toBe('2');
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
    expect(FakeEventSource.instances[0]!.url).toBe(`/api/events?topics=${mockSession.session_id}`);
  });

  it('carries both sessions on the one shared connection when the panes differ', async () => {
    // Simplification Plan step 19: a second session's topic joining the
    // shared connection rebuilds it to carry both, rather than opening a
    // second `EventSource` — so the LATEST source, not a second one, is
    // what ends up carrying `session-a` too.
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

    await waitFor(() =>
      expect(FakeEventSource.instances.at(-1)!.url).toBe('/api/events?topics=session-a%2Csession-b'),
    );
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

    emitOps(FakeEventSource.instances[1]!, 1, [upsert(userTurn('t1', 'still here'))]);

    await waitFor(() => expect(seen.length).toBeGreaterThan(0));
  });

  it('gives a pane that joins an open stream its open gate at once', async () => {
    const { setPendingFirstMessage } = await import('@/lib/draft-session');
    sendAnswer = () => ({ outcome: 'turn', message_id: 'msg-turn-1' });

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

describe('palette Clear Chat', () => {
  // The palette entry (Ctrl+K) is the same clear as `/clear`: the daemon
  // clears the model context and sends context_cleared. The pane does not
  // drop its transcript on its own.
  it('sends /clear to the daemon and keeps the transcript', async () => {
    statusBarActions.setActiveSessionId(ID);
    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));
    await waitFor(() => expect(env.fetch.calls(HISTORY_ROUTE)).toBeGreaterThan(0));
    screen.getByText('Send').click();
    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('1'));

    getBus().emit('clearChat', {});

    await waitFor(() => expect(sentCommands).toEqual(['/clear']));
    expect(screen.getByTestId('count').textContent).toBe('1');
    statusBarActions.setActiveSessionId(null);
  });
});
