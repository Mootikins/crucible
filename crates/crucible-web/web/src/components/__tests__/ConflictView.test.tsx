import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent } from '@solidjs/testing-library';
import type { Conflicted } from '@/lib/offline/outbox';
import type { WriteOutcome } from '@/lib/offline/sync';

const pendingConflicts = vi.fn(async (): Promise<Conflicted[]> => []);
const resolveConflict = vi.fn(
  async (_path: string, _text: string): Promise<WriteOutcome> => ({
    queued: false,
    stale: false,
    hash: 'h10',
  }),
);
vi.mock('@/lib/offline/sync', () => ({
  pendingConflicts: () => pendingConflicts(),
  resolveConflict: (path: string, text: string) => resolveConflict(path, text),
}));

const addNotification = vi.fn();
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotification(...a) },
}));

const { ConflictView } = await import('../ConflictView');
const { __resetConflictStore } = await import('@/lib/conflicts');

const PATH = '/kilns/notes/Note.md';

/** Two regions, ours already in the merged text and theirs waiting beside it. */
function conflict(over: Partial<Conflicted> = {}): Conflicted {
  return {
    path: PATH,
    base: 'h0',
    kiln: '/kilns/notes',
    currentHash: 'h9',
    currentContent: ['one', 'THEIRS1', 'three', 'THEIRS2', 'five', ''].join('\n'),
    mergedContent: ['one', 'MINE1', 'three', 'MINE2', 'five', ''].join('\n'),
    regions: [
      { start_line: 2, end_line: 3, base: 'BASE1\n', ours: 'MINE1\n', theirs: 'THEIRS1\n' },
      { start_line: 4, end_line: 5, base: 'BASE2\n', ours: 'MINE2\n', theirs: 'THEIRS2\n' },
    ],
    ...over,
  };
}

/** The note the whole of which both writers rewrote: one region, both texts. */
const WHOLE = conflict({
  currentContent: 'their one\ntheir two\n',
  mergedContent: 'mine one\nmine two\n',
  regions: [
    {
      start_line: 1,
      end_line: 3,
      base: '',
      ours: 'mine one\nmine two\n',
      theirs: 'their one\ntheir two\n',
    },
  ],
});

async function open(row: Conflicted, props: Record<string, unknown> = {}) {
  pendingConflicts.mockResolvedValue([row]);
  render(() => <ConflictView path={row.path} {...props} />);
  await waitFor(() => expect(screen.getByTestId('conflict-region-0')).toBeInTheDocument());
}

/** What Save handed the writer. */
const written = () => resolveConflict.mock.calls[0][1];

beforeEach(() => {
  __resetConflictStore();
  pendingConflicts.mockReset();
  resolveConflict.mockClear();
  addNotification.mockClear();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('ConflictView', () => {
  it('keep mine and keep theirs rewrite the region and the counter', async () => {
    await open(conflict());
    expect(screen.getByTestId('conflict-counter').textContent).toContain('0 of 2 regions');

    fireEvent.click(screen.getByTestId('keep-mine-0'));
    await waitFor(() =>
      expect(screen.getByTestId('conflict-counter').textContent).toContain('1 of 2 regions'),
    );
    // The choice is made: the widget is gone and cannot be made twice.
    expect(screen.queryByTestId('keep-mine-0')).toBeNull();

    fireEvent.click(screen.getByTestId('keep-theirs-1'));
    await waitFor(() =>
      expect(screen.getByTestId('conflict-counter').textContent).toContain('2 of 2 regions'),
    );

    fireEvent.click(screen.getByTestId('conflict-save'));
    await waitFor(() => expect(resolveConflict).toHaveBeenCalled());
    expect(written()).toBe(['one', 'MINE1', 'three', 'THEIRS2', 'five', ''].join('\n'));
  });

  it('keep both keeps ours then theirs', async () => {
    await open(conflict({ regions: [conflict().regions[0]] }));

    fireEvent.click(screen.getByTestId('keep-both-0'));
    await waitFor(() => expect(screen.getByTestId('conflict-save')).not.toBeDisabled());

    fireEvent.click(screen.getByTestId('conflict-save'));
    await waitFor(() => expect(resolveConflict).toHaveBeenCalled());
    expect(written()).toBe(['one', 'MINE1', 'THEIRS1', 'three', 'MINE2', 'five', ''].join('\n'));
  });

  it('save is disabled while a region is open and writes with the current hash', async () => {
    const onResolved = vi.fn();
    await open(conflict(), { onResolved });
    expect(screen.getByTestId('conflict-save')).toBeDisabled();

    fireEvent.click(screen.getByTestId('keep-mine-0'));
    await waitFor(() => expect(screen.queryByTestId('keep-mine-0')).toBeNull());
    expect(screen.getByTestId('conflict-save'), 'one region is still open').toBeDisabled();

    fireEvent.click(screen.getByTestId('keep-theirs-1'));
    await waitFor(() => expect(screen.getByTestId('conflict-save')).not.toBeDisabled());

    fireEvent.click(screen.getByTestId('conflict-save'));
    // The hash is the entry's `currentHash`, and `resolveConflict` is the one
    // door that knows it. The view never writes a note itself.
    await waitFor(() => expect(resolveConflict).toHaveBeenCalledWith(PATH, expect.any(String)));
    await waitFor(() => expect(onResolved).toHaveBeenCalled());
  });

  it('a whole-note region needs a choice before save is enabled', async () => {
    await open(WHOLE);
    expect(screen.getByTestId('conflict-counter').textContent).toContain('0 of 1 region');
    expect(screen.getByTestId('conflict-save')).toBeDisabled();

    fireEvent.click(screen.getByTestId('keep-theirs-0'));
    await waitFor(() => expect(screen.getByTestId('conflict-save')).not.toBeDisabled());

    fireEvent.click(screen.getByTestId('conflict-save'));
    await waitFor(() => expect(resolveConflict).toHaveBeenCalled());
    expect(written()).toBe('their one\ntheir two\n');
  });

  it('leaving with open regions asks', async () => {
    const onClose = vi.fn();
    const ask = vi.spyOn(window, 'confirm').mockReturnValue(false);
    await open(conflict(), { onClose });

    fireEvent.click(screen.getByTestId('conflict-close'));
    expect(ask).toHaveBeenCalled();
    expect(onClose, 'a refused leave keeps the choices on screen').not.toHaveBeenCalled();

    ask.mockReturnValue(true);
    fireEvent.click(screen.getByTestId('conflict-close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('leaving with every region settled asks nothing', async () => {
    const onClose = vi.fn();
    const ask = vi.spyOn(window, 'confirm').mockReturnValue(false);
    await open(conflict({ regions: [conflict().regions[0]] }), { onClose });

    fireEvent.click(screen.getByTestId('keep-mine-0'));
    await waitFor(() => expect(screen.getByTestId('conflict-save')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('conflict-close'));

    expect(ask).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  // The note moved AGAIN between the merge and the choice. Nothing is settled,
  // so the conflict stays and the user is told rather than left believing it
  // landed.
  it('a note that moved again keeps the conflict open', async () => {
    const onResolved = vi.fn();
    resolveConflict.mockResolvedValueOnce({ queued: false, stale: true, current: 'h11' });
    await open(conflict({ regions: [conflict().regions[0]] }), { onResolved });

    fireEvent.click(screen.getByTestId('keep-mine-0'));
    await waitFor(() => expect(screen.getByTestId('conflict-save')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('conflict-save'));

    await waitFor(() => expect(addNotification).toHaveBeenCalledWith('warning', expect.any(String)));
    expect(onResolved).not.toHaveBeenCalled();
  });
});
