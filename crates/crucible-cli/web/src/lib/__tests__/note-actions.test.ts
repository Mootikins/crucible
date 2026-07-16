import { describe, it, expect, vi, beforeEach } from 'vitest';

const getNoteMock = vi.fn();
const getConfigMock = vi.fn();
const getFileContentMock = vi.fn();
const openFileInEditorMock = vi.fn();

vi.mock('../api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getNote: (...args: unknown[]) => getNoteMock(...args),
  getConfig: (...args: unknown[]) => getConfigMock(...args),
  getFileContent: (...args: unknown[]) => getFileContentMock(...args),
}));

vi.mock('../file-actions', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openFileInEditor: (...args: unknown[]) => openFileInEditorMock(...args),
}));

import {
  noteExcerpt,
  noteAbsolutePath,
  fetchNotePreview,
  openNoteInEditor,
  clearNotePreviewCache,
  insertWikilink,
} from '../note-actions';

beforeEach(() => {
  vi.clearAllMocks();
  clearNotePreviewCache();
});

describe('noteExcerpt', () => {
  it('strips YAML frontmatter', () => {
    const content = '---\ntitle: X\ntags: [a]\n---\n\n# Heading\n\nBody text.';
    expect(noteExcerpt(content)).toBe('# Heading\n\nBody text.');
  });

  it('returns short content unchanged', () => {
    expect(noteExcerpt('Just a line.')).toBe('Just a line.');
  });

  it('truncates long content on a line boundary with ellipsis', () => {
    const content = Array.from({ length: 100 }, (_, i) => `Line number ${i}`).join('\n');
    const excerpt = noteExcerpt(content, 200);
    expect(excerpt.length).toBeLessThanOrEqual(202);
    expect(excerpt.endsWith('…')).toBe(true);
    // No half-line at the cut: every line before the ellipsis is intact.
    const lines = excerpt.split('\n');
    for (const line of lines.slice(0, -1)) {
      expect(line).toMatch(/^Line number \d+$/);
    }
  });
});

describe('noteAbsolutePath', () => {
  it('joins kiln-relative paths onto the kiln root', () => {
    expect(noteAbsolutePath('notes/rust.md', '/home/u/kiln')).toBe('/home/u/kiln/notes/rust.md');
    expect(noteAbsolutePath('notes/rust.md', '/home/u/kiln/')).toBe('/home/u/kiln/notes/rust.md');
  });

  it('keeps absolute paths as-is', () => {
    expect(noteAbsolutePath('/abs/note.md', '/kiln')).toBe('/abs/note.md');
  });
});

describe('fetchNotePreview', () => {
  it('resolves note metadata and content excerpt', async () => {
    getNoteMock.mockResolvedValue({
      name: 'rust',
      path: 'notes/rust.md',
      title: 'Rust',
      tags: [],
      updated_at: '',
    });
    getFileContentMock.mockResolvedValue('---\ntitle: Rust\n---\nRust is great.');

    const preview = await fetchNotePreview('rust', '/kiln');
    expect(preview).toEqual({
      title: 'Rust',
      path: 'notes/rust.md',
      absPath: '/kiln/notes/rust.md',
      excerpt: 'Rust is great.',
    });
    expect(getFileContentMock).toHaveBeenCalledWith('/kiln/notes/rust.md');
  });

  it('returns null for unresolvable notes and caches the miss', async () => {
    getNoteMock.mockRejectedValue(new Error('404 Not Found'));

    expect(await fetchNotePreview('ghost', '/kiln')).toBeNull();
    expect(await fetchNotePreview('ghost', '/kiln')).toBeNull();
    expect(getNoteMock).toHaveBeenCalledTimes(1);
  });

  it('degrades to metadata-only preview when the content read fails', async () => {
    getNoteMock.mockResolvedValue({
      name: 'rust',
      path: 'notes/rust.md',
      title: 'Rust',
      tags: [],
      updated_at: '',
    });
    getFileContentMock.mockRejectedValue(new Error('boom'));

    const preview = await fetchNotePreview('rust', '/kiln');
    expect(preview?.title).toBe('Rust');
    expect(preview?.excerpt).toBe('');
  });

  it('caches hits per kiln and note name', async () => {
    getNoteMock.mockResolvedValue({
      name: 'rust',
      path: 'notes/rust.md',
      title: 'Rust',
      tags: [],
      updated_at: '',
    });
    getFileContentMock.mockResolvedValue('content');

    await fetchNotePreview('rust', '/kiln');
    await fetchNotePreview('Rust', '/kiln'); // case-insensitive cache key
    expect(getNoteMock).toHaveBeenCalledTimes(1);
  });
});

describe('openNoteInEditor', () => {
  it('opens the resolved note by absolute path', async () => {
    getNoteMock.mockResolvedValue({
      name: 'rust',
      path: 'notes/rust.md',
      title: 'Rust',
      tags: [],
      updated_at: '',
    });

    await openNoteInEditor('rust', '/kiln');
    expect(openFileInEditorMock).toHaveBeenCalledWith('/kiln/notes/rust.md', 'Rust');
  });

  it('falls back to the configured kiln when none is given', async () => {
    getConfigMock.mockResolvedValue({ kiln_path: '/default-kiln' });
    getNoteMock.mockResolvedValue({
      name: 'rust',
      path: '/default-kiln/notes/rust.md',
      title: 'Rust',
      tags: [],
      updated_at: '',
    });

    await openNoteInEditor('rust');
    expect(getNoteMock).toHaveBeenCalledWith('rust', '/default-kiln');
    expect(openFileInEditorMock).toHaveBeenCalledWith('/default-kiln/notes/rust.md', 'Rust');
  });

  // Regression: the real GET /api/notes/{name} payload has NO `name` field
  // (path/title/tags/links only) — passing note.name straight through minted
  // tabs literally titled "undefined" ("Discard unsaved changes to
  // undefined?" on close).
  it('derives a tab title when the payload has no name field', async () => {
    getNoteMock.mockResolvedValue({
      path: 'Help/Wikilinks.md',
      title: null,
      tags: [],
      updated_at: '',
    });

    await openNoteInEditor('Wikilinks', '/kiln');
    expect(openFileInEditorMock).toHaveBeenCalledWith('/kiln/Help/Wikilinks.md', 'Wikilinks');
  });
});

describe('insertWikilink', () => {
  const s = (mention: string, target: string, offset: number) => ({ mention, target, offset });

  it('wraps the mention at the given offset', () => {
    expect(insertWikilink('Other Note is here.', s('Other Note', 'Other Note', 0))).toBe(
      '[[Other Note]] is here.',
    );
  });

  it('uses target|alias form when mention text differs from the target', () => {
    expect(insertWikilink('see rust today', s('rust', 'Rust Notes', 4))).toBe(
      'see [[Rust Notes|rust]] today',
    );
  });

  it('falls back to text search when the offset has drifted', () => {
    // Offset points elsewhere after an edit; the mention still exists.
    expect(insertWikilink('xx Other Note yy', s('Other Note', 'Other Note', 12))).toBe(
      'xx [[Other Note]] yy',
    );
  });

  it('returns null when the mention no longer exists', () => {
    expect(insertWikilink('nothing here', s('Other Note', 'Other Note', 0))).toBeNull();
  });

  it('refuses to double-wrap an existing wikilink', () => {
    expect(insertWikilink('[[Other Note]] is here.', s('Other Note', 'Other Note', 2))).toBeNull();
  });
});
