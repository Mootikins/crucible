import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import type { InteractionRequest, Message, Session } from '@/lib/types';

/**
 * The empty state and a pending interaction are mutually exclusive.
 *
 * A permission request can arrive before the transcript holds a single
 * message — the agent's first act of a turn can be a write it must ask for.
 * The empty state used to gate on `messages().length === 0` alone, so the
 * record rendered with "Start a conversation…" floating under it, at the
 * exact moment the user decides whether the agent may touch the disk.
 *
 * The transcript holds the RECORD of the request, one line at the point the
 * agent stopped. The card that answers it is docked on the composer, so the
 * control the session is parked on cannot scroll out of sight.
 */

const [messages, setMessages] = createSignal<Message[]>([]);
const [pending, setPending] = createSignal<InteractionRequest | null>(null);
const [session, setSession] = createSignal<Session | null>(null);

vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    messages,
    isStreaming: () => false,
    sessionId: () => 's1',
    sendMessage: async () => {},
    pendingInteraction: pending,
    respondToInteraction: async () => {},
  }),
}));
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: session,
    sessions: () => [],
  }),
}));
vi.mock('@/components/DiffViewer', () => ({
  DiffViewer: () => <div data-testid="diff-viewer" />,
}));

const { MessageList } = await import('../MessageList');

const activeSession = (): Session => ({
  session_id: 's1',
  type: 'chat',
  kilns: ['/repo'],
  workspace: '/repo',
  state: 'active',
  title: null,
  agent_model: null,
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
});

const permission = (): InteractionRequest => ({
  kind: 'permission',
  id: 'perm-1',
  action_type: 'bash',
  tokens: ['rm', '-rf', 'build'],
});

afterEach(() => {
  cleanup();
  setMessages([]);
  setPending(null);
  setSession(null);
});

describe('MessageList empty state', () => {
  it('renders with a session and no messages', () => {
    setSession(activeSession());
    render(() => <MessageList />);
    expect(screen.getByTestId('message-list-empty')).toBeInTheDocument();
  });

  it('renders with no session at all', () => {
    render(() => <MessageList />);
    expect(screen.getByTestId('message-list-empty')).toBeInTheDocument();
    expect(screen.getByText('Select or create a session to start chatting')).toBeInTheDocument();
  });

  it('yields to a pending interaction that arrives before the first message', () => {
    setSession(activeSession());
    setPending(permission());
    render(() => <MessageList />);

    // The record, not the card: the card is ChatInput's.
    const record = screen.getByTestId('interaction-record');
    expect(record.textContent).toContain('Asked to run rm -rf build');
    expect(record.textContent).toContain('waiting');
    expect(screen.queryByText('Permission Required')).toBeNull();
    expect(screen.queryByTestId('perm-allow')).toBeNull();
    // ...and nothing invites the user to "start a conversation" underneath it.
    expect(screen.queryByTestId('message-list-empty')).toBeNull();
    expect(
      screen.queryByText('Start a conversation by typing a message or using voice input'),
    ).toBeNull();
  });

  it('yields to a pending interaction even with no session bound', () => {
    setPending(permission());
    render(() => <MessageList />);

    expect(screen.getByTestId('interaction-record')).toBeInTheDocument();
    expect(screen.queryByTestId('message-list-empty')).toBeNull();
    expect(screen.queryByText('Select or create a session to start chatting')).toBeNull();
  });

  it('comes back once the interaction is answered and the transcript is still empty', () => {
    setSession(activeSession());
    setPending(permission());
    render(() => <MessageList />);
    expect(screen.queryByTestId('message-list-empty')).toBeNull();

    setPending(null);
    expect(screen.getByTestId('message-list-empty')).toBeInTheDocument();
  });
});

describe('MessageList interaction record', () => {
  it('names the file a write asked for', () => {
    setSession(activeSession());
    setPending({
      kind: 'permission',
      id: 'perm-2',
      action_type: 'write',
      tokens: ['docs/Meta/Product.md'],
    });
    render(() => <MessageList />);

    expect(screen.getByTestId('interaction-record').textContent).toContain(
      'Asked to write docs/Meta/Product.md',
    );
  });

  it('quotes the question an ask asked', () => {
    setSession(activeSession());
    setPending({
      kind: 'ask',
      id: 'ask-1',
      question: 'Which branch should I use?',
    } as InteractionRequest);
    render(() => <MessageList />);

    const record = screen.getByTestId('interaction-record');
    expect(record.textContent).toContain('Asked: Which branch should I use?');
    expect(record.textContent).toContain('waiting');
  });

  it('draws nothing while no request is open', () => {
    setSession(activeSession());
    setMessages([
      {
        id: 'm1',
        role: 'user',
        content: 'hi',
        timestamp: Date.now(),
      } as Message,
    ]);
    render(() => <MessageList />);

    expect(screen.queryByTestId('interaction-record')).toBeNull();
  });
});
