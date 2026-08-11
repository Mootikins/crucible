import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, waitFor } from '@solidjs/testing-library';
import type { ComposedHunk } from '@/lib/review-types';

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
vi.mock('@/lib/api', () => ({
  listKilns: vi.fn(async () => []),
  subscribeToEvents: () => () => {},
}));
const listReviewHunks = vi.fn();
vi.mock('@/lib/review-api', () => ({
  listReviewHunks: (...a: unknown[]) => listReviewHunks(...a),
  setHunkState: vi.fn(),
  addReviewComment: vi.fn(),
  resolveReviewComment: vi.fn(),
}));

const { default: FileViewerPanel } = await import('../FileViewerPanel');
const { __resetReviewStore, pendingReveal, reviewActions, revealedToolCall } = await import(
  '@/lib/review-store'
);

function hunk(over: Partial<ComposedHunk> = {}): ComposedHunk {
  return {
    id: 'h1',
    root: '/repo',
    path: 'src/a.rs',
    base_range: { start: 2, end: 4 },
    current_range: { start: 2, end: 4 },
    before_content: 'x\n',
    after_content: 'y\n',
    tool_call_ids: ['call-1'],
    state: 'unreviewed',
    reapplied: false,
    ...over,
  };
}

const seed = async (hunks: ComposedHunk[]) => {
  listReviewHunks.mockImplementation(async () => ({
    session_id: 's1',
    hunks: structuredClone(hunks),
    comments: [],
  }));
  await reviewActions.refresh('s1');
};

beforeEach(() => {
  listReviewHunks.mockResolvedValue({ session_id: 's1', hunks: [], comments: [] });
});

afterEach(() => {
  cleanup();
  __resetReviewStore();
  vi.clearAllMocks();
});

describe('FileViewerPanel — inline review layer', () => {
  it('decorates the composed hunks in the real buffer, with an attribution chip', async () => {
    await seed([hunk()]);
    const { container } = render(() => <FileViewerPanel filePath={FILE_PATH} />);

    // Review happens INLINE — same buffer, same highlighting, same folding —
    // rather than in a second side-by-side viewer.
    await waitFor(() =>
      expect(container.querySelectorAll('.cm-review-unreviewed')).toHaveLength(2),
    );
    const chip = container.querySelector('.cm-review-chip')!;
    expect(chip.getAttribute('data-hunk-id')).toBe('h1');
  });

  it('the gutter chip points the transcript at the tool call that wrote it', async () => {
    await seed([hunk()]);
    const { container } = render(() => <FileViewerPanel filePath={FILE_PATH} />);
    await waitFor(() => expect(container.querySelector('.cm-review-chip')).toBeTruthy());

    container
      .querySelector('.cm-review-chip')!
      .dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(revealedToolCall()).toBe('call-1');
  });

  it('hunks belonging to another file never reach this buffer', async () => {
    await seed([hunk({ path: 'src/other.rs' })]);
    const { container } = render(() => <FileViewerPanel filePath={FILE_PATH} />);
    await waitFor(() => expect(container.querySelector('.cm-editor')).toBeTruthy());
    // Give the deferred install a turn before asserting the absence.
    await new Promise((r) => setTimeout(r, 0));
    expect(container.querySelectorAll('.cm-review-chip')).toHaveLength(0);
  });

  it('an external hunk is marked as unowned, not blamed on a tool', async () => {
    await seed([hunk({ tool_call_ids: [] })]);
    const { container } = render(() => <FileViewerPanel filePath={FILE_PATH} />);
    await waitFor(() => expect(container.querySelectorAll('.cm-review-external')).toHaveLength(2));
    expect(container.querySelector('.cm-review-chip')!.textContent).toBe('external');
  });

  it('a reveal for THIS file scrolls the buffer and is consumed', async () => {
    await seed([hunk()]);
    const { container } = render(() => <FileViewerPanel filePath={FILE_PATH} />);
    await new Promise((r) => setTimeout(r, 0));

    reviewActions.reveal(FILE_PATH, 4);
    // The cursor lands on the hunk's first line, which is what makes the jump
    // visible in a buffer the user may already have been scrolled inside.
    await waitFor(() =>
      expect(container.querySelector('.cm-activeLine')?.textContent).toBe('four'),
    );
    // Consumed, so a later panel mount cannot re-fire a stale jump.
    expect(pendingReveal()).toBeNull();
  });

  it('a reveal for another file is left alone for the panel that owns it', async () => {
    await seed([hunk()]);
    render(() => <FileViewerPanel filePath={FILE_PATH} />);
    await new Promise((r) => setTimeout(r, 0));

    reviewActions.reveal('/repo/src/other.rs', 4);
    await new Promise((r) => setTimeout(r, 0));
    expect(pendingReveal()).toEqual({ path: '/repo/src/other.rs', line: 4 });
  });
});
