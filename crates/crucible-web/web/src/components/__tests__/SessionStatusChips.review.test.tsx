import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import type { Session, ChatEvent } from '@/lib/types';
import type { ReviewAwareMode } from '@/lib/review-types';

const [currentSession, setCurrentSession] = createSignal<Session | undefined>(undefined);
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ currentSession }),
}));

const handlers: ((e: ChatEvent) => void)[] = [];
const listModes = vi.fn();
vi.mock('@/lib/api', () => ({
  getSessionStatus: vi.fn(async () => []),
  listModes: (...a: unknown[]) => listModes(...a),
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
  id,
  session_type: 'chat',
  kilns: ['/repo'],
  workspace: '/repo',
  state: 'active',
  title: null,
  agent_model: null,
  agent_mode: null,
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
});

const modes = (current: string, ...list: ReviewAwareMode[]) =>
  listModes.mockResolvedValue({ current_mode_id: current, modes: list });

const mode = (id: string, review_policy?: ReviewAwareMode['review_policy']): ReviewAwareMode => ({
  id,
  name: id,
  description: null,
  icon: null,
  color: null,
  ...(review_policy ? { review_policy } : {}),
});

beforeEach(() => {
  handlers.length = 0;
  listReviewHunks.mockResolvedValue({ session_id: 's1', hunks: [], comments: [] });
  modes('normal', mode('normal'));
});

afterEach(() => {
  cleanup();
  setCurrentSession(undefined);
  __resetReviewStore();
  vi.clearAllMocks();
});

describe('SessionStatusChips — effective review policy', () => {
  it('renders the policy the daemon says is IN FORCE', async () => {
    modes('normal', mode('normal', 'pre_write'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    const chip = await waitFor(() => screen.getByTestId('session-review-policy'));
    expect(chip.textContent).toBe('gated');
  });

  it('an ACP session in normal mode reads "review at turn end", not "gated"', async () => {
    // The daemon degrades pre_write to post_turn for an external agent, whose
    // tools run in its own process — a chip reading "gated" there would be a
    // lie about a safety property. Nothing here re-derives it from the mode id,
    // which is the only way that lie could get told.
    modes('normal', mode('normal', 'post_turn'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    const chip = await waitFor(() => screen.getByTestId('session-review-policy'));
    expect(chip.textContent).toBe('review at turn end');
  });

  it('a mode with no gate gets no chip', async () => {
    modes('plan', mode('plan', 'none'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(listModes).toHaveBeenCalled());
    expect(screen.queryByTestId('session-review-policy')).toBeNull();
  });

  it('a daemon that predates the field gets no chip rather than a guessed one', async () => {
    modes('normal', mode('normal'));
    setCurrentSession(session());
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(listModes).toHaveBeenCalled());
    expect(screen.queryByTestId('session-review-policy')).toBeNull();
  });

  it('drops the previous session policy before the new one answers', async () => {
    modes('normal', mode('normal', 'pre_write'));
    setCurrentSession(session('s1'));
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-review-policy')).toBeInTheDocument());

    let release: (v: unknown) => void = () => {};
    listModes.mockImplementation(() => new Promise((r) => (release = r as (v: unknown) => void)));
    setCurrentSession(session('s2'));
    await waitFor(() => expect(screen.queryByTestId('session-review-policy')).toBeNull());
    release({ current_mode_id: 'plan', modes: [mode('plan', 'none')] });
  });
});

describe('SessionStatusChips — waiting on review', () => {
  const gate = (blocked: boolean, extra: Record<string, unknown> = {}) =>
    handlers[0]({
      type: 'session_event',
      event_type: 'review_gate',
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
    expect(listModes).not.toHaveBeenCalled();
  });
});
