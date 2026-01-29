import { render, screen, waitFor } from '@solidjs/testing-library';
import { describe, it, expect, vi } from 'vitest';
import { ChatProvider, useChat } from './ChatContext';
import * as api from '@/lib/api';
import type { ChatEvent } from '@/lib/types';

vi.mock('@/lib/api', () => ({
  sendChatMessage: vi.fn(),
  generateMessageId: () => `msg_${Date.now()}_test`,
}));

const mockSendChatMessage = api.sendChatMessage as ReturnType<typeof vi.fn>;

// Test component that uses the context
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

describe('ChatContext', () => {
  it('starts with empty messages', () => {
    render(() => (
      <ChatProvider>
        <TestConsumer />
      </ChatProvider>
    ));

    expect(screen.getByTestId('count').textContent).toBe('0');
    expect(screen.getByTestId('loading').textContent).toBe('idle');
  });

  it('adds user message when sending', async () => {
    mockSendChatMessage.mockImplementation(
      async (_msg: string, onEvent: (event: ChatEvent) => void) => {
        onEvent({ type: 'message_complete', id: 'msg_1', content: 'ok', tool_calls: [] });
      }
    );

    render(() => (
      <ChatProvider>
        <TestConsumer />
      </ChatProvider>
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

  it('streams assistant response', async () => {
    mockSendChatMessage.mockImplementation(
      async (_msg: string, onEvent: (event: ChatEvent) => void) => {
        onEvent({ type: 'token', content: 'This is a ' });
        onEvent({ type: 'token', content: 'test response' });
        onEvent({
          type: 'message_complete',
          id: 'msg_1',
          content: 'This is a test response',
          tool_calls: [],
        });
      }
    );

    render(() => (
      <ChatProvider>
        <TestConsumer />
      </ChatProvider>
    ));

    screen.getByText('Send').click();

    await waitFor(() => {
      const items = screen.getAllByRole('listitem');
      expect(items[1].getAttribute('data-role')).toBe('assistant');
      expect(items[1].textContent).toContain('test response');
    });
  });

  it('shows loading state while sending', async () => {
    let resolveMessage: () => void;
    const messagePromise = new Promise<void>((resolve) => {
      resolveMessage = resolve;
    });

    mockSendChatMessage.mockImplementation(
      async (_msg: string, onEvent: (event: ChatEvent) => void) => {
        await messagePromise;
        onEvent({ type: 'message_complete', id: 'msg_1', content: 'done', tool_calls: [] });
      }
    );

    render(() => (
      <ChatProvider>
        <TestConsumer />
      </ChatProvider>
    ));

    screen.getByText('Send').click();

    await waitFor(() => {
      expect(screen.getByTestId('loading').textContent).toBe('loading');
    });

    resolveMessage!();

    await waitFor(() => {
      expect(screen.getByTestId('loading').textContent).toBe('idle');
    });
  });
});
