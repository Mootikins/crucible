import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { keys } from '../keys';
import {
  KILN_NOTES_STALE_MS,
  MISS_STALE_MS,
  invalidateNotesUnder,
  fetchKilnNotesOnce,
  fetchNotesOnce,
  fetchResolvedNoteOnce,
  invalidateNotes,
  useGetBacklinks,
  useGetKilnGraph,
  useListKilnNotes,
  useListNotes,
  useResolveNotePath,
} from '../notes';

/**
 * The note entities as one cache: the index, the resolver, the backlinks and
 * the graph.
 *
 * Every one of them is keyed by the KILN, because a note name means nothing
 * on its own — two kilns hold an `Index.md` each, and a list held under the
 * bare name would answer the second reader with the first reader's vault.
 * That is what the first case of each block asserts.
 *
 * The resolver holds a MISS as data. A wikilink that names no note is the
 * common case while the user is still typing one, and an unheld miss is one
 * walk of the whole kiln per keystroke.
 */

const KILN = '/kiln';
const OTHER = '/other';

let env: TestQueryEnv;
let dispose: (() => void) | null = null;
/** The `kiln` of every note request this test answered, per route. */
let asked: { route: string; kiln: string; name: string }[] = [];

function noteRoutes() {
  asked = [];
  const record = (route: string, nameParam: string) => (request: Request) => {
    const params = new URL(request.url).searchParams;
    asked.push({
      route,
      kiln: params.get('kiln') ?? '',
      name: params.get(nameParam) ?? '',
    });
    return answerOf(route, params.get(nameParam) ?? '');
  };
  return {
    'GET /api/notes': record('notes', 'path_filter'),
    'GET /api/notes/resolve': (request: Request) => {
      const params = new URL(request.url).searchParams;
      const name = params.get('name') ?? '';
      asked.push({ route: 'resolve', kiln: params.get('kiln') ?? '', name });
      if (name.toLowerCase() !== 'rust') {
        return new Response(JSON.stringify({ error: { code: 404, message: 'no such note' } }), {
          status: 404,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      return { path: 'Rust.md', absolutePath: `${params.get('kiln')}/Rust.md`, title: 'Rust' };
    },
    'GET /api/backlinks': record('backlinks', 'note'),
    'GET /api/kiln/notes': record('kilnNotes', 'kiln'),
    'GET /api/kiln/graph': record('graph', 'kiln'),
  };
}

/** The body of one route, distinct per route so a mixed-up key shows up. */
function answerOf(route: string, name: string): unknown {
  switch (route) {
    case 'notes':
      return { notes: [{ name: 'A', path: 'A.md', title: 'A', tags: [] }] };
    case 'backlinks':
      return { linked: [{ abs_path: `${KILN}/${name}` }], unlinked: [] };
    case 'kilnNotes':
      return { files: [{ name: 'A', path: 'A.md' }] };
    default:
      return { nodes: [], edges: [] };
  }
}

/** How many times one route was asked, whatever it was asked about. */
const countOf = (route: string) => asked.filter((seen) => seen.route === route).length;

function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

afterEach(() => {
  vi.useRealTimers();
  dispose?.();
  dispose = null;
  env?.restore();
});

describe('useListNotes', () => {
  it('asks once for two readers of one kiln', async () => {
    env = createTestQueryEnv(noteRoutes());

    const both = inRoot(() => ({
      palette: useListNotes(() => KILN),
      picker: useListNotes(() => KILN),
    }));

    await waitFor(() => expect(both.palette.data).toBeDefined());
    await waitFor(() => expect(both.picker.data).toBeDefined());
    expect(countOf('notes')).toBe(1);
    expect(env.client.getQueryData(keys.notesList(KILN))).toEqual(both.palette.data);
  });

  // The negative: the kiln is IN the key, so a second vault is a second entry
  // and not the first vault's answer under another name.
  it('holds a second kiln apart from the first', async () => {
    env = createTestQueryEnv(noteRoutes());

    const both = inRoot(() => ({
      here: useListNotes(() => KILN),
      there: useListNotes(() => OTHER),
    }));

    await waitFor(() => expect(both.here.data).toBeDefined());
    await waitFor(() => expect(both.there.data).toBeDefined());
    expect(asked.filter((seen) => seen.route === 'notes').map((seen) => seen.kiln)).toEqual([
      KILN,
      OTHER,
    ]);
  });

  it('holds nothing while no kiln is chosen', async () => {
    env = createTestQueryEnv(noteRoutes());

    const query = inRoot(() => useListNotes(() => null));

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(query.data).toBeUndefined();
    expect(countOf('notes')).toBe(0);
  });
});

describe('fetchNotesOnce and invalidateNotes', () => {
  it('answers a held kiln without asking again', async () => {
    env = createTestQueryEnv(noteRoutes());

    await fetchNotesOnce(KILN);
    await fetchNotesOnce(KILN);

    expect(countOf('notes')).toBe(1);
  });

  it('asks again after the tree says the kiln moved', async () => {
    env = createTestQueryEnv(noteRoutes());

    await fetchNotesOnce(KILN);
    await invalidateNotes(KILN);
    await fetchNotesOnce(KILN);

    expect(countOf('notes')).toBe(2);
  });
});

describe('useResolveNotePath', () => {
  it('asks once for two readers of one target', async () => {
    env = createTestQueryEnv(noteRoutes());

    const both = inRoot(() => ({
      editor: useResolveNotePath(() => KILN, () => 'rust'),
      preview: useResolveNotePath(() => KILN, () => 'rust'),
    }));

    await waitFor(() => expect(both.editor.data).toBeTruthy());
    await waitFor(() => expect(both.preview.data).toBeTruthy());
    expect(countOf('resolve')).toBe(1);
  });

  /**
   * A miss is DATA, not an error.
   *
   * The daemon answers a name it cannot place by walking the whole kiln, and
   * the hover preview asks about every wikilink under the pointer. An unheld
   * miss is that walk once per hover, forever, for a link that is simply
   * broken.
   */
  it('holds a miss and does not ask a second time', async () => {
    env = createTestQueryEnv(noteRoutes());

    expect(await fetchResolvedNoteOnce(KILN, 'ghost')).toBeNull();
    expect(await fetchResolvedNoteOnce(KILN, 'ghost')).toBeNull();

    expect(countOf('resolve')).toBe(1);
  });

  /**
   * A miss goes stale in seconds, a hit does not.
   *
   * A held miss is the point — the hover preview asks about every link under
   * the pointer, and the daemon answers a name it cannot place by walking the
   * whole kiln. But the commonest miss is a link to a note that does not exist
   * YET, written a moment before the note is, and holding that for the
   * app-wide five minutes leaves the link broken on screen long after the note
   * is on disk.
   */
  it('goes stale within seconds for a miss, and not for a hit', async () => {
    env = createTestQueryEnv(noteRoutes());
    vi.useFakeTimers({ shouldAdvanceTime: true });

    await fetchResolvedNoteOnce(KILN, 'ghost');
    await fetchResolvedNoteOnce(KILN, 'rust');
    expect(countOf('resolve')).toBe(2);

    vi.setSystemTime(Date.now() + MISS_STALE_MS + 1);
    await fetchResolvedNoteOnce(KILN, 'ghost');
    await fetchResolvedNoteOnce(KILN, 'rust');

    // The miss is asked again; the hit is not. A note does not move under a
    // link that resolved, and re-asking is a walk of the whole kiln.
    expect(asked.filter((seen) => seen.route === 'resolve' && seen.name === 'ghost')).toHaveLength(2);
    expect(asked.filter((seen) => seen.route === 'resolve' && seen.name === 'rust')).toHaveLength(1);
  });

  // A wikilink target is case-insensitive — the daemon matches a stem by
  // lowercase — so `[[Rust]]` and `[[rust]]` are one question and must not be
  // two entries and two walks.
  it('reads one target written in two cases as one entry', async () => {
    env = createTestQueryEnv(noteRoutes());

    const first = await fetchResolvedNoteOnce(KILN, 'Rust');
    const second = await fetchResolvedNoteOnce(KILN, 'rust');

    expect(second).toEqual(first);
    expect(countOf('resolve')).toBe(1);
  });
});

describe('useGetBacklinks', () => {
  it('asks once for two readers of one note', async () => {
    env = createTestQueryEnv(noteRoutes());

    const both = inRoot(() => ({
      panel: useGetBacklinks(() => KILN, () => 'A.md'),
      other: useGetBacklinks(() => KILN, () => 'A.md'),
    }));

    await waitFor(() => expect(both.panel.data).toBeDefined());
    await waitFor(() => expect(both.other.data).toBeDefined());
    expect(countOf('backlinks')).toBe(1);
    expect(env.client.getQueryData(keys.notesBacklinks(KILN, 'A.md'))).toEqual(both.panel.data);
  });

  it('holds a second note apart from the first', async () => {
    env = createTestQueryEnv(noteRoutes());

    const both = inRoot(() => ({
      panel: useGetBacklinks(() => KILN, () => 'A.md'),
      other: useGetBacklinks(() => KILN, () => 'B.md'),
    }));

    await waitFor(() => expect(both.panel.data).toBeDefined());
    await waitFor(() => expect(both.other.data).toBeDefined());
    expect(countOf('backlinks')).toBe(2);
  });
});

describe('useGetKilnGraph', () => {
  it('asks once for two readers of one kiln', async () => {
    env = createTestQueryEnv(noteRoutes());

    const both = inRoot(() => ({
      panel: useGetKilnGraph(() => KILN),
      block: useGetKilnGraph(() => KILN),
    }));

    await waitFor(() => expect(both.panel.data).toBeDefined());
    await waitFor(() => expect(both.block.data).toBeDefined());
    expect(countOf('graph')).toBe(1);
  });
});

describe('useListKilnNotes', () => {
  it('asks once for the completion source and the autocomplete together', async () => {
    env = createTestQueryEnv(noteRoutes());

    const query = inRoot(() => useListKilnNotes(() => KILN));
    await waitFor(() => expect(query.data).toBeDefined());
    await fetchKilnNotesOnce(KILN);

    expect(countOf('kilnNotes')).toBe(1);
  });

  /**
   * The completion list is deliberately short-lived.
   *
   * It exists to coalesce the burst of asks one person makes while typing a
   * link, not to be a store: a note created, renamed or moved must appear in
   * the next completion, not five minutes later.
   */
  it('goes stale within seconds, not within the app-wide window', async () => {
    env = createTestQueryEnv(noteRoutes());

    vi.useFakeTimers({ shouldAdvanceTime: true });
    await fetchKilnNotesOnce(KILN);
    expect(KILN_NOTES_STALE_MS).toBeLessThanOrEqual(10_000);

    // And the window is the one the read actually runs under, not a constant
    // beside it: past it, the next ask reaches the daemon again.
    vi.setSystemTime(Date.now() + KILN_NOTES_STALE_MS + 1);
    await fetchKilnNotesOnce(KILN);

    expect(countOf('kilnNotes')).toBe(2);
  });

});

describe('invalidateNotesUnder', () => {
  /**
   * A note written on disk makes everything held about its kiln wrong.
   *
   * The commonest case is the one that made this necessary: a link to a note
   * that does not exist yet, hovered, held as a miss, and then the note is
   * created. Nothing else was going to drop that miss, so the link read as
   * broken for five minutes after the note was on disk.
   */
  it('drops what is held about the kiln a written note is in', async () => {
    env = createTestQueryEnv(noteRoutes());

    await fetchNotesOnce(KILN);
    await fetchResolvedNoteOnce(KILN, 'ghost');
    await fetchKilnNotesOnce(KILN);
    expect(countOf('notes')).toBe(1);

    await invalidateNotesUnder([`${KILN}/Ghost.md`]);

    await fetchNotesOnce(KILN);
    await fetchResolvedNoteOnce(KILN, 'ghost');
    await fetchKilnNotesOnce(KILN);
    expect(countOf('notes')).toBe(2);
    expect(countOf('resolve')).toBe(2);
    expect(countOf('kilnNotes')).toBe(2);
  });

  // The negative: the kiln is a PREFIX of the path, so a note written in one
  // vault leaves the other vault's index and resolutions alone.
  it('leaves another kiln alone', async () => {
    env = createTestQueryEnv(noteRoutes());

    await fetchNotesOnce(KILN);
    await fetchNotesOnce(OTHER);

    await invalidateNotesUnder([`${OTHER}/Ghost.md`]);

    await fetchNotesOnce(KILN);
    await fetchNotesOnce(OTHER);
    expect(asked.filter((seen) => seen.route === 'notes' && seen.kiln === KILN)).toHaveLength(1);
    expect(asked.filter((seen) => seen.route === 'notes' && seen.kiln === OTHER)).toHaveLength(2);
  });

  // A kiln that is a string prefix of another is not a parent of it.
  it('does not read a sibling kiln as a parent of one', async () => {
    env = createTestQueryEnv(noteRoutes());

    await fetchNotesOnce(KILN);

    await invalidateNotesUnder([`${KILN}-archive/Ghost.md`]);

    await fetchNotesOnce(KILN);
    expect(countOf('notes')).toBe(1);
  });
});
