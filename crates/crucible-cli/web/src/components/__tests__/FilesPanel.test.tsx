import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// The old test read FilesPanel.tsx as a string and asserted the SOURCE did not
// contain emoji and did contain Lucide identifiers ("FileText", "Folder", …).
// That passes even if the component never renders. Here we RENDER FilesPanel
// with a mocked project + note listing and assert the emitted DOM: file rows
// carry real <svg> icons (Lucide) and no emoji glyph reaches the screen.

const EMOJI = ['📝', '🔷', '🟨', '🦀', '📋', '⚙️', '🎨', '🌐', '🌙', '📄', '📂', '📁'];

const listNotesMock = vi.fn();

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listNotes: (...args: unknown[]) => listNotesMock(...args),
}));

// FilesPanel reads the current project from ProjectContext to decide which
// kiln to list. Provide a stable project with one kiln.
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    currentProject: () => ({
      path: '/project',
      name: 'Project',
      kilns: [{ path: '/project/kiln', name: 'kiln' }],
      last_accessed: '',
    }),
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
