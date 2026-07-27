import { describe, it, expect, vi, beforeEach } from 'vitest';

const getNoteMock = vi.fn();
const getConfigMock = vi.fn();
const resolveNotePathMock = vi.fn();
const openFileInEditorMock = vi.fn();

vi.mock('../api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getNote: (...args: unknown[]) => getNoteMock(...args),
  getConfig: (...args: unknown[]) => getConfigMock(...args),
  resolveNotePath: (...args: unknown[]) => resolveNotePathMock(...args),
}));

vi.mock('../file-actions', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openFileInEditor: (...args: unknown[]) => openFileInEditorMock(...args),
}));

import {
  noteAbsolutePath,
  kilnForPath,
  kilnRoot,
  fetchNotePreview,
  openNoteInEditor,
  clearNotePreviewCache,
  insertWikilink,
} from '../note-actions';

beforeEach(() => {
  vi.clearAllMocks();
  clearNotePreviewCache();
  // The on-disk resolver misses by default, so each test opts INTO it; the
  // index fallback stays the path most of these assertions exercise.
  resolveNotePathMock.mockRejectedValue(new Error('404 Not Found'));
});

describe('kilnRoot', () => {
  it('strips a trailing .crucible config dir to the kiln root', () => {
    expect(kilnRoot('/home/moot/crucible/docs/.crucible')).toBe(
      '/home/moot/crucible/docs'
    );
  });

  it('leaves a kiln root untouched (no-op once the registry reports the root)', () => {
    expect(kilnRoot('/home/moot/crucible/docs')).toBe('/home/moot/crucible/docs');
  });

  it('tolerates a trailing slash', () => {
    expect(kilnRoot('/vault/.crucible/')).toBe('/vault');
  });

  it('does not strip a .crucible that is a note name segment, only the config dir', () => {
    // A directory literally named ".crucible" mid-path is not the config dir suffix.
    expect(kilnRoot('/vault/.crucible/notes')).toBe('/vault/.crucible/notes');
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

describe('kilnForPath', () => {
  const kilns = [
    { path: '/home/moot/crucible/docs' },
    { path: '/home/moot/canvas-demo' },
    { path: '/home/moot/canvas-demo/Nested' },
  ];

  it('attributes a file to the kiln containing it, not the first or the active one', () => {
    expect(kilnForPath('/home/moot/canvas-demo/Notes/A.md', kilns)).toBe('/home/moot/canvas-demo');
  });

  it('gives a nested kiln its own files, so links cannot widen to the outer kiln', () => {
    expect(kilnForPath('/home/moot/canvas-demo/Nested/A.md', kilns)).toBe(
      '/home/moot/canvas-demo/Nested',
    );
  });

  it('requires a path-segment boundary', () => {
    expect(kilnForPath('/home/moot/canvas-demo-archive/A.md', kilns)).toBeUndefined();
  });

  it('returns undefined for a file in no kiln, rather than guessing one', () => {
    expect(kilnForPath('/etc/passwd', kilns)).toBeUndefined();
  });

  it('understands the registry reporting a kiln as its .crucible config dir', () => {
    expect(kilnForPath('/vault/A.md', [{ path: '/vault/.crucible' }])).toBe('/vault');
  });
});

describe('fetchNotePreview', () => {
  it('resolves note metadata', async () => {
    getNoteMock.mockResolvedValue({
      name: 'rust',
      path: 'notes/rust.md',
      title: 'Rust',
      tags: [],
      updated_at: '',
    });

    const preview = await fetchNotePreview('rust', '/kiln');
    expect(preview).toEqual({
      title: 'Rust',
      path: 'notes/rust.md',
      absPath: '/kiln/notes/rust.md',
    });
  });

  it('returns null for unresolvable notes and caches the miss', async () => {
    getNoteMock.mockRejectedValue(new Error('404 Not Found'));

    expect(await fetchNotePreview('ghost', '/kiln')).toBeNull();
    expect(await fetchNotePreview('ghost', '/kiln')).toBeNull();
    expect(getNoteMock).toHaveBeenCalledTimes(1);
  });

  it('caches hits per kiln and note name', async () => {
    getNoteMock.mockResolvedValue({
      name: 'rust',
      path: 'notes/rust.md',
      title: 'Rust',
      tags: [],
      updated_at: '',
    });

    await fetchNotePreview('rust', '/kiln');
    await fetchNotePreview('Rust', '/kiln'); // case-insensitive cache key
    expect(getNoteMock).toHaveBeenCalledTimes(1);
  });

  /**
   * The index is a fuzzy matcher: asking it for "Architecture" in a kiln that
   * has no such note happily returns "Component Architecture". A preview that
   * only ever asks the index therefore shows a DIFFERENT note than the click
   * would open, and shows nothing at all for a kiln that was never processed.
   * Same order as openNoteInEditor: exact on-disk path first, index second.
   */
  it('prefers the on-disk resolver over the fuzzy index, like opening does', async () => {
    resolveNotePathMock.mockResolvedValue({
      path: 'Notes/Architecture.md',
      absolutePath: '/kiln/Notes/Architecture.md',
      title: 'Architecture',
    });
    getNoteMock.mockResolvedValue({
      name: 'Component Architecture',
      path: 'Meta/Component Architecture.md',
      title: 'Component Architecture',
      tags: [],
      updated_at: '',
    });

    expect(await fetchNotePreview('Architecture', '/kiln')).toEqual({
      title: 'Architecture',
      path: 'Notes/Architecture.md',
      absPath: '/kiln/Notes/Architecture.md',
    });
    expect(resolveNotePathMock).toHaveBeenCalledWith('/kiln', 'Architecture');
    expect(getNoteMock).not.toHaveBeenCalled();
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
