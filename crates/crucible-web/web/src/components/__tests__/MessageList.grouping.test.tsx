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
    sessionId: () => 's1',
    sendMessage: async () => {},
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

describe('MessageList structural rows', () => {
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

  it('groups tools-before-answer and the answer into one assistant turn', () => {
    // user → tools run → answer: everything the agent did for the prompt is
    // one turn block, with the tool group ordered before the answer text.
    mockMessages = [
      msg('u1', 'user', 'hi'),
      msg('t1', 'tool'),
      msg('a1', 'assistant', 'answer'),
    ];
    const { getAllByTestId, getByTestId } = render(() => <MessageList />);
    const turn = getByTestId('assistant-turn');
    // One turn contains both the tool group and the answer segment.
    expect(getAllByTestId('tool-group')).toHaveLength(1);
    expect(turn.querySelector('[data-testid="tool-group"]')).not.toBeNull();
    expect(turn.querySelector('[data-testid="message-assistant"]')).not.toBeNull();
    // The tool group is rendered before the answer text within the turn.
    const toolGroup = turn.querySelector('[data-testid="tool-group"]') as HTMLElement;
    const answer = turn.querySelector('[data-testid="message-assistant"]') as HTMLElement;
    expect(toolGroup.compareDocumentPosition(answer) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('renders a text→tools→text response as ONE assistant-turn container', () => {
    mockMessages = [
      msg('u1', 'user', 'hi'),
      msg('a1', 'assistant', 'one'),
      msg('t1', 'tool'),
      msg('a2', 'assistant', 'two'),
    ];
    const { getAllByTestId } = render(() => <MessageList />);
    // Interleaved segments collapse into a single turn, not three top-level rows.
    expect(getAllByTestId('assistant-turn')).toHaveLength(1);
    // Both text segments and the tool run live inside it.
    expect(getAllByTestId('message-assistant')).toHaveLength(2);
    expect(getAllByTestId('tool-group')).toHaveLength(1);
  });

  it('appending a streamed token recreates NO DOM nodes anywhere', () => {
    // Mirrors the store's behavior: appending a token replaces ONLY the
    // streaming assistant object; user + tool references are unchanged. With
    // the structural row model a token append changes no id/role sequence, so
    // the row signature is identical and every wrapper is reused — zero
    // remounts. Capture element references before/after and require identity.
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
    const turnBefore = getByTestId('assistant-turn');

    // Token append: new assistant object, same user + tool references.
    setMsgs([msgs()[0], toolMsg, { ...assistant, content: 'Hello' }]);

    // Unchanged rows keep the exact same DOM nodes (nothing recreated).
    expect(getByTestId('tool-group')).toBe(groupBefore);
    expect(getByTestId('message-user')).toBe(userBefore);
    expect(getByTestId('assistant-turn')).toBe(turnBefore);
  });
});
