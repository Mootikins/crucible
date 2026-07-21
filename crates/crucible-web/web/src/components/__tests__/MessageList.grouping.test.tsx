import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup } from '@solidjs/testing-library';
import { MessageList } from '../MessageList';
import type { Message } from '@/lib/types';

let mockMessages: Message[] = [];
vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    messages: () => mockMessages,
    isStreaming: () => false,
    pendingInteraction: () => null,
    respondToInteraction: async () => {},
  }),
}));
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => null,
    sessions: () => [],
  }),
}));

const msg = (id: string, role: Message['role'], content = ''): Message => ({
  id,
  role,
  content,
  timestamp: 0,
  ...(role === 'tool'
    ? { toolCall: { id, callId: id, name: `tool-${id}`, args: '', status: 'complete' as const } }
    : {}),
});

afterEach(() => {
  cleanup();
  mockMessages = [];
});

describe('MessageList tool grouping', () => {
  it('collapses consecutive tool calls into one block', () => {
    mockMessages = [
      msg('u1', 'user', 'hi'),
      msg('t1', 'tool'),
      msg('t2', 'tool'),
      msg('t3', 'tool'),
      msg('a1', 'assistant', 'answer'),
    ];
    const { getAllByTestId } = render(() => <MessageList />);
    const groups = getAllByTestId('tool-group');
    expect(groups).toHaveLength(1);
    // All three tools live inside the single block.
    expect(groups[0].textContent).toContain('tool-t1');
    expect(groups[0].textContent).toContain('tool-t2');
    expect(groups[0].textContent).toContain('tool-t3');
  });

  it('non-consecutive tool runs form separate blocks', () => {
    mockMessages = [
      msg('t1', 'tool'),
      msg('a1', 'assistant', 'thinking aloud'),
      msg('t2', 'tool'),
      msg('t3', 'tool'),
    ];
    const { getAllByTestId } = render(() => <MessageList />);
    const groups = getAllByTestId('tool-group');
    expect(groups).toHaveLength(2);
    expect(groups[0].textContent).toContain('tool-t1');
    expect(groups[0].textContent).not.toContain('tool-t2');
    expect(groups[1].textContent).toContain('tool-t2');
    expect(groups[1].textContent).toContain('tool-t3');
  });
});
