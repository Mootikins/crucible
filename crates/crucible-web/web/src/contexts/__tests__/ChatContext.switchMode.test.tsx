import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { resetTranscriptsForTests } from '../transcriptStore';
import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';

// No `vi.mock('@/lib/api')`. The provider subscribes to the (fake) stream and
// bootstraps through the query layer on mount; the three routes it touches
// answer here, so the switchMode flow is what is left under test — and the
// assertions read what reached the wire, not a module double.

/** What `GET /api/session/s1` nests under `agent`: the only route that sends a mode at all. */
let agent: { model: string; mode: string } | null = null;
/** The bodies the mode route took, in order. */
const modeWrites: { mode: string }[] = [];
/** What `POST /api/session/s1/mode` answers; a case that means a refusal replaces it. */
let modeAnswer: () => Response = () => new Response(null, { status: 204 });

let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  agent = null;
  modeWrites.length = 0;
  modeAnswer = () => new Response(null, { status: 204 });
  env = createTestQueryEnv({
    'GET /api/interactions/pending': () => ({ pending: [] }),
    'GET /api/session/s1': () => ({
      session_id: 's1',
      type: 'chat',
      title: 'T',
      state: 'active',
      kilns: ['/k'],
      workspace: '/w',
      agent_model: null,
      agent,
      started_at: '',
      event_count: 0,
      archived: false,
    }),
    'GET /api/session/s1/history': () => ({ session_id: 's1', history: [], total_events: 0 }),
    'POST /api/session/s1/mode': async (request) => {
      modeWrites.push((await request.clone().json()) as { mode: string });
      return modeAnswer();
    },
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
  const Probe = () => {
    ctx = useChat();
    return null;
  };
  render(() => (
    <ChatProvider sessionId="s1">
      <Probe />
    </ChatProvider>
  ));
  return ctx;
}

describe('ChatContext.switchMode', () => {
  it('optimistically sets the mode and persists it via the API', async () => {
    const ctx = mountProvider();

    ctx.switchMode('plan');

    expect(ctx.chatMode()).toBe('plan'); // optimistic
    await waitFor(() => {
      // The wire took the mode the chip offered, for this session.
      expect(modeWrites).toEqual([{ mode: 'plan' }]);
    });
    expect(ctx.chatMode()).toBe('plan'); // stays after success
  });

  it('hydrates the persisted mode from session.get on mount', async () => {
    agent = { model: 'm', mode: 'plan' };
    const ctx = mountProvider();

    await waitFor(() => {
      // Reloading the page must not silently show Normal while the daemon
      // agent stays in plan mode.
      expect(ctx.chatMode()).toBe('plan');
    });
  });

  it('reverts the optimistic mode when the daemon rejects it', async () => {
    modeAnswer = () =>
      new Response(JSON.stringify({ error: { code: 422, message: 'unknown mode' } }), {
        status: 422,
        headers: { 'Content-Type': 'application/json' },
      });
    const ctx = mountProvider();

    ctx.switchMode('plan');
    expect(ctx.chatMode()).toBe('plan');

    await waitFor(() => {
      // Plan mode that is not enforced server-side must not LOOK enabled.
      expect(ctx.chatMode()).toBe('ask');
    });
  });
});
