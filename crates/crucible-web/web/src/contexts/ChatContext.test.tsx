import { render, screen, waitFor } from '@solidjs/testing-library';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createEffect, createSignal } from 'solid-js';
import { ChatProvider, useChat, useChatSafe } from './ChatContext';
import * as api from '@/lib/api';
import { resetSseForTests } from '@/lib/query/sse';
import type { QueryClient } from '@tanstack/solid-query';
import { queryClientOptions, setQueryClientForTests } from '@/lib/query/client';
import { keys } from '@/lib/query/keys';
import { createTestQueryClient } from '@/test-utils/query';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';
import type { Session } from '@/lib/types';

vi.mock('@/lib/api', () => ({
  // Resolves the backend-minted turn id (the transcript is keyed on it).
  sendChatMessage: vi.fn(async () => 'msg-turn-1'),
  subscribeToEvents: vi.fn(() => () => {}),
  cancelSession: vi.fn(async () => true),
  getSession: vi.fn(),
  getSessionHistory: vi.fn(async () => ({ history: [], total_events: 0 })),
  getConfig: vi.fn(async () => ({ kiln_path: '/tmp/test-kiln' })),
  listSessions: vi.fn(async () => []),
  listModes: vi.fn(async () => ({
    current_mode_id: 'ask',
    modes: [
      { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
      { id: 'review', name: 'Review', description: null, icon: null, color: null },
    ],
  })),
  setSessionTitle: vi.fn(),
  setSessionMode: vi.fn(async () => {}),
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

// Every pane of one session shares one root in `lib/query/sse.ts`, and a root
// outlives the test that opened it. Forget them between cases, so a source of
// one test cannot answer the next one. The cache is per case for the same
// reason: `session.get` is answered from it, so one case's session would
// hydrate the next case's mode.
let queryClient: QueryClient;

beforeEach(() => {
  queryClient = createTestQueryClient();
  setQueryClientForTests(queryClient);
});

afterEach(() => {
  resetSseForTests();
  setQueryClientForTests(null);
});

const mockSendChatMessage = api.sendChatMessage as ReturnType<typeof vi.fn>;
const mockSubscribeToEvents = api.subscribeToEvents as ReturnType<typeof vi.fn>;
const mockGetSession = api.getSession as ReturnType<typeof vi.fn>;
const mockGetSessionHistory = api.getSessionHistory as ReturnType<typeof vi.fn>;
const mockListSessions = api.listSessions as ReturnType<typeof vi.fn>;

const mockSession: Session = {
  id: 'test-session-1',
  session_type: 'chat',
  kilns: ['/tmp/test-kiln'],
  workspace: '/tmp/test-workspace',
  state: 'active',
  title: 'Test Session',
  agent_model: 'test-model',
  agent_mode: null,
  started_at: new Date().toISOString(),
  event_count: 0,
};

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
  return <ChatProvider sessionId={session()?.id ?? ''}>{props.children}</ChatProvider>;
}

describe('ChatContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSubscribeToEvents.mockReturnValue(() => {});
    mockGetSession.mockResolvedValue(mockSession);
    mockListSessions.mockResolvedValue([]);
  });

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
    mockSendChatMessage.mockResolvedValue('msg_server_1');

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    // Let the mount-time bootstrap (empty history) fire its load first — the
    // merge in loadHistory must not clobber the optimistic messages. Anchor on
    // the actual bootstrap call rather than an arbitrary sleep.
    await waitFor(() => expect(mockGetSessionHistory).toHaveBeenCalled());

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
    mockSendChatMessage.mockResolvedValue('msg_server_1');

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
    expect(mockSendChatMessage).not.toHaveBeenCalled();
  });

  it('shows loading state while sending', async () => {
    let eventCallback: ((event: any) => void) | null = null;
    
    mockSubscribeToEvents.mockImplementation((_sessionId: string, callback: (event: any) => void) => {
      eventCallback = callback;
      return () => { eventCallback = null; };
    });
    mockSendChatMessage.mockResolvedValue('msg_server_1');

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    screen.getByText('Send').click();

    await waitFor(() => {
      expect(screen.getByTestId('loading').textContent).toBe('loading');
    });

    eventCallback!({
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
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSession.mockResolvedValue(mockSession);
    mockListSessions.mockResolvedValue([]);
    mockGetSessionHistory.mockResolvedValue({ history: [], total_events: 0 });
  });

  it('reconciles a message minted by a token that beat the send POST (no orphan bubble)', async () => {
    let eventCallback: ((event: any) => void) | null = null;
    mockSubscribeToEvents.mockImplementation(
      (_sessionId: string, callback: (event: any) => void, onOpen?: () => void) => {
        eventCallback = callback;
        onOpen?.();
        return () => { eventCallback = null; };
      },
    );
    // Hold the POST open so a token can arrive mid-flight.
    let resolveSend!: (id: string) => void;
    mockSendChatMessage.mockReturnValue(new Promise<string>((r) => { resolveSend = r; }));

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(eventCallback).not.toBeNull());
    screen.getByText('Send').click();
    await waitFor(() => expect(mockSendChatMessage).toHaveBeenCalled());

    // Token arrives before the POST resolves → reducer mints a random-id
    // assistant and streams into it.
    eventCallback!({ type: 'token', content: 'partial ' });

    // POST resolves with the canonical turn id. The early streaming message
    // must be reconciled into `${id}-response`, not left orphaned beside a new
    // empty placeholder.
    resolveSend('msg-turn-1');
    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('2'));

    eventCallback!({ type: 'message_complete', id: 'msg-turn-1', content: 'partial answer' });

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
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSession.mockResolvedValue(mockSession);
    mockListSessions.mockResolvedValue([]);
    mockGetSessionHistory.mockResolvedValue({ history: [], total_events: 0 });
  });

  // The daemon names the reason on `message_complete` and WORDS the note
  // beside it. This is where a reader meets it: a system line under the reply.
  //
  // The text below is deliberately not a wording the daemon ships. The page
  // must draw the string it received, so a test that used the real wording
  // could pass while the page derived the words itself.
  it('draws the note the daemon worded', async () => {
    let eventCallback: ((event: any) => void) | null = null;
    mockSubscribeToEvents.mockImplementation(
      (_sessionId: string, callback: (event: any) => void, onOpen?: () => void) => {
        eventCallback = callback;
        onOpen?.();
        return () => { eventCallback = null; };
      },
    );
    mockSendChatMessage.mockResolvedValue('msg-turn-1');

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(eventCallback).not.toBeNull());
    screen.getByText('Send').click();
    await waitFor(() => expect(mockSendChatMessage).toHaveBeenCalled());

    eventCallback!({
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
    let eventCallback: ((event: any) => void) | null = null;
    mockSubscribeToEvents.mockImplementation(
      (_sessionId: string, callback: (event: any) => void, onOpen?: () => void) => {
        eventCallback = callback;
        onOpen?.();
        return () => { eventCallback = null; };
      },
    );
    mockSendChatMessage.mockResolvedValue('msg-turn-2');

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(eventCallback).not.toBeNull());
    screen.getByText('Send').click();
    await waitFor(() => expect(mockSendChatMessage).toHaveBeenCalled());

    eventCallback!({
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
  beforeEach(() => {
    vi.clearAllMocks();
    mockListSessions.mockResolvedValue([]);
  });

  afterEach(async () => {
    // The staged message survives rendering now (peek, not consume — the
    // destructive read happens only at dispatch, which these tests hold
    // open). Drain it so it can't leak into later tests as a phantom
    // optimistic turn.
    const { consumePendingFirstMessage } = await import('@/lib/draft-session');
    consumePendingFirstMessage(mockSession.id);
  });

  it('renders the user message and working indicator immediately, before bootstrap and SSE resolve', async () => {
    // Neither gate ever resolves: bootstrap hangs, SSE never opens. The
    // optimistic turn must render anyway — the user should never stare at an
    // empty transcript after sending their first draft message.
    mockGetSession.mockReturnValue(new Promise(() => {}));
    mockGetSessionHistory.mockReturnValue(new Promise(() => {}));
    mockSubscribeToEvents.mockImplementation(() => () => {});
    mockSendChatMessage.mockResolvedValue('msg-turn-1');

    const { setPendingFirstMessage } = await import('@/lib/draft-session');
    setPendingFirstMessage(mockSession.id, 'first message from draft');

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
    expect(mockSendChatMessage).not.toHaveBeenCalled();
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
    queryClient.setQueryData(keys.session(mockSession.id), seeded);
    mockGetSession.mockResolvedValue(seeded);
    mockGetSessionHistory.mockResolvedValue({ history: [], total_events: 0 });
    mockSubscribeToEvents.mockImplementation(() => () => {});
    mockSendChatMessage.mockResolvedValue('msg-turn-1');

    const { setPendingFirstMessage } = await import('@/lib/draft-session');
    setPendingFirstMessage(mockSession.id, 'first message from draft');

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
    await waitFor(() => expect(mockGetSessionHistory).toHaveBeenCalled());
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
    id: 'test-session-2',
    session_type: 'chat',
    kilns: ['/tmp/test-kiln'],
    workspace: '/tmp/test-workspace',
    state: 'active',
    title: 'Test Session 2',
    agent_model: 'test-model',
    agent_mode: null,
    started_at: new Date().toISOString(),
    event_count: 0,
  };

  function DynamicTestWrapper(props: { children: any }) {
    const [session, setSession] = createSignal<Session | null>(mockSession);
    return (
      <ChatProvider sessionId={session()?.id ?? ''}>
        {props.children}
        <button data-testid="switch-session" onClick={() => setSession(mockSession2)}>Switch</button>
        <button data-testid="clear-session" onClick={() => setSession(null)}>Clear</button>
      </ChatProvider>
    );
  }

  beforeEach(() => {
    vi.clearAllMocks();
    mockSubscribeToEvents.mockReturnValue(() => {});
    mockGetSession.mockResolvedValue(mockSession);
    mockListSessions.mockResolvedValue([]);
  });

  it('does not clear messages on initial mount', async () => {
    mockSendChatMessage.mockResolvedValue('msg_server_1');

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
    mockSendChatMessage.mockResolvedValue('msg_server_1');

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

  beforeEach(() => {
    vi.clearAllMocks();
    mockSubscribeToEvents.mockReturnValue(() => {});
    mockGetSession.mockResolvedValue(mockSession);
    mockListSessions.mockResolvedValue([]);
  });

  it('is true during history load and false after', async () => {
    let resolveHistory!: (value: any) => void;
    const historyPromise = new Promise((resolve) => {
      resolveHistory = resolve;
    });
    mockGetSessionHistory.mockReturnValue(historyPromise);

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => {
      expect(screen.getByTestId('history-loading').textContent).toBe('loading');
    });

    resolveHistory({ history: [], total_events: 0 });

    await waitFor(() => {
      expect(screen.getByTestId('history-loading').textContent).toBe('idle');
    });
  });

  it('resets to false on error', async () => {
    mockGetSessionHistory.mockRejectedValue(new Error('Network error'));

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
    mockGetSessionHistory.mockResolvedValue({
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
    mockGetSessionHistory.mockResolvedValue({
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
    mockGetSessionHistory.mockResolvedValue({
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
    mockGetSessionHistory.mockResolvedValue({
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
    mockGetSessionHistory.mockResolvedValue({
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
    mockGetSessionHistory.mockResolvedValue({
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
    mockGetSession.mockRejectedValue(new Error('Session not found'));
    mockListSessions.mockResolvedValue([
      {
        ...mockSession,
        id: 'test-session-1',
        kilns: ['/tmp/test-kiln'],
      },
    ]);
    mockGetSessionHistory.mockResolvedValue({
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

    expect(mockGetSessionHistory).toHaveBeenCalledWith(
      'test-session-1',
      10000,
      undefined,
      expect.any(AbortSignal),
    );
  });

  it('dedups a live canonical user message against a pre-canonical reconstructed one', async () => {
    let eventCallback: ((event: any) => void) | null = null;
    mockSubscribeToEvents.mockImplementation(
      (_sessionId: string, callback: (event: any) => void, onOpen?: () => void) => {
        eventCallback = callback;
        onOpen?.();
        return () => { eventCallback = null; };
      },
    );
    // Hold history open so a live SSE echo lands in the message list first.
    let resolveHistory!: (value: any) => void;
    mockGetSessionHistory.mockReturnValue(new Promise((resolve) => { resolveHistory = resolve; }));

    render(() => (
      <TestWrapper>
        <HistoryTestConsumer />
      </TestWrapper>
    ));

    // Live echo adds the prompt under its canonical id.
    await waitFor(() => expect(eventCallback).not.toBeNull());
    eventCallback!({
      type: 'session_event',
      event: 'user_message',
      data: { message_id: 'msg-live', content: 'hello' },
    });
    await waitFor(() => expect(screen.getByTestId('msg-count').textContent).toBe('1'));
    await waitFor(() => expect(mockGetSessionHistory).toHaveBeenCalled());

    // History replays the SAME prompt from an old event that predates canonical
    // message_ids → reconstructed under a fallback id (user-0).
    resolveHistory({
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
    (api.listModes as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('daemon down'));
    mockGetSession.mockResolvedValue({ ...mockSession, agent_mode: 'review' });

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
    mockGetSession.mockResolvedValue(mockSession);

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
    mockGetSession.mockResolvedValue({ ...mockSession, agent_mode: 'review' });
    (api.listModes as ReturnType<typeof vi.fn>).mockResolvedValue({
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
    const listModes = api.listModes as ReturnType<typeof vi.fn>;
    listModes.mockRejectedValueOnce(new Error('daemon down'));
    listModes.mockResolvedValue({
      current_mode_id: 'ask',
      modes: [
        { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
        { id: 'review', name: 'Review', description: null, icon: null, color: null },
      ],
    });
    (api.setSessionMode as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new Error("unknown mode 'plan'")
    );

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
    let emit: ((event: unknown) => void) | null = null;
    mockSubscribeToEvents.mockImplementation(
      (_sessionId: string, onEvent: (event: unknown) => void, onOpen?: () => void) => {
        emit = onEvent;
        onOpen?.();
        return () => {};
      },
    );
    // Held open until the live turn has finished.
    let releaseHistory: ((value: unknown) => void) | null = null;
    mockGetSessionHistory.mockImplementation(
      () => new Promise((resolve) => {
        releaseHistory = resolve;
      }),
    );
    mockSendChatMessage.mockResolvedValue('msg-turn-1');

    render(() => (
      <TestWrapper>
        <TestConsumer />
      </TestWrapper>
    ));

    await waitFor(() => expect(emit).not.toBeNull());
    screen.getByText('Send').click();
    // The send POST returns the turn id, which renames the optimistic
    // placeholder to the canonical response id.
    await waitFor(() => expect(screen.queryByTestId('msg-msg-turn-1-response')).not.toBeNull());

    // Live turn: reason, call a tool, reason again, then answer.
    emit!({ type: 'thinking', content: 'The user wants the guide. ' });
    emit!({ type: 'tool_call', id: 'call-a', title: 'read_note', arguments: { path: 'g.md' } });
    emit!({ type: 'tool_result', id: 'call-a', result: '{}' });
    emit!({ type: 'thinking', content: 'Now I can answer. ' });
    emit!({ type: 'token', content: ANSWER });
    emit!({ type: 'message_complete', id: 'msg-turn-1', content: ANSWER, total_tokens: 6707 });

    await waitFor(() =>
      expect(screen.getAllByRole('listitem').some((li) => li.textContent === ANSWER)).toBe(true)
    );

    // The daemon's own record of the same turn arrives now.
    releaseHistory!({
      history: [
        { event: 'user_message', data: { message_id: 'msg-turn-1', content: 'test message' } },
        { event: 'tool_call', data: { call_id: 'call-a', tool: 'read_note', args: { path: 'g.md' } } },
        { event: 'tool_result', data: { call_id: 'call-a', result: '{}' } },
        { event: 'message_complete', data: { message_id: 'msg-turn-1', full_response: ANSWER } },
      ],
      total_events: 4,
    });

    await waitFor(() => expect(mockGetSessionHistory).toHaveBeenCalled());
    await waitFor(() => {
      const answers = screen.getAllByRole('listitem').filter((li) => li.textContent === ANSWER);
      expect(answers).toHaveLength(1);
    });
  });
});


// The stream itself, not the mock of it. `lib/query/sse.ts` owns one source per
// session and every pane subscribes to it; these cases prove that the provider
// reaches the stream through that root, so the count of EventSources is the
// count of sessions on screen and not the count of panes.
describe('the shared session stream', () => {
  let realSubscribeToEvents: typeof api.subscribeToEvents;

  beforeEach(async () => {
    const actual = await vi.importActual<typeof import('@/lib/api')>('@/lib/api');
    realSubscribeToEvents = actual.subscribeToEvents;
    installFakeEventSource();
    mockSubscribeToEvents.mockImplementation(realSubscribeToEvents);
    mockGetSessionHistory.mockResolvedValue({ history: [], total_events: 0 });
  });

  afterEach(async () => {
    const { consumePendingFirstMessage } = await import('@/lib/draft-session');
    consumePendingFirstMessage(mockSession.id);
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
        <ChatProvider sessionId={mockSession.id}>
          <span />
        </ChatProvider>
        <ChatProvider sessionId={mockSession.id}>
          <span />
        </ChatProvider>
      </>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    expect(FakeEventSource.instances[0]!.url).toBe(`/api/chat/events/${mockSession.id}`);
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
      <ChatProvider sessionId={mockSession.id}>
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
        <ChatProvider sessionId={mockSession.id}>
          <RetryConsumer />
        </ChatProvider>
        <ChatProvider sessionId={mockSession.id}>
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
    mockSendChatMessage.mockResolvedValue('msg-turn-1');

    render(() => (
      <ChatProvider sessionId={mockSession.id}>
        <span />
      </ChatProvider>
    ));
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    FakeEventSource.instances[0]!.open();

    // The message is staged after the first pane bound, so only the pane below
    // has one to send. Its gate is the stream's open, which happened already.
    setPendingFirstMessage(mockSession.id, 'first message from draft');
    render(() => (
      <ChatProvider sessionId={mockSession.id}>
        <span />
      </ChatProvider>
    ));

    await waitFor(() =>
      expect(mockSendChatMessage).toHaveBeenCalledWith(mockSession.id, 'first message from draft'),
    );
    expect(FakeEventSource.instances).toHaveLength(1);
  });
});

// The transcript itself, not the mock of it. `lib/query/history.ts` owns one
// document per session, and every pane reads that one; these cases prove the
// provider reaches it through that key, so the count of history requests is
// the count of sessions read and not the count of binds onto them.
describe('the shared session transcript', () => {
  /** The session of each history request, in order. */
  const asked = () => mockGetSessionHistory.mock.calls.map((call) => call[0]);

  /** Lets every pending answer land, so a second request would be counted. */
  const flush = async () => {
    for (let tick = 0; tick < 3; tick += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
  };

  beforeEach(() => {
    vi.clearAllMocks();
    // The describe above hands the provider the real `subscribeToEvents`; this
    // one counts binds, so it hands back a stream that does nothing.
    mockSubscribeToEvents.mockImplementation(() => () => {});
    // The record answers under the id it was asked for: the bind reads the
    // transcript of the session the daemon named, so a fixed record would
    // have every pane read one transcript whatever it bound to.
    mockGetSession.mockImplementation(async (id: string) => ({ ...mockSession, id }));
    mockListSessions.mockResolvedValue([]);
    mockGetSessionHistory.mockResolvedValue({ history: [], total_events: 0 });
  });

  it('reads the transcript once for two panes on one session', async () => {
    render(() => (
      <>
        <ChatProvider sessionId={mockSession.id}>
          <span />
        </ChatProvider>
        <ChatProvider sessionId={mockSession.id}>
          <span />
        </ChatProvider>
      </>
    ));

    await waitFor(() => expect(asked()).toEqual([mockSession.id]));
    // Both panes have bound, and a second request would have been made by the
    // time the first one's answer has been folded twice over.
    await flush();
    expect(asked()).toEqual([mockSession.id]);
  });

  it('a bind onto another session asks for that one, and not again for the first', async () => {
    // The pane used to abort the load in flight and start over on every bind,
    // so coming back to a session it had read asked for the whole transcript
    // again. The test client drops an unobserved entry at once (`gcTime: 0`);
    // this case is about the lifetime the app gives an entry, so it asks for
    // that lifetime.
    queryClient.setDefaultOptions({ queries: { ...queryClientOptions.defaultOptions?.queries } });
    const [id, setId] = createSignal('session-a');

    render(() => (
      <ChatProvider sessionId={id()}>
        <span />
      </ChatProvider>
    ));
    await waitFor(() => expect(asked()).toEqual(['session-a']));

    setId('session-b');
    await waitFor(() => expect(asked()).toEqual(['session-a', 'session-b']));

    setId('session-a');
    await flush();
    expect(asked()).toEqual(['session-a', 'session-b']);
  });
});
