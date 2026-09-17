import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const kilnInUse = vi.hoisted(() => ({ path: null as string | null }));
vi.mock('@/stores/kilnStore', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  mostRecentKilnPath: () => kilnInUse.path,
}));
const openFileInEditorMock = vi.fn();

// No `vi.mock('../api')`: nothing under test imports it any more. The note
// metadata and config reads these cases guard against have no module double
// left to stub — the claims are proven by what reaches the daemon below.

vi.mock('../file-actions', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openFileInEditor: (...args: unknown[]) => openFileInEditorMock(...args),
}));

import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import {
  noteAbsolutePath,
  kilnForPath,
  kilnRoot,
  fetchNotePreview,
  openNoteInEditor,
  clearNotePreviewCache,
  insertWikilink,
} from '../note-actions';

/**
 * The resolver answers on the WIRE, not through a mocked module.
 *
 * Its answers are held in the query cache now, and a mock counts the calls
 * that reach the module rather than the ones that reach the daemon — so a
 * caller that went around the cache would still look like one call.
 */
let env: TestQueryEnv;
/** The `(kiln, name)` of every resolve the daemon answered, in order. */
let resolved: [string, string][] = [];
/** The hit `GET /api/notes/resolve` answers next, or `null` for a miss. */
let resolveHit: { path: string; absolutePath: string; title?: string } | null = null;

/** What the daemon answers for the next resolve. Absent means a miss. */
function setResolveHit(hit: { path: string; absolutePath: string; title?: string }): void {
  resolveHit = hit;
}

beforeEach(() => {
  vi.clearAllMocks();
  resolved = [];
  // The on-disk resolver misses by default, so each test opts INTO it; the
  // index fallback stays the path most of these assertions exercise.
  resolveHit = null;
  env = createTestQueryEnv({
    'GET /api/notes/resolve': (request: Request) => {
      const params = new URL(request.url).searchParams;
      resolved.push([params.get('kiln') ?? '', params.get('name') ?? '']);
      if (resolveHit) return resolveHit;
      return new Response(JSON.stringify({ error: { code: 404, message: 'Not Found' } }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      });
    },
  });
  clearNotePreviewCache();
});

afterEach(() => env?.restore());

describe('kilnRoot', () => {
  it('strips a trailing .crucible config dir to the kiln root', () => {
    expect(kilnRoot('/home/user/crucible/docs/.crucible')).toBe(
      '/home/user/crucible/docs'
    );
  });

  it('leaves a kiln root untouched (no-op once the registry reports the root)', () => {
    expect(kilnRoot('/home/user/crucible/docs')).toBe('/home/user/crucible/docs');
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
    { path: '/home/user/crucible/docs' },
    { path: '/home/user/canvas-demo' },
    { path: '/home/user/canvas-demo/Nested' },
  ];

  it('attributes a file to the kiln containing it, not the first or the active one', () => {
    expect(kilnForPath('/home/user/canvas-demo/Notes/A.md', kilns)).toBe('/home/user/canvas-demo');
  });

  it('gives a nested kiln its own files, so links cannot widen to the outer kiln', () => {
    expect(kilnForPath('/home/user/canvas-demo/Nested/A.md', kilns)).toBe(
      '/home/user/canvas-demo/Nested',
    );
  });

  it('requires a path-segment boundary', () => {
    expect(kilnForPath('/home/user/canvas-demo-archive/A.md', kilns)).toBeUndefined();
  });

  it('returns undefined for a file in no kiln, rather than guessing one', () => {
    expect(kilnForPath('/etc/passwd', kilns)).toBeUndefined();
  });

  it('understands the registry reporting a kiln as its .crucible config dir', () => {
    expect(kilnForPath('/vault/A.md', [{ path: '/vault/.crucible' }])).toBe('/vault');
  });
});

describe('fetchNotePreview', () => {
  it('resolves note metadata via the on-disk resolver', async () => {
    setResolveHit({
      path: 'notes/rust.md',
      absolutePath: '/kiln/notes/rust.md',
      title: 'Rust',
    });

    const preview = await fetchNotePreview('rust', '/kiln');
    expect(preview).toEqual({
      title: 'Rust',
      path: 'notes/rust.md',
      absPath: '/kiln/notes/rust.md',
    });
  });

  it('returns null for unresolvable notes and caches the miss', async () => {
    expect(await fetchNotePreview('ghost', '/kiln')).toBeNull();
    expect(await fetchNotePreview('ghost', '/kiln')).toBeNull();
    expect(resolved).toHaveLength(1);
  });

  it('caches hits per kiln and note name', async () => {
    setResolveHit({
      path: 'notes/rust.md',
      absolutePath: '/kiln/notes/rust.md',
      title: 'Rust',
    });

    await fetchNotePreview('rust', '/kiln');
    await fetchNotePreview('Rust', '/kiln'); // case-insensitive cache key
    expect(resolved).toHaveLength(1);
  });

  /**
   * The index is a fuzzy matcher: asking it for "Architecture" in a kiln that
   * has no such note happily returns "Component Architecture". A preview that
   * only ever asks the index therefore shows a DIFFERENT note than the click
   * would open, and shows nothing at all for a kiln that was never processed.
   * Same order as openNoteInEditor: exact on-disk path, then a unique filename stem.
   */
  it('prefers the on-disk resolver over the fuzzy index, like opening does', async () => {
    setResolveHit({
      path: 'Notes/Architecture.md',
      absolutePath: '/kiln/Notes/Architecture.md',
      title: 'Architecture',
    });

    expect(await fetchNotePreview('Architecture', '/kiln')).toEqual({
      title: 'Architecture',
      path: 'Notes/Architecture.md',
      absPath: '/kiln/Notes/Architecture.md',
    });
    // The one read the daemon took was the resolver's — no fuzzy index, no
    // second note-metadata request beside it.
    expect(resolved).toEqual([['/kiln', 'Architecture']]);
  });
});

describe('openNoteInEditor', () => {
  it('opens the resolved note by absolute path', async () => {
    setResolveHit({
      path: 'notes/rust.md',
      absolutePath: '/kiln/notes/rust.md',
      title: 'Rust',
    });

    await openNoteInEditor('rust', '/kiln');
    expect(openFileInEditorMock).toHaveBeenCalledWith('/kiln/notes/rust.md', 'Rust');
  });

  /**
   * There is deliberately NO default kiln. Resolving a link in "whichever kiln
   * is configured" is how a link in one vault opened a same-named note from
   * another; content with no known kiln has no links to follow.
   */
  it('refuses to resolve when no kiln is given, rather than guessing one', async () => {
    setResolveHit({
      path: 'notes/rust.md',
      absolutePath: '/default-kiln/notes/rust.md',
      title: 'Rust',
    });

    await openNoteInEditor('rust');

    expect(resolved).toEqual([]);
    // No default kiln was read from anywhere: the daemon was not even asked.
    expect(env.fetch.calls('GET /api/config')).toBe(0);
    expect(openFileInEditorMock).not.toHaveBeenCalled();
  });

  it('titles the tab from the resolved file when the resolver gives no title', async () => {
    // No `title` in the payload: the tab falls back to the link target.
    setResolveHit({
      path: 'Help/Wikilinks.md',
      absolutePath: '/kiln/Help/Wikilinks.md',
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

describe('openNoteInEditor with no kiln on the element', () => {
  it('falls back to the kiln in use instead of giving up', async () => {
    // A transcript whose session names a kiln the registry cannot map
    // carries no data-kiln. The click used to resolve nothing and toast
    // "Note not found" for a note that exists in the kiln on screen.
    kilnInUse.path = '/vault';
    setResolveHit({ path: 'A.md', absolutePath: '/vault/A.md', title: 'A' });
    const { openNoteInEditor } = await import('../note-actions');
    await openNoteInEditor('A');
    expect(resolved).toEqual([['/vault', 'A']]);
  });
});
