import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';
import { resetKilnsForTests } from '@/lib/query/kilns';

const FILE_PATH = '/repo/src/a.rs';
const CONTENT = ['one', 'two', 'three', 'four', 'five', 'six'].join('\n');

const openFilesValue = [{ path: FILE_PATH, content: CONTENT, dirty: false }];

vi.mock('@/contexts/EditorContext', () => ({
  useEditorSafe: () => ({
    openFiles: () => openFilesValue,
    activeFile: () => FILE_PATH,
    openFile: vi.fn(async () => {}),
    closeFile: vi.fn(),
    saveFile: vi.fn(async () => {}),
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
        autosaveSeconds: 0,
        vimMode: false,
        showSaveButton: true,
        maxLineWidth: 0,
        renderMath: true,
        renderDiagrams: true,
      },
    },
    updateSetting: vi.fn(),
  }),
}));

vi.mock('@/lib/file-actions', () => ({ findTabByFilePath: vi.fn(() => null) }));
vi.mock('@/stores/windowStore', () => ({
  windowActions: { updateTab: vi.fn() },
  windowStore: { tabGroups: {}, layout: { id: 'p', type: 'pane', tabGroupId: null } },
  setStore: vi.fn(),
}));

const { default: FileViewerPanel } = await import('../FileViewerPanel');
const { __resetReviewStore, pendingReveal, reviewActions } = await import('@/lib/review-store');

// The panel asks which kiln owns the open file. Nothing here is in one, and
// the empty roster now arrives over the fetch instead of from a module stub.
let kilnEnv: ReturnType<typeof createTestQueryEnv>;

beforeEach(() => {
  installFakeEventSource();
  resetKilnsForTests();
  kilnEnv = createTestQueryEnv({ 'GET /api/kilns': () => ({ kilns: [] }) });
});

afterEach(() => {
  cleanup();
  kilnEnv.restore();
  resetKilnsForTests();
  __resetReviewStore();
  vi.clearAllMocks();
});

describe('FileViewerPanel — reveal', () => {
  // The daemon no longer lists hunks, so the buffer draws no review layer.
  it('draws no review layer in the buffer', async () => {
    const { container } = render(() => <FileViewerPanel filePath={FILE_PATH} />);
    await waitFor(() => expect(container.querySelector('.cm-editor')).toBeTruthy());
    await new Promise((r) => setTimeout(r, 0));
    expect(container.querySelector('[class*="cm-review-"]')).toBeNull();
  });

  it('a reveal for THIS file scrolls the buffer and is consumed', async () => {
    const { container } = render(() => <FileViewerPanel filePath={FILE_PATH} />);
    await waitFor(() => expect(container.querySelector('.cm-editor')).toBeTruthy());

    reviewActions.reveal(FILE_PATH, 4);
    // The cursor lands on the line, which is what makes the jump
    // visible in a buffer the user may already have been scrolled inside.
    await waitFor(() =>
      expect(container.querySelector('.cm-activeLine')?.textContent).toBe('four'),
    );
    // Consumed, so a later panel mount cannot re-fire a stale jump.
    expect(pendingReveal()).toBeNull();
  });

  it('a reveal for another file is left alone for the panel that owns it', async () => {
    render(() => <FileViewerPanel filePath={FILE_PATH} />);
    await new Promise((r) => setTimeout(r, 0));

    reviewActions.reveal('/repo/src/other.rs', 4);
    await new Promise((r) => setTimeout(r, 0));
    expect(pendingReveal()).toEqual({ path: '/repo/src/other.rs', line: 4 });
  });
});
