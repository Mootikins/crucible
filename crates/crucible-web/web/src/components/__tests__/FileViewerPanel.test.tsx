import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';

// FileViewerPanel calls useEditorSafe() + useSettingsSafe() unconditionally.
// The editor's reactive shape varies per test (empty for "rendering", a dirty
// file for "save UX"), so the mock reads module-level state at call time and
// each describe's beforeEach stages the value its tests need.
const saveFile = vi.fn(async () => {});
const openFileSpy = vi.fn(async () => {});
const FILE_PATH = '/kiln/notes/from-tui.md';

let openFilesValue: { path: string; content: string; dirty: boolean }[] = [];
let activeFileValue: string | null = null;
let autosaveSeconds = 0;

// Mock EditorContext — FileViewerPanel calls useEditorSafe()
vi.mock('@/contexts/EditorContext', () => ({
  useEditorSafe: () => ({
    openFiles: () => openFilesValue,
    activeFile: () => activeFileValue,
    openFile: openFileSpy,
    closeFile: vi.fn(),
    saveFile,
    setActiveFile: vi.fn(),
    updateFileContent: vi.fn(),
    isLoading: () => false,
    error: () => null,
  }),
}));

// Mock SettingsContext — save UX reads autosaveSeconds; "rendering" tests get 0
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

// Mock file-actions (used internally by FileViewerPanel for dirty sync)
vi.mock('@/lib/file-actions', () => ({
  findTabByFilePath: vi.fn(() => null),
}));

// Mock windowStore actions (used internally by FileViewerPanel for dirty sync)
vi.mock('@/stores/windowStore', () => ({
  windowActions: {
    updateTab: vi.fn(),
  },
  windowStore: {
    tabGroups: {},
    layout: { id: 'pane-1', type: 'pane', tabGroupId: null },
  },
  setStore: vi.fn(),
}));

// Dynamically import after mocks are in place
const { default: FileViewerPanel } = await import('../FileViewerPanel');

// Web test convention: unmount between tests so Solid reactive roots don't
// leak across describes.
afterEach(cleanup);

// The panel registry is a module-level singleton. The first describe seeds it
// via registerPanels(); reset it before EVERY test (not just that describe) so
// the "rendering" describe can never inherit a registry populated by an earlier
// run, regardless of describe order.
beforeEach(() => {
  resetGlobalRegistry();
});

describe('FileViewerPanel — panel registry', () => {

  it('registers "file" content type via registerPanels()', () => {
    registerPanels();
    const panel = getGlobalRegistry().get('file');
    expect(panel).toBeDefined();
    expect(panel!.id).toBe('file');
    expect(panel!.title).toBe('File');
    expect(panel!.defaultZone).toBe('center');
  });

  it('registers file panel with a valid component', () => {
    registerPanels();
    const panel = getGlobalRegistry().get('file');
    expect(panel).toBeDefined();
    expect(typeof panel!.component).toBe('function');
  });
});

describe('FileViewerPanel — rendering', () => {
  beforeEach(() => {
    openFilesValue = [];
    activeFileValue = null;
    vi.clearAllMocks();
  });

  it('renders "No file selected" when filePath is undefined', () => {
    render(() => <FileViewerPanel />);
    expect(screen.getByText('No file selected')).toBeInTheDocument();
  });

  it('renders "No file selected" when filePath is not provided', () => {
    render(() => <FileViewerPanel filePath={undefined} />);
    expect(screen.getByText('No file selected')).toBeInTheDocument();
  });

  it('renders loading fallback when filePath is provided but file not yet loaded', () => {
    render(() => <FileViewerPanel filePath="/docs/readme.md" />);
    // With mocked useEditorSafe returning empty openFiles and isLoading=false,
    // the component shows the "Loading file..." fallback inside the <Show> fallback
    expect(screen.getByText('Loading file...')).toBeInTheDocument();
  });

  it('shows an image instead of asking the text endpoint for its bytes', () => {
    // Opening a PNG used to hand it to the editor, which fetches
    // /api/kiln/file — a text read that fails on the first non-UTF-8 byte and
    // answered 404, so an image in the tree could not be opened at all.
    render(() => <FileViewerPanel filePath="/kiln/assets/shot.png" />);

    const img = screen.getByTestId('file-image') as HTMLImageElement;
    expect(img.getAttribute('src')).toBe(
      '/api/file/raw?path=%2Fkiln%2Fassets%2Fshot.png',
    );
    // No editor, and no "Loading file..." spinner waiting on a text read that
    // is never coming.
    expect(document.querySelector('.cm-editor')).toBeNull();
    expect(screen.queryByText('Loading file...')).toBeNull();
  });

  it('gives an image the zoom toolbar, with shortcuts named for screen readers', () => {
    render(() => <FileViewerPanel filePath="/kiln/assets/shot.png" />);

    expect(screen.getByLabelText('Zoom in (plus key)')).toBeInTheDocument();
    expect(screen.getByLabelText('Zoom out (minus key)')).toBeInTheDocument();
    expect(screen.getByLabelText('Fit to pane (zero key)')).toBeInTheDocument();
    expect(screen.getByLabelText('Actual size, 100% (one key)')).toBeInTheDocument();
    // Opens at fit; with no measured pane/image that reads as 100%.
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('100%');
  });

  it('zoom buttons and keyboard shortcuts move the zoom level', () => {
    render(() => <FileViewerPanel filePath="/kiln/assets/shot.png" />);

    fireEvent.click(screen.getByTestId('image-zoom-in'));
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('125%');
    fireEvent.click(screen.getByTestId('image-zoom-in'));
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('156%');
    fireEvent.click(screen.getByTestId('image-zoom-out'));
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('125%');

    const scroller = screen.getByTestId('image-scroller');
    fireEvent.keyDown(scroller, { key: '0' });
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('100%');
    fireEvent.keyDown(scroller, { key: '+' });
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('125%');
    fireEvent.keyDown(scroller, { key: '-' });
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('100%');
    fireEvent.click(screen.getByTestId('image-zoom-actual'));
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('100%');
  });

  it('a different image resets zoom to fit', () => {
    const [path, setPath] = createSignal('/kiln/assets/shot.png');
    render(() => <FileViewerPanel filePath={path()} />);

    fireEvent.click(screen.getByTestId('image-zoom-in'));
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('125%');

    setPath('/kiln/assets/other.png');
    expect(screen.getByTestId('image-zoom-level').textContent).toBe('100%');
    expect(screen.getByTestId('file-image').getAttribute('src')).toBe(
      '/api/file/raw?path=%2Fkiln%2Fassets%2Fother.png',
    );
  });

  it('leaves text files to the editor', () => {
    render(() => <FileViewerPanel filePath="/kiln/notes/note.md" />);
    expect(screen.queryByTestId('file-image')).toBeNull();
    expect(openFileSpy).toHaveBeenCalledWith('/kiln/notes/note.md', expect.anything());
  });

  it('never asks the text endpoint to open an image', () => {
    // Rendering an <img> is only half of it: the load effect runs whatever the
    // panel returns, so without a guard the panel still fired the text read
    // and took a 404 for a file it was already displaying correctly.
    render(() => <FileViewerPanel filePath="/kiln/assets/shot.png" />);
    expect(openFileSpy).not.toHaveBeenCalled();
  });
});

describe('FileViewerPanel — save UX', () => {
  beforeEach(() => {
    openFilesValue = [{ path: FILE_PATH, content: 'hello', dirty: true }];
    activeFileValue = FILE_PATH;
    // Each test sets its own value, but pinning the default keeps test ordering robust.
    autosaveSeconds = 0;
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
