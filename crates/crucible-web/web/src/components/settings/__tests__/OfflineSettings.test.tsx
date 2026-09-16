import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

const calls = vi.hoisted(() => ({ cached: [] as string[], dropped: [] as string[] }));
vi.mock('@/lib/offline/sync', () => ({
  cacheKiln: async (kiln: string) => {
    calls.cached.push(kiln);
    return { notes: 2, attachments: 0, failed: [] };
  },
  dropKiln: async (kiln: string) => {
    calls.dropped.push(kiln);
  },
  kilnSize: async () => ({ notes: 2048, attachments: 0 }),
  syncNow: async () => ({ sent: 0, conflicted: [], foreign: 0, failed: 0 }),
  pendingCount: async () => 3,
}));

import { OfflineSettingsSection } from '@/components/settings/OfflineSettings';
import { KEPT_KILNS_KEY, keptMode } from '@/lib/offline/kept';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetKilnsForTests } from '@/lib/query/kilns';

// `useKilns` runs the real `listKilns` against this fetch, so the section is
// driven by the answer the daemon would give rather than by a stubbed module.
let env: TestQueryEnv;

beforeEach(() => {
  localStorage.clear();
  resetKilnsForTests();
  env = createTestQueryEnv({
    'GET /api/kilns': () => ({
      kilns: [
        { path: '/kilns/notes', name: 'notes' },
        { path: '/kilns/work', name: 'work' },
      ],
    }),
  });
  calls.cached = [];
  calls.dropped = [];
});

afterEach(() => {
  env.restore();
  resetKilnsForTests();
});

describe('the Offline settings group', () => {
  it('lists every kiln, none kept to begin with', async () => {
    render(() => <OfflineSettingsSection />);
    const picker = await waitFor(() => screen.getByTestId('offline-mode-notes'));
    expect((picker as HTMLSelectElement).value).toBe('off');
    expect(screen.getAllByText('Not kept on this device.')).toHaveLength(2);
  });

  // The choice a user makes here is what fetches the kiln.
  it('keeps a kiln, and fetches it', async () => {
    render(() => <OfflineSettingsSection />);
    const picker = await waitFor(() => screen.getByTestId('offline-mode-notes'));
    fireEvent.change(picker, { target: { value: 'everything' } });

    await waitFor(() => expect(calls.cached).toEqual(['/kilns/notes']));
    expect(keptMode('/kilns/notes')).toBe('everything');
    expect(JSON.parse(localStorage.getItem(KEPT_KILNS_KEY)!)).toEqual({
      '/kilns/notes': { mode: 'everything' },
    });
  });

  it('drops what a kiln kept when the user turns it off', async () => {
    render(() => <OfflineSettingsSection />);
    const picker = await waitFor(() => screen.getByTestId('offline-mode-notes'));
    fireEvent.change(picker, { target: { value: 'notes' } });
    await waitFor(() => expect(calls.cached).toHaveLength(1));

    fireEvent.change(picker, { target: { value: 'off' } });
    await waitFor(() => expect(calls.dropped).toEqual(['/kilns/notes']));
    expect(keptMode('/kilns/notes')).toBeNull();
  });

  // Unsent edits exist only on this device, so the count is worth showing.
  it('shows how many edits the daemon has not received', async () => {
    render(() => <OfflineSettingsSection />);
    await waitFor(() => expect(screen.getByTestId('offline-queued').textContent).toBe('3'));
    expect(screen.getByRole('button', { name: 'Send now' })).toBeTruthy();
  });
});
