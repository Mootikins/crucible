import { describe, it, expect, afterEach, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';
import type { Transcript } from '@/lib/transcript';
import { resetTranscriptsForTests } from '../transcriptStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';
import { historyOf, segment, toolCard, userTurn } from '@/test-utils/transcript';

// A reloaded transcript draws each tool card in the state the daemon's fold
// gave it. The daemon decides the state (`crucible-core/src/transcript/`);
// the pane maps it. No frame arrives on the stream, so the snapshot is the
// only source.

const SESSION = {
  session_id: 's1', type: 'chat', title: 'T', state: 'active', kilns: ['k'], workspace: '/w',
  agent: { model: null }, started_at: '', event_count: 0, archived: false,
};

/** The history document the route answers; each case sets it. */
let held: unknown;
let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({
    'GET /api/interactions/pending': () => ({ pending: [] }),
    'GET /api/session/s1': () => SESSION,
    'GET /api/session/s1/history': () => held,
  });
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

/** One turn whose only tool card has the fields `extra`. */
const turnWithTool = (extra: Parameters<typeof toolCard>[2]) =>
  historyOf('s1', [
    userTurn('turn-1', 'Do it'),
    toolCard('turn-1', 'call-1', { name: 'update_note', ...extra }),
    segment('turn-1', 0, 'Done.'),
  ]);

async function reloadedTool(extra: Parameters<typeof toolCard>[2]) {
  held = turnWithTool(extra);
  const ctx = mountProvider();
  await waitFor(() => expect(ctx.messages().length).toBe(3));
  return ctx.messages().find((m) => m.role === 'tool')?.toolCall;
}

describe('ChatContext draws the state the daemon gave a tool', () => {
  it('draws a tool that never answered as the error the live transcript shows', async () => {
    const tool = await reloadedTool({ status: 'incomplete' });
    expect(tool?.status).toBe('error');
    expect(tool?.result).toBe('tool did not complete');
  });

  it('draws a tool whose result the log holds as complete', async () => {
    const tool = await reloadedTool({ status: 'complete', result: 'note saved' });
    expect(tool?.status).toBe('complete');
    expect(tool?.result).toBe('note saved');
  });

  it('draws a failed tool as an error with its message', async () => {
    const tool = await reloadedTool({ status: 'failed', error: 'disk full' });
    expect(tool?.status).toBe('error');
    expect(tool?.result).toBe('disk full');
  });

  it('draws the args and the render the daemon holds for the card', async () => {
    const tool = await reloadedTool({
      args: { command: 'ls crates' } as never,
      display: { kind: 'file_read', tool: 'read_file', render: { line: 'a.rs', summary: '1 lines' } } as never,
    });
    expect(tool?.args).toBe(JSON.stringify({ command: 'ls crates' }));
    expect(tool?.display?.render?.summary).toBe('1 lines');
  });

  it('shows the card line and the diffs of an old transcript', async () => {
    // The daemon's fold of `old_wire_session.jsonl`, which a test of
    // `crucible-daemon/src/session_manager/tests.rs` checks.
    const golden = JSON.parse(readFileSync(
      resolve(process.cwd(), '../../../assets/fixtures/golden/transcript/old_wire_session.json'),
      'utf8',
    )) as Transcript;
    held = { session_id: 's1', history: [], total_events: 0, transcript: golden };
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
