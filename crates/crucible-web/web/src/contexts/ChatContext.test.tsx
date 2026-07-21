import { render, screen, waitFor } from '@solidjs/testing-library';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createSignal } from 'solid-js';
import { ChatProvider, useChat, useChatSafe } from './ChatContext';
import * as api from '@/lib/api';
import type { Session } from '@/lib/types';

vi.mock('@/lib/api', () => ({
  // Resolves the backend-minted turn id (the transcript is keyed on it).
  sendChatMessage: vi.fn(async () => 'msg-turn-1'),
  subscribeToEvents: vi.fn(() => () => {}),
  respondToInteraction: vi.fn(),
  getSession: vi.fn(),
  getSessionHistory: vi.fn(async () => ({ history: [], total_events: 0 })),
  getConfig: vi.fn(async () => ({ kiln_path: '/tmp/test-kiln' })),
  listSessions: vi.fn(async () => []),
  setSessionTitle: vi.fn(),
  // Monotonic — sendMessage mints two temp ids back-to-back, and a
  // Date.now()-based id would collide within one millisecond.
  generateMessageId: (() => {
    let n = 0;
    return () => `msg_${++n}_test`;
  })(),
  turnResponseId: (id: string) => `${id}-response`,
  turnSegmentId: (id: string, index: number) => `${id}-seg-${index}`,
  stripFrozenPrefix: (full: string, segs: string[]) => {
    const prefix = segs.join('');
    return prefix && full.startsWith(prefix) ? full.slice(prefix.length) : full;
  },
}));

const mockSendChatMessage = api.sendChatMessage as ReturnType<typeof vi.fn>;
const mockSubscribeToEvents = api.subscribeToEvents as ReturnType<typeof vi.fn>;
const mockGetSession = api.getSession as ReturnType<typeof vi.fn>;
const mockGetSessionHistory = api.getSessionHistory as ReturnType<typeof vi.fn>;
const mockListSessions = api.listSessions as ReturnType<typeof vi.fn>;

const mockSession: Session = {
  id: 'test-session-1',
  session_type: 'chat',
  kiln: '/tmp/test-kiln',
  workspace: '/tmp/test-workspace',
  connected_kilns: [],
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
});

describe('session switching', () => {
  const mockSession2: Session = {
    id: 'test-session-2',
    session_type: 'chat',
    kiln: '/tmp/test-kiln',
    workspace: '/tmp/test-workspace',
    connected_kilns: [],
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
        kiln: '/tmp/test-kiln',
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
      '/tmp/test-kiln',
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
      event_type: 'user_message',
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
