import { describe, it, expect, afterEach, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// A reloaded transcript must not claim a tool completed when the events say
// it never answered. Live, a tool whose turn ended with no result renders the
// error the reducer words ('tool did not complete'); the reload used to stamp
// every replayed tool_call 'complete' regardless — the same tool showed ✗
// before the reload and ✓ after it.

const T0 = Date.parse('2026-09-15T18:01:00.000Z');
/** The transcript the history route answers with; each case sets the events it means. */
let held: unknown[] = [];

// No `vi.mock('@/lib/api')`. The provider reads the daemon through the query
// layer, so the ROUTES answer: the session record, the transcript the reload
// reconstructs, and the pending aggregate a bind asks for. The stream is the
// fake one below; no frame ever arrives, so the reload is the only source.
const SESSION = {
  session_id: 's1', type: 'chat', title: 'T', state: 'active', kilns: ['k'], workspace: '/w',
  agent: { model: null }, started_at: '', event_count: 0, archived: false,
};

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
// The same turn, with a failed tool. The daemon persists the failure in the
// `data.result` envelope as `{"error": …}`, and success as `{"result": …}`.
const TURN_WITH_FAILED_TOOL = [
  TURN_WITH_DANGLING_TOOL[0],
  TURN_WITH_DANGLING_TOOL[1],
  { type: 'event', session_id: 's1', event: 'tool_result', data: { call_id: 'call-1', result: { error: 'disk full' } }, timestamp: new Date(T0 + 2000).toISOString(), seq: 3 },
  { type: 'event', session_id: 's1', event: 'message_complete', data: { full_response: 'Done.', message_id: 'turn-1' }, timestamp: new Date(T0 + 76_000).toISOString(), seq: 4 },
];
// An ACP turn whose agent announced the tool WITHOUT arguments and supplied
// them in a follow-up frame. The update is part of the record now; a reload
// must show the arguments the agent actually ran with, not the `{}` the
// announcement carried.
const TURN_WITH_LATE_ARGS = [
  TURN_WITH_DANGLING_TOOL[0],
  TURN_WITH_DANGLING_TOOL[1],
  { type: 'event', session_id: 's1', event: 'tool_call_update', data: { call_id: 'call-1', args: { command: 'ls crates' } }, timestamp: new Date(T0 + 1500).toISOString(), seq: 3 },
  { type: 'event', session_id: 's1', event: 'tool_result', data: { call_id: 'call-1', result: { result: 'out' } }, timestamp: new Date(T0 + 2000).toISOString(), seq: 4 },
  { type: 'event', session_id: 's1', event: 'message_complete', data: { full_response: 'Done.', message_id: 'turn-1' }, timestamp: new Date(T0 + 76_000).toISOString(), seq: 5 },
];

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';
import { resetTranscriptsForTests } from '../transcriptStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';

// An old transcript, as the daemon's history loader answers it. The daemon
// test `an_old_transcript_loads_in_its_current_form` makes this file from
// `old_wire_session.jsonl` and fails when its output changes.
const OLD_TRANSCRIPT: unknown[] = JSON.parse(readFileSync(
  resolve(process.cwd(), '../../../assets/fixtures/old_wire_session.migrated.json'), 'utf8'));

let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({
    'GET /api/interactions/pending': () => ({ pending: [] }),
    'GET /api/session/s1': () => SESSION,
    'GET /api/session/s1/history': () => ({ session_id: 's1', history: held, total_events: held.length }),
  });
});

afterEach(() => {
  env?.restore();
  // The transcript store is a module singleton keyed by session; forget it
  // so one case's session cannot answer the next one.
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

  it('renders a failed tool as the error the live transcript shows', async () => {
    held = TURN_WITH_FAILED_TOOL;
    const ctx = mountProvider();
    await waitFor(() => expect(ctx.messages().length).toBe(3));
    const tool = ctx.messages().find((m) => m.role === 'tool');
    expect(tool?.toolCall?.status).toBe('error');
    expect(tool?.toolCall?.result).toBe('disk full');
  });

  it('replays the late args an ACP agent supplied after announcing the call', async () => {
    held = TURN_WITH_LATE_ARGS;
    const ctx = mountProvider();
    await waitFor(() => expect(ctx.messages().length).toBe(3));
    const tool = ctx.messages().find((m) => m.role === 'tool');
    expect(tool?.toolCall?.args).toBe(JSON.stringify({ command: 'ls crates' }));
    expect(tool?.toolCall?.result).toBe('out');
  });

  it('shows the card line and the diffs of an old transcript', async () => {
    held = OLD_TRANSCRIPT;
    const ctx = mountProvider();
    await waitFor(() => expect(ctx.messages().filter((m) => m.role === 'tool').length).toBe(2));
    const [edit, acp] = ctx.messages().filter((m) => m.role === 'tool').map((m) => m.toolCall);
    expect(edit?.display?.render?.line).toBe('lib.rs (from Lua)');
    expect(edit?.display?.diffs?.[0]?.new_content).toBe('b\n');
    expect(acp?.display?.render?.line).toBe('src/main.rs');
    expect(acp?.display?.diffs?.[0]?.new_content).toBe('y\n');
    expect(acp?.args).toBe(JSON.stringify({ file_path: 'src/main.rs' }));
  });
});
