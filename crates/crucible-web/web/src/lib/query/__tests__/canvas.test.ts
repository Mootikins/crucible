import { describe, it, expect, afterEach } from 'vitest';
import { createRoot } from 'solid-js';
import { waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { CanvasDoc } from '@/lib/canvas-types';
import { keys } from '../keys';
import { fetchCanvasOnce, saveCanvasOnce, useGetCanvas, useSaveCanvas } from '../canvas';

/**
 * One `.canvas` document, held under its path.
 *
 * A canvas is a FILE that two panes can have open at once — the board in the
 * centre and the same board in a split — and the two used to read it
 * separately and write it separately. The later write then carried whatever
 * the earlier read had, so a card moved in one pane came back to where the
 * other pane still believed it was.
 *
 * A save PATCHES the entry rather than invalidating it: the document just
 * written is the document the daemon now holds, so re-reading it is a round
 * trip for an answer the writer already has.
 */

const BOARD = '/kiln/Board.canvas';

const doc = (id: string): CanvasDoc => ({
  nodes: [{ id, type: 'text', text: id, x: 0, y: 0, width: 100, height: 100 }],
  edges: [],
});

let env: TestQueryEnv;
let dispose: (() => void) | null = null;
/** The path of every read, and the body of every write, in order. */
let reads: string[] = [];
let writes: { path: string; content: string }[] = [];
/** The document the daemon answers with. */
let onDisk: CanvasDoc = doc('text-1');

function canvasRoutes() {
  reads = [];
  writes = [];
  onDisk = doc('text-1');
  return {
    'GET /api/canvas': (request: Request) => {
      reads.push(new URL(request.url).searchParams.get('path') ?? '');
      return { kiln: '/kiln', rejected: [], canvas: onDisk };
    },
    'PUT /api/canvas': async (request: Request) => {
      writes.push((await request.json()) as { path: string; content: string });
      return {};
    },
  };
}

function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
});

describe('useGetCanvas', () => {
  it('reads one board once for two panes', async () => {
    env = createTestQueryEnv(canvasRoutes());

    const both = inRoot(() => ({
      centre: useGetCanvas(() => BOARD),
      split: useGetCanvas(() => BOARD),
    }));

    await waitFor(() => expect(both.centre.data).toBeDefined());
    await waitFor(() => expect(both.split.data).toBeDefined());
    expect(reads).toEqual([BOARD]);
    expect(env.client.getQueryData(keys.canvas(BOARD))).toEqual(both.centre.data);
  });

  // The negative: the path is the key, so a second board is a second entry.
  it('holds a second board apart from the first', async () => {
    env = createTestQueryEnv(canvasRoutes());

    const both = inRoot(() => ({
      one: useGetCanvas(() => BOARD),
      two: useGetCanvas(() => '/kiln/Other.canvas'),
    }));

    await waitFor(() => expect(both.one.data).toBeDefined());
    await waitFor(() => expect(both.two.data).toBeDefined());
    expect(reads).toEqual([BOARD, '/kiln/Other.canvas']);
  });

  it('holds nothing while no board is open', async () => {
    env = createTestQueryEnv(canvasRoutes());

    const query = inRoot(() => useGetCanvas(() => null));

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(query.data).toBeUndefined();
    expect(reads).toEqual([]);
  });
});

describe('saveCanvasOnce', () => {
  /**
   * The write the OTHER pane reads without asking.
   *
   * It is a patch and not an invalidation because the document written is the
   * document the daemon now holds: re-reading it is a round trip for an answer
   * the writer already has, and it would race the next keystroke on the board.
   */
  it('puts what it wrote into the entry the other pane is reading', async () => {
    env = createTestQueryEnv(canvasRoutes());

    const reader = inRoot(() => useGetCanvas(() => BOARD));
    await waitFor(() => expect(reader.data).toBeDefined());

    await saveCanvasOnce(BOARD, doc('moved'));

    await waitFor(() => expect(reader.data?.canvas).toEqual(doc('moved')));
    expect(reads).toEqual([BOARD]);
    expect(writes).toHaveLength(1);
    expect(JSON.parse(writes[0].content)).toEqual(doc('moved'));
  });

  /**
   * A board nobody is reading is not minted from a write.
   *
   * The panel flushes a queued edit from its cleanup, which runs as the pane
   * goes away. Writing a document into an empty cache there would answer the
   * next reader a canvas with no kiln and no refusals beside it.
   */
  it('mints no entry for a board no one holds', async () => {
    env = createTestQueryEnv(canvasRoutes());

    await saveCanvasOnce(BOARD, doc('moved'));

    expect(env.client.getQueryData(keys.canvas(BOARD))).toBeUndefined();
    expect(writes).toHaveLength(1);
  });

  it('leaves another board alone', async () => {
    env = createTestQueryEnv(canvasRoutes());

    const other = inRoot(() => useGetCanvas(() => '/kiln/Other.canvas'));
    await waitFor(() => expect(other.data).toBeDefined());

    await saveCanvasOnce(BOARD, doc('moved'));

    expect(other.data?.canvas).toEqual(doc('text-1'));
  });
});

describe('useSaveCanvas and fetchCanvasOnce', () => {
  it('writes through the mutation and patches the same entry', async () => {
    env = createTestQueryEnv(canvasRoutes());

    const both = inRoot(() => ({
      reader: useGetCanvas(() => BOARD),
      save: useSaveCanvas(() => BOARD),
    }));
    await waitFor(() => expect(both.reader.data).toBeDefined());

    await both.save.mutateAsync(doc('by-mutation'));

    await waitFor(() => expect(both.reader.data?.canvas).toEqual(doc('by-mutation')));
  });

  it('answers a held board without asking again', async () => {
    env = createTestQueryEnv(canvasRoutes());

    await fetchCanvasOnce(BOARD);
    await fetchCanvasOnce(BOARD);

    expect(reads).toEqual([BOARD]);
  });
});
