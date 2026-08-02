import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { SessionStatusChips } from '../SessionStatusChips';
import type { Session } from '@/lib/types';

const [currentSession, setCurrentSession] = createSignal<Session | undefined>(undefined);
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ currentSession }),
}));

const getSessionStatusMock = vi.fn();
vi.mock('@/lib/api', () => ({
  getSessionStatus: (...args: unknown[]) => getSessionStatusMock(...args),
}));

const baseSession = (id = 's1'): Session => ({
  id,
  session_type: 'chat',
  kiln: '/kilns/main',
  workspace: '/kilns/main',
  connected_kilns: [],
  state: 'active',
  title: null,
  agent_model: null,
  agent_mode: null,
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  setCurrentSession(undefined);
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

  it('refetches when the session changes and drops the previous session slots', async () => {
    setCurrentSession(baseSession('s1'));
    getSessionStatusMock.mockImplementation(async (id: string) =>
      id === 's1' ? [{ key: 'a', plugin: 'p', text: 'first', level: 'info' }] : [],
    );

    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status-a')).toBeInTheDocument());

    // Switching sessions must not leave the old session's chips on screen —
    // a stale "sandboxed" chip on an unsandboxed session is a lie.
    setCurrentSession(baseSession('s2'));
    await waitFor(() => expect(getSessionStatusMock).toHaveBeenCalledWith('s2'));
    await waitFor(() => expect(screen.queryByTestId('session-status-a')).toBeNull());
  });
});
