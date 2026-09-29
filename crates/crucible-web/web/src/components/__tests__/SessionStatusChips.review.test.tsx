import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import type { ModeDescriptor, Session } from '@/lib/types';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

const [currentSession, setCurrentSession] = createSignal<Session | undefined>(undefined);
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ currentSession }),
}));
// No `vi.mock('@/lib/api')`. The status slots and the mode list are read
// through `lib/query/`, so they answer the ROUTES below — which is what proves
// the chips ask about the session they are pointed at.

const { SessionStatusChips } = await import('../SessionStatusChips');

const session = (id = 's1'): Session => ({
  session_id: id,
  type: 'chat',
  kilns: ['/repo'],
  workspace: '/repo',
  state: 'active',
  title: null,
  agent_model: null,
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
  archived: false,
});

const MODES = 'GET /api/session/s1/modes';

let env: TestQueryEnv;
/** What the mode route answers this case, which each test names up front. */
let modeReply: { current_mode_id: string; modes: ModeDescriptor[] };
/** Holds the SECOND session's mode list open, for the case that needs a gap. */
let secondSessionModes: Promise<void>;

const modes = (current: string, ...list: ModeDescriptor[]) => {
  modeReply = { current_mode_id: current, modes: list };
};

const mode = (id: string, writes: ModeDescriptor['writes'] = 'apply'): ModeDescriptor => ({
  id,
  name: id,
  description: null,
  icon: null,
  color: null,
  writes,
});

beforeEach(() => {
  modes('ask', mode('ask'));
  // The mode list is a query, and its cache outlives one case. A fresh client
  // per case keeps one session's answer from serving the next one.
  secondSessionModes = Promise.resolve();
  env = createTestQueryEnv({
    'GET /api/session/s1/status': () => ({ status: [] }),
    'GET /api/session/s2/status': () => ({ status: [] }),
    [MODES]: () => modeReply,
    'GET /api/session/s2/modes': async () => {
      await secondSessionModes;
      return { current_mode_id: 'plan', modes: [mode('plan')] };
    },
  });
});

afterEach(() => {
  cleanup();
  setCurrentSession(undefined);
  env?.restore();
  vi.clearAllMocks();
});

describe('SessionStatusChips — effective write mode', () => {
  it('names the write mode the daemon says is IN FORCE', async () => {
    modes('propose', mode('ask'), mode('propose', 'propose'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    // The write mode is not a chip: the mode control already says
    // "proposes". It rides the wrapper as data for tests and plugins.
    const wrap = await waitFor(() => screen.getByTestId('session-status'));
    await waitFor(() => expect(wrap.dataset.writes).toBe('propose'));
    expect(wrap.dataset.reviewPolicy).toBeUndefined();
  });

  it('an ACP session reads "apply", because the daemon cannot hold its writes', async () => {
    // The daemon degrades `propose` to `apply` for an external agent. Nothing
    // here derives the value from the mode id.
    modes('propose', mode('propose', 'apply'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    const wrap = await waitFor(() => screen.getByTestId('session-status'));
    await waitFor(() => expect(wrap.dataset.writes).toBe('apply'));
  });

  it('drops the previous session write mode before the new one answers', async () => {
    modes('ask', mode('ask'));
    setCurrentSession(session('s1'));
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status').dataset.writes).toBeDefined());

    // The second session's mode list stays open on purpose: the assertion
    // must fall inside the window where a stale value could sit.
    let release: () => void = () => {};
    secondSessionModes = new Promise<void>((resolve) => {
      release = resolve;
    });

    setCurrentSession(session('s2'));
    await waitFor(() =>
      expect(screen.queryByTestId('session-status')?.dataset.writes).toBeUndefined(),
    );
    release();
  });
});

describe('SessionStatusChips — no session', () => {
  it('no session means no chips at all', () => {
    render(() => <SessionStatusChips />);
    expect(screen.queryByTestId('session-status')).toBeNull();
    expect(env.fetch.calls(MODES)).toBe(0);
  });
});
