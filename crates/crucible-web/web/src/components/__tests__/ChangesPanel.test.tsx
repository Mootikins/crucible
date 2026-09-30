import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent, within } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';
import { proposalFixture, proposalRoutes } from '@/test-utils/proposals';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';
import type { Session } from '@/lib/types';
import type { DiffComment, DiffFileEntry, UnreadableRoot } from '@/lib/diffset';
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

// The session record and its comments come from the diffset API.
const getDiffset = vi.fn();
const getDiffComments = vi.fn();
const resolveDiffComment = vi.fn(async () => ({ comment_id: 'c1' }));
vi.mock('@/lib/diff-api', () => ({
  getDiffset: (...a: unknown[]) => getDiffset(...a),
  getDiffComments: (...a: unknown[]) => getDiffComments(...a),
  resolveDiffComment: (...a: unknown[]) => resolveDiffComment(...(a as [])),
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
  openDiff: (...a: unknown[]) => openDiff(...a),
}));

// A refused resolve is reported in a toast.
const addNotification = vi.fn();
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotification(...a) },
}));

const { ChangesPanel } = await import('../ChangesPanel');
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

function file(over: Partial<DiffFileEntry> = {}): DiffFileEntry {
  return {
    root: '/repo',
    path: 'src/a.rs',
    status: { kind: 'modified' },
    added: 2,
    removed: 1,
    binary: false,
    too_large: false,
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
  archived: false,
});

const answer = (
  files: DiffFileEntry[],
  comments: DiffComment[] = [],
  unreadable: UnreadableRoot[] = [],
) => {
  getDiffset.mockImplementation(async (source: { session: string }) => ({
    id: `session-${source.session}`,
    source,
    files: structuredClone(files),
    unreadable_roots: structuredClone(unreadable),
  }));
  getDiffComments.mockImplementation(async () =>
    structuredClone(comments).map((comment) => ({ comment, outdated: false })),
  );
};

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

describe('ChangesPanel — the session record', () => {
  it('says so when no session is selected', () => {
    render(() => <ChangesPanel />);
    expect(screen.getByText('No session selected.')).toBeInTheDocument();
    expect(getDiffset).not.toHaveBeenCalled();
  });

  it('reads the session record diffset of the session', async () => {
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() =>
      expect(getDiffset).toHaveBeenCalledWith({ kind: 'session_record', session: 's1' }),
    );
    expect(getDiffComments).toHaveBeenCalledWith({ kind: 'session_record', session: 's1' });
  });

  it('groups roots → files and counts the files', async () => {
    answer([
      file({ path: 'src/a.rs' }),
      file({ path: 'src/b.rs', status: { kind: 'added' }, added: 5, removed: 0 }),
      file({ root: '/other', path: 'x.md' }),
    ]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    await waitFor(() => expect(screen.getByTestId('changes-file-src/a.rs')).toBeInTheDocument());
    expect(screen.getByTestId('changes-file-src/b.rs').textContent).toContain('added');
    expect(screen.getByTestId('changes-file-src/b.rs').textContent).toContain('+5');
    expect(screen.getByTestId('changes-file-x.md')).toBeInTheDocument();
    // Both roots get a header of their own.
    expect(screen.getByTitle('/repo')).toBeInTheDocument();
    expect(screen.getByTitle('/other')).toBeInTheDocument();
    expect(screen.getByTestId('changes-count').textContent).toContain('3 files');
  });

  // No change has a decision now, so the panel offers none.
  it('draws no decision controls, no scope and no unreviewed filter', async () => {
    answer([file()]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('changes-file-src/a.rs')).toBeInTheDocument());
    expect(screen.queryByTestId('changes-filter-unreviewed')).toBeNull();
    expect(screen.queryByTestId('changes-scope-turn')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Reject' })).toBeNull();
  });

  it('names each root that the daemon cannot read, above the files', async () => {
    answer(
      [file({ path: 'src/a.rs' })],
      [],
      [
        { root: '/gone', reason: 'tracked root no longer exists' },
        { root: '/old', reason: 'session base snapshot abc is no longer stored' },
      ],
    );
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    const banner = await screen.findByTestId('diff-unreadable-roots');
    expect(banner.textContent).toContain('This diff leaves out the files of these roots:');
    const rows = within(banner).getAllByTestId('diff-unreadable-root');
    expect(rows.map((row) => row.textContent)).toEqual([
      '/gone — tracked root no longer exists',
      '/old — session base snapshot abc is no longer stored',
    ]);
    // The banner comes before the files of the readable root.
    const listed = screen.getByTestId('changes-file-src/a.rs');
    expect(banner.compareDocumentPosition(listed) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('a record with no unreadable root shows no banner', async () => {
    answer([file()]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('changes-file-src/a.rs')).toBeInTheDocument());
    expect(screen.queryByTestId('diff-unreadable-roots')).toBeNull();
  });

  it('a session with no changes says so once the list has answered', async () => {
    answer([]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('changes-empty')).toBeInTheDocument());
  });

  it('a failed list surfaces the daemon message', async () => {
    getDiffset.mockRejectedValue(new Error('nothing to read'));
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() =>
      expect(screen.getByTestId('changes-error').textContent).toContain('nothing to read'),
    );
    expect(screen.queryByTestId('changes-empty')).toBeNull();
  });

  it('clicking a file opens the session record in the diff pane', async () => {
    answer([file()]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('changes-file-src/a.rs')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('changes-file-src/a.rs'));
    // The pane opens on the whole record, and the clicked file is its focus.
    expect(openDiff).toHaveBeenCalledWith(
      { kind: 'session_record', session: 's1' },
      { root: '/repo', path: 'src/a.rs' },
    );
  });

  it('lists open comments and resolves them', async () => {
    const comment: DiffComment = {
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
    await waitFor(() =>
      expect(resolveDiffComment).toHaveBeenCalledWith(
        { kind: 'session_record', session: 's1' },
        'c1',
      ),
    );
  });
});

describe('ChangesPanel — conflicts', () => {
  // A conflict is a write nobody has disposed of. It goes ABOVE the session
  // record: it is the one row here that cannot go away on its own.
  it('lists a conflicted note above the session record', async () => {
    conflicts.rows = [conflict('/repo/notes/A.md')];
    answer([file()]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);

    const section = await screen.findByTestId('changes-conflicts');
    expect(within(section).getByText('/repo/notes/A.md')).toBeInTheDocument();
    const row = await screen.findByTestId('changes-file-src/a.rs');
    expect(section.compareDocumentPosition(row)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
  });

  // A conflict belongs to no session's record, and the desktop has no
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
    setCurrentSession(session());
    env.restore();
    env = createTestQueryEnv(
      proposalRoutes([
        proposalFixture(OPEN, { kind: 'open' }),
        proposalFixture(STALE, { kind: 'stale' }, { title: 'Stale change' }),
        proposalFixture(
          CONFLICTED,
          { kind: 'conflicted', files: [] },
          { title: 'Conflicted change' },
        ),
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

    // The pane opens with the chat of this panel, which takes its comments.
    fireEvent.click(screen.getByTestId(`changes-proposal-open-${CONFLICTED}`));
    expect(openDiff).toHaveBeenCalledWith({ kind: 'proposal', id: CONFLICTED }, undefined, 's1');
  });

  it('draws no proposal section when no proposal needs a merge', async () => {
    env.restore();
    const served = createTestQueryEnv(proposalRoutes([proposalFixture(OPEN, { kind: 'open' })]));
    env = served;
    render(() => <ChangesPanel />);

    await waitFor(() => expect(served.fetch.calls('POST /api/rpc/proposal.list')).toBe(1));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByTestId('changes-proposals')).toBeNull();
  });
});
