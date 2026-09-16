import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import type { Session, ChatEvent } from '@/lib/types';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { ReviewAwareMode } from '@/lib/review-types';

const [currentSession, setCurrentSession] = createSignal<Session | undefined>(undefined);
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ currentSession }),
}));

// Only `subscribeToEvents` is stubbed, and only because the review store
// still opens its own stream. The status slots and the mode list are read
// through `lib/query/`, so they answer the ROUTES below — which is what proves
// the chips ask about the session they are pointed at.
const handlers: ((e: ChatEvent) => void)[] = [];
vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  subscribeToEvents: (_id: string, onEvent: (e: ChatEvent) => void) => {
    handlers.push(onEvent);
    return () => {};
  },
}));

const listReviewHunks = vi.fn();
vi.mock('@/lib/review-api', () => ({
  listReviewHunks: (...a: unknown[]) => listReviewHunks(...a),
  setHunkState: vi.fn(),
  addReviewComment: vi.fn(),
  resolveReviewComment: vi.fn(),
}));

const { SessionStatusChips } = await import('../SessionStatusChips');
const { __resetReviewStore } = await import('@/lib/review-store');

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
});

const MODES = 'GET /api/session/s1/modes';

let env: TestQueryEnv;
/** What the mode route answers this case, which each test names up front. */
let modeReply: { current_mode_id: string; modes: ReviewAwareMode[] };
/** Holds the SECOND session's mode list open, for the case that needs a gap. */
let secondSessionModes: Promise<void>;

const modes = (current: string, ...list: ReviewAwareMode[]) => {
  modeReply = { current_mode_id: current, modes: list };
};

const mode = (
  id: string,
  review_policy: ReviewAwareMode['review_policy'] = 'none',
): ReviewAwareMode => ({
  id,
  name: id,
  description: null,
  icon: null,
  color: null,
  review_policy,
});

beforeEach(() => {
  handlers.length = 0;
  listReviewHunks.mockResolvedValue({ session_id: 's1', hunks: [], comments: [] });
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
      return { current_mode_id: 'plan', modes: [mode('plan', 'none')] };
    },
  });
});

afterEach(() => {
  cleanup();
  setCurrentSession(undefined);
  __resetReviewStore();
  env?.restore();
  vi.clearAllMocks();
});

describe('SessionStatusChips — effective review policy', () => {
  it('renders the policy the daemon says is IN FORCE', async () => {
    modes('ask', mode('ask', 'pre_write'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    // The policy is no longer a chip a reader must decode; it rides the
    // wrapper as data for tests and plugins.
    const wrap = await waitFor(() => screen.getByTestId('session-status'));
    await waitFor(() => expect(wrap.dataset.reviewPolicy).toBe('pre_write'));
  });

  it('an ACP session in normal mode reads "review at turn end", not "gated"', async () => {
    // The daemon degrades pre_write to post_turn for an external agent, whose
    // tools run in its own process — a chip reading "gated" there would be a
    // lie about a safety property. Nothing here re-derives it from the mode id,
    // which is the only way that lie could get told.
    modes('ask', mode('ask', 'post_turn'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    const wrap = await waitFor(() => screen.getByTestId('session-status'));
    await waitFor(() => expect(wrap.dataset.reviewPolicy).toBe('post_turn'));
  });

  it('a mode with no gate gets no chip', async () => {
    modes('plan', mode('plan', 'none'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(env.fetch.calls(MODES)).toBe(1));
    expect(screen.queryByTestId('session-review-policy')).toBeNull();
  });

  it('a daemon that predates the field gets no chip rather than a guessed one', async () => {
    modes('ask', mode('ask'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(env.fetch.calls(MODES)).toBe(1));
    expect(screen.queryByTestId('session-review-policy')).toBeNull();
  });

  it('drops the previous session policy before the new one answers', async () => {
    modes('ask', mode('ask', 'pre_write'));
    setCurrentSession(session('s1'));
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status').dataset.reviewPolicy).toBeDefined());

    // The second session's mode list is held open on purpose: the assertion
    // has to fall inside the window a stale policy could sit in.
    let release: () => void = () => {};
    secondSessionModes = new Promise<void>((resolve) => {
      release = resolve;
    });

    setCurrentSession(session('s2'));
    await waitFor(() => expect(screen.queryByTestId('session-status')?.dataset.reviewPolicy).toBeUndefined());
    release();
  });
});

describe('SessionStatusChips — waiting on review', () => {
  const gate = (blocked: boolean, extra: Record<string, unknown> = {}) =>
    handlers[0]({
      type: 'session_event',
      event: 'review_gate',
      data: { blocked, tool: 'Edit', path: '/repo/src/a.rs', ...extra },
    });

  it('a held agent gets a loud chip naming what it is waiting on', async () => {
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(handlers).toHaveLength(1));

    gate(true);
    const chip = await waitFor(() => screen.getByTestId('session-review-gate'));
    // A blocked agent must never read as a stalled one.
    expect(chip.textContent).toContain('waiting on review');
    expect(chip.getAttribute('title')).toContain('/repo/src/a.rs');
    expect(chip.getAttribute('title')).toContain('Edit');
  });

  it('the chip clears when the gate releases', async () => {
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(handlers).toHaveLength(1));

    gate(true);
    await waitFor(() => expect(screen.getByTestId('session-review-gate')).toBeInTheDocument());
    gate(false);
    await waitFor(() => expect(screen.queryByTestId('session-review-gate')).toBeNull());
  });

  it('counts what is owed, ignoring the user’s own external edits', async () => {
    listReviewHunks.mockResolvedValue({
      session_id: 's1',
      hunks: [
        {
          id: 'a',
          root: '/repo',
          path: 'src/a.rs',
          base_range: { start: 1, end: 2 },
          current_range: { start: 1, end: 2 },
          before_content: '',
          after_content: 'x\n',
          tool_call_ids: ['c1'],
          state: 'unreviewed',
        },
        {
          id: 'b',
          root: '/repo',
          path: 'src/a.rs',
          base_range: { start: 9, end: 10 },
          current_range: { start: 9, end: 10 },
          before_content: '',
          after_content: 'y\n',
          tool_call_ids: [],
          state: 'unreviewed',
        },
      ],
      comments: [],
    });
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(handlers).toHaveLength(1));
    gate(true);

    const chip = await waitFor(() => screen.getByTestId('session-review-gate'));
    await waitFor(() => expect(chip.textContent).toContain('(1)'));
  });

  it('no session means no chips at all', () => {
    render(() => <SessionStatusChips />);
    expect(screen.queryByTestId('session-status')).toBeNull();
    expect(env.fetch.calls(MODES)).toBe(0);
  });
});
