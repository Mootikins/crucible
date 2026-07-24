/**
 * Pending proposed-edit diffs, keyed by absolute file path. A tool approval (or
 * any surface proposing an edit) registers `{ original, proposed }` here, then
 * opens/focuses the file; the editor for that path renders the proposed content
 * as an inline unified-merge diff against `original` (green additions / red
 * deletions, per-chunk accept/reject), the way Cursor overlays a pending change
 * in the real buffer. Cleared when the review is dismissed or applied.
 */
import { createStore, produce } from 'solid-js/store';

export interface PendingDiff {
  /** The file's current on-disk content (the diff baseline). */
  original: string;
  /** The agent's proposed new content (what the editor shows). */
  proposed: string;
}

const [diffs, setDiffs] = createStore<Record<string, PendingDiff>>({});

export const pendingDiffStore = {
  /** Reactive accessor: the pending diff for a path, or undefined. */
  get(path: string): PendingDiff | undefined {
    return diffs[path];
  },
};

export const pendingDiffActions = {
  set(path: string, diff: PendingDiff): void {
    setDiffs(path, diff);
  },
  /** Delete the key rather than storing `undefined` — a store path set to
   * undefined keeps the key, so every reviewed file would leak an entry. */
  clear(path: string): void {
    setDiffs(
      produce((s) => {
        delete s[path];
      }),
    );
  },
};
