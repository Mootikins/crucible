import { render, screen, waitFor } from '@solidjs/testing-library';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createSignal } from 'solid-js';
import { ChatProvider, useChat, useChatSafe } from './ChatContext';
import * as api from '@/lib/api';
import type { Session } from '@/lib/types';

vi.mock('@/lib/api', () => ({
  sendChatMessage: vi.fn(),
  subscribeToEvents: vi.fn(() => () => {}),
  respondToInteraction: vi.fn(),
  generateMessageId: () => `msg_${Date.now()}_test`,
}));

const mockSendChatMessage = api.sendChatMessage as ReturnType<typeof vi.fn>;
const mockSubscribeToEvents = api.subscribeToEvents as ReturnType<typeof vi.fn>;

const mockSession: Session = {
  id: 'test-session-1',
  session_type: 'chat',
  kiln: '/tmp/test-kiln',
  workspace: '/tmp/test-workspace',
  state: 'active',
  title: 'Test Session',
  agent_model: 'test-model',
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
  return <ChatProvider session={session} setSessionTitle={async () => {}}>{props.children}</ChatProvider>;
}

describe('ChatContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSubscribeToEvents.mockReturnValue(() => {});
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

    await new Promise((r) => setTimeout(r, 50));

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
      tool_calls: [],
    });

    await waitFor(() => {
      expect(screen.getByTestId('loading').textContent).toBe('idle');
    });
  });
});

describe('session switching', () => {
  const mockSession2: Session = {
    id: 'test-session-2',
    session_type: 'chat',
    kiln: '/tmp/test-kiln',
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
      <ChatProvider session={session} setSessionTitle={async () => {}}>
        {props.children}
        <button data-testid="switch-session" onClick={() => setSession(mockSession2)}>Switch</button>
        <button data-testid="clear-session" onClick={() => setSession(null)}>Clear</button>
      </ChatProvider>
    );
  }

  beforeEach(() => {
    vi.clearAllMocks();
    mockSubscribeToEvents.mockReturnValue(() => {});
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
