import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { SessionStatusChips } from '../SessionStatusChips';
import { ChatProvider } from '@/contexts/ChatContext';
import { __resetReviewStore } from '@/lib/review-store';
import type { Session } from '@/lib/types';
import { setQueryClientForTests } from '@/lib/query/client';
import { createTestQueryClient } from '@/test-utils/query';

const [currentSession, setCurrentSession] = createSignal<Session | undefined>(undefined);
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ currentSession }),
}));

const getSessionStatusMock = vi.fn();
const listModesMock = vi.fn(async () => ({ current_mode_id: 'ask', modes: [] }));
// The chips retain the session's review state, which opens one SSE stream and
// lists the composed diff. Both are stubbed: this suite is about the chips.
//
// The last case mounts the chat pane around the chips, to count the reads of
// one mode list; the rest of this factory is what that pane touches on mount.
vi.mock('@/lib/api', () => ({
  getSessionStatus: (...args: unknown[]) => getSessionStatusMock(...args),
  listModes: (...args: unknown[]) => listModesMock(...(args as [])),
  subscribeToEvents: () => () => {},
  getSession: vi.fn(async (id: string) => ({
    id,
    session_type: 'chat',
    kilns: ['/kilns/main'],
    workspace: '/kilns/main',
    state: 'active',
    title: null,
    agent_model: null,
    agent_mode: null,
    started_at: '2026-01-01T00:00:00Z',
    event_count: 0,
  })),
  getSessionHistory: vi.fn(async () => ({ session_id: 's1', history: [], total_events: 0 })),
  listPendingInteractions: vi.fn(async () => []),
  generateMessageId: () => 'msg_test',
  turnResponseId: (id: string) => `${id}-response`,
  turnSegmentId: (id: string, index: number) => `${id}-seg-${index}`,
  turnThinkingId: (id: string) => `${id}-thinking`,
  stripFrozenPrefix: (full: string) => full,
}));
vi.mock('@/lib/review-api', () => ({
  listReviewHunks: vi.fn(async () => ({ session_id: 's', hunks: [], comments: [] })),
}));

const baseSession = (id = 's1'): Session => ({
  id,
  session_type: 'chat',
  kilns: ['/kilns/main'],
  workspace: '/kilns/main',
  state: 'active',
  title: null,
  agent_model: null,
  agent_mode: null,
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
});

beforeEach(() => {
  // The mode list is a query now, and its cache outlives one case. A fresh
  // client per case keeps one session's answer from serving the next one.
  setQueryClientForTests(createTestQueryClient());
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  setCurrentSession(undefined);
  __resetReviewStore();
  setQueryClientForTests(null);
});

describe('SessionStatusChips', () => {
  it('renders a slot from a plugin it has never heard of', async () => {
    // The anti-regression test for the generic-rendering rule: nothing in the
    // frontend knows what these keys mean. If a new plugin ever needs a code
    // change here to show up, the channel stopped being generic.
    setCurrentSession(baseSession());
    getSessionStatusMock.mockResolvedValue([
      { key: 'zarquon', plugin: 'zarquon', text: 'flux capacitor charged', level: 'info' },
    ]);

    render(() => <SessionStatusChips />);

    const chip = await waitFor(() => screen.getByTestId('session-status-zarquon'));
    expect(chip.textContent).toContain('flux capacitor charged');
    // The owning plugin is attributed, not interpreted.
    expect(chip.getAttribute('title')).toContain('zarquon');
  });

  it('styles by level and falls back for a level it does not enumerate', async () => {
    setCurrentSession(baseSession());
    getSessionStatusMock.mockResolvedValue([
      { key: 'a', plugin: 'p', text: 'fine', level: 'info' },
      { key: 'b', plugin: 'p', text: 'careful', level: 'warn' },
      { key: 'c', plugin: 'p', text: 'broken', level: 'error' },
      { key: 'd', plugin: 'p', text: 'nautical', level: 'chartreuse' },
    ]);

    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status-d')).toBeInTheDocument());

    const cls = (key: string) => screen.getByTestId(`session-status-${key}`).className;
    expect(cls('a')).not.toBe(cls('b'));
    expect(cls('b')).not.toBe(cls('c'));
    // An unknown level renders — quietly, like info — rather than vanishing.
    expect(screen.getByTestId('session-status-d').textContent).toContain('nautical');
  });

  it('renders nothing when the session published no slots', async () => {
    setCurrentSession(baseSession());
    getSessionStatusMock.mockResolvedValue([]);
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(getSessionStatusMock).toHaveBeenCalled());
    expect(screen.queryByTestId('session-status')).toBeNull();
  });

  it('a failed fetch is silence, not an error banner', async () => {
    // Every daemon reconnect fails this request; a session with no chips is
    // the normal case, so a failure must look like one.
    setCurrentSession(baseSession());
    getSessionStatusMock.mockRejectedValue(new Error('HTTP 502'));
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(getSessionStatusMock).toHaveBeenCalled());
    expect(screen.queryByTestId('session-status')).toBeNull();
  });

  it('reads the one mode list the chat pane asked for', async () => {
    // The gate: the chips held a second `createResource` over `listModes` and
    // refetched the list on every mount, beside the chat pane's own read.
    setCurrentSession(baseSession());
    getSessionStatusMock.mockResolvedValue([]);

    render(() => (
      <ChatProvider sessionId="s1">
        <SessionStatusChips />
      </ChatProvider>
    ));

    await waitFor(() => expect(listModesMock).toHaveBeenCalled());
    // Both readers have bound; a second request would have been made by now.
    for (let tick = 0; tick < 3; tick += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    expect(listModesMock).toHaveBeenCalledTimes(1);
  });

  it('drops the previous session slots BEFORE the new fetch resolves', async () => {
    // The second session's fetch is held open on purpose. Letting it resolve
    // with `[]` clears the chips on its own, so the assertion passed with the
    // clear-on-change deleted — it has to be made during the gap, which is the
    // entire window in which a stale "sandboxed" chip could sit over a session
    // that is not sandboxed.
    let releaseSecond: (slots: unknown[]) => void = () => {};
    getSessionStatusMock.mockImplementation(async (id: string) => {
      if (id === 's1') return [{ key: 'a', plugin: 'p', text: 'first', level: 'info' }];
      return new Promise((resolve) => {
        releaseSecond = resolve as (slots: unknown[]) => void;
      });
    });

    setCurrentSession(baseSession('s1'));
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status-a')).toBeInTheDocument());

    setCurrentSession(baseSession('s2'));
    await waitFor(() => expect(getSessionStatusMock).toHaveBeenCalledWith('s2'));
    expect(screen.queryByTestId('session-status-a')).toBeNull();

    // ...and the in-flight answer still lands when it finally arrives.
    releaseSecond([{ key: 'b', plugin: 'p', text: 'second', level: 'info' }]);
    await waitFor(() => expect(screen.getByTestId('session-status-b')).toBeInTheDocument());
  });
});
