import { describe, it, expect, vi, beforeEach } from 'vitest';
import fs from 'fs';
import path from 'path';
import fc from 'fast-check';
import type {
  ChatEvent,
  Message,
  ToolCallDisplay,
  SubagentEvent,
  ContextUsage,
  ChatMode,
  InteractionRequest,
} from '@/lib/types';

vi.mock('@/stores/statusBarStore', () => ({
  statusBarActions: {
    setContextUsage: vi.fn(),
    setChatMode: vi.fn(),
  },
}));

vi.mock('@/lib/api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api')>();
  // Unique per call: a segmented turn (text → tool → text → tool → text)
  // materializes a fresh streaming assistant message per segment, and a fixed
  // id would collide so updateMessage would target the wrong bubble.
  let counter = 0;
  return {
    ...actual,
    generateMessageId: () => `gen-msg-id-${counter++}`,
  };
});

// Import AFTER the mocks so the reducer picks them up.
import { createChatEventReducer } from './chatEventReducer';
import { statusBarActions } from '@/stores/statusBarStore';
import { SSE_EVENT_TYPES } from '@/lib/api';

const mockedStatusBar = statusBarActions as unknown as {
  setContextUsage: ReturnType<typeof vi.fn>;
  setChatMode: ReturnType<typeof vi.fn>;
};

// ============================================================================
// Test harness: builds a deps record whose getters reflect mutable state.
// ============================================================================

interface ReducerHarness {
  reducer: (event: ChatEvent) => void;
  /** Tool transcript entries (role "tool"), in transcript order. */
  tools: () => ToolCallDisplay[];
  state: {
    messages: Message[];
    currentStreamingMessageId: string | null;
    subagentEvents: SubagentEvent[];
    contextUsage: ContextUsage | null;
    chatMode: ChatMode;
    pendingInteraction: InteractionRequest | null;
    error: string | null;
    isLoading: boolean;
    isStreaming: boolean;
  };
  spies: {
    onTitleChanged: ReturnType<typeof vi.fn>;
    addMessage: ReturnType<typeof vi.fn>;
    updateMessage: ReturnType<typeof vi.fn>;
    appendToMessage: ReturnType<typeof vi.fn>;
  };
  /** Mutate state for setup (e.g. install a streaming message before token). */
  setUp: {
    streamingMessage: (id: string) => void;
  };
}

function createHarness(): ReducerHarness {
  const state: ReducerHarness['state'] = {
    messages: [],
    currentStreamingMessageId: null,
    subagentEvents: [],
    contextUsage: null,
    chatMode: 'normal',
    pendingInteraction: null,
    error: null,
    isLoading: false,
    isStreaming: false,
  };
  const spies = {
    onTitleChanged: vi.fn(),
    addMessage: vi.fn((message: Message) => {
      state.messages.push(message);
    }),
    updateMessage: vi.fn((id: string, updates: Partial<Message>) => {
      const idx = state.messages.findIndex((m) => m.id === id);
      if (idx >= 0) state.messages[idx] = { ...state.messages[idx], ...updates };
    }),
    appendToMessage: vi.fn((id: string, content: string) => {
      const idx = state.messages.findIndex((m) => m.id === id);
      if (idx >= 0) {
        state.messages[idx] = {
          ...state.messages[idx],
          content: state.messages[idx].content + content,
        };
      }
    }),
  };

  const reducer = createChatEventReducer({
    messages: () => state.messages,
    currentStreamingMessageId: () => state.currentStreamingMessageId,
    setCurrentStreamingMessageId: (id) => {
      state.currentStreamingMessageId = id;
    },
    onTitleChanged: spies.onTitleChanged,
    addMessage: spies.addMessage,
    updateMessage: spies.updateMessage,
    appendToMessage: spies.appendToMessage,
    // Mirrors ChatContext.addToolMessage: insert before the still-empty
    // streaming assistant placeholder, else append.
    addToolMessage: (tool) => {
      const toolMessage: Message = {
        id: `tool-${tool.callId ?? tool.id}`,
        role: 'tool',
        content: '',
        timestamp: 0,
        toolCall: tool,
      };
      const streamingId = state.currentStreamingMessageId;
      const idx = streamingId ? state.messages.findIndex((m) => m.id === streamingId) : -1;
      if (idx !== -1 && state.messages[idx].content === '') {
        state.messages.splice(idx, 0, toolMessage);
      } else {
        state.messages.push(toolMessage);
      }
    },
    updateToolMessage: (callId, updater) => {
      for (const m of state.messages) {
        const tool = m.toolCall;
        if (m.role === 'tool' && tool && tool.callId === callId) {
          m.toolCall = updater(tool);
        }
      }
    },
    setSubagentEvents: (value) => {
      state.subagentEvents = typeof value === 'function'
        ? value([...state.subagentEvents])
        : value;
    },
    setContextUsage: (usage) => {
      state.contextUsage = usage;
    },
    setChatMode: (mode) => {
      state.chatMode = mode;
    },
    setPendingInteraction: (req) => {
      state.pendingInteraction = req;
    },
    setError: (value) => {
      state.error = value;
    },
    setIsLoading: (value) => {
      state.isLoading = value;
    },
    setIsStreaming: (value) => {
      state.isStreaming = value;
    },
  });

  return {
    reducer,
    tools: () => state.messages
      .filter((m) => m.role === 'tool' && m.toolCall)
      .map((m) => m.toolCall!),
    state,
    spies,
    setUp: {
      streamingMessage: (id: string) => {
        state.currentStreamingMessageId = id;
        if (!state.messages.find((m) => m.id === id)) {
          state.messages.push({
            id,
            role: 'assistant',
            content: '',
            timestamp: 0,
          });
        }
      },
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

// ============================================================================
// Event-shape coverage matrix
// One example-based test per ChatEvent variant. New variants (e.g. terminate
// flag, plugin events) MUST add a row here so the matrix stays exhaustive.
// ============================================================================

describe('event matrix — covers every ChatEvent variant', () => {
  // ---- Parameterized clusters (homogeneous shape across many variants) ----

  // tool_call / tool_call_start: dispatch one tool-call event, assert the
  // ToolCallDisplay shape that lands in the transcript.
  it.each([
    {
      name: 'tool_call: adds a running ToolCallDisplay with title and arguments',
      event: { type: 'tool_call', id: 'tc-1', title: 'list_files', arguments: { path: '/tmp' } },
      expected: {
        id: 'tc-1', name: 'list_files',
        args: JSON.stringify({ path: '/tmp' }), status: 'running', callId: 'tc-1',
      },
    },
    {
      name: 'tool_call_start: same as tool_call but uses `name`',
      event: { type: 'tool_call_start', id: 'tc-2', name: 'bash', arguments: { cmd: 'ls' } },
      expected: { name: 'bash', status: 'running' },
    },
    {
      name: 'tool_call: handles missing arguments gracefully',
      event: { type: 'tool_call', id: 'tc-3', title: 'noop' },
      expected: { args: '' },
    },
    {
      name: 'tool_call: arguments present but undefined stringifies to ""',
      event: { type: 'tool_call', id: 'tc-4', title: 'noop', arguments: undefined },
      expected: { args: '""' },
    },
  ])('$name', ({ event, expected }) => {
    const h = createHarness();
    h.reducer(event as ChatEvent);
    expect(h.tools()[0]).toMatchObject(expected);
  });

  // tool_result* lifecycle: each row sets up a tool_call then dispatches the
  // listed result events and asserts the resulting ToolCallDisplay state.
  it.each([
    {
      name: 'tool_result: marks tool complete and stores result',
      events: [{ type: 'tool_result', id: 'tc-1', result: 'done' }],
      expected: { result: 'done', status: 'complete' },
    },
    {
      name: 'tool_result: defaults empty string when result is missing',
      events: [{ type: 'tool_result', id: 'tc-1' }],
      expected: { result: '', status: 'complete' },
    },
    {
      name: 'tool_result: stores terminate=true when the daemon signaled early-stop',
      events: [{ type: 'tool_result', id: 'tc-1', result: 'final', terminate: true }],
      expected: { result: 'final', status: 'complete', terminate: true },
    },
    {
      name: 'tool_result: terminate defaults to false when omitted (backward compat)',
      events: [{ type: 'tool_result', id: 'tc-1', result: 'done' }],
      expected: { terminate: false },
    },
    {
      name: 'tool_result_delta: appends to existing result',
      events: [
        { type: 'tool_result_delta', id: 'tc-1', delta: 'partial-' },
        { type: 'tool_result_delta', id: 'tc-1', delta: 'output' },
      ],
      expected: { result: 'partial-output' },
    },
    {
      name: 'tool_result_delta: tools without a result accumulate from empty',
      events: [{ type: 'tool_result_delta', id: 'tc-1', delta: 'x' }],
      expected: { result: 'x' },
    },
    {
      name: 'tool_result_complete: marks tool complete without changing result',
      events: [
        { type: 'tool_result_delta', id: 'tc-1', delta: 'stream' },
        { type: 'tool_result_complete', id: 'tc-1' },
      ],
      expected: { result: 'stream', status: 'complete' },
    },
    {
      name: 'tool_result_error: marks tool error and stores message',
      events: [{ type: 'tool_result_error', id: 'tc-1', error: 'boom' }],
      expected: { result: 'boom', status: 'error' },
    },
  ])('$name', ({ events, expected }) => {
    const h = createHarness();
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'noop' });
    for (const e of events) h.reducer(e as ChatEvent);
    expect(h.tools()[0]).toMatchObject(expected);
  });

  // subagent_* / delegation_*: every variant mutates h.state.subagentEvents
  // via the same upsert path. Each row lists the events to dispatch and the
  // expected final array (strict equality preserves the array-length check).
  it.each([
    {
      name: 'subagent_spawned: adds spawned event',
      events: [{ type: 'subagent_spawned', id: 'sa-1', prompt: 'go' }],
      expected: [{ id: 'sa-1', prompt: 'go', status: 'spawned' }],
    },
    {
      name: 'subagent_completed: upserts into existing spawned event',
      events: [
        { type: 'subagent_spawned', id: 'sa-1', prompt: 'go' },
        { type: 'subagent_completed', id: 'sa-1', summary: 'done' },
      ],
      expected: [{ id: 'sa-1', prompt: 'go', status: 'completed', summary: 'done' }],
    },
    {
      name: 'subagent_completed: creates a new entry when no matching spawn',
      events: [{ type: 'subagent_completed', id: 'sa-orphan', summary: 'done' }],
      expected: [{ id: 'sa-orphan', prompt: '', status: 'completed', summary: 'done' }],
    },
    {
      name: 'subagent_failed: upserts with error',
      events: [
        { type: 'subagent_spawned', id: 'sa-1', prompt: 'go' },
        { type: 'subagent_failed', id: 'sa-1', error: 'oom' },
      ],
      expected: [{ id: 'sa-1', prompt: 'go', status: 'failed', error: 'oom' }],
    },
    {
      name: 'subagent_failed: creates new entry when no matching spawn',
      events: [{ type: 'subagent_failed', id: 'sa-orphan', error: 'oom' }],
      expected: [{ id: 'sa-orphan', prompt: '', status: 'failed', error: 'oom' }],
    },
    {
      name: 'delegation_spawned: adds spawned event with targetAgent',
      events: [{ type: 'delegation_spawned', id: 'd-1', prompt: 'analyze', target_agent: 'claude' }],
      expected: [{ id: 'd-1', prompt: 'analyze', status: 'spawned', targetAgent: 'claude' }],
    },
    {
      name: 'delegation_completed: upserts summary',
      events: [
        { type: 'delegation_spawned', id: 'd-1', prompt: 'analyze' },
        { type: 'delegation_completed', id: 'd-1', summary: 'finished' },
      ],
      expected: [{ id: 'd-1', prompt: 'analyze', status: 'completed', summary: 'finished' }],
    },
    {
      name: 'delegation_failed: upserts error',
      events: [
        { type: 'delegation_spawned', id: 'd-1', prompt: 'x' },
        { type: 'delegation_failed', id: 'd-1', error: 'agent unreachable' },
      ],
      expected: [{ id: 'd-1', prompt: 'x', status: 'failed', error: 'agent unreachable' }],
    },
  ])('$name', ({ events, expected }) => {
    const h = createHarness();
    for (const e of events) h.reducer(e as ChatEvent);
    expect(h.state.subagentEvents).toEqual(expected);
  });

  // ---- Individual tests (heterogeneous setup/assertion shapes) ----

  it('token: appends content to current streaming message', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'Hello ' });
    h.reducer({ type: 'token', content: 'world' });
    expect(h.state.messages[0].content).toBe('Hello world');
    expect(h.spies.appendToMessage).toHaveBeenCalledTimes(2);
  });

  it('token: materializes a streaming assistant message when none is active (mid-turn attach)', () => {
    const h = createHarness();
    h.reducer({ type: 'token', content: 'late ' });
    h.reducer({ type: 'token', content: 'viewer' });
    expect(h.state.messages).toHaveLength(1);
    expect(h.state.messages[0]).toMatchObject({ role: 'assistant', content: 'late viewer' });
    expect(h.state.currentStreamingMessageId).toBe(h.state.messages[0].id);
  });

  it('tool_call: inserts before the empty streaming assistant placeholder so transcript order is user → tools → answer', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'search' });
    expect(h.state.messages.map((m) => m.role)).toEqual(['tool', 'assistant']);
  });

  it('tool_call: appends after the assistant message once it has content', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'answer so far' });
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'search' });
    expect(h.state.messages.map((m) => m.role)).toEqual(['assistant', 'tool']);
  });

  it('text → tool → text yields separate assistant segments; narration renders exactly once', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'Let me look that up.' });
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'semantic_search' });
    h.reducer({ type: 'tool_result', id: 'tc-1', result: 'notes...' });
    h.reducer({ type: 'token', content: 'Here is what I found.' });
    // The real daemon accumulates the WHOLE turn's text and sends it in
    // message_complete — pre-tool narration INCLUDED — not just the post-tool
    // tail. The reducer must strip the already-frozen prefix so the final
    // bubble holds only the trailing segment.
    h.reducer({
      type: 'message_complete',
      id: 'srv',
      content: 'Let me look that up.Here is what I found.',
    });

    expect(h.state.messages.map((m) => m.role)).toEqual(['assistant', 'tool', 'assistant']);
    expect(h.state.messages[0].content).toBe('Let me look that up.');
    // Final bubble is ONLY the trailing segment, not the full accumulated text.
    expect(h.state.messages[2]).toMatchObject({
      id: 'srv-response',
      content: 'Here is what I found.',
    });
    // The narration appears exactly once across the whole transcript.
    const transcript = h.state.messages.map((m) => m.content).join('');
    expect(transcript.split('Let me look that up.').length - 1).toBe(1);
  });

  it('session_event user_message: adopts an optimistic temp entry instead of duplicating it', () => {
    // The echo can win the race against the send POST's canonicalization —
    // e.g. when a remounted provider rendered the pending first message
    // optimistically but a sibling dispatcher sent it. Same content + a
    // client-minted `msg_` id ⇒ rename, never a second user bubble.
    const h = createHarness();
    h.state.messages.push({ id: 'msg_123_temp', role: 'user', content: 'hello there', timestamp: 1 });
    h.reducer({
      type: 'session_event',
      event_type: 'user_message',
      data: { message_id: 'msg-canonical-1', content: 'hello there' },
    });
    const users = h.state.messages.filter((m) => m.role === 'user');
    expect(users).toHaveLength(1);
    expect(users[0].id).toBe('msg-canonical-1');
  });

  it('message_complete finalizes thinking left streaming on a frozen segment', () => {
    // Thinking streamed before a tool boundary lives on the frozen segment,
    // which the messageId-targeted finalization in message_complete never
    // touches (the streaming id was cleared at the boundary). Regression: the
    // segment rendered "Thinking…" forever instead of "Thought for N tokens".
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'Here is the answer.' });
    h.reducer({ type: 'thinking', content: 'Considering the options.' });
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'read_file' });
    h.reducer({ type: 'tool_result', id: 'tc-1', result: 'contents' });
    // Whole-turn text == the frozen segment: no trailing bubble is added.
    h.reducer({ type: 'message_complete', id: 'srv', content: 'Here is the answer.' });

    const frozen = h.state.messages.find((m) => m.role === 'assistant' && m.thinking);
    expect(frozen?.thinking).toMatchObject({
      isStreaming: false,
      tokenCount: 'Considering the options.'.length,
    });
  });

  it('text → tool → text → tool → text strips every frozen prefix (each narration once)', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'First. ' });
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'search' });
    h.reducer({ type: 'tool_result', id: 'tc-1', result: 'r1' });
    h.reducer({ type: 'token', content: 'Second. ' });
    h.reducer({ type: 'tool_call', id: 'tc-2', title: 'search' });
    h.reducer({ type: 'tool_result', id: 'tc-2', result: 'r2' });
    h.reducer({ type: 'token', content: 'Third.' });
    h.reducer({
      type: 'message_complete',
      id: 'srv',
      content: 'First. Second. Third.',
    });

    expect(h.state.messages.map((m) => m.role)).toEqual([
      'assistant', 'tool', 'assistant', 'tool', 'assistant',
    ]);
    expect(h.state.messages[0].content).toBe('First. ');
    expect(h.state.messages[2].content).toBe('Second. ');
    expect(h.state.messages[4]).toMatchObject({ id: 'srv-response', content: 'Third.' });
    // Full concatenation equals the daemon payload — no text lost, none doubled.
    expect(h.state.messages.map((m) => m.content).join('')).toBe('First. Second. Third.');
  });

  it('segment_complete: freezes the open streaming message and renames it to the canonical id', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'Let me look that up.' });
    h.reducer({ type: 'segment_complete', message_id: 'msg-turn-1', index: 0, content: 'Let me look that up.' });
    expect(h.state.messages).toHaveLength(1);
    expect(h.state.messages[0]).toMatchObject({
      id: 'msg-turn-1-seg-0',
      role: 'assistant',
      content: 'Let me look that up.',
    });
    expect(h.state.currentStreamingMessageId).toBeNull();
  });

  it('segment_complete: new-daemon text→tool→text converges on canonical ids without double-freezing', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'Let me look that up.' });
    // The daemon emits segment_complete BEFORE the tool_call.
    h.reducer({ type: 'segment_complete', message_id: 'msg-turn-1', index: 0, content: 'Let me look that up.' });
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'semantic_search' });
    h.reducer({ type: 'tool_result', id: 'tc-1', result: 'notes...' });
    h.reducer({ type: 'token', content: 'Here is what I found.' });
    h.reducer({
      type: 'message_complete',
      id: 'msg-turn-1',
      content: 'Let me look that up.Here is what I found.',
    });

    expect(h.state.messages.map((m) => m.role)).toEqual(['assistant', 'tool', 'assistant']);
    expect(h.state.messages[0]).toMatchObject({ id: 'msg-turn-1-seg-0', content: 'Let me look that up.' });
    expect(h.state.messages[2]).toMatchObject({ id: 'msg-turn-1-response', content: 'Here is what I found.' });
    // The tool_call fallback must NOT have frozen the segment a second time.
    const transcript = h.state.messages.map((m) => m.content).join('');
    expect(transcript.split('Let me look that up.').length - 1).toBe(1);
  });

  it('segment_complete: late attach with no streaming message adds the segment bubble under the canonical id', () => {
    const h = createHarness();
    h.reducer({ type: 'segment_complete', message_id: 'msg-turn-9', index: 1, content: 'earlier narration' });
    expect(h.state.messages).toHaveLength(1);
    expect(h.state.messages[0]).toMatchObject({
      id: 'msg-turn-9-seg-1',
      role: 'assistant',
      content: 'earlier narration',
    });
  });

  it('segment_complete: a replayed signal does not duplicate an existing segment bubble', () => {
    const h = createHarness();
    h.reducer({ type: 'segment_complete', message_id: 'msg-turn-9', index: 0, content: 'seg' });
    h.reducer({ type: 'segment_complete', message_id: 'msg-turn-9', index: 0, content: 'seg' });
    expect(h.state.messages).toHaveLength(1);
  });

  it('segment_complete: adopts the canonical id on a segment the tool_call fallback already froze', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'narration' });
    // Fallback path (unusual ordering): tool_call freezes the text first, under
    // the random streaming id.
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'search' });
    // segment_complete for the same text: adopt the canonical id, do NOT add a
    // second bubble or double-count the frozen prefix.
    h.reducer({ type: 'segment_complete', message_id: 'msg-turn-1', index: 0, content: 'narration' });
    const assistants = () => h.state.messages.filter((m) => m.role === 'assistant');
    expect(assistants()).toHaveLength(1);
    expect(assistants()[0].id).toBe('msg-turn-1-seg-0');
    h.reducer({ type: 'token', content: 'tail' });
    h.reducer({ type: 'message_complete', id: 'msg-turn-1', content: 'narrationtail' });
    // Two bubbles, each rendered once: the frozen segment and the trailing text.
    expect(assistants().map((m) => m.content).join('|')).toBe('narration|tail');
  });

  it('message_complete: uses event.content verbatim when it does not start with the frozen prefix', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'Frozen narration.' });
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'search' });
    h.reducer({ type: 'tool_result', id: 'tc-1', result: 'r' });
    h.reducer({ type: 'token', content: 'tail' });
    // Payload that does NOT begin with the frozen segment (shape drift): the
    // reducer must not crash or mangle it — it keeps event.content as-is.
    h.reducer({ type: 'message_complete', id: 'srv', content: 'completely different' });
    expect(h.state.messages[2]).toMatchObject({
      id: 'srv-response',
      content: 'completely different',
    });
  });

  // message_complete dangling-tool finalization: a still-running tool at turn
  // end is finalized based on whether it has any partial result.
  it.each([
    {
      name: 'message_complete finalizes a still-running tool with no result as an error',
      preEvents: [],
      expected: { status: 'error', result: 'tool did not complete' },
    },
    {
      name: 'message_complete finalizes a still-running tool that has a partial result as complete',
      preEvents: [{ type: 'tool_result_delta', id: 'tc-1', delta: 'partial output' }],
      expected: { status: 'complete', result: 'partial output' },
    },
  ])('$name', ({ preEvents, expected }) => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'search' });
    for (const e of preEvents) h.reducer(e as ChatEvent);
    // No tool_result_complete arrives; the turn ends with status still running.
    h.reducer({ type: 'message_complete', id: 'srv', content: 'done' });
    expect(h.tools()[0]).toMatchObject(expected);
  });

  it('tool entries persist in the transcript after message_complete', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'tool_call', id: 'tc-1', title: 'search' });
    h.reducer({ type: 'tool_result', id: 'tc-1', result: 'found it' });
    h.reducer({ type: 'message_complete', id: 'srv', content: 'done' });
    expect(h.tools()).toHaveLength(1);
    expect(h.tools()[0]).toMatchObject({ status: 'complete', result: 'found it' });
  });

  it('thinking: appends to thinking block when streaming', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'thinking', content: 'first chunk ' });
    h.reducer({ type: 'thinking', content: 'second chunk' });
    expect(h.state.messages[0].thinking).toEqual({
      content: 'first chunk second chunk',
      isStreaming: true,
    });
  });

  it('thinking: materializes a streaming assistant message when none is active', () => {
    const h = createHarness();
    h.reducer({ type: 'thinking', content: 'pondering' });
    expect(h.state.messages).toHaveLength(1);
    expect(h.state.messages[0].thinking).toEqual({ content: 'pondering', isStreaming: true });
  });

  it('message_complete: finalizes the streaming message and clears streaming state', () => {
    const h = createHarness();
    h.setUp.streamingMessage('msg-stream');
    h.reducer({ type: 'thinking', content: 'reasoning' });

    h.reducer({
      type: 'message_complete',
      id: 'msg-server-1',
      content: 'final',
      prompt_tokens: 100,
      completion_tokens: 50,
      total_tokens: 150,
      cache_read_tokens: 10,
      cache_creation_tokens: 20,
    });

    expect(h.state.messages[0]).toMatchObject({
      id: 'msg-server-1-response',
      content: 'final',
      usage: {
        promptTokens: 100,
        completionTokens: 50,
        totalTokens: 150,
        cacheReadTokens: 10,
        cacheCreationTokens: 20,
      },
      thinking: { content: 'reasoning', isStreaming: false, tokenCount: 9 },
    });
    expect(h.state.isStreaming).toBe(false);
    expect(h.state.isLoading).toBe(false);
    expect(h.tools()).toEqual([]);
    expect(h.state.currentStreamingMessageId).toBeNull();
  });

  it('message_complete: omits usage when total_tokens is missing/zero', () => {
    const h = createHarness();
    h.setUp.streamingMessage('msg-stream');
    h.reducer({
      type: 'message_complete',
      id: 'msg-server-1',
      content: 'final',
    });
    expect(h.state.messages[0].usage).toBeUndefined();
  });

  it('message_complete: with no streaming message, appends the completed turn (late attach)', () => {
    const h = createHarness();
    h.reducer({
      type: 'message_complete',
      id: 'srv',
      content: 'full text',
    });
    expect(h.state.messages).toHaveLength(1);
    expect(h.state.messages[0]).toMatchObject({
      id: 'srv-response',
      role: 'assistant',
      content: 'full text',
    });
    // Replayed completion (reconnect) must not duplicate.
    h.reducer({ type: 'message_complete', id: 'srv', content: 'full text' });
    expect(h.state.messages).toHaveLength(1);
    expect(h.state.isStreaming).toBe(false);
  });

  it('session_event user_message: adds the prompt for mid-turn attachers, exact-id dedupe', () => {
    const h = createHarness();
    const evt = {
      type: 'session_event',
      event_type: 'user_message',
      data: { message_id: 'msg-turn-9', content: 'the prompt' },
    } as ChatEvent;
    h.reducer(evt);
    expect(h.state.messages).toHaveLength(1);
    expect(h.state.messages[0]).toMatchObject({ id: 'msg-turn-9', role: 'user' });
    // Echo received by the sender (id already present) is a no-op.
    h.reducer(evt);
    expect(h.state.messages).toHaveLength(1);
  });

  it('error: sets error string and updates streaming message content', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({
      type: 'error',
      code: 'rate_limit',
      message: 'Slow down',
    });
    expect(h.state.error).toBe('Slow down (rate_limit)');
    expect(h.state.messages[0].content).toBe('Error: Slow down');
    expect(h.state.isStreaming).toBe(false);
    expect(h.state.isLoading).toBe(false);
    expect(h.state.currentStreamingMessageId).toBeNull();
  });

  it('error: works without an active streaming message', () => {
    const h = createHarness();
    h.reducer({ type: 'error', code: 'x', message: 'y' });
    expect(h.state.error).toBe('y (x)');
  });

  it('connection: reconnect does NOT corrupt the in-flight streaming turn', () => {
    const h = createHarness();
    h.setUp.streamingMessage('asst-1');
    h.reducer({ type: 'token', content: 'partial answer' });

    // A transport reconnect mid-stream must not touch the message or its id.
    h.reducer({ type: 'connection', status: 'reconnecting', message: 'Reconnecting…' });
    expect(h.state.messages[0].content).toBe('partial answer');
    expect(h.state.currentStreamingMessageId).toBe('asst-1');
    expect(h.state.error).toBe('Reconnecting…');

    // Reconnecting clears the transient banner without disturbing the stream.
    h.reducer({ type: 'connection', status: 'connected' });
    expect(h.state.error).toBeNull();
    expect(h.state.currentStreamingMessageId).toBe('asst-1');
    expect(h.state.messages[0].content).toBe('partial answer');
  });

  it('interaction_requested: stores request stripped of type discriminator', () => {
    const h = createHarness();
    h.reducer({
      type: 'interaction_requested',
      id: 'req-1',
      kind: 'ask',
      question: 'Continue?',
    } as ChatEvent);
    expect(h.state.pendingInteraction).toEqual({
      id: 'req-1',
      kind: 'ask',
      question: 'Continue?',
    });
  });

  it('context_usage: updates local state AND statusBar', () => {
    const h = createHarness();
    h.reducer({ type: 'context_usage', used: 1234, total: 8000 });
    expect(h.state.contextUsage).toEqual({ used: 1234, total: 8000 });
    expect(mockedStatusBar.setContextUsage).toHaveBeenCalledWith({
      used: 1234,
      total: 8000,
    });
  });

  it('precognition_result: attaches metadata to the most recent user message', () => {
    const h = createHarness();
    h.state.messages.push({
      id: 'user-1',
      role: 'user',
      content: 'tell me about widgets',
      timestamp: 0,
    });
    h.reducer({
      type: 'precognition_result',
      notes_count: 2,
      notes: [
        { name: 'Note A', relevance: 0.9 },
        { name: 'Note B', relevance: 0.7 },
      ],
    });
    // No synthetic system message — metadata lives on the user message.
    expect(h.state.messages).toHaveLength(1);
    expect(h.state.messages[0].precognition).toEqual({
      notesCount: 2,
      notes: [
        { name: 'Note A', relevance: 0.9 },
        { name: 'Note B', relevance: 0.7 },
      ],
    });
  });

  it('precognition_result: no-op when there is no user message yet', () => {
    const h = createHarness();
    h.reducer({ type: 'precognition_result', notes_count: 0, notes: [] });
    expect(h.state.messages).toHaveLength(0);
  });

  it('mode_changed: updates local mode AND statusBar', () => {
    const h = createHarness();
    h.reducer({ type: 'mode_changed', mode: 'plan' });
    expect(h.state.chatMode).toBe('plan');
    expect(mockedStatusBar.setChatMode).toHaveBeenCalledWith('plan');
  });

  it('title_changed: forwards the daemon-generated title', () => {
    const h = createHarness();
    h.reducer({ type: 'title_changed', title: 'Merkle tree sync design' });
    expect(h.spies.onTitleChanged).toHaveBeenCalledWith('Merkle tree sync design');
  });

  it('session_event: no-op (acknowledged but not surfaced)', () => {
    const h = createHarness();
    expect(() =>
      h.reducer({
        type: 'session_event',
        event_type: 'state_changed',
        data: { state: 'paused' },
      }),
    ).not.toThrow();
    expect(h.state.messages).toHaveLength(0);
  });
});

// ============================================================================
// Property tests — invariants over random event sequences
// ============================================================================

// Compact arbitrary builder: every ChatEvent variant is `fc.constant(type)`
// plus a handful of fields — the helper removes the per-variant boilerplate.
const evt = <T extends string>(type: T, fields: Record<string, fc.Arbitrary<unknown>> = {}) =>
  fc.record({ type: fc.constant(type), ...fields });

const strId = fc.string({ minLength: 1, maxLength: 10 });

// Hoisted interaction arbitraries — used both inside arbChatEvent and by the
// "interaction_requested has kind" property test below.
const interactionAsk = evt('interaction_requested', {
  id: strId,
  kind: fc.constant('ask' as const),
  question: fc.string({ maxLength: 100 }),
});
const interactionPopup = evt('interaction_requested', {
  id: strId,
  kind: fc.constant('popup' as const),
  title: fc.string({ maxLength: 50 }),
  entries: fc.array(fc.record({ label: fc.string({ maxLength: 20 }) }), { maxLength: 5 }),
});
const interactionPerm = evt('interaction_requested', {
  id: strId,
  kind: fc.constant('permission' as const),
  action_type: fc.constantFrom('bash' as const, 'read' as const, 'write' as const, 'tool' as const),
  tokens: fc.array(fc.string({ maxLength: 20 }), { maxLength: 5 }),
});

// Generator for any ChatEvent. Kept small but covers every variant so totality
// holds across the union.
const arbChatEvent = (): fc.Arbitrary<ChatEvent> => fc.oneof(
  evt('token', { content: fc.string() }),
  evt('tool_call', { id: strId, title: fc.string() }),
  evt('tool_call_start', { id: strId, name: fc.string() }),
  evt('tool_result', { id: strId, result: fc.string() }),
  evt('tool_result_delta', { id: strId, delta: fc.string() }),
  evt('tool_result_complete', { id: strId }),
  evt('tool_result_error', { id: strId, error: fc.string() }),
  evt('thinking', { content: fc.string() }),
  evt('message_complete', { id: strId, content: fc.string() }),
  evt('segment_complete', {
    message_id: strId,
    index: fc.nat({ max: 5 }),
    content: fc.string(),
  }),
  evt('error', { code: fc.string({ minLength: 1, maxLength: 20 }), message: fc.string() }),
  fc.oneof(interactionAsk, interactionPopup, interactionPerm),
  evt('subagent_spawned', { id: strId, prompt: fc.string() }),
  evt('subagent_completed', { id: strId, summary: fc.string() }),
  evt('subagent_failed', { id: strId, error: fc.string() }),
  evt('delegation_spawned', { id: strId, prompt: fc.string() }),
  evt('delegation_completed', { id: strId, summary: fc.string() }),
  evt('delegation_failed', { id: strId, error: fc.string() }),
  evt('context_usage', { used: fc.nat({ max: 1_000_000 }), total: fc.nat({ max: 1_000_000 }) }),
  evt('precognition_result', {
    notes_count: fc.nat({ max: 20 }),
    notes: fc.array(
      fc.record({ name: fc.string(), relevance: fc.float({ min: 0, max: 1 }) }),
      { maxLength: 10 },
    ),
  }),
  evt('mode_changed', { mode: fc.constantFrom('normal' as const, 'plan' as const, 'auto' as const) }),
  evt('session_event', { event_type: fc.string(), data: fc.anything() }),
) as fc.Arbitrary<ChatEvent>;

describe('property: totality', () => {
  it('any sequence of events runs to completion without throwing (pre-seeded state)', () => {
    fc.assert(
      fc.property(fc.array(arbChatEvent(), { maxLength: 50 }), (events) => {
        const h = createHarness();
        h.setUp.streamingMessage('asst-1');
        for (const event of events) {
          h.reducer(event);
        }
      }),
      { numRuns: 100 },
    );
  });

  it('any sequence of events runs to completion without throwing (clean state)', () => {
    // No setUp calls — this exercises the un-initialized state space where
    // currentStreamingMessageId is null. A reducer regression that forgets
    // to guard for missing streaming context (e.g. a future variant that
    // dereferences messages() without null-checking) would surface here.
    fc.assert(
      fc.property(fc.array(arbChatEvent(), { maxLength: 50 }), (events) => {
        const h = createHarness();
        for (const event of events) {
          h.reducer(event);
        }
      }),
      { numRuns: 100 },
    );
  });

  it('every interaction_requested produces a pendingInteraction with a kind field', () => {
    // Totality + well-formedness: if the reducer ever stops preserving `kind`,
    // downstream interaction components crash at runtime — this catches that
    // class of regression before it ships.
    fc.assert(
      fc.property(
        fc.oneof(interactionAsk, interactionPopup, interactionPerm),
        (event) => {
          const h = createHarness();
          h.reducer(event as ChatEvent);
          expect(h.state.pendingInteraction).not.toBeNull();
          expect(h.state.pendingInteraction!.kind).toBeDefined();
        },
      ),
      { numRuns: 50 },
    );
  });
});

describe('property: message_complete second call is a safe no-op', () => {
  // After the first message_complete, currentStreamingMessageId is null. The
  // reducer's second call enters the early-skip branch. This property pins
  // that "second call doesn't crash and doesn't mutate" — useful regression
  // gate, but NOT a test of updateMessage idempotency.
  it('applying message_complete twice never throws or mutates state', () => {
    fc.assert(
      fc.property(
        fc.string({ minLength: 1, maxLength: 50 }),
        fc.string({ minLength: 1, maxLength: 50 }),
        (msgId, content) => {
          const h = createHarness();
          h.setUp.streamingMessage('asst-1');
          const event: ChatEvent = {
            type: 'message_complete',
            id: msgId,
            content,
          };
          h.reducer(event);
          const firstSnapshot = JSON.stringify(h.state.messages);
          h.reducer(event);
          expect(JSON.stringify(h.state.messages)).toBe(firstSnapshot);
        },
      ),
      { numRuns: 50 },
    );
  });
});

// NOTE: a "true idempotency under re-seed" test was attempted here but the
// reducer's contract is "message_complete CONSUMES the streaming placeholder
// by mutating its id to the server-assigned id." Re-seeding a placeholder
// with the same local id between calls creates a second placeholder (since
// the first one no longer matches the local id), and the reducer correctly
// updates that second one — producing two messages. That's the right behavior,
// not a regression. The "second call is safe no-op" property above is the
// meaningful idempotency claim for this reducer.

describe('property: streaming order preserved', () => {
  it('token chunks accumulate in arrival order regardless of interleaved non-token events', () => {
    fc.assert(
      fc.property(
        fc.array(fc.string({ maxLength: 8 }), { minLength: 1, maxLength: 15 }),
        fc.array(arbChatEvent(), { maxLength: 10 }),
        (chunks, interleaved) => {
          const h = createHarness();
          h.setUp.streamingMessage('asst-1');
          // Filter out events that would corrupt the assertion: tokens add to
          // the stream (changing expected content); message_complete and error
          // clear currentStreamingMessageId so later tokens become no-ops.
          const safeInterleaved = interleaved.filter(
            (e) =>
              e.type !== 'message_complete' &&
              e.type !== 'error' &&
              e.type !== 'token',
          );
          // Interleave tokens with random other events.
          const allEvents: ChatEvent[] = [];
          for (let i = 0; i < chunks.length; i++) {
            allEvents.push({ type: 'token', content: chunks[i] });
            if (safeInterleaved[i]) allEvents.push(safeInterleaved[i]);
          }
          for (const event of allEvents) h.reducer(event);
          // Interleaved tool_call events legitimately SEGMENT the stream
          // (text → tool → text becomes separate assistant messages), so the
          // invariant is that the concatenation of all assistant segments
          // preserves token arrival order.
          const assistantContent = h.state.messages
            .filter((m) => m.role === 'assistant')
            .map((m) => m.content)
            .join('');
          expect(assistantContent).toBe(chunks.join(''));
        },
      ),
      { numRuns: 50 },
    );
  });
});

describe('property: tool lifecycle reaches terminal state', () => {
  it('tool_result and tool_result_error both terminate a tool', () => {
    fc.assert(
      fc.property(
        fc.string({ minLength: 1, maxLength: 10 }),
        fc.boolean(),
        (id, useError) => {
          const h = createHarness();
          h.reducer({ type: 'tool_call', id, title: 'noop' });
          if (useError) {
            h.reducer({ type: 'tool_result_error', id, error: 'boom' });
            expect(h.tools()[0].status).toBe('error');
          } else {
            h.reducer({ type: 'tool_result', id, result: 'ok' });
            expect(h.tools()[0].status).toBe('complete');
          }
        },
      ),
      { numRuns: 30 },
    );
  });
});

// ============================================================================
// Contract checks: the SSE subscription list (api.ts) must match the set the
// reducer actually handles. Drift between the two means events arrive on the
// wire but are silently dropped, or vice versa.
// ============================================================================

describe('contract: SSE subscription parity with reducer handlers', () => {
  // The set of variants the reducer's switch handles. Update this when adding
  // a new ChatEvent variant — the parity test below will catch any mismatch
  // with the SSE_EVENT_TYPES constant in api.ts.
  const REDUCER_HANDLED_TYPES = [
    'token',
    'tool_call',
    'tool_call_start',
    'tool_result',
    'tool_result_delta',
    'tool_result_complete',
    'tool_result_error',
    'thinking',
    'segment_complete',
    'message_complete',
    'error',
    'interaction_requested',
    'session_event',
    'subagent_spawned',
    'subagent_completed',
    'subagent_failed',
    'delegation_spawned',
    'delegation_completed',
    'delegation_failed',
    'context_usage',
    'precognition_result',
    'mode_changed',
    'title_changed',
  ] as const;

  it('SSE_EVENT_TYPES and reducer-handled types are identical', () => {
    // Independent comparison — if either drifts, the diff makes the missing
    // variant obvious.
    const sseSorted = [...SSE_EVENT_TYPES].sort();
    const reducerSorted = [...REDUCER_HANDLED_TYPES].sort();
    expect(sseSorted).toEqual(reducerSorted);
  });

  it('every reducer-handled type has a matrix test above', () => {
    // Each reducer-handled variant must appear as a quoted literal somewhere
    // in the matrix block — covering `it('variant: ...')` names, `describe`
    // blocks, and parameterized `it.each` tables where the variant is a
    // `type:` field or row value. A newly-handled variant added without any
    // matrix coverage FAILS here — the old version only matched `it('prefix:`,
    // which silently passed when an `it.each` row was the only mention.
    const source = fs.readFileSync(path.join(__dirname, 'chatEventReducer.test.ts'), 'utf-8');
    const matrixStart = source.indexOf("describe('event matrix");
    const matrixEnd = source.indexOf("describe('property: totality'");
    expect(matrixStart).toBeGreaterThanOrEqual(0);
    expect(matrixEnd).toBeGreaterThan(matrixStart);
    const matrixBlock = source.slice(matrixStart, matrixEnd);

    const missing = REDUCER_HANDLED_TYPES.filter((t) => !matrixBlock.includes(`'${t}'`));
    expect(missing).toEqual([]);

    // Belt-and-suspenders: each handled type also survives a minimal example
    // through the real reducer without throwing.
    for (const t of REDUCER_HANDLED_TYPES) {
      const h = createHarness();
      // Build a minimal placeholder event — most variants need at least an id.
      const minimal: Record<string, unknown> = { type: t };
      if (t !== 'token' && t !== 'thinking' && t !== 'context_usage' &&
          t !== 'precognition_result' && t !== 'mode_changed' &&
          t !== 'title_changed' && t !== 'session_event' && t !== 'error') {
        minimal.id = 'placeholder';
      }
      if (t === 'token' || t === 'thinking') minimal.content = '';
      if (t === 'context_usage') { minimal.used = 0; minimal.total = 0; }
      if (t === 'precognition_result') { minimal.notes_count = 0; minimal.notes = []; }
      if (t === 'mode_changed') minimal.mode = 'normal';
      if (t === 'title_changed') minimal.title = 'A generated title';
      if (t === 'session_event') { minimal.event_type = 'x'; minimal.data = null; }
      if (t === 'error') { minimal.code = 'x'; minimal.message = ''; }
      if (t === 'message_complete') { minimal.content = ''; }
      if (t === 'segment_complete') { minimal.message_id = 'placeholder'; minimal.index = 0; minimal.content = ''; }
      if (t === 'tool_call' || t === 'tool_call_start') minimal.title = minimal.name = 'noop';
      if (t === 'tool_result_delta') minimal.delta = '';
      if (t === 'tool_result_error' || t === 'subagent_failed' || t === 'delegation_failed') {
        minimal.error = '';
      }
      if (t === 'subagent_completed' || t === 'delegation_completed') minimal.summary = '';
      if (t === 'subagent_spawned' || t === 'delegation_spawned') minimal.prompt = '';
      expect(() => h.reducer(minimal as ChatEvent)).not.toThrow();
    }
  });
});
