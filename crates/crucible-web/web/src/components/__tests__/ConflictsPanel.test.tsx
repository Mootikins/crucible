import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@solidjs/testing-library';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';
import type { Conflicted } from '@/lib/offline/outbox';

const conflicts = vi.hoisted(() => ({ rows: [] as Conflicted[] }));
vi.mock('@/lib/offline/sync', () => ({
  pendingConflicts: async () => conflicts.rows,
  resolveConflict: async () => ({ queued: false, stale: false, hash: 'h' }),
}));

// The view mounts a CodeMirror over the merged text; this panel's own job is
// which conflict is on screen.
vi.mock('../ConflictView', () => ({
  ConflictView: (props: { path: string }) => (
    <div data-testid="conflict-view-stub">{props.path}</div>
  ),
}));

const { ConflictsPanel } = await import('../ConflictsPanel');
const { __resetConflictStore, conflictActions } = await import('@/lib/conflicts');

const conflict = (path: string): Conflicted => ({
  path,
  base: 'h0',
  kiln: '/kilns/notes',
  currentHash: 'h9',
  currentContent: 'theirs\n',
  mergedContent: 'mine\n',
  regions: [{ start_line: 1, end_line: 2, base: '', ours: 'mine\n', theirs: 'theirs\n' }],
});

beforeEach(() => {
  conflicts.rows = [];
  __resetConflictStore();
  resetGlobalRegistry();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('ConflictsPanel', () => {
  // One surface both shells reach: the phone opens it from the More sheet and
  // the badge, the desktop from the Changes panel's Conflicts section.
  it('registers "conflicts" as a tab either shell can open', () => {
    registerPanels();
    const panel = getGlobalRegistry().get('conflicts');
    expect(panel).toBeDefined();
    expect(panel!.title).toBe('Conflicts');
  });

  it('lists what waits and opens one in the view', async () => {
    conflicts.rows = [conflict('/kilns/notes/A.md'), conflict('/kilns/notes/B.md')];
    render(() => <ConflictsPanel />);

    const row = await screen.findByTestId('conflict-open-/kilns/notes/B.md');
    expect(screen.queryByTestId('conflict-view-stub')).toBeNull();

    fireEvent.click(row);
    expect(screen.getByTestId('conflict-view-stub').textContent).toBe('/kilns/notes/B.md');
  });

  it('says so when nothing waits', async () => {
    render(() => <ConflictsPanel />);
    expect(await screen.findByTestId('conflicts-empty')).toBeInTheDocument();
  });

  // A conflict opened from elsewhere — the badge, the Changes panel — is the
  // one the panel draws, so the two doors do not disagree.
  it('draws the conflict another surface selected', async () => {
    conflicts.rows = [conflict('/kilns/notes/A.md')];
    await conflictActions.refresh();
    conflictActions.select('/kilns/notes/A.md');
    render(() => <ConflictsPanel />);

    expect(screen.getByTestId('conflict-view-stub').textContent).toBe('/kilns/notes/A.md');
  });
});
