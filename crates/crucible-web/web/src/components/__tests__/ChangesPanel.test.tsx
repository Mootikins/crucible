import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent, within } from '@solidjs/testing-library';
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

// The shell is decided once at page load; the test stages it before a render.
const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));

const listReviewHunks = vi.fn();
const setHunkState = vi.fn(async () => ({
  hunk_id: 'h',
  state: 'accepted' as const,
}));
// The bulk answers echo the ids they were given, the way the mock daemon does.
const setHunkStates = vi.fn(async (_s: string, ids: string[], state: string) => ({
  applied: ids,
  failed: [] as { hunk_id: string; reason: string }[],
  state,
}));
const undoReject = vi.fn(async () => ({
  applied: ['h1'],
  failed: [] as { hunk_id: string; reason: string }[],
}));
const addReviewComment = vi.fn(async () => ({ comment: {} }));
const resolveReviewComment = vi.fn(async () => ({ comment_id: 'c1' }));
const rebaseReview = vi.fn(async () => ({ roots: [] }));
vi.mock('@/lib/review-api', () => ({
  listReviewHunks: (...a: unknown[]) => listReviewHunks(...a),
  setHunkState: (...a: unknown[]) => setHunkState(...(a as [])),
  setHunkStates: (...a: unknown[]) => setHunkStates(...(a as [string, string[], string])),
  undoReject: (...a: unknown[]) => undoReject(...(a as [])),
  addReviewComment: (...a: unknown[]) => addReviewComment(...(a as [])),
  resolveReviewComment: (...a: unknown[]) => resolveReviewComment(...(a as [])),
  rebaseReview: (...a: unknown[]) => rebaseReview(...(a as [])),
}));

const openFileInEditor = vi.fn();
vi.mock('@/lib/file-actions', () => ({
  openFileInEditor: (...a: unknown[]) => openFileInEditor(...a),
}));

// Rejecting rewrites a file; the toast is how the user learns it happened.
const addNotification = vi.fn();
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotification(...a) },
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
  setHunkStates.mockReset();
  setHunkStates.mockImplementation(async (_s, ids, state) => ({ applied: ids, failed: [], state }));
  undoReject.mockReset();
  undoReject.mockResolvedValue({ applied: ['h1'], failed: [] });
});

/** The `Undo` the last toast offered, or a failure naming what was posted. */
function lastUndo(): () => void {
  const calls = addNotification.mock.calls as [string, string, { label: string; run: () => void }?][];
  const withUndo = calls.filter((c) => c[2]?.label === 'Undo');
  expect(withUndo, JSON.stringify(calls)).not.toHaveLength(0);
  return withUndo.at(-1)![2]!.run;
}

afterEach(() => {
  cleanup();
  setCurrentSession(undefined);
  device.compact = false;
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
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-h1'));
    await waitFor(() => expect(setHunkState).toHaveBeenCalledWith('s1', 'h1', 'rejected'));
    confirm.mockRestore();
  });

  // The gradient: deleting a session — which loses nothing on disk — already
  // confirmed, while this, which rewrites a file the daemon cannot restore,
  // was one unguarded click on a 22px glyph.
  it('asks before reverting, and names the lines it is about to rewrite', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-h1'));
    await waitFor(() => expect(setHunkState).toHaveBeenCalledOnce());

    const prompt = confirm.mock.calls[0][0] as string;
    expect(prompt).toContain('src/a.rs');
    expect(prompt).toContain('L4');
    // The daemon keeps a stack of rejects now; a prompt that still said
    // "cannot be undone" would be a lie the user acts on.
    expect(prompt).not.toContain('cannot be undone');
    expect(prompt).toContain('Undo');
    confirm.mockRestore();
  });

  it('touches no disk when the user backs out of the confirm', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-h1'));
    expect(confirm).toHaveBeenCalledOnce();
    expect(setHunkState).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  // Accept is the cheap, recoverable half of the pair. Confirming it too would
  // be the fatigue the `re-applied` flag exists to make visible.
  it('accept stays one click', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('accept-h1'));
    await waitFor(() => expect(setHunkState).toHaveBeenCalledWith('s1', 'h1', 'accepted'));
    expect(confirm).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it('leaves a receipt naming what it reverted', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-h1'));
    await waitFor(() => expect(addNotification).toHaveBeenCalled());
    const [type, message] = addNotification.mock.calls.at(-1) as [string, string];
    expect(type).toBe('info');
    expect(message).toContain('src/a.rs');
    confirm.mockRestore();
  });

  // A single reject is a batch of one on the daemon's stack, so it gets the
  // same way back as a bulk one.
  it('a single reject offers an undo that pops the daemon stack', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-h1'));
    await waitFor(() => expect(addNotification).toHaveBeenCalled());
    lastUndo()();
    await waitFor(() => expect(undoReject).toHaveBeenCalledWith('s1'));
    confirm.mockRestore();
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

  // The daemon decides. CodeMirror's own accept/reject `action` would edit the
  // browser's copy of the text and leave the disk untouched, so the controls
  // the merge view draws call the same review actions as the row's buttons.
  it('an expanded hunk mounts the merge view with accept and reject controls', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    answer([hunk({ id: 'h1', before_content: 'a\n', after_content: 'b\n' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());
    expect(screen.queryByTestId('hunk-merge')).toBeNull();

    fireEvent.click(screen.getByTestId('hunk-h1').querySelector('button')!);
    const merge = await waitFor(() => screen.getByTestId('hunk-merge'));
    const accept = await waitFor(() => within(merge).getByRole('button', { name: 'Accept' }));
    const reject = within(merge).getByRole('button', { name: 'Reject' });
    expect(accept.className).toContain('min-h-11');
    expect(reject.className).toContain('min-h-11');

    fireEvent.click(reject);
    await waitFor(() => expect(setHunkState).toHaveBeenCalledWith('s1', 'h1', 'rejected'));
    // The merge view still shows the hunk as the daemon composed it.
    expect(merge.textContent).toContain('a');
    expect(merge.textContent).toContain('b');
    confirm.mockRestore();
  });

  it("an external hunk's merge view offers accept and no reject", async () => {
    answer([hunk({ id: 'ext', tool_call_ids: [], before_content: 'a\n', after_content: 'b\n' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-ext')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('hunk-ext').querySelector('button')!);
    const merge = await waitFor(() => screen.getByTestId('hunk-merge'));
    await waitFor(() => within(merge).getByRole('button', { name: 'Accept' }));
    expect(within(merge).queryByRole('button', { name: 'Reject' })).toBeNull();
  });

  it('on a compact shell every hunk control is at least 44 px', async () => {
    device.compact = true;
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());
    for (const id of ['accept-h1', 'reject-h1', 'comment-h1']) {
      expect(screen.getByTestId(id).className, id).toContain('min-h-11');
      expect(screen.getByTestId(id).className, id).toContain('min-w-11');
    }
  });

  it('on the desktop shell the hunk controls keep their dense size', async () => {
    answer([hunk({ id: 'h1' })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-h1')).toBeInTheDocument());
    for (const id of ['accept-h1', 'reject-h1', 'comment-h1']) {
      expect(screen.getByTestId(id).className, id).not.toContain('min-h-11');
    }
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

describe('ChangesPanel — bulk decisions', () => {
  // Composed-diff order, with one file's hunks interleaved with another's, an
  // external hunk and an already-decided one: the bulk buttons must pick the
  // right subset and keep the order.
  const queue = () => [
    hunk({ id: 'a' }),
    hunk({ id: 'x', path: 'src/b.rs' }),
    hunk({ id: 'b', current_range: { start: 20, end: 21 } }),
    hunk({ id: 'ext', tool_call_ids: [], current_range: { start: 30, end: 31 } }),
    hunk({ id: 'done', state: 'accepted', current_range: { start: 40, end: 41 } }),
    hunk({ id: 'y', root: '/other', path: 'x.md' }),
  ];

  it('reject all on a file asks once and then undoes as one', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    answer(queue());
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-a')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-all-src/a.rs'));
    expect(confirm).toHaveBeenCalledOnce();
    expect(confirm.mock.calls[0][0]).toContain('src/a.rs');
    expect(confirm.mock.calls[0][0]).toContain('2');
    // One call, the file's unreviewed non-external hunks, in file order.
    await waitFor(() => expect(setHunkStates).toHaveBeenCalledOnce());
    expect(setHunkStates).toHaveBeenCalledWith('s1', ['a', 'b'], 'rejected');
    expect(setHunkState).not.toHaveBeenCalled();

    // The receipt carries the way back, and the way back is one daemon call.
    await waitFor(() => expect(addNotification).toHaveBeenCalled());
    lastUndo()();
    await waitFor(() => expect(undoReject).toHaveBeenCalledWith('s1'));
    confirm.mockRestore();
  });

  it('reject all is not sent when the confirm is declined', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    answer(queue());
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-a')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-all-src/a.rs'));
    expect(confirm).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByTestId('changes-reject-all'));
    expect(confirm).toHaveBeenCalledTimes(2);
    expect(setHunkStates).not.toHaveBeenCalled();
    expect(addNotification).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it('accept all on a file is one click and one call', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    answer(queue());
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-a')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('accept-all-src/a.rs'));
    await waitFor(() => expect(setHunkStates).toHaveBeenCalledWith('s1', ['a', 'b'], 'accepted'));
    expect(confirm).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it('the review-wide buttons decide every root in composed order', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    answer(queue());
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-a')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('changes-accept-all'));
    await waitFor(() =>
      expect(setHunkStates).toHaveBeenCalledWith('s1', ['a', 'x', 'b', 'y'], 'accepted'),
    );
    expect(confirm).not.toHaveBeenCalled();

    // The pair is one door: the second click waits for the first to settle.
    await waitFor(() => expect(screen.getByTestId('changes-reject-all')).toBeEnabled());
    fireEvent.click(screen.getByTestId('changes-reject-all'));
    expect(confirm).toHaveBeenCalledOnce();
    expect(confirm.mock.calls[0][0]).toContain('4');
    await waitFor(() =>
      expect(setHunkStates).toHaveBeenCalledWith('s1', ['a', 'x', 'b', 'y'], 'rejected'),
    );
    confirm.mockRestore();
  });

  it('a file with nothing left to decide offers no bulk buttons', async () => {
    answer([hunk({ id: 'done', state: 'accepted' }), hunk({ id: 'ext', tool_call_ids: [] })]);
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-done')).toBeInTheDocument());

    expect(screen.queryByTestId('accept-all-src/a.rs')).toBeNull();
    expect(screen.queryByTestId('reject-all-src/a.rs')).toBeNull();
    expect(screen.getByTestId('changes-accept-all')).toBeDisabled();
    expect(screen.getByTestId('changes-reject-all')).toBeDisabled();
  });

  // The daemon applies what it can and names what it refused. Silence on the
  // refused half would read as "everything went through".
  it('names the hunks the daemon refused in one notification', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    setHunkStates.mockResolvedValue({
      applied: ['a'],
      failed: [{ hunk_id: 'b', reason: 'unknown hunk b' }],
      state: 'rejected',
    });
    answer(queue());
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-a')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-all-src/a.rs'));
    await waitFor(() => expect(setHunkStates).toHaveBeenCalledOnce());
    const warning = await waitFor(() => {
      const c = addNotification.mock.calls.find((c) => c[0] === 'warning');
      expect(c).toBeDefined();
      return c as [string, string];
    });
    // Named as the user saw the hunk, not by its hash.
    expect(warning[1]).toContain('src/a.rs L20');
    expect(warning[1]).toContain('unknown hunk b');
    // The one that landed still has its way back.
    expect(lastUndo()).toBeTypeOf('function');
    confirm.mockRestore();
  });

  it('an undo the daemon refused says which hunks moved on', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    undoReject.mockResolvedValue({
      applied: [],
      failed: [{ hunk_id: 'a', reason: 'src/a.rs changed since the hunk was computed' }],
    });
    answer(queue());
    setCurrentSession(session());
    render(() => <ChangesPanel />);
    await waitFor(() => expect(screen.getByTestId('hunk-a')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('reject-all-src/a.rs'));
    await waitFor(() => expect(addNotification).toHaveBeenCalled());
    lastUndo()();
    await waitFor(() => expect(undoReject).toHaveBeenCalledOnce());
    await waitFor(() => {
      const c = addNotification.mock.calls.find((c) => c[0] === 'warning') as [string, string];
      expect(c).toBeDefined();
      expect(c[1]).toContain('changed since the hunk was computed');
    });
    confirm.mockRestore();
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
