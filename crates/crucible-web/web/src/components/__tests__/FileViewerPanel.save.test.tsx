import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from '@solidjs/testing-library';

// A dirty, open file. Mirrors the real EditorContext shape (useEditorSafe).
const saveFile = vi.fn(async () => {});
const FILE_PATH = '/kiln/notes/from-tui.md';

let autosaveSeconds = 0;

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

vi.mock('@/contexts/SettingsContext', () => ({
  useSettingsSafe: () => ({
    settings: {
      editor: {
        get autosaveSeconds() {
          return autosaveSeconds;
        },
        vimMode: false,
        showSaveButton: true,
      },
    },
    updateSetting: vi.fn(),
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

describe('FileViewerPanel — save UX', () => {
  beforeEach(() => {
    saveFile.mockClear();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders no in-panel save toolbar (saving lives in keybinds/status bar/autosave)', () => {
    const { container } = render(() => <FileViewerPanel filePath={FILE_PATH} />);
    expect(container.querySelector('[data-testid="file-save"]')).toBeNull();
    expect(container.querySelector('[data-testid="file-dirty-indicator"]')).toBeNull();
  });

  it('autosaves a dirty buffer after the configured idle interval', () => {
    autosaveSeconds = 2;
    render(() => <FileViewerPanel filePath={FILE_PATH} />);
    expect(saveFile).not.toHaveBeenCalled();
    vi.advanceTimersByTime(2100);
    expect(saveFile).toHaveBeenCalledWith(FILE_PATH);
  });

  it('autosave stays off at 0 seconds', () => {
    autosaveSeconds = 0;
    render(() => <FileViewerPanel filePath={FILE_PATH} />);
    vi.advanceTimersByTime(10_000);
    expect(saveFile).not.toHaveBeenCalled();
  });
});
