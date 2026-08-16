import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';
import type { Session } from '@/lib/types';
import type { ComposedHunk, ReviewComment } from '@/lib/review-types';

const [currentSession, setCurrentSession] = createSignal<Session | undefined>(undefined);
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ currentSession }),
}));

// The panel lives in the RIGHT edge region, outside any ChatProvider — its
// only inputs are SessionContext and its own event stream.
vi.mock('@/lib/api', () => ({
  subscribeToEvents: () => () => {},
}));

const listReviewHunks = vi.fn();
const setHunkState = vi.fn(async () => ({
  hunk_id: 'h',
  state: 'accepted' as const,
}));
const addReviewComment = vi.fn(async () => ({ comment: {} }));
const resolveReviewComment = vi.fn(async () => ({ comment_id: 'c1' }));
const rebaseReview = vi.fn(async () => ({ roots: [] }));
vi.mock('@/lib/review-api', () => ({
  listReviewHunks: (...a: unknown[]) => listReviewHunks(...a),
  setHunkState: (...a: unknown[]) => setHunkState(...(a as [])),
  addReviewComment: (...a: unknown[]) => addReviewComment(...(a as [])),
  resolveReviewComment: (...a: unknown[]) => resolveReviewComment(...(a as [])),
  rebaseReview: (...a: unknown[]) => rebaseReview(...(a as [])),
}));

// DiffViewer pulls in Shiki; the panel's job is the queue, not tokenization.
vi.mock('../DiffViewer', () => ({
  DiffViewer: (props: { oldContent: string; newContent: string }) => (
    <div data-testid="diff-viewer">
      old:{props.oldContent}|new:{props.newContent}
    </div>
  ),
}));

const openFileInEditor = vi.fn();
vi.mock('@/lib/file-actions', () => ({
  openFileInEditor: (...a: unknown[]) => openFileInEditor(...a),
}));

const { ChangesPanel } = await import('../ChangesPanel');
const { __resetReviewStore, pendingReveal } = await import('@/lib/review-store');

function hunk(over: Partial<ComposedHunk> = {}): ComposedHunk {
  return {
    id: 'h1',
    root: '/repo',
    path: 'src/a.rs',
    base_range: { start: 4, end: 6 },
    current_range: { start: 4, end: 6 },
    before_content: 'old\n',
    after_content: 'new\n',
    tool_call_ids: ['call-1'],
    state: 'unreviewed',
    reapplied: false,
    ...over,
  };
}

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

const answer = (
  hunks: ComposedHunk[],
  comments: ReviewComment[] = [],
  extra: Record<string, unknown> = {},
) =>
  listReviewHunks.mockImplementation(async () => ({
    session_id: 's1',
    hunks: structuredClone(hunks),
    comments: structuredClone(comments),
    ...structuredClone(extra),
  }));

beforeEach(() => {
  resetGlobalRegistry();
  answer([]);
  // clearMocks wipes call history, not implementations — the in-flight test
  // installs one that never settles.
  setHunkState.mockReset();
  setHunkState.mockResolvedValue({ hunk_id: 'h', state: 'accepted' });
});

afterEach(() => {
  cleanup();
  setCurrentSession(undefined);
  __resetReviewStore();
  vi.clearAllMocks();
});

describe('ChangesPanel — registration', () => {
  it('registers "changes" in the right region, beside Activity and Backlinks', () => {
    registerPanels();
    const panel = getGlobalRegistry().get('changes');
    expect(panel).toBeDefined();
    expect(panel!.title).toBe('Changes');
    expect(panel!.defaultZone).toBe('right');
    expect(getGlobalRegistry().get('activity')!.defaultZone).toBe('right');
  });
});

describe('ChangesPanel — the queue', () => {
  it('says so when no session is selected', () => {
    render(() => <ChangesPanel />);
    expect(screen.getByText('No session selected.')).toBeInTheDocument();
    expect(listReviewHunks).not.toHaveBeenCalled();
  });

  it('groups roots → files → hunks and counts what is owed', async () => {
    answer([
      hunk({ id: 'a' }),
      hunk({ id: 'b', current_range: { start: 20, end: 21 } }),
      hunk({ id: 'c', path: 'src/b.rs' }),
      hunk({ id: 'd', root: '/other', path: 'x.md', state: 'accepted' }),
    ]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('hunk-a')).toBeInTheDocument());
    expect(screen.getByTestId('changes-file-src/a.rs')).toBeInTheDocument();
    expect(screen.getByTestId('changes-file-src/b.rs')).toBeInTheDocument();
    expect(screen.getByTestId('changes-file-x.md')).toBeInTheDocument();
    // Both roots get a header of their own.
    expect(screen.getByTitle('/repo')).toBeInTheDocument();
    expect(screen.getByTitle('/other')).toBeInTheDocument();
    expect(screen.getByTestId('changes-count').textContent).toContain('3 unreviewed');
    expect(screen.getByTestId('changes-count').textContent).toContain('4 total');
  });

  it('external hunks render for context with NO reject affordance', async () => {
    answer([hunk({ id: 'ext', tool_call_ids: [] })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('hunk-ext')).toBeInTheDocument());
    expect(screen.getByTestId('hunk-external')).toBeInTheDocument();
    // Reverting it would destroy the user's own edit while reporting that an
    // agent edit was undone.
    expect(screen.queryByTestId('reject-ext')).toBeNull();
    // Accepting is still meaningful: it clears it from the queue.
    expect(screen.getByTestId('accept-ext')).toBeInTheDocument();
    // ...and it is not counted as work the agent owes.
    expect(screen.getByTestId('changes-count').textContent).toContain('0 unreviewed');
  });

  it('accept reaches the daemon with the hunk id', async () => {
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('accept-h1'));
    await waitFor(() => expect(setHunkState).toHaveBeenCalledWith('s1', 'h1', 'accepted'));
  });

  it('reject is a state, not a separate verb — the daemon reverts on the same call', async () => {
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-h1'));
    await waitFor(() => expect(setHunkState).toHaveBeenCalledWith('s1', 'h1', 'rejected'));
  });

  it('a hunk in flight refuses a second click', async () => {
    answer([hunk({ id: 'h1' })]);
    let release: () => void = () => {};
    setHunkState.mockImplementation(
      () => new Promise<never>((_, __) => (release = () => _(undefined as never))),
    );
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('accept-h1'));
    await waitFor(() => expect(screen.getByTestId('reject-h1')).toBeDisabled());
    fireEvent.click(screen.getByTestId('reject-h1'));
    expect(setHunkState).toHaveBeenCalledTimes(1);
    release();
  });

  it('the unreviewed filter drains to "nothing left", not to an empty panel', async () => {
    answer([hunk({ id: 'done', state: 'accepted' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-done')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('changes-filter-unreviewed'));
    await waitFor(() => expect(screen.getByTestId('changes-all-reviewed')).toBeInTheDocument());
    // The changes are still there to browse — a drained queue is not a
    // completion state, so there is nothing to dismiss and no mode to leave.
    expect(screen.queryByTestId('changes-empty')).toBeNull();
  });

  it('the filter shows exactly what the badge says is owed', async () => {
    answer([hunk({ id: 'mine' }), hunk({ id: 'theirs', tool_call_ids: [] })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-theirs')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('changes-filter-unreviewed'));
    await waitFor(() => expect(screen.queryByTestId('hunk-theirs')).toBeNull());
    expect(screen.getByTestId('hunk-mine')).toBeInTheDocument();
    expect(screen.getByTestId('changes-count').textContent).toContain('1 unreviewed');
  });

  it('a session with no changes says so once the list has answered', async () => {
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('changes-empty')).toBeInTheDocument());
  });

  it('a failed list surfaces the daemon message', async () => {
    listReviewHunks.mockRejectedValue(new Error('nothing to review'));
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() =>
      expect(screen.getByTestId('changes-error').textContent).toContain('nothing to review'),
    );
  });

  it('clicking a file opens it AND asks the buffer to scroll to the first hunk', async () => {
    answer([hunk({ id: 'h1', current_range: { start: 12, end: 14 } })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('changes-file-src/a.rs'));
    expect(openFileInEditor).toHaveBeenCalledWith('/repo/src/a.rs', 'a.rs');
    // Tab metadata cannot carry this — an already-open panel never re-reads it.
    expect(pendingReveal()).toEqual({ path: '/repo/src/a.rs', line: 12 });
  });

  it('expanding a hunk shows its composed before/after through the one DiffViewer', async () => {
    answer([hunk({ id: 'h1', before_content: 'a\n', after_content: 'b\n' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());
    expect(screen.queryByTestId('diff-viewer')).toBeNull();

    fireEvent.click(screen.getByTestId('hunk-h1').querySelector('button')!);
    await waitFor(() =>
      expect(screen.getByTestId('diff-viewer').textContent).toBe('old:a\n|new:b\n'),
    );
  });

  it('comments a range, not a hunk id', async () => {
    answer([hunk({ id: 'h1', current_range: { start: 8, end: 11 } })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('comment-h1'));
    const box = await waitFor(() => screen.getByTestId('comment-body-h1'));
    fireEvent.input(box, { target: { value: 'use a slice here' } });
    fireEvent.click(screen.getByTestId('comment-submit-h1'));

    await waitFor(() =>
      expect(addReviewComment).toHaveBeenCalledWith('s1', {
        root: '/repo',
        path: 'src/a.rs',
        line_start: 8,
        line_end: 11,
        body: 'use a slice here',
      }),
    );
  });

  it('lists open comments and resolves them', async () => {
    const comment: ReviewComment = {
      id: 'c1',
      root: '/repo',
      path: 'src/a.rs',
      base_tree: 'abc',
      line_range: { start: 3, end: 4 },
      body: 'why?',
      author: 'human',
      resolved: false,
      created_at: '2026-01-01T00:00:00Z',
    };
    answer([], [comment, { ...comment, id: 'c2', resolved: true }]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('comment-c1')).toBeInTheDocument());
    // A resolved comment is history, not queue.
    expect(screen.queryByTestId('comment-c2')).toBeNull();

    fireEvent.click(screen.getByTestId('resolve-c1'));
    await waitFor(() => expect(resolveReviewComment).toHaveBeenCalledWith('s1', 'c1'));
  });
});

describe('ChangesPanel — degradation', () => {
  // A degraded root contributes ZERO hunks while the gate holds every write
  // under it, so the panel drew "No changes in this session yet" for exactly
  // the state in which nothing can proceed — no reason, no root, no release.
  it('a degraded root is named instead of reported as an empty queue', async () => {
    answer([], [], {
      degraded: [
        {
          root: '/repo',
          degraded: 'session base tree deadbeef is no longer in the object store',
        },
      ],
    });
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('changes-degraded')).toBeInTheDocument());
    expect(screen.queryByTestId('changes-empty')).toBeNull();
    expect(screen.getByTestId('changes-degraded').textContent).toContain('/repo');
    expect(screen.getByTestId('changes-degraded').textContent).toContain('object store');
  });

  // The worst loss names no root at all: a `review.jsonl` that will not read
  // leaves nothing that can identify a repository, so `degraded` comes back
  // EMPTY while the gate holds everything.
  it('an unscoped journal loss is surfaced even though it names no root', async () => {
    answer([], [], {
      degraded: [],
      integrity: {
        skips: [
          {
            record: { kind: 'session' },
            line: 0,
            reason: 'review journal unreadable',
          },
        ],
      },
    });
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('changes-degraded')).toBeInTheDocument());
    expect(screen.getByTestId('changes-degraded').textContent).toContain(
      'review journal unreadable',
    );
  });

  // A lost comment costs no safety property; a banner for it would be noise
  // that teaches people to ignore the banner that matters.
  it('an informational skip does not claim the session is blocked', async () => {
    answer([], [], {
      integrity: {
        skips: [
          {
            record: { kind: 'informational' },
            line: 4,
            reason: 'truncated comment',
          },
        ],
      },
    });
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('changes-empty')).toBeInTheDocument());
    expect(screen.queryByTestId('changes-degraded')).toBeNull();
  });

  // The only release. Reviewing cannot clear a degraded root — it contributes
  // no hunks — and for a while `review.rebase` existed as a daemon method with
  // no client anywhere, reachable only by hand-written JSON-RPC.
  it('offers the rebase that is the only way out of the block', async () => {
    answer([], [], {
      degraded: [{ root: '/repo', degraded: 'tracked root no longer exists' }],
    });
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('changes-rebase')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('changes-rebase'));
    await waitFor(() => expect(rebaseReview).toHaveBeenCalledWith('s1'));
  });
});

describe('ChangesPanel — re-applied changes', () => {
  // The flag's entire justification is visibility: a change the user already
  // rejected, applied again, reports `state: 'unreviewed'` and is otherwise
  // indistinguishable from first-time work — which is the accept-out-of-fatigue
  // outcome it was introduced to prevent.
  it('marks a change the agent re-applied after a rejection', async () => {
    answer([hunk({ id: 'h1' }), { ...hunk({ id: 'h2' }), reapplied: true }]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('hunk-h2')).toBeInTheDocument());
    expect(screen.getByTestId('hunk-reapplied-h2')).toBeInTheDocument();
    expect(screen.queryByTestId('hunk-reapplied-h1')).toBeNull();
  });
});
