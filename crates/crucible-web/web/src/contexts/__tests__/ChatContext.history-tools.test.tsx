import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// A reloaded transcript must not claim a tool completed when the events say
// it never answered. Live, a tool whose turn ended with no result renders the
// error the reducer words ('tool did not complete'); the reload used to stamp
// every replayed tool_call 'complete' regardless — the same tool showed ✗
// before the reload and ✓ after it.

const T0 = Date.parse('2026-09-15T18:01:00.000Z');
/** The log the mock answers with; each case sets the events it means. */
let held: unknown[] = [];

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api')>()),
  subscribeToEvents: () => () => {},
  getSessionHistory: async () => ({ session_id: 's1', history: held, total_events: held.length }),
  // `session.get`'s wire shape: `session_id`, `type`, a `kilns` LIST of
  // registry names, and the model nested under `agent`.
  getSession: async () => ({
    session_id: 's1', type: 'chat', title: 'T', state: 'active', kilns: ['k'], workspace: '/w',
    agent: { model: null }, started_at: '', event_count: 0, archived: false,
  }),
}));

// One turn whose only tool never answered: a tool_call with no tool_result.
const TURN_WITH_DANGLING_TOOL = [
  { type: 'event', session_id: 's1', event: 'user_message', data: { content: 'Do it', message_id: 'turn-1' }, timestamp: new Date(T0).toISOString(), seq: 1 },
  { type: 'event', session_id: 's1', event: 'tool_call', data: { call_id: 'call-1', tool: 'update_note', args: {} }, timestamp: new Date(T0 + 1000).toISOString(), seq: 2 },
  { type: 'event', session_id: 's1', event: 'message_complete', data: { full_response: 'Done.', message_id: 'turn-1' }, timestamp: new Date(T0 + 76_000).toISOString(), seq: 3 },
];
// The same turn, with the answer the daemon persisted for the tool.
const TURN_WITH_ANSWERED_TOOL = [
  TURN_WITH_DANGLING_TOOL[0],
  TURN_WITH_DANGLING_TOOL[1],
  { type: 'event', session_id: 's1', event: 'tool_result', data: { call_id: 'call-1', result: 'note saved' }, timestamp: new Date(T0 + 2000).toISOString(), seq: 3 },
  { type: 'event', session_id: 's1', event: 'message_complete', data: { full_response: 'Done.', message_id: 'turn-1' }, timestamp: new Date(T0 + 76_000).toISOString(), seq: 4 },
];

import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';
import { resetTranscriptsForTests } from '../transcriptStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';

let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({ 'GET /api/interactions/pending': () => ({ pending: [] }) });
});

afterEach(() => {
  env?.restore();
  resetTranscriptsForTests();
});

function mountProvider(): ChatContextValue {
  let ctx!: ChatContextValue;
  const Probe = () => { ctx = useChat(); return null; };
  render(() => (<ChatProvider sessionId="s1"><Probe /></ChatProvider>));
  return ctx;
}

describe('ChatContext reloads the state a tool was left in', () => {
  it('renders a tool with no result as the error the live transcript shows', async () => {
    held = TURN_WITH_DANGLING_TOOL;
    const ctx = mountProvider();
    await waitFor(() => expect(ctx.messages().length).toBe(3));
    const tool = ctx.messages().find((m) => m.role === 'tool');
    expect(tool?.toolCall?.status).toBe('error');
    expect(tool?.toolCall?.result).toBe('tool did not complete');
  });

  it('still completes a tool whose result the log holds', async () => {
    held = TURN_WITH_ANSWERED_TOOL;
    const ctx = mountProvider();
    await waitFor(() => expect(ctx.messages().length).toBe(3));
    const tool = ctx.messages().find((m) => m.role === 'tool');
    expect(tool?.toolCall?.status).toBe('complete');
    expect(tool?.toolCall?.result).toBe('note saved');
  });
});
