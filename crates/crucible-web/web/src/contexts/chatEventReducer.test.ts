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

/** A `SessionEventPayload` frame: `{event, data}`, the daemon's own shape. */
const sessionEvent = (event: string, data: unknown): ChatEvent =>
  ({ event, data }) as ChatEvent;

describe('event matrix — covers the reducer-handled SessionEventPayload names', () => {
  it.each([
    sessionEvent('text_delta', { content: 'a' }),
    sessionEvent('thinking', { content: 'a' }),
    sessionEvent('tool_call', { call_id: 'c', tool: 'read' }),
    sessionEvent('tool_result', { call_id: 'c', tool: 'read', result: 'r' }),
    sessionEvent('segment_complete', { message_id: 'm', index: 0, content: 'a' }),
  ])('$event: marks a turn of another client as streaming', (event) => {
    const h = createHarness();
    h.reducer(event);
    expect(h.state.isStreaming).toBe(true);
  });

  it('message_complete: ends the busy state', () => {
    const h = createHarness();
    h.state.isLoading = true;
    h.state.isStreaming = true;
    h.reducer(sessionEvent('message_complete', { message_id: 'm', full_response: 'answer' }));
    expect(h.state.isLoading).toBe(false);
    expect(h.state.isStreaming).toBe(false);
  });

  it('turn_finished: ends the busy state of a cancelled turn', () => {
    const h = createHarness();
    h.state.isLoading = true;
    h.state.isStreaming = true;
    h.reducer(sessionEvent('turn_finished', { status: 'cancelled' }));
    expect(h.state.isLoading).toBe(false);
    expect(h.state.isStreaming).toBe(false);
    expect(h.state.error).toBeNull();
  });

  it('turn_finished: a failed turn shows its error', () => {
    const h = createHarness();
    h.reducer(sessionEvent('turn_finished', { status: 'failed', error: 'provider down' }));
    expect(h.state.error).toBe('provider down (turn_failed)');
  });

  it('turn_finished: a turn that a handler cancelled shows its reason', () => {
    const h = createHarness();
    h.reducer(sessionEvent('turn_finished', { status: 'handler_cancelled', error: 'loop guard' }));
    expect(h.state.error).toBe('loop guard (turn_handler_cancelled)');
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

  it('interaction_requested: stores the request with its request_id as `id`', () => {
    const h = createHarness();
    h.reducer(
      sessionEvent('interaction_requested', {
        request_id: 'req-1',
        request: { kind: 'ask', question: 'Proceed?' },
      }),
    );
    expect(h.state.pendingInteraction).toEqual({ id: 'req-1', kind: 'ask', question: 'Proceed?' });
  });

  it('mode_changed: sets the mode and tells the status bar', () => {
    const h = createHarness();
    h.reducer(sessionEvent('mode_changed', { mode: 'plan' }));
    expect(h.state.chatMode).toBe('plan');
    expect(mockedStatusBar.setChatMode).toHaveBeenCalledWith('plan');
    expect(h.spies.onUnknownMode).toHaveBeenCalledWith('plan');
  });

  it('title_changed: forwards the daemon-generated title', () => {
    const h = createHarness();
    h.reducer(sessionEvent('title_changed', { title: 'Merkle tree sync design' }));
    expect(h.spies.onTitleChanged).toHaveBeenCalledWith('Merkle tree sync design');
  });

  // The transcript items carry these, or another store owns them. The
  // reducer changes nothing in the pane's own state for them. A sample of
  // the ~55 no-op names, not all of them — the exhaustive switch in
  // `chatEventReducer.ts` is what makes a NEW no-op name a compile error
  // instead of a silent gap; see the `SessionEventPayload` type-flow test
  // below.
  it.each([
    { type: 'transcript', seq: 1, ops: [] },
    sessionEvent('commands_changed', {}),
    sessionEvent('user_message', { message_id: 'm', content: 'q' }),
    sessionEvent('context_cleared', {}),
    sessionEvent('context_injected', { role: 'user', content: 'x' }),
    sessionEvent('tool_call_update', { call_id: 'c' }),
    sessionEvent('delegation_spawned', { delegation_id: 'd', prompt: 'p' }),
    sessionEvent('delegation_completed', { delegation_id: 'd', result_summary: 's' }),
    sessionEvent('delegation_failed', { delegation_id: 'd', error: 'e' }),
    sessionEvent('precognition_complete', { notes_count: 1, notes: [] }),
    sessionEvent('interaction_completed', { request_id: 'r', response: { kind: 'cancelled' } }),
    sessionEvent('post_llm_call', {}),
    sessionEvent('stream_gap', { dropped: 3 }),
    sessionEvent('file_changed', { path: '/a', kind: 'modified' }),
    sessionEvent('file_deleted', { path: '/a' }),
    sessionEvent('file_moved', { from: '/a', to: '/b' }),
    sessionEvent('classification_required', {}),
    sessionEvent('process_complete', { kiln: 'k' }),
    sessionEvent('ui_style_changed', {}),
    sessionEvent('status_items_changed', { status: [] }),
    sessionEvent('note:created', { path: 'a.md' }),
    sessionEvent('note:modified', { path: 'a.md' }),
    sessionEvent('note:deleted', { path: 'a.md' }),
    sessionEvent('note:renamed', { from: 'a.md', to: 'b.md' }),
    sessionEvent('base:changed', { path: 'a.base', change: {} }),
    sessionEvent('webhook:received', { name: 'w' }),
    sessionEvent('replay_complete', { status: 'ok' }),
    sessionEvent('session:created', { session_id: 's' }),
    sessionEvent('session:ended', { session_id: 's', reason: 'r' }),
    sessionEvent('surface_changed', { plugin: 'p', name: 'n', version: 1 }),
    sessionEvent('publication_changed', { plugin: 'p', key: 'k' }),
    sessionEvent('proposal_changed', { id: 'p-1' }),
    // Session-settings acknowledgements.
    sessionEvent('model_switched', { model_id: 'm', provider: 'p' }),
    sessionEvent('scope_changed', { kilns: [] }),
    sessionEvent('system_prompt_changed', { system_prompt: 's' }),
    sessionEvent('precognition_toggled', { enabled: true }),
    sessionEvent('context_strategy_changed', { context_strategy: 's' }),
    sessionEvent('plugin_approval_changed', { plugin: 'p', approval: 'ask' }),
    sessionEvent('plugin_turn_limit_changed', { limit: 5 }),
    // Setup-phase notices.
    sessionEvent('session_initialized', { model: 'm', mode: 'ask', kilns: [], workspace_path: '/w' }),
    sessionEvent('providers_listed', { providers: [] }),
    sessionEvent('context_limit_resolved', { limit: 1000, source: 'config' }),
    sessionEvent('workspace_indexed', { files: [] }),
    sessionEvent('kiln_notes_indexed', { notes: [] }),
    sessionEvent('plugins_discovered', { plugins: [] }),
    sessionEvent('mcp_servers_ready', { servers: [] }),
    sessionEvent('acp_resume_fallback', { agent: 'a', new_session_id: 's', reason: 'r' }),
    // Delegated job lifecycle beyond `delegation_*`.
    sessionEvent('bash_job_spawned', { job_id: 'j', command: 'ls' }),
    sessionEvent('bash_job_completed', { job_id: 'j', output: 'ok' }),
    sessionEvent('bash_job_failed', { job_id: 'j', error: 'e' }),
    sessionEvent('background_job_completed', { job_id: 'j', kind: 'k', summary: 's' }),
    // Review/undo and notifications.
    sessionEvent('review_changed', { reason: 'accepted' }),
    sessionEvent('session_undo', { turns_undone: 1, messages_removed: 2 }),
    sessionEvent('notification_added', { notification_id: 'n' }),
    sessionEvent('notification_dismissed', { notification_id: 'n' }),
    // Workflow-engine progress.
    sessionEvent('workflow.step_started', { step_id: 's', title: 't' }),
    sessionEvent('workflow.step_completed', { step_id: 's' }),
    sessionEvent('workflow.gate_reached', { gate_id: 'g', owner: 'o' }),
    sessionEvent('workflow.gate_approved', { gate_id: 'g' }),
    sessionEvent('workflow.completed', {}),
    sessionEvent('workflow.assessed', { runnable_passed: [], runnable_failed: [], manual_entries: [] }),
    sessionEvent('workflow.failed', { reason: 'r' }),
    sessionEvent('workflow.cancelled', {}),
  ] as ChatEvent[])('$type$event: changes nothing in the pane', (event) => {
    const h = createHarness();
    const before = snapshot(h);
    h.reducer(event);
    expect(snapshot(h)).toBe(before);
  });
});

// ============================================================================
// Property-based tests
// ============================================================================

const strId = fc.string({ minLength: 1, maxLength: 10 });

/** A `SessionEventPayload` frame arbitrary: `{event: name, data}`. */
const sessionEvt = <T extends string>(name: T, data: Record<string, fc.Arbitrary<unknown>> = {}) =>
  fc.record({ event: fc.constant(name), data: fc.record(data) });

const interactionAsk = sessionEvt('interaction_requested', {
  request_id: strId,
  request: fc.record({ kind: fc.constant('ask' as const), question: fc.string({ maxLength: 100 }) }),
});
const interactionPerm = sessionEvt('interaction_requested', {
  request_id: strId,
  request: fc.record({
    kind: fc.constant('permission' as const),
    action: fc.oneof(
      fc.record({
        type: fc.constant('bash' as const),
        tokens: fc.array(fc.string({ maxLength: 20 }), { maxLength: 5 }),
      }),
      fc.record({
        type: fc.constant('tool' as const),
        name: fc.string({ minLength: 1, maxLength: 20 }),
        args: fc.constant({}),
      }),
    ),
  }),
});

const arbChatEvent = (): fc.Arbitrary<ChatEvent> => fc.oneof(
  sessionEvt('text_delta', { content: fc.string() }),
  sessionEvt('tool_call', { call_id: strId, tool: fc.string() }),
  sessionEvt('thinking', { content: fc.string() }),
  sessionEvt('message_complete', { message_id: strId, full_response: fc.string() }),
  sessionEvt('turn_finished', { status: fc.constantFrom('completed', 'cancelled', 'failed') }),
  fc.oneof(interactionAsk, interactionPerm),
  sessionEvt('mode_changed', { mode: fc.constantFrom('ask' as const, 'plan' as const, 'auto' as const) }),
  sessionEvt('user_message', { message_id: strId, content: fc.string() }),
  sessionEvt('stream_gap', { dropped: fc.nat() }),
  fc.record({ type: fc.constant('transcript' as const), seq: fc.nat(), ops: fc.constant([]) }),
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
        h.reducer(sessionEvent('turn_finished', { status: 'completed' }));
        expect(h.state.isStreaming).toBe(false);
        expect(h.state.isLoading).toBe(false);
      }),
      { numRuns: 50 },
    );
  });
});

// ============================================================================
// Contract checks: the SSE subscription list (api.ts) must match the set the
// reducer names. Drift between the two means events arrive on the wire but
// are silently dropped, or the reverse.
//
// The primary proof is `bun run typecheck`: `chatEventReducer.ts`'s switch on
// `SessionEventPayload['event']` ends in `const unhandled: never = event`, so
// a variant this file does not name fails the BUILD, not a test. What
// follows is a run-time cross-check that the two hand-written lists
// (`SSE_EVENT_TYPES` in `lib/api.ts`, the reducer's own `case` labels) still
// agree — useful because a name can be spelled two ways and still typecheck.
// ============================================================================

describe('contract: SSE subscription parity with reducer case labels', () => {
  it('every SSE_EVENT_TYPES name the reducer must decide about is a case label, or is handled by the type-tag branch', () => {
    const source = fs.readFileSync(path.join(__dirname, 'chatEventReducer.ts'), 'utf-8');
    const caseLabels = new Set(
      Array.from(source.matchAll(/case '([a-z0-9_.:]+)':/g), (m) => m[1]),
    );
    // `connection` and `transcript` are decided by the `'type' in event`
    // branch, by their own `.type`, never by a `case` label on `.event`.
    const decidedByTypeTag = new Set(['connection', 'transcript']);

    const missing = SSE_EVENT_TYPES.filter(
      (name) => !caseLabels.has(name) && !decidedByTypeTag.has(name),
    );
    expect(missing).toEqual([]);
  });

  it('the reducer names no case the document does not declare', () => {
    const source = fs.readFileSync(path.join(__dirname, 'chatEventReducer.ts'), 'utf-8');
    const caseLabels = Array.from(source.matchAll(/case '([a-z0-9_.:]+)':/g), (m) => m[1]);
    const declared = new Set<string>(SSE_EVENT_TYPES);
    const extra = caseLabels.filter((name) => !declared.has(name));
    expect(extra).toEqual([]);
  });
});
