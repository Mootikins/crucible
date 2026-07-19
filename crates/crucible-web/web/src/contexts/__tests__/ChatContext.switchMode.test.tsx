import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// The provider subscribes to SSE and bootstraps history on mount; stub the
// whole api surface it touches so only the switchMode flow is under test.
const setSessionModeMock = vi.fn();
let getSessionMode: string | undefined;

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api')>()),
  setSessionMode: (...args: unknown[]) => setSessionModeMock(...args),
  subscribeToEvents: () => () => {},
  getSessionHistory: async () => ({ history: [] }),
  // Mapped Session shape (mapSession output) — the mock must mirror the real
  // client contract, not the raw daemon JSON: hydration reads agent_mode.
  getSession: async () => ({
    id: 's1',
    session_type: 'chat',
    title: 'T',
    state: 'active',
    kiln: '/k',
    workspace: '/w',
    agent_model: null,
    agent_mode: getSessionMode ?? null,
    started_at: '',
    event_count: 0,
    archived: false,
  }),
}));

import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';

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

beforeEach(() => {
  vi.clearAllMocks();
  getSessionMode = undefined;
});

describe('ChatContext.switchMode', () => {
  it('optimistically sets the mode and persists it via the API', async () => {
    setSessionModeMock.mockResolvedValue(undefined);
    const ctx = mountProvider();

    ctx.switchMode('plan');

    expect(ctx.chatMode()).toBe('plan'); // optimistic
    await waitFor(() => {
      expect(setSessionModeMock).toHaveBeenCalledWith('s1', 'plan');
    });
    expect(ctx.chatMode()).toBe('plan'); // stays after success
  });

  it('hydrates the persisted mode from session.get on mount', async () => {
    getSessionMode = 'plan';
    const ctx = mountProvider();

    await waitFor(() => {
      // Reloading the page must not silently show Normal while the daemon
      // agent stays in plan mode.
      expect(ctx.chatMode()).toBe('plan');
    });
  });

  it('reverts the optimistic mode when the daemon rejects it', async () => {
    setSessionModeMock.mockRejectedValue(new Error('unknown mode'));
    const ctx = mountProvider();

    ctx.switchMode('plan');
    expect(ctx.chatMode()).toBe('plan');

    await waitFor(() => {
      // Plan mode that is not enforced server-side must not LOOK enabled.
      expect(ctx.chatMode()).toBe('normal');
    });
  });
});
