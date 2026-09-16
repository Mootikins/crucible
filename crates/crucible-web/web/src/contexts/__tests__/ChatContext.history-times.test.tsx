import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// A reloaded turn must show the same duration the live one did, so the
// reconstruction reads the daemon's event stamps: the user message is the
// turn's start, and message_complete is its end.
const T0 = Date.parse('2026-09-15T18:01:00.000Z');
const history = [
  { type: 'event', session_id: 's1', event: 'user_message', data: { content: 'Read it', message_id: 'turn-1' }, timestamp: new Date(T0).toISOString(), seq: 1 },
  { type: 'event', session_id: 's1', event: 'message_complete', data: { full_response: 'Done.', message_id: 'turn-1' }, timestamp: new Date(T0 + 76_000).toISOString(), seq: 2 },
];

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api')>()),
  subscribeToEvents: () => () => {},
  getSessionHistory: async () => ({ session_id: 's1', history, total_events: history.length }),
  getSession: async () => ({
    id: 's1', session_type: 'chat', title: 'T', state: 'active', kiln: '/k', workspace: '/w',
    agent_model: null, started_at: '', event_count: 0, archived: false,
  }),
}));

import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';

// The pending aggregate is the shared query now, so it answers the route
// rather than a stub of `listPendingInteractions`.
let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({ 'GET /api/interactions/pending': () => ({ pending: [] }) });
});

afterEach(() => {
  env?.restore();
});

function mountProvider(): ChatContextValue {
  let ctx!: ChatContextValue;
  const Probe = () => { ctx = useChat(); return null; };
  render(() => (<ChatProvider sessionId="s1"><Probe /></ChatProvider>));
  return ctx;
}

describe('ChatContext reloads real event times', () => {
  it('stamps the turn start on both bubbles and the end on the answer', async () => {
    const ctx = mountProvider();
    await waitFor(() => expect(ctx.messages().length).toBe(2));
    const [user, answer] = ctx.messages();
    expect(user.timestamp).toBe(T0);
    expect(answer.timestamp).toBe(T0);
    expect(answer.completedAt).toBe(T0 + 76_000);
  });
});
