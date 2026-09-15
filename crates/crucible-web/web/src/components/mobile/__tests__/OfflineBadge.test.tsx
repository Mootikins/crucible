import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@solidjs/testing-library';
import type { Conflicted } from '@/lib/offline/outbox';

const state = vi.hoisted(() => ({
  online: true,
  queued: 0,
  conflicts: [] as Conflicted[],
  synced: 0,
  warmed: 0,
}));
vi.mock('@/lib/offline/sync', () => ({
  isOnline: () => state.online,
  pendingCount: async () => state.queued,
  pendingConflicts: async () => state.conflicts,
  syncNow: async () => {
    state.synced += 1;
    return { sent: 0, conflicted: [], foreign: 0, failed: 0 };
  },
  warmIdentity: async () => {
    state.warmed += 1;
  },
}));

// The badge is chrome: it points at the conflict surface, it does not draw it.
const openPanelTab = vi.fn();
vi.mock('@/lib/panel-actions', () => ({ openPanelTab: (id: string) => openPanelTab(id) }));

import { OfflineBadge } from '@/components/OfflineBadge';
import { __resetConflictStore } from '@/lib/conflicts';

/** One note whose write the daemon could neither take nor merge. */
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
  state.online = true;
  state.queued = 0;
  state.conflicts = [];
  state.synced = 0;
  state.warmed = 0;
  openPanelTab.mockClear();
  __resetConflictStore();
});

describe('OfflineBadge', () => {
  // Nothing to say when everything is sent and the daemon answers.
  it('shows nothing when online with an empty queue', async () => {
    render(() => <OfflineBadge />);
    // `waitFor` runs its callback immediately, so asserting absence alone
    // passes at t=0 — before the async count has even been read. Wait for a
    // POSITIVE signal that the component has done its work, THEN assert.
    await waitFor(() => expect(state.warmed).toBeGreaterThan(0));
    expect(screen.queryByTestId('offline-badge')).toBeNull();
  });

  it('says so when the device is offline', async () => {
    state.online = false;
    render(() => <OfflineBadge />);
    await waitFor(() =>
      expect(screen.getByTestId('offline-badge').getAttribute('aria-label')).toBe(
        'Offline, 0 unsent edits',
      ),
    );
  });

  // A queued write that looks saved and is not is the failure this prevents.
  it('counts the writing the daemon has not received', async () => {
    state.queued = 2;
    render(() => <OfflineBadge />);
    await waitFor(() =>
      expect(screen.getByTestId('offline-badge').getAttribute('aria-label')).toBe(
        '2 unsent edits',
      ),
    );
  });

  it('sends the queue when the browser reconnects', async () => {
    state.online = false;
    state.queued = 1;
    render(() => <OfflineBadge />);
    await waitFor(() => expect(screen.queryByTestId('offline-badge')).not.toBeNull());
    window.dispatchEvent(new Event('online'));
    await waitFor(() => expect(state.synced).toBe(1));
  });

  // A write queued OFFLINE is stamped with the daemon's identity, and that is
  // the one moment it cannot be fetched. The badge is what learns it in time.
  it('learns which daemon this is while the network is up', async () => {
    render(() => <OfflineBadge />);
    await waitFor(() => expect(state.warmed).toBeGreaterThan(0));
  });

  it('learns it again on reconnect, in case the daemon changed', async () => {
    state.online = false;
    render(() => <OfflineBadge />);
    const before = state.warmed;

    state.online = true;
    window.dispatchEvent(new Event('online'));
    await waitFor(() => expect(state.warmed).toBeGreaterThan(before));
  });

  // A conflict is NOT owed to the network: no send will ever clear it, and a
  // count that mixed the two would tell a user to press Sync forever.
  it('counts conflicts apart from unsent edits', async () => {
    state.queued = 2;
    state.conflicts = [conflict('/kilns/notes/A.md')];
    render(() => <OfflineBadge />);
    await waitFor(() =>
      expect(screen.getByTestId('offline-badge').getAttribute('aria-label')).toBe(
        '1 conflict, 2 unsent edits',
      ),
    );
  });

  // Nothing is owed and the daemon answers, so the queue count says nothing.
  // The writing still has not landed on the note it was made in.
  it('shows for a conflict even when the queue is empty', async () => {
    state.conflicts = [conflict('/kilns/notes/A.md')];
    render(() => <OfflineBadge />);
    await waitFor(() => expect(screen.queryByTestId('offline-badge')).not.toBeNull());
  });

  it('opens the conflict list instead of sending when one waits', async () => {
    state.conflicts = [conflict('/kilns/notes/A.md')];
    render(() => <OfflineBadge />);
    await waitFor(() => expect(screen.queryByTestId('offline-badge')).not.toBeNull());

    fireEvent.click(screen.getByTestId('offline-badge'));
    await waitFor(() => expect(openPanelTab).toHaveBeenCalledWith('conflicts'));
    expect(state.synced).toBe(0);
  });
});
