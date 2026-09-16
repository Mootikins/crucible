import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { getCanvas, saveCanvas } from '@/lib/api';
import type { CanvasDoc, CanvasResponse } from '@/lib/canvas-types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * One `.canvas` document, held under its path.
 *
 * A canvas is a FILE, and two panes can have one open at the same time — the
 * board in the centre and the same board in a split. They read it separately
 * and wrote it separately before, so the later write carried whatever the
 * earlier read had held: a card moved in one pane came back to where the other
 * pane still believed it was.
 */

/**
 * How long the board waits after an edit before it writes.
 *
 * A canvas edit is a DRAG, which arrives as one event per frame. Writing on
 * each of them is a file write per frame; waiting this long turns one gesture
 * into one write. It lives here beside the write it belongs to, and the panel
 * that owns the timer reads it from here.
 */
export const CANVAS_SAVE_DEBOUNCE_MS = 600;

/** The options of one board. */
function canvasOptions(path: string) {
  return { queryKey: keys.canvas(path), queryFn: () => getCanvas(path) };
}

/**
 * One board.
 *
 * The path is an accessor because a pane follows the tab the user is on, and
 * `null` is "no board open", which is a different state from an empty board.
 */
export function useGetCanvas(
  path: Accessor<string | null>,
): UseQueryResult<CanvasResponse, Error> {
  return useQuery(() => {
    const asked = path();
    return { ...canvasOptions(asked ?? ''), enabled: asked !== null };
  }, getQueryClient);
}

/** One board, as a promise, for a reader that acts rather than renders. */
export function fetchCanvasOnce(path: string): Promise<CanvasResponse> {
  return getQueryClient().fetchQuery(canvasOptions(path));
}

/**
 * Writes one board, then puts what it wrote where the other pane reads it.
 *
 * A PATCH and not an invalidation. The document just written is the document
 * the daemon now holds, so a refetch is a round trip for an answer the writer
 * already has — and it would race the next frame of a drag that is still
 * going. The kiln and the refusals beside the document are left as they were:
 * the write does not move either, and the daemon reports a refusal by
 * throwing rather than in the reply.
 *
 * A board NOBODY holds is not minted from a write. The panel flushes a queued
 * edit from its cleanup, which runs as the pane goes away; writing an entry
 * there would answer the next reader a canvas with no kiln beside it.
 *
 * It is a plain function beside the hook because that cleanup is exactly when
 * one caller writes: a save that needed a live observer would make closing a
 * tab into data loss.
 */
export function saveCanvasOnce(path: string, canvas: CanvasDoc): Promise<void> {
  return saveCanvas(path, canvas).then(() => {
    const client = getQueryClient();
    // The stamp stays where it was. A reader adopts a document when the stamp
    // MOVES, and the pane that wrote this one is a reader of the same entry —
    // so a fresh stamp told that pane about its own write, six hundred
    // milliseconds after every edit, and it rebuilt its board from the echo
    // and threw away the undo stack behind it. Writing what you already know
    // is not news, and the stamp is what says whether something is.
    const stamp = client.getQueryState<CanvasResponse>(keys.canvas(path))?.dataUpdatedAt;
    client.setQueryData<CanvasResponse>(
      keys.canvas(path),
      (held) => (held ? { ...held, canvas } : held),
      { updatedAt: stamp },
    );
  });
}

/** The same write, for a caller that wants its pending and error state. */
export function useSaveCanvas(
  path: Accessor<string | null>,
): UseMutationResult<void, Error, CanvasDoc> {
  return useMutation(
    () => ({
      mutationFn: (canvas: CanvasDoc) => {
        const asked = path();
        if (!asked) return Promise.resolve();
        return saveCanvasOnce(asked, canvas);
      },
    }),
    getQueryClient,
  );
}
