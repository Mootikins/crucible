import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';

// A dirty, open file so the Save affordance is enabled. Mirrors the real
// EditorContext shape (useEditorSafe).
const saveFile = vi.fn(async () => {});
const FILE_PATH = '/kiln/notes/from-tui.md';

vi.mock('@/contexts/EditorContext', () => ({
  useEditorSafe: () => ({
    openFiles: () => [{ path: FILE_PATH, content: 'hello', dirty: true }],
    activeFile: () => FILE_PATH,
    openFile: vi.fn(async () => {}),
    closeFile: vi.fn(),
    saveFile,
    setActiveFile: vi.fn(),
    updateFileContent: vi.fn(),
    isLoading: () => false,
    error: () => null,
  }),
}));

vi.mock('@/lib/file-actions', () => ({
  findTabByFilePath: vi.fn(() => null),
}));

vi.mock('@/stores/windowStore', () => ({
  windowActions: { updateTab: vi.fn() },
  windowStore: { tabGroups: {}, layout: { id: 'pane-1', type: 'pane', tabGroupId: null } },
  setStore: vi.fn(),
}));

const { default: FileViewerPanel } = await import('../FileViewerPanel');

describe('FileViewerPanel — Save affordance (bug 4)', () => {
  beforeEach(() => {
    saveFile.mockClear();
  });

  it('shows an enabled Save button and a dirty indicator for a modified file', () => {
    render(() => <FileViewerPanel filePath={FILE_PATH} />);
    const save = screen.getByTestId('file-save') as HTMLButtonElement;
    expect(save).toBeInTheDocument();
    expect(save.disabled).toBe(false);
    expect(screen.getByTestId('file-dirty-indicator')).toBeInTheDocument();
  });

  it('clicking Save calls EditorContext.saveFile with the active file path', () => {
    render(() => <FileViewerPanel filePath={FILE_PATH} />);
    fireEvent.click(screen.getByTestId('file-save'));
    expect(saveFile).toHaveBeenCalledWith(FILE_PATH);
  });
});
