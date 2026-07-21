import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { MessageList } from '../MessageList';
import type { Message } from '@/lib/types';

let mockMessages: Message[] = [];
// Indirection so a test can swap in a reactive accessor (a signal) to drive
// re-renders; static tests keep reading the plain module-level array.
let messagesAccessor: () => Message[] = () => mockMessages;
vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    messages: () => messagesAccessor(),
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
  messagesAccessor = () => mockMessages;
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

  it('keeps stable rows mounted when the streaming assistant message appends tokens', () => {
    // Mirrors the store's behavior: appending a token replaces ONLY the
    // streaming assistant object; the tool message keeps its reference. Rows
    // whose underlying message is unchanged must not be disposed/recreated
    // (that reset ToolCard's expanded state and re-rendered all markdown).
    const toolMsg = msg('t1', 'tool');
    const assistant = msg('a1', 'assistant', 'Hel');
    const [msgs, setMsgs] = createSignal<Message[]>([
      msg('u1', 'user', 'hi'),
      toolMsg,
      assistant,
    ]);
    messagesAccessor = () => msgs();

    const { getByTestId } = render(() => <MessageList />);
    const groupBefore = getByTestId('tool-group');
    const userBefore = getByTestId('message-user');

    // Token append: new assistant object, same user + tool references.
    setMsgs([msgs()[0], toolMsg, { ...assistant, content: 'Hello' }]);

    // The unchanged rows keep the exact same DOM nodes (row not recreated).
    expect(getByTestId('tool-group')).toBe(groupBefore);
    expect(getByTestId('message-user')).toBe(userBefore);
  });
});
