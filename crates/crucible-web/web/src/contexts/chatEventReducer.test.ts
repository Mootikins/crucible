import { describe, it, expect, vi } from 'vitest';
import fs from 'fs';
import path from 'path';
import fc from 'fast-check';
import type { ChatEvent, ChatMode, InteractionRequest, ConnectionStatus } from '@/lib/types';

vi.mock('@/stores/statusBarStore', () => ({
  statusBarActions: {
    setChatMode: vi.fn(),
  },
}));

// Import AFTER the mocks so the reducer picks them up.
import { createChatEventReducer } from './chatEventReducer';
import { statusBarActions } from '@/stores/statusBarStore';
import { SSE_EVENT_TYPES } from '@/lib/api';

const mockedStatusBar = statusBarActions as unknown as {
  setChatMode: ReturnType<typeof vi.fn>;
};

// ============================================================================
// Test harness: builds a deps record whose getters reflect mutable state.
//
// The reducer keeps the state AROUND the transcript. The transcript is the
// daemon's fold, which `transcriptStore` applies; see `lib/transcript.test.ts`.
// ============================================================================

interface ReducerHarness {
  reducer: (event: ChatEvent) => void;
  state: {
    chatMode: ChatMode;
    pendingInteraction: InteractionRequest | null;
    error: string | null;
    notices: string[];
    connectionStatus: ConnectionStatus;
    isLoading: boolean;
    isStreaming: boolean;
  };
  spies: {
    onTitleChanged: ReturnType<typeof vi.fn>;
    onUnknownMode: ReturnType<typeof vi.fn>;
  };
}

function createHarness(): ReducerHarness {
  const state: ReducerHarness['state'] = {
    chatMode: 'ask',
    pendingInteraction: null,
    error: null,
    notices: [],
    connectionStatus: 'connected',
    isLoading: false,
    isStreaming: false,
  };
  const spies = { onTitleChanged: vi.fn(), onUnknownMode: vi.fn() };
  const reducer = createChatEventReducer({
    setChatMode: (mode) => {
      state.chatMode = mode;
    },
    onUnknownMode: spies.onUnknownMode,
    onTitleChanged: spies.onTitleChanged,
    setPendingInteraction: (request) => {
      state.pendingInteraction = request;
    },
    setError: (value) => {
      state.error = value;
    },
    addErrorNotice: (message) => {
      state.notices.push(message);
    },
    setConnectionStatus: (value) => {
      state.connectionStatus = value;
    },
    setIsLoading: (value) => {
      state.isLoading = value;
    },
    isStreaming: () => state.isStreaming,
    setIsStreaming: (value) => {
      state.isStreaming = value;
    },
  });
  return { reducer, state, spies };
}

/** The state as JSON, for a check that an event changed nothing. */
const snapshot = (h: ReducerHarness) => JSON.stringify(h.state);

describe('event matrix — covers every ChatEvent variant', () => {
  it.each([
    { type: 'token', content: 'a' },
    { type: 'thinking', content: 'a' },
    { type: 'tool_call', id: 'c', title: 'read' },
    { type: 'tool_result', id: 'c', result: 'r' },
    { type: 'tool_result_delta', id: 'c', delta: 'r' },
    { type: 'tool_result_complete', id: 'c' },
    { type: 'tool_result_error', id: 'c', error: 'e' },
    { type: 'segment_complete', message_id: 'm', index: 0, content: 'a' },
  ] as ChatEvent[])('$type: marks a turn of another client as streaming', (event) => {
    const h = createHarness();
    h.reducer(event);
    expect(h.state.isStreaming).toBe(true);
  });

  it('message_complete: ends the busy state', () => {
    const h = createHarness();
    h.state.isLoading = true;
    h.state.isStreaming = true;
    h.reducer({ type: 'message_complete', id: 'm', content: 'answer' });
    expect(h.state.isLoading).toBe(false);
    expect(h.state.isStreaming).toBe(false);
  });

  it('turn_finished: ends the busy state of a cancelled turn', () => {
    const h = createHarness();
    h.state.isLoading = true;
    h.state.isStreaming = true;
    h.reducer({ type: 'turn_finished', status: 'cancelled' });
    expect(h.state.isLoading).toBe(false);
    expect(h.state.isStreaming).toBe(false);
    expect(h.state.error).toBeNull();
  });

  it('turn_finished: a failed turn shows its error', () => {
    const h = createHarness();
    h.reducer({ type: 'turn_finished', status: 'failed', error: 'provider down' });
    expect(h.state.error).toBe('provider down (turn_failed)');
  });

  it('turn_finished: a turn that a handler cancelled shows its reason', () => {
    const h = createHarness();
    h.reducer({ type: 'turn_finished', status: 'handler_cancelled', error: 'loop guard' });
    expect(h.state.error).toBe('loop guard (turn_handler_cancelled)');
  });

  it('error: shows the daemon error and ends the busy state', () => {
    const h = createHarness();
    h.state.isStreaming = true;
    h.reducer({ type: 'error', code: 'E1', message: 'boom' });
    expect(h.state.error).toBe('boom (E1)');
    // A reconnect banner can replace the error line, so the transcript keeps it.
    expect(h.state.notices).toEqual(['Error: boom']);
    expect(h.state.isStreaming).toBe(false);
  });

  it('connection: a reconnect is a banner, and an open clears it', () => {
    const h = createHarness();
    h.state.isStreaming = true;
    h.reducer({ type: 'connection', status: 'reconnecting', message: 'Reconnecting…' });
    expect(h.state.connectionStatus).toBe('reconnecting');
    expect(h.state.error).toBe('Reconnecting…');
    // The turn is still in flight.
    expect(h.state.isStreaming).toBe(true);
    h.reducer({ type: 'connection', status: 'connected' });
    expect(h.state.connectionStatus).toBe('connected');
    expect(h.state.error).toBeNull();
  });

  it('interaction_requested: stores the request without its type tag', () => {
    const h = createHarness();
    h.reducer({
      type: 'interaction_requested',
      id: 'req-1',
      kind: 'ask',
      question: 'Proceed?',
    } as ChatEvent);
    expect(h.state.pendingInteraction).toEqual({ id: 'req-1', kind: 'ask', question: 'Proceed?' });
  });

  it('mode_changed: sets the mode and tells the status bar', () => {
    const h = createHarness();
    h.reducer({ type: 'mode_changed', mode: 'plan' });
    expect(h.state.chatMode).toBe('plan');
    expect(mockedStatusBar.setChatMode).toHaveBeenCalledWith('plan');
    expect(h.spies.onUnknownMode).toHaveBeenCalledWith('plan');
  });

  it('title_changed: forwards the daemon-generated title', () => {
    const h = createHarness();
    h.reducer({ type: 'title_changed', title: 'Merkle tree sync design' });
    expect(h.spies.onTitleChanged).toHaveBeenCalledWith('Merkle tree sync design');
  });

  // The transcript items carry these. The reducer changes nothing for them.
  it.each([
    { type: 'commands_changed' },
    { type: 'transcript', seq: 1, ops: [] },
    { type: 'delegation_spawned', id: 'd', prompt: 'p' },
    { type: 'delegation_completed', id: 'd', summary: 's' },
    { type: 'delegation_failed', id: 'd', error: 'e' },
    { type: 'precognition_result', notes_count: 1, notes: [] },
    { type: 'session_event', event: 'user_message', data: { message_id: 'm', content: 'q' } },
    { type: 'session_event', event: 'stream_gap', data: { dropped: 3 } },
  ] as ChatEvent[])('$type: changes nothing in the pane', (event) => {
    const h = createHarness();
    const before = snapshot(h);
    h.reducer(event);
    expect(snapshot(h)).toBe(before);
  });
});

// ============================================================================
// Property-based tests
// ============================================================================

const evt = <T extends string>(type: T, fields: Record<string, fc.Arbitrary<unknown>> = {}) =>
  fc.record({ type: fc.constant(type), ...fields });

const strId = fc.string({ minLength: 1, maxLength: 10 });

const interactionAsk = evt('interaction_requested', {
  id: strId,
  kind: fc.constant('ask' as const),
  question: fc.string({ maxLength: 100 }),
});
const interactionPerm = evt('interaction_requested', {
  id: strId,
  kind: fc.constant('permission' as const),
  action_type: fc.constantFrom('bash' as const, 'read' as const, 'write' as const, 'tool' as const),
  tokens: fc.array(fc.string({ maxLength: 20 }), { maxLength: 5 }),
});

const arbChatEvent = (): fc.Arbitrary<ChatEvent> => fc.oneof(
  evt('token', { content: fc.string() }),
  evt('tool_call', { id: strId, title: fc.string() }),
  evt('thinking', { content: fc.string() }),
  evt('message_complete', { id: strId, content: fc.string() }),
  evt('turn_finished', { status: fc.constantFrom('completed', 'cancelled', 'failed') }),
  evt('error', { code: fc.string({ minLength: 1, maxLength: 20 }), message: fc.string() }),
  fc.oneof(interactionAsk, interactionPerm),
  evt('mode_changed', { mode: fc.constantFrom('ask' as const, 'plan' as const, 'auto' as const) }),
  evt('session_event', { event: fc.string(), data: fc.anything() }),
  evt('transcript', { seq: fc.nat(), ops: fc.constant([]) }),
) as fc.Arbitrary<ChatEvent>;

describe('property: totality', () => {
  it('any sequence of events runs to completion without throwing', () => {
    fc.assert(
      fc.property(fc.array(arbChatEvent(), { maxLength: 50 }), (events) => {
        const h = createHarness();
        for (const event of events) h.reducer(event);
      }),
      { numRuns: 100 },
    );
  });

  it('the busy state is off after the last turn_finished', () => {
    fc.assert(
      fc.property(fc.array(arbChatEvent(), { maxLength: 30 }), (events) => {
        const h = createHarness();
        for (const event of events) h.reducer(event);
        h.reducer({ type: 'turn_finished', status: 'completed' });
        expect(h.state.isStreaming).toBe(false);
        expect(h.state.isLoading).toBe(false);
      }),
      { numRuns: 50 },
    );
  });
});

// ============================================================================
// Contract checks: the SSE subscription list (api.ts) must match the set the
// reducer handles. Drift between the two means events arrive on the wire but
// are silently dropped, or the reverse.
// ============================================================================

describe('contract: SSE subscription parity with reducer handlers', () => {
  const REDUCER_HANDLED_TYPES = [
    'token',
    'tool_call',
    'tool_result',
    'tool_result_delta',
    'tool_result_complete',
    'tool_result_error',
    'thinking',
    'segment_complete',
    'message_complete',
    'turn_finished',
    'error',
    'interaction_requested',
    'session_event',
    'delegation_spawned',
    'delegation_completed',
    'delegation_failed',
    'precognition_result',
    'mode_changed',
    'title_changed',
    'commands_changed',
    'transcript',
  ] as const;

  it('SSE_EVENT_TYPES and reducer-handled types are identical', () => {
    expect([...SSE_EVENT_TYPES].sort()).toEqual([...REDUCER_HANDLED_TYPES].sort());
  });

  it('every reducer-handled type has a matrix test above', () => {
    const source = fs.readFileSync(path.join(__dirname, 'chatEventReducer.test.ts'), 'utf-8');
    const matrixStart = source.indexOf("describe('event matrix");
    const matrixEnd = source.indexOf("describe('property: totality'");
    expect(matrixStart).toBeGreaterThanOrEqual(0);
    expect(matrixEnd).toBeGreaterThan(matrixStart);
    const matrixBlock = source.slice(matrixStart, matrixEnd);
    const missing = REDUCER_HANDLED_TYPES.filter((t) => !matrixBlock.includes(`'${t}'`));
    expect(missing).toEqual([]);
  });
});
