import { render, screen, waitFor } from '@solidjs/testing-library';
import { describe, it, expect, vi } from 'vitest';
import { ChatProvider, useChat } from './ChatContext';

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
    render(() => (
      <ChatProvider>
        <TestConsumer />
      </ChatProvider>
    ));

    const sendButton = screen.getByText('Send');
    sendButton.click();

    // Should immediately add user message
    await waitFor(() => {
      expect(screen.getByTestId('count').textContent).toBe('2'); // user + assistant placeholder
    });

    // First message should be user's
    const items = screen.getAllByRole('listitem');
    expect(items[0].getAttribute('data-role')).toBe('user');
    expect(items[0].textContent).toBe('test message');
  });

  it('streams assistant response', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });

    render(() => (
      <ChatProvider>
        <TestConsumer />
      </ChatProvider>
    ));

    screen.getByText('Send').click();

    // Wait for streaming to complete
    await vi.runAllTimersAsync();

    const items = screen.getAllByRole('listitem');
    expect(items[1].getAttribute('data-role')).toBe('assistant');
    expect(items[1].textContent).toContain('test response');

    vi.useRealTimers();
  });

  it('shows loading state while sending', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });

    render(() => (
      <ChatProvider>
        <TestConsumer />
      </ChatProvider>
    ));

    screen.getByText('Send').click();

    // Should be loading immediately after click
    expect(screen.getByTestId('loading').textContent).toBe('loading');

    // After all timers, should be idle
    await vi.runAllTimersAsync();
    expect(screen.getByTestId('loading').textContent).toBe('idle');

    vi.useRealTimers();
  });
});
