import { describe, it, expect, afterEach, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// A permission request that the daemon still holds must survive a page
// reload. The SSE stream only carries NEW requests, so the provider has to
// ask the daemon's pending aggregate once when it binds to a session.
const pendingEntry = {
  session_id: 's1',
  request_id: 'perm-1',
  request: {
    kind: 'permission',
    id: 'perm-1',
    action_type: 'tool',
    tokens: ['update_note'],
    tool_name: 'update_note',
    tool_args: {},
  },
};

// No `vi.mock('@/lib/api')`. The aggregate IS the shared
// `usePendingInteractions()` list, so it answers the ROUTE below — which is
// also what proves the provider asks for it on bind. The session records and
// the transcripts each session's bind reads answer beside it.
const sessionOf = (id: string) => ({
  session_id: id,
  type: 'chat',
  title: 'T',
  state: 'active',
  kilns: ['k'],
  workspace: '/w',
  agent: { model: null },
  started_at: '',
  event_count: 0,
  archived: false,
});

import { resetTranscriptsForTests } from '../transcriptStore';
import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';

let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({
    'GET /api/interactions/pending': () => ({ pending: [pendingEntry] }),
    'GET /api/session/s1': () => sessionOf('s1'),
    'GET /api/session/s2': () => sessionOf('s2'),
    'GET /api/session/s1/history': () => ({ session_id: 's1', history: [], total_events: 0 }),
    'GET /api/session/s2/history': () => ({ session_id: 's2', history: [], total_events: 0 }),
  });
});


afterEach(() => {
  env?.restore();
  // The transcript store is a module singleton keyed by session; forget it
  // so one case's session cannot answer the next one.
  resetTranscriptsForTests();
});

function mountProvider(sessionId: string): ChatContextValue {
  let ctx!: ChatContextValue;
  const Probe = () => {
    ctx = useChat();
    return null;
  };
  render(() => (
    <ChatProvider sessionId={sessionId}>
      <Probe />
    </ChatProvider>
  ));
  return ctx;
}

describe('ChatContext restores a pending interaction on bind', () => {
  it('seeds pendingInteraction from the daemon aggregate for this session', async () => {
    const ctx = mountProvider('s1');
    await waitFor(() => {
      expect(ctx.pendingInteraction()?.id).toBe('perm-1');
    });
  });

  it('ignores a pending request that belongs to another session', async () => {
    const ctx = mountProvider('s2');
    // Give the bootstrap a tick to settle, then assert nothing leaked in.
    await new Promise((r) => setTimeout(r, 20));
    expect(ctx.pendingInteraction()).toBeNull();
  });
});
