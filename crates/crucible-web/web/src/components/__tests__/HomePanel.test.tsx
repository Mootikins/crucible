import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@solidjs/testing-library';
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

// Hand out a copy so a component can never retain (and later observe mutations
// to) the shared `notes` array.
vi.mock('@/lib/api', () => ({
  listNotes: vi.fn(() => Promise.resolve([...notes])),
  listPendingInteractions: vi.fn().mockResolvedValue([]),
}));

// Snapshot the pristine fixture so per-test mutations (a push to exercise the
// relative-path click) can't leak into later tests — even if an assertion
// throws before an inline restore.
const pristineNotes = notes.map((n) => ({ ...n }));

const openFileInEditorMock = vi.fn();
vi.mock('@/lib/file-actions', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openFileInEditor: (...args: unknown[]) => openFileInEditorMock(...args),
}));

const openDraftSessionMock = vi.fn();
const setDraftPrefillMock = vi.fn();
vi.mock('@/lib/draft-session', () => ({
  openDraftSession: (...args: unknown[]) => openDraftSessionMock(...args),
  setDraftPrefill: (...args: unknown[]) => setDraftPrefillMock(...args),
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
  // Restore the shared fixture in case a test mutated it.
  notes.length = 0;
  notes.push(...pristineNotes.map((n) => ({ ...n })));
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
    expect(getByText(/all clear/i)).toBeTruthy();
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
    expect(queryByText(/all clear/i)).toBeNull();
  });

  it('opens recent notes by absolute path (records are kiln-relative)', async () => {
    // Regression: the daemon's note records carry kiln-relative paths, but
    // /api/kiln/file addresses files absolutely — passing the raw record
    // path 404'd every recent-note click.
    notes.push({
      name: 'Relative Note',
      path: 'Guides/Relative Note.md',
      title: 'Relative Note',
      tags: [],
      updated_at: '2026-07-12T09:00:00Z',
    });
    const { getByText } = render(() => <HomePanel />);
    await waitFor(() => {
      expect(getByText('Relative Note')).toBeTruthy();
    });

    getByText('Relative Note').closest('button')!.click();
    expect(openFileInEditorMock).toHaveBeenCalledWith(
      '/home/user/kilns/helios/Guides/Relative Note.md',
      'Relative Note',
    );
    // No inline pop(): the afterEach restore reverts the fixture even if an
    // assertion above throws first.
  });

  it('offers the composer, an editor shortcut, and the graph teaser', () => {
    const { getByTestId, getByText } = render(() => <HomePanel />);
    expect(getByTestId('home-composer')).toBeTruthy();
    expect(getByText(/Open editor/)).toBeTruthy();
    expect(getByText('GRAPH')).toBeTruthy();
  });

  it('starts a session with the typed text prefilled into the draft', () => {
    const { getByTestId } = render(() => <HomePanel />);
    const composer = getByTestId('home-composer');
    fireEvent.input(composer, { target: { value: 'refactor the auth module' } });
    fireEvent.keyDown(composer, { key: 'Enter' });

    expect(setDraftPrefillMock).toHaveBeenCalledWith('refactor the auth module');
    expect(openDraftSessionMock).toHaveBeenCalledTimes(1);
  });

  it('opens an empty draft when the composer is empty', () => {
    const { getByTestId } = render(() => <HomePanel />);
    fireEvent.keyDown(getByTestId('home-composer'), { key: 'Enter' });

    expect(setDraftPrefillMock).not.toHaveBeenCalled();
    expect(openDraftSessionMock).toHaveBeenCalledTimes(1);
  });
});
