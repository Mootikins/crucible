import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import type { ToolCallDisplay } from '@/lib/types';

// Real diff extraction/apply; stub the heavy DiffViewer.
vi.mock('../DiffViewer', () => ({
  DiffViewer: () => <div data-testid="diff-viewer" />,
}));
vi.mock('../MultiEditDiff', () => ({ MultiEditDiff: () => <div data-testid="multi-edit-diff" /> }));

// The file's current on-disk content the button diffs against.
const CURRENT = 'top\nlet x = 1;\nbottom\n';
const getFileContentMock = vi.fn(async (..._a: unknown[]) => CURRENT);
const openFileWithDiffMock = vi.fn();
const addNotificationMock = vi.fn();

vi.mock('@/lib/api', async (orig) => ({
  ...(await orig<Record<string, unknown>>()),
  getFileContent: (...a: unknown[]) => getFileContentMock(...a),
}));
vi.mock('@/lib/file-actions', () => ({
  openFileWithDiff: (...a: unknown[]) => openFileWithDiffMock(...a),
}));
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotificationMock(...a) },
}));

import { ToolCard } from '../ToolCard';

const editTool = (): ToolCallDisplay => ({
  id: 'tc-1',
  name: 'Edit',
  status: 'complete',
  args: JSON.stringify({
    file_path: '/proj/app.ts',
    old_string: 'let x = 1;',
    new_string: 'let x = 42;',
  }),
});

beforeEach(() => vi.clearAllMocks());

describe('ToolCard — Open in editor', () => {
  it('opens the file with the current content diffed against the applied edit', async () => {
    render(() => <ToolCard toolCall={editTool()} />);
    // Expand the card to reveal the diff section + the button.
    fireEvent.click(screen.getByText('Edit'));

    const btn = await screen.findByTestId('tool-open-in-editor');
    fireEvent.click(btn);

    await waitFor(() => expect(openFileWithDiffMock).toHaveBeenCalledTimes(1));
    expect(getFileContentMock).toHaveBeenCalledWith('/proj/app.ts');
    // original = current file; proposed = current with the edit applied.
    expect(openFileWithDiffMock).toHaveBeenCalledWith(
      '/proj/app.ts',
      CURRENT,
      'top\nlet x = 42;\nbottom\n',
      'app.ts',
    );
  });

  it('has no Open-in-editor button for a non-diff tool', () => {
    render(() => <ToolCard toolCall={{ id: 't', name: 'read_file', args: '{}', status: 'complete', result: 'ok' }} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.queryByTestId('tool-open-in-editor')).not.toBeInTheDocument();
  });

  it('has no Open-in-editor button while the tool is still running', () => {
    render(() => <ToolCard toolCall={{ ...editTool(), status: 'running' }} />);
    fireEvent.click(screen.getByText('Edit'));
    expect(screen.queryByTestId('tool-open-in-editor')).not.toBeInTheDocument();
  });

  // An unreadable path with an Edit means we have no baseline; applying the
  // edit to '' would yield an EMPTY proposed document — a delete-everything
  // diff the user could save over their file.
  it('refuses to open an Edit diff when the file cannot be read', async () => {
    getFileContentMock.mockRejectedValueOnce(new Error('404'));
    render(() => <ToolCard toolCall={editTool()} />);
    fireEvent.click(screen.getByText('Edit'));
    fireEvent.click(await screen.findByTestId('tool-open-in-editor'));

    await waitFor(() => expect(addNotificationMock).toHaveBeenCalledTimes(1));
    expect(addNotificationMock.mock.calls[0][0]).toBe('warning');
    expect(openFileWithDiffMock).not.toHaveBeenCalled();
  });

  // A Write carries the whole file, so an unreadable path is just a new file.
  it('opens a Write diff against empty when the file does not exist yet', async () => {
    getFileContentMock.mockRejectedValueOnce(new Error('404'));
    const writeTool: ToolCallDisplay = {
      id: 'tc-2',
      name: 'Write',
      status: 'complete',
      args: JSON.stringify({ file_path: '/proj/new.ts', content: 'fresh\n' }),
    };
    render(() => <ToolCard toolCall={writeTool} />);
    fireEvent.click(screen.getByText('Write'));
    fireEvent.click(await screen.findByTestId('tool-open-in-editor'));

    await waitFor(() => expect(openFileWithDiffMock).toHaveBeenCalledTimes(1));
    expect(openFileWithDiffMock).toHaveBeenCalledWith('/proj/new.ts', '', 'fresh\n', 'new.ts');
    expect(addNotificationMock).not.toHaveBeenCalled();
  });

  it('ignores repeat clicks while a diff is still being fetched', async () => {
    let release!: (v: string) => void;
    getFileContentMock.mockReturnValueOnce(new Promise<string>((r) => { release = r; }));
    render(() => <ToolCard toolCall={editTool()} />);
    fireEvent.click(screen.getByText('Edit'));

    const btn = await screen.findByTestId('tool-open-in-editor');
    fireEvent.click(btn);
    fireEvent.click(btn);
    release(CURRENT);

    await waitFor(() => expect(openFileWithDiffMock).toHaveBeenCalledTimes(1));
    expect(getFileContentMock).toHaveBeenCalledTimes(1);
  });
});
