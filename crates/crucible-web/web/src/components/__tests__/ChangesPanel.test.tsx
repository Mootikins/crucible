import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent, within } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';
import { proposalFixture, proposalRoutes } from '@/test-utils/proposals';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';
import type { Session } from '@/lib/types';
import type { ComposedHunk, ReviewComment } from '@/lib/review-types';
import type { Conflicted } from '@/lib/offline/outbox';

const [currentSession, setCurrentSession] = createSignal<Session | undefined>(undefined);
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ currentSession }),
}));

// The panel lives in the RIGHT edge region, outside any ChatProvider — its
// only inputs are SessionContext and its own event stream, which the
// `FakeEventSource` of `beforeEach` answers through the REAL
// `subscribeToEvents`.

// The shell is decided once at page load; the test stages it before a render.
const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));

const listReviewHunks = vi.fn();
const addReviewComment = vi.fn(async () => ({ comment: {} }));
const resolveReviewComment = vi.fn(async () => ({ comment_id: 'c1' }));
vi.mock('@/lib/review-api', () => ({
  listReviewHunks: (...a: unknown[]) => listReviewHunks(...a),
  addReviewComment: (...a: unknown[]) => addReviewComment(...(a as [])),
  resolveReviewComment: (...a: unknown[]) => resolveReviewComment(...(a as [])),
}));

const openFileInEditor = vi.fn();
vi.mock('@/lib/file-actions', () => ({
  openFileInEditor: (...a: unknown[]) => openFileInEditor(...a),
}));

// A conflict is a note write waiting on a person, read from the outbox. The
// panel lists it; the conflict view itself opens as a tab.
const conflicts = vi.hoisted(() => ({ rows: [] as Conflicted[] }));
vi.mock('@/lib/offline/sync', () => ({
  pendingConflicts: async () => conflicts.rows,
  resolveConflict: async () => ({ queued: false, stale: false, hash: 'h' }),
}));
const openPanelTab = vi.fn();
const openDiff = vi.fn();
vi.mock('@/lib/panel-actions', () => ({
  openPanelTab: (id: string) => openPanelTab(id),
  openDiff: (source: unknown) => openDiff(source),
}));

// A refused comment is reported in a toast.
const addNotification = vi.fn();
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotification(...a) },
}));

const { ChangesPanel } = await import('../ChangesPanel');
const { __resetReviewStore, pendingReveal } = await import('@/lib/review-store');
const { __resetConflictStore, conflictStore } = await import('@/lib/conflicts');

/** One note whose write the daemon could neither take nor merge. */
const conflict = (path: string): Conflicted => ({
  path,
  base: 'h0',
  kiln: '/repo',
  currentHash: 'h9',
  currentContent: 'theirs\n',
  mergedContent: 'mine\n',
  regions: [{ start_line: 1, end_line: 2, base: '', ours: 'mine\n', theirs: 'theirs\n' }],
});

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

/**
 * A fresh cache per case.
 *
 * The listing is a cache entry now, so without this the empty answer of the
 * first case is still fresh five minutes later and every case after it reads
 * that instead of its own.
 */
let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({});
  resetGlobalRegistry();
  conflicts.rows = [];
  __resetConflictStore();
  answer([]);
});

afterEach(() => {
  cleanup();
  setCurrentSession(undefined);
  device.compact = false;
  __resetReviewStore();
  env.restore();
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

  it('external hunks render for context and are not counted as owed', async () => {
    answer([hunk({ id: 'ext', tool_call_ids: [] })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('hunk-ext')).toBeInTheDocument());
    expect(screen.getByTestId('hunk-external')).toBeInTheDocument();
    // It is not counted as work the agent owes.
    expect(screen.getByTestId('changes-count').textContent).toContain('0 unreviewed');
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

  it('the turn scope asks the daemon for the turn', async () => {
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());
    expect(listReviewHunks).toHaveBeenLastCalledWith('s1', 'session');
    expect(screen.getByTestId('changes-scope-session').getAttribute('aria-pressed')).toBe('true');

    // The daemon decides what the turn holds; the panel only asks.
    answer([]);
    fireEvent.click(screen.getByTestId('changes-scope-turn'));
    await waitFor(() => expect(listReviewHunks).toHaveBeenLastCalledWith('s1', 'turn'));
    expect(screen.getByTestId('changes-scope-turn').getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByTestId('changes-scope-session').getAttribute('aria-pressed')).toBe('false');
    // The empty state names the scope it is empty under.
    await waitFor(() => expect(screen.getByTestId('changes-empty')).toBeInTheDocument());
    expect(screen.getByTestId('changes-empty').textContent).toContain('turn');

    fireEvent.click(screen.getByTestId('changes-scope-session'));
    await waitFor(() => expect(listReviewHunks).toHaveBeenLastCalledWith('s1', 'session'));
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

  // The daemon no longer accepts or reverts a hunk, so neither the row nor
  // the merge view offers a decision.
  it('an expanded hunk mounts the merge view with no decision controls', async () => {
    answer([hunk({ id: 'h1', before_content: 'a\n', after_content: 'b\n' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());
    expect(screen.queryByTestId('hunk-merge')).toBeNull();
    expect(screen.queryByTestId('accept-h1')).toBeNull();
    expect(screen.queryByTestId('reject-h1')).toBeNull();
    expect(screen.queryByTestId('changes-accept-all')).toBeNull();
    expect(screen.queryByTestId('changes-reject-all')).toBeNull();

    fireEvent.click(screen.getByTestId('hunk-h1').querySelector('button')!);
    const merge = await waitFor(() => screen.getByTestId('hunk-merge'));
    // The merge view shows the hunk as the daemon composed it.
    await waitFor(() => expect(merge.textContent).toContain('b'));
    expect(merge.textContent).toContain('a');
    expect(within(merge).queryByRole('button', { name: 'Accept' })).toBeNull();
    expect(within(merge).queryByRole('button', { name: 'Reject' })).toBeNull();
  });

  it('on a compact shell every hunk control is at least 44 px', async () => {
    device.compact = true;
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());
    const comment = screen.getByTestId('comment-h1');
    expect(comment.className).toContain('min-h-11');
    expect(comment.className).toContain('min-w-11');
  });

  it('on the desktop shell the hunk controls keep their dense size', async () => {
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());
    expect(screen.getByTestId('comment-h1').className).not.toContain('min-h-11');
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
      diffset: 'session-s1',
      root: '/repo',
      path: 'src/a.rs',
      anchor: { kind: 'snapshot', id: 'abc' },
      side: 'current',
      line_range: { start: 3, end: 4 },
      quoted: 'x\n',
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

describe('ChangesPanel — conflicts', () => {
  // The panel is the disposition surface for a session's writing, and a
  // conflict is a write nobody has disposed of. It goes ABOVE the hunks: it is
  // the one row here that cannot drain on its own.
  it('lists a conflicted note above the hunks', async () => {
    conflicts.rows = [conflict('/repo/notes/A.md')];
    answer([hunk({ id: 'a' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    const section = await screen.findByTestId('changes-conflicts');
    expect(within(section).getByText('/repo/notes/A.md')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId('hunk-a')).toBeInTheDocument());
    expect(section.compareDocumentPosition(screen.getByTestId('hunk-a'))).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
  });

  // A conflict belongs to no session's composed diff, and the desktop has no
  // offline badge; listing one only under a selected session would leave a
  // write nothing on this shell mentions.
  it('lists a conflict with no session selected', async () => {
    conflicts.rows = [conflict('/repo/notes/A.md')];
    render(() => <ChangesPanel />);

    expect(await screen.findByTestId('changes-conflicts')).toBeInTheDocument();
    expect(screen.getByText('No session selected.')).toBeInTheDocument();
  });

  it('opens the conflict a row names', async () => {
    conflicts.rows = [conflict('/repo/notes/A.md')];
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    fireEvent.click(await screen.findByTestId('changes-conflict-open-/repo/notes/A.md'));
    expect(openPanelTab).toHaveBeenCalledWith('conflicts');
    expect(conflictStore.selected()).toBe('/repo/notes/A.md');
  });
});

/**
 * A stale or conflicted proposal needs the user as a conflict does, so the
 * panel lists it beside the outbox conflicts. An open proposal waits in the
 * Inbox only, and a superseded one waits for no decision.
 */
describe('ChangesPanel — proposals', () => {
  const OPEN = '7a1c2f3e-0000-4000-8000-000000000001';
  const STALE = '7a1c2f3e-0000-4000-8000-000000000002';
  const CONFLICTED = '7a1c2f3e-0000-4000-8000-000000000003';
  const SUPERSEDED = '7a1c2f3e-0000-4000-8000-000000000004';

  it('lists stale and conflicted proposals', async () => {
    env.restore();
    env = createTestQueryEnv(
      proposalRoutes([
        proposalFixture(OPEN, { kind: 'open' }),
        proposalFixture(STALE, { kind: 'stale' }, { title: 'Stale change' }),
        proposalFixture(CONFLICTED, { kind: 'conflicted', files: [] }, { title: 'Conflicted change' }),
        proposalFixture(SUPERSEDED, { kind: 'superseded', by: OPEN }),
      ]),
    );
    render(() => <ChangesPanel />);

    const section = await screen.findByTestId('changes-proposals');
    expect(within(section).getByText('Stale change')).toBeInTheDocument();
    expect(within(section).getByText('Conflicted change')).toBeInTheDocument();
    expect(screen.getByTestId(`changes-proposal-state-${STALE}`).textContent).toBe('stale');
    expect(screen.getByTestId(`changes-proposal-state-${CONFLICTED}`).textContent).toBe(
      'conflicted',
    );
    expect(screen.queryByTestId(`changes-proposal-${OPEN}`)).toBeNull();
    expect(screen.queryByTestId(`changes-proposal-${SUPERSEDED}`)).toBeNull();

    fireEvent.click(screen.getByTestId(`changes-proposal-open-${CONFLICTED}`));
    expect(openDiff).toHaveBeenCalledWith({ kind: 'proposal', id: CONFLICTED });
  });

  it('draws no proposal section when no proposal needs a merge', async () => {
    env.restore();
    const served = createTestQueryEnv(proposalRoutes([proposalFixture(OPEN, { kind: 'open' })]));
    env = served;
    render(() => <ChangesPanel />);

    await waitFor(() => expect(served.fetch.calls('GET /api/proposals')).toBe(1));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByTestId('changes-proposals')).toBeNull();
  });
});
