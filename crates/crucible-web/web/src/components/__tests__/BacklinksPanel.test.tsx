import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, fireEvent } from '@solidjs/testing-library';
import type { BacklinksResponse } from '@/lib/types';

const getBacklinksMock = vi.fn();
const getConfigMock = vi.fn();
const openFileInEditorMock = vi.fn();
const updateFileContentMock = vi.fn();

let activeFilePath: string | null = '/kiln/notes/focused.md';
let openFileContent = 'Other Note is mentioned here.';

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getBacklinks: (...args: unknown[]) => getBacklinksMock(...args),
  getConfig: (...args: unknown[]) => getConfigMock(...args),
}));

vi.mock('@/lib/file-actions', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openFileInEditor: (...args: unknown[]) => openFileInEditorMock(...args),
}));

vi.mock('@/contexts/EditorContext', () => ({
  useEditorSafe: () => ({
    activeFile: () => activeFilePath,
    openFiles: () =>
      activeFilePath ? [{ path: activeFilePath, content: openFileContent, dirty: false }] : [],
    updateFileContent: (...args: unknown[]) => updateFileContentMock(...args),
  }),
}));

import { BacklinksPanel, noteKeyForPath } from '../BacklinksPanel';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetKilnsForTests } from '@/lib/query/kilns';

const RESPONSE: BacklinksResponse = {
  note: { path: 'notes/focused.md', abs_path: '/kiln/notes/focused.md', title: 'Focused Note' },
  linked: [
    { name: 'linker', path: 'notes/linker.md', abs_path: '/kiln/notes/linker.md', title: 'Linker Note' },
  ],
  unlinked: [{ mention: 'Other Note', target: 'Other Note', offset: 0 }],
};

// The panel derives its kiln from the focused file's own path, so `useKilns`
// must attribute that file to a kiln. The real `listKilns` runs against this.
let env: TestQueryEnv;

afterEach(() => {
  env.restore();
  resetKilnsForTests();
});

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  resetKilnsForTests();
  env = createTestQueryEnv({ 'GET /api/kilns': () => ({ kilns: [{ path: '/kiln', name: 'kiln' }] }) });
  activeFilePath = '/kiln/notes/focused.md';
  openFileContent = 'Other Note is mentioned here.';
  getConfigMock.mockResolvedValue({ kiln_path: '/kiln' });
  getBacklinksMock.mockResolvedValue(RESPONSE);
});

describe('noteKeyForPath', () => {
  it('strips the kiln prefix to a relative path', () => {
    expect(noteKeyForPath('/kiln/notes/rust.md', '/kiln')).toBe('notes/rust.md');
    expect(noteKeyForPath('/kiln/notes/rust.md', '/kiln/')).toBe('notes/rust.md');
  });

  it('falls back to the file stem outside the kiln', () => {
    expect(noteKeyForPath('/elsewhere/rust.md', '/kiln')).toBe('rust');
    expect(noteKeyForPath('/elsewhere/rust.md', null)).toBe('rust');
  });
});

describe('BacklinksPanel', () => {
  it('renders linked and unlinked mentions for the focused note', async () => {
    const { getByTestId, getAllByTestId } = render(() => <BacklinksPanel />);

    await waitFor(() => {
      expect(getByTestId('backlinks-note-title').textContent).toBe('Focused Note');
    });

    const linked = getAllByTestId('backlinks-linked-item');
    expect(linked).toHaveLength(1);
    expect(linked[0].textContent).toContain('Linker Note');
    expect(linked[0].textContent).toContain('notes/linker.md');
    // Rows opt into the app-wide hover preview.
    expect(linked[0].getAttribute('data-note')).toBe('linker');

    const unlinked = getAllByTestId('backlinks-unlinked-item');
    expect(unlinked).toHaveLength(1);
    expect(unlinked[0].textContent).toContain('Other Note');

    expect(getBacklinksMock).toHaveBeenCalledWith('/kiln', 'notes/focused.md');
  });

  it('shows an empty state when no note is focused', async () => {
    activeFilePath = null;
    const { getByTestId } = render(() => <BacklinksPanel />);

    await waitFor(() => {
      expect(getByTestId('backlinks-empty').textContent).toContain('Open a note');
    });
    expect(getBacklinksMock).not.toHaveBeenCalled();
  });

  it('ignores non-markdown files', async () => {
    activeFilePath = '/kiln/src/main.rs';
    const { getByTestId } = render(() => <BacklinksPanel />);

    await waitFor(() => {
      expect(getByTestId('backlinks-empty')).not.toBeNull();
    });
    expect(getBacklinksMock).not.toHaveBeenCalled();
  });

  it('clicking a linked mention dispatches the global open-file event', async () => {
    const events: Array<{ path: string; name?: string }> = [];
    const listener = (e: Event) =>
      events.push((e as CustomEvent<{ path: string; name?: string }>).detail);
    window.addEventListener('crucible:open-file', listener);

    const { getAllByTestId } = render(() => <BacklinksPanel />);
    await waitFor(() => {
      expect(getAllByTestId('backlinks-linked-item')).toHaveLength(1);
    });

    fireEvent.click(getAllByTestId('backlinks-linked-item')[0]);
    window.removeEventListener('crucible:open-file', listener);
    expect(events).toEqual([{ path: '/kiln/notes/linker.md', name: 'linker' }]);
  });

  it('renders an error state, not an empty one, when the fetch fails', async () => {
    // The panel used to swallow every failure and say "No notes link here
    // yet" — a claim about the data that it had no answer for.
    getBacklinksMock.mockRejectedValue(Object.assign(new Error('not found'), { status: 404 }));

    const { getByTestId, queryByTestId } = render(() => <BacklinksPanel />);

    await waitFor(() => {
      expect(getByTestId('backlinks-error')).not.toBeNull();
    });
    const error = getByTestId('backlinks-error');
    expect(error.textContent).toContain('Backlinks are unavailable');
    expect(error.textContent).toContain('404');
    expect(error.getAttribute('data-tone')).toBe('error');
    // The empty states must not render beside the failure.
    expect(queryByTestId('backlinks-linked-empty')).toBeNull();
    expect(queryByTestId('backlinks-unlinked-empty')).toBeNull();
  });

  it('Retry re-runs the fetch and clears the error', async () => {
    getBacklinksMock.mockRejectedValueOnce(Object.assign(new Error('boom'), { status: 500 }));
    getBacklinksMock.mockResolvedValue(RESPONSE);

    const { getByTestId, getAllByTestId } = render(() => <BacklinksPanel />);

    await waitFor(() => {
      expect(getByTestId('backlinks-error')).not.toBeNull();
    });
    const retry = getByTestId('backlinks-error').querySelector<HTMLButtonElement>(
      '[data-testid="empty-state-action"]',
    )!;
    expect(retry.textContent).toContain('Retry');

    fireEvent.click(retry);
    await waitFor(() => {
      expect(getAllByTestId('backlinks-linked-item')).toHaveLength(1);
    });
    expect(getBacklinksMock).toHaveBeenCalledTimes(2);
  });

  it('keeps the empty tone for a real empty answer', async () => {
    getBacklinksMock.mockResolvedValue({ ...RESPONSE, linked: [], unlinked: [] });

    const { getByTestId, queryByTestId } = render(() => <BacklinksPanel />);

    await waitFor(() => {
      expect(getByTestId('backlinks-linked-empty')).not.toBeNull();
    });
    expect(getByTestId('backlinks-linked-empty').getAttribute('data-tone')).toBe('empty');
    expect(getByTestId('backlinks-unlinked-empty').getAttribute('data-tone')).toBe('empty');
    expect(queryByTestId('backlinks-error')).toBeNull();
  });

  it('one-click Link wraps the mention as a wikilink in the open buffer', async () => {
    const { getAllByTestId, queryAllByTestId } = render(() => <BacklinksPanel />);
    await waitFor(() => {
      expect(getAllByTestId('backlinks-link-button')).toHaveLength(1);
    });

    fireEvent.click(getAllByTestId('backlinks-link-button')[0]);
    expect(updateFileContentMock).toHaveBeenCalledWith(
      '/kiln/notes/focused.md',
      '[[Other Note]] is mentioned here.',
    );
    // Applied suggestion disappears from the list.
    await waitFor(() => {
      expect(queryAllByTestId('backlinks-unlinked-item')).toHaveLength(0);
    });
  });
});
