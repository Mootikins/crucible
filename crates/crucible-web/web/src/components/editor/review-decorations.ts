/**
 * Review decorations: the composed diff, marked up inside the real buffer.
 *
 * Review happens inline — real highlighting, real folding, real go-to-
 * definition, and your spatial memory of the file survives — so this is a
 * decoration set, not a second side-by-side viewer.
 *
 * The layer puts one `Decoration.line` on each line of every live hunk. The
 * review state of the hunk sets the tone. The layer has no gutter: a text chip
 * took too much width next to reflowed prose.
 *
 * The hunks arrive by effect (`setReviewHunks`) rather than by closure so the
 * extension is a plain value with no owner: `FileViewerPanel` dispatches into
 * whatever view is mounted, and a view with no field simply drops the effect.
 */
import {
  EditorState,
  RangeSetBuilder,
  StateEffect,
  StateField,
  type Extension,
} from '@codemirror/state';
import { Decoration, EditorView } from '@codemirror/view';
import type { ReviewState } from '@/lib/review-types';

/** One hunk, already projected onto this buffer's line numbers. */
export interface ReviewHunkMark {
  id: string;
  /** 1-based first line. */
  start: number;
  /** 1-based, exclusive. Equal to `start` for a pure deletion. */
  end: number;
  state: ReviewState;
  /** Unattributed (§5) — rendered for context, never blamed on the agent. */
  external: boolean;
}

const setReviewHunks = StateEffect.define<ReviewHunkMark[]>();

export const reviewHunksField = StateField.define<ReviewHunkMark[]>({
  create: () => [],
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setReviewHunks)) return e.value;
    return value;
  },
});

const LINE_CLASS: Record<ReviewState, string> = {
  unreviewed: 'cm-review-unreviewed',
  accepted: 'cm-review-accepted',
  // A rejected hunk has already been reverted on disk, so its lines are gone
  // from the buffer. The class exists only for the frame between the optimistic
  // mark and the refresh that drops the hunk.
  rejected: 'cm-review-rejected',
};

const decorationFor = (h: ReviewHunkMark) =>
  Decoration.line({ class: h.external ? 'cm-review-external' : LINE_CLASS[h.state] });

function buildDecorations(state: EditorState) {
  const builder = new RangeSetBuilder<Decoration>();
  const lines = state.doc.lines;
  // Line decorations must be added in document order; hunks arrive in composed
  // diff order, which is per-file and per-root, not per-buffer.
  const ordered = [...state.field(reviewHunksField)].sort((a, b) => a.start - b.start);
  for (const h of ordered) {
    for (let n = h.start; n < Math.max(h.end, h.start + 1); n++) {
      if (n < 1 || n > lines) continue;
      builder.add(state.doc.line(n).from, state.doc.line(n).from, decorationFor(h));
    }
  }
  return builder.finish();
}

/** The theme lives with the extension so a host that adds one line gets the
 * complete surface. Tokens, not literals, so it tracks the shell palette. */
const reviewTheme = EditorView.baseTheme({
  '.cm-review-unreviewed': { backgroundColor: 'color-mix(in srgb, var(--color-attention) 12%, transparent)' },
  '.cm-review-accepted': { backgroundColor: 'color-mix(in srgb, var(--color-ok) 10%, transparent)' },
  '.cm-review-rejected': { opacity: '0.5' },
  '.cm-review-external': { backgroundColor: 'color-mix(in srgb, var(--color-muted) 10%, transparent)' },
});

export function reviewDecorations(): Extension {
  return [
    reviewHunksField,
    EditorView.decorations.compute([reviewHunksField], buildDecorations),
    reviewTheme,
  ];
}

/**
 * Add the review layer to a live view, once.
 *
 * `CodeMirrorEditor` rebuilds its whole configuration with
 * `StateEffect.reconfigure` whenever any of seven props changes, which
 * discards anything appended from outside — so this is written to be called
 * again after every such rebuild and to do nothing when the layer survived.
 * The proper home is that component's `createExtensions()`; this is the
 * in-place equivalent for a caller that cannot edit it.
 *
 * Returns whether it appended.
 */
export function ensureReviewLayer(view: EditorView): boolean {
  if (view.state.field(reviewHunksField, false) !== undefined) return false;
  view.dispatch({ effects: StateEffect.appendConfig.of(reviewDecorations()) });
  return true;
}

/** Push the current hunks into a view. Harmless on a view with no layer. */
export function applyReviewHunks(view: EditorView, hunks: ReviewHunkMark[]): void {
  view.dispatch({ effects: setReviewHunks.of(hunks) });
}

/**
 * Bring a view's review layer in line with the hunks it must show.
 *
 * The layer has no gutter, so it takes no width. It installs on a hunk-free
 * file too. It installs again after a host reconfigure.
 */
export function syncReviewLayer(view: EditorView, hunks: ReviewHunkMark[]): void {
  ensureReviewLayer(view);
  applyReviewHunks(view, hunks);
}
