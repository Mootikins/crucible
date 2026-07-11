import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@solidjs/testing-library';
import HomePanel, { greetingForHour, relativeTime } from '../HomePanel';
import { attentionStore, attentionActions } from '@/stores/attentionStore';
import { statusBarActions } from '@/stores/statusBarStore';
import type { NoteEntry, PermRequest } from '@/lib/types';

const notes: NoteEntry[] = [
  {
    name: 'Architecture',
    path: '/kiln/Architecture.md',
    title: 'Architecture',
    tags: [],
    updated_at: '2026-07-09T10:00:00Z',
  },
  {
    name: 'Scheduler Redesign',
    path: '/kiln/Scheduler Redesign.md',
    title: 'Scheduler Redesign',
    tags: [],
    updated_at: '2026-07-11T09:00:00Z',
  },
];

vi.mock('@/lib/api', () => ({
  listNotes: vi.fn(() => Promise.resolve(notes)),
}));

function clearAttention() {
  for (const id of Object.keys(attentionStore.entries)) {
    attentionActions.clear(id);
  }
}

beforeEach(() => {
  clearAttention();
  statusBarActions.setKilnPath('/home/user/kilns/helios');
});
afterEach(() => {
  cleanup();
  clearAttention();
  statusBarActions.setKilnPath(null);
  vi.clearAllMocks();
});

describe('HomePanel helpers', () => {
  it('greets by hour', () => {
    expect(greetingForHour(3)).toBe('Up late');
    expect(greetingForHour(9)).toBe('Good morning');
    expect(greetingForHour(14)).toBe('Good afternoon');
    expect(greetingForHour(21)).toBe('Good evening');
  });

  it('formats relative time', () => {
    const now = Date.parse('2026-07-11T12:00:00Z');
    expect(relativeTime('2026-07-11T11:59:40Z', now)).toBe('just now');
    expect(relativeTime('2026-07-11T11:30:00Z', now)).toBe('30m ago');
    expect(relativeTime('2026-07-11T09:00:00Z', now)).toBe('3h ago');
    expect(relativeTime('2026-07-08T12:00:00Z', now)).toBe('3d ago');
    expect(relativeTime('garbage', now)).toBe('');
  });
});

describe('HomePanel', () => {
  it('shows the kiln name and all-clear when nothing needs attention', async () => {
    const { getByText } = render(() => <HomePanel />);
    expect(getByText(/helios/)).toBeTruthy();
    expect(getByText(/all clear/)).toBeTruthy();
    // Recent notes load, most recently updated first.
    await waitFor(() => {
      expect(getByText('Scheduler Redesign')).toBeTruthy();
      expect(getByText('Architecture')).toBeTruthy();
    });
  });

  it('shows the needs-you strip when sessions await input', () => {
    const perm: PermRequest = {
      kind: 'permission',
      id: 'r1',
      action_type: 'bash',
      tokens: ['ls'],
    };
    attentionActions.report('s1', { pendingInteraction: perm, title: 't' });

    const { getByText, queryByText } = render(() => <HomePanel />);
    expect(getByText('1 need you')).toBeTruthy();
    expect(getByText(/open inbox/)).toBeTruthy();
    expect(queryByText(/all clear/)).toBeNull();
  });

  it('always offers new session and editor entry points', () => {
    const { getByText } = render(() => <HomePanel />);
    expect(getByText('+ new session')).toBeTruthy();
    expect(getByText(/open editor/)).toBeTruthy();
    expect(getByText('GRAPH')).toBeTruthy();
  });
});
