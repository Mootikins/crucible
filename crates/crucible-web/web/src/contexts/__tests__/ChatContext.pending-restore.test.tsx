import { describe, it, expect, vi } from 'vitest';
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

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api')>()),
  subscribeToEvents: () => () => {},
  getSessionHistory: async () => ({ history: [] }),
  listPendingInteractions: async () => [pendingEntry],
  getSession: async () => ({
    id: 's1',
    session_type: 'chat',
    title: 'T',
    state: 'active',
    kiln: '/k',
    workspace: '/w',
    agent_model: null,
    agent_mode: null,
    started_at: '',
    event_count: 0,
    archived: false,
  }),
}));

import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';

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
