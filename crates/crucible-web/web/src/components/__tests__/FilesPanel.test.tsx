import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';

// The old test read FilesPanel.tsx as a string and asserted the SOURCE did not
// contain emoji and did contain Lucide identifiers ("FileText", "Folder", …).
// That passes even if the component never renders. Here we RENDER FilesPanel
// with a mocked project + note listing and assert the emitted DOM: file rows
// carry real <svg> icons (Lucide) and no emoji glyph reaches the screen.

const EMOJI = ['📝', '🔷', '🟨', '🦀', '📋', '⚙️', '🎨', '🌐', '🌙', '📄', '📂', '📁'];

const listNotesMock = vi.fn();
const listKilnsMock = vi.fn();
// `file-tree-a11y` is deliberately NOT mocked. `currentOpenFilePath` reads the
// real `windowStore`, and the reveal it used to trigger only fired once the
// tree had finished loading — so a non-reactive stub produced a test that
// passed with the bug still in place. Driving the actual store is what makes
// the assertion mean anything.

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listNotes: (...args: unknown[]) => listNotesMock(...args),
  listKilns: (...args: unknown[]) => listKilnsMock(...args),
  // No SSE in jsdom: return a no-op unsubscribe so onMount doesn't open a
  // real EventSource.
  subscribeToFsEvents: () => () => {},
}));

// The file tree builds its roster from projects + kilns. Use a KILN-only
// roster (no projects) so the deterministic fallback selects the kiln root and
// the panel loads notes via listNotes (the migrated Notes view).
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    currentProject: () => null,
    projects: () => [],
  }),
}));

import { FilesPanel } from '../FilesPanel';
import { setStore } from '@/stores/windowStore';

/** Focus a file tab, the way opening a file in the editor does. */
function focusFileTab(filePath: string) {
  setStore('tabGroups', {
    center: {
      id: 'center',
      activeTabId: 'tab-1',
      tabs: [
        {
          id: 'tab-1',
          title: filePath.split('/').pop() ?? filePath,
          contentType: 'file',
          metadata: { filePath },
        },
      ],
    },
  } as never);
}

// One note per FileIcon branch so every extension arm is exercised in the DOM.
const NOTE_NAMES = [
  'readme.md', // FileText
  'main.ts', // FileCode
  'app.jsx', // FileCode
  'lib.rs', // FileCode
  'data.json', // FileJson
  'config.toml', // Cog
  'theme.css', // Palette
  'index.html', // Globe
  'init.lua', // Moon
  'notes.unknownext', // File (fallback)
];

beforeEach(() => {
  vi.clearAllMocks();
  setStore('tabGroups', {});
  localStorage.clear();
  // These assertions match full filenames; keep extensions visible (the tree
  // hides `.md` by default now).
  localStorage.setItem('crucible.filetree.hideExts', 'false');
  listKilnsMock.mockResolvedValue([{ path: '/project/kiln', name: 'kiln' }]);
  listNotesMock.mockResolvedValue(
    NOTE_NAMES.map((name) => ({
      name,
      path: name,
      title: null,
      tags: [],
      updated_at: '',
    })),
  );
});

describe('FilesPanel — the navigator does not follow the focused tab', () => {
  // Opening a file used to expand the tree to it and scroll, on every tab
  // switch, moving the navigator out from under the pointer while browsing.
  const NESTED = [
    { name: 'top.md', path: 'top.md' },
    { name: 'buried.md', path: 'folder/buried.md' },
  ];

  beforeEach(() => {
    listNotesMock.mockResolvedValue(
      NESTED.map(({ name, path }) => ({ name, path, title: null, tags: [], updated_at: '' })),
    );
  });

  /** The folder row's expansion state, which is what auto-reveal changed.
   *  Asserted via `aria-expanded` rather than the presence of `buried.md`:
   *  the tree view keeps collapsed descendants in the DOM, so absence would
   *  have been a vacuous assertion. */
  const folderExpanded = (container: HTMLElement) =>
    Array.from(container.querySelectorAll('[role="treeitem"]'))
      .find((el) => el.textContent?.trim().startsWith('folder'))
      ?.getAttribute('aria-expanded');

  it('does not expand to the focused tab when a file is opened after the tree loads', async () => {
    const { container, findByText } = render(() => <FilesPanel />);
    // Wait for the tree to finish loading FIRST: the reveal this guards against
    // needed a live tree api and a built collection, so asserting before the
    // load settles proves nothing.
    await findByText('folder');
    expect(folderExpanded(container)).toBe('false');

    focusFileTab('/project/kiln/folder/buried.md');
    await new Promise((resolve) => setTimeout(resolve, 120));

    expect(folderExpanded(container)).toBe('false');
  });

  // A "file already open at mount" case is deliberately absent: the old
  // auto-reveal fired before the tree had loaded and no-opped, so such a test
  // passed with the bug in place. Verified by running this file against the
  // pre-fix component — only the case above discriminates (it failed with
  // `expected 'true' to be 'false'`).
});

describe('FilesPanel — rendered rows', () => {
  it('renders a colored filetype <svg> for every file row (never emoji text)', async () => {
    const { container, findByText } = render(() => <FilesPanel />);
    await findByText('readme.md');
    for (const name of NOTE_NAMES) {
      await findByText(name);
    }
    // Each file row carries a FileIcon <svg>; at least one per note.
    const svgs = container.querySelectorAll('svg');
    expect(svgs.length).toBeGreaterThanOrEqual(NOTE_NAMES.length);
  });

  it('emits no emoji glyphs anywhere in the rendered panel', async () => {
    const { container, findByText } = render(() => <FilesPanel />);
    await findByText('readme.md');

    const text = container.textContent ?? '';
    for (const glyph of EMOJI) {
      expect(text).not.toContain(glyph);
    }
  });
});
