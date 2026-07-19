import { describe, it, expect, vi, beforeEach } from 'vitest';
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

const RESPONSE: BacklinksResponse = {
  note: { path: 'notes/focused.md', abs_path: '/kiln/notes/focused.md', title: 'Focused Note' },
  linked: [
    { name: 'linker', path: 'notes/linker.md', abs_path: '/kiln/notes/linker.md', title: 'Linker Note' },
  ],
  unlinked: [{ mention: 'Other Note', target: 'Other Note', offset: 0 }],
};

beforeEach(() => {
  vi.clearAllMocks();
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
