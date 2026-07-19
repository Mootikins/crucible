import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// The old test read FilesPanel.tsx as a string and asserted the SOURCE did not
// contain emoji and did contain Lucide identifiers ("FileText", "Folder", …).
// That passes even if the component never renders. Here we RENDER FilesPanel
// with a mocked project + note listing and assert the emitted DOM: file rows
// carry real <svg> icons (Lucide) and no emoji glyph reaches the screen.

const EMOJI = ['📝', '🔷', '🟨', '🦀', '📋', '⚙️', '🎨', '🌐', '🌙', '📄', '📂', '📁'];

const listNotesMock = vi.fn();
const listKilnsMock = vi.fn();

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
  localStorage.clear();
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

describe('FilesPanel — rendered icons', () => {
  it('renders a Lucide <svg> icon for every file row (never emoji text)', async () => {
    const { container, findByText } = render(() => <FilesPanel />);

    // Wait for the async note listing to populate the tree.
    await findByText('readme.md');
    for (const name of NOTE_NAMES) {
      await findByText(name);
    }

    // Each file row renders exactly one FileIcon <svg>; there are as many
    // rows as notes, so at least that many svgs must be present.
    await waitFor(() => {
      const svgs = container.querySelectorAll('svg');
      expect(svgs.length).toBeGreaterThanOrEqual(NOTE_NAMES.length);
    });
  });

  it('emits no emoji glyphs anywhere in the rendered panel', async () => {
    const { container, findByText } = render(() => <FilesPanel />);
    await findByText('readme.md');

    const text = container.textContent ?? '';
    for (const glyph of EMOJI) {
      expect(text).not.toContain(glyph);
    }
  });

  // Note: FilesPanel lists notes only (is_dir is always false), so FolderIcon
  // and ChevronIcon — the directory-only icons — are not reachable through this
  // panel's data. The emoji-free assertion above still covers the whole
  // rendered subtree, and the file-icon svg assertion covers FileIcon directly.
});
