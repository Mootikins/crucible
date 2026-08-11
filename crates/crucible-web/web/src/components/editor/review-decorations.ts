/**
 * Review decorations: the composed diff, marked up inside the real buffer.
 *
 * Review happens inline — real highlighting, real folding, real go-to-
 * definition, and your spatial memory of the file survives — so this is a
 * decoration set plus a gutter, not a second side-by-side viewer.
 *
 * Two pieces:
 *   - one `Decoration.line` per line of every live hunk, toned by review state;
 *   - a per-hunk gutter chip carrying the attribution, which scrolls the chat
 *     transcript to the tool call that made the change when clicked.
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
import { Decoration, EditorView, gutter, GutterMarker } from '@codemirror/view';
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
  /** What the gutter chip says; the tool that wrote it. */
  label: string;
  /** Where the chip jumps to. Null for an external hunk. */
  toolCallId: string | null;
}

export const setReviewHunks = StateEffect.define<ReviewHunkMark[]>();

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

class ChipMarker extends GutterMarker {
  constructor(
    private readonly hunk: ReviewHunkMark,
    private readonly onClick: (hunk: ReviewHunkMark) => void,
  ) {
    super();
  }

  eq(other: ChipMarker): boolean {
    return other.hunk.id === this.hunk.id && other.hunk.state === this.hunk.state;
  }

  toDOM(): Node {
    const el = document.createElement('span');
    el.className = `cm-review-chip cm-review-chip-${this.hunk.external ? 'external' : this.hunk.state}`;
    el.textContent = this.hunk.label;
    el.title = this.hunk.toolCallId
      ? `${this.hunk.label} — show the tool call that made this change`
      : 'Changed outside any tool call';
    el.setAttribute('data-hunk-id', this.hunk.id);
    if (this.hunk.toolCallId) {
      el.addEventListener('mousedown', (e) => {
        // mousedown, not click: the gutter is inside the editor and a click
        // would first move the cursor, scrolling the buffer out from under the
        // jump the user asked for.
        e.preventDefault();
        this.onClick(this.hunk);
      });
    }
    return el;
  }
}

/** The theme lives with the extension so a host that adds one line gets the
 * complete surface. Tokens, not literals, so it tracks the shell palette. */
const reviewTheme = EditorView.baseTheme({
  '.cm-review-unreviewed': { backgroundColor: 'color-mix(in srgb, var(--color-attention) 12%, transparent)' },
  '.cm-review-accepted': { backgroundColor: 'color-mix(in srgb, var(--color-ok) 10%, transparent)' },
  '.cm-review-rejected': { opacity: '0.5' },
  '.cm-review-external': { backgroundColor: 'color-mix(in srgb, var(--color-muted) 10%, transparent)' },
  // No minimum: the column is as wide as the chips in it, so one whose hunks
  // have all been resolved takes no space while it waits for a reconfigure.
  '.cm-review-gutter': { padding: '0 2px' },
  '.cm-review-chip': {
    display: 'inline-block',
    maxWidth: '7rem',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
    padding: '0 4px',
    fontSize: '10px',
    lineHeight: '1.4',
    borderRadius: '3px',
    cursor: 'pointer',
    border: '1px solid var(--color-hairline-strong, #322f38)',
    color: 'var(--color-muted, #928d99)',
  },
  '.cm-review-chip-unreviewed': { color: 'var(--color-attention, #d4a72c)' },
  '.cm-review-chip-accepted': { color: 'var(--color-ok, #7bc47f)' },
  '.cm-review-chip-external': { cursor: 'default' },
});

export interface ReviewDecorationOptions {
  /** Chip click. Called with the hunk; only fires when it has a tool call. */
  onReveal: (hunk: ReviewHunkMark) => void;
}

export function reviewDecorations(opts: ReviewDecorationOptions): Extension {
  return [
    reviewHunksField,
    EditorView.decorations.compute([reviewHunksField], buildDecorations),
    gutter({
      class: 'cm-review-gutter',
      lineMarker(view, line) {
        const n = view.state.doc.lineAt(line.from).number;
        // One chip per hunk, on its first line — a chip per line would be a
        // column of noise saying the same thing.
        const hunk = view.state.field(reviewHunksField).find((h) => h.start === n);
        return hunk ? new ChipMarker(hunk, opts.onReveal) : null;
      },
      lineMarkerChange: (update) =>
        update.transactions.some((tr) => tr.effects.some((e) => e.is(setReviewHunks))),
    }),
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
export function ensureReviewLayer(view: EditorView, opts: ReviewDecorationOptions): boolean {
  if (view.state.field(reviewHunksField, false) !== undefined) return false;
  view.dispatch({ effects: StateEffect.appendConfig.of(reviewDecorations(opts)) });
  return true;
}

/** Push the current hunks into a view. Harmless on a view with no layer. */
export function applyReviewHunks(view: EditorView, hunks: ReviewHunkMark[]): void {
  view.dispatch({ effects: setReviewHunks.of(hunks) });
}

/**
 * Bring a view's review layer in line with the hunks it should be showing —
 * installing it on the first hunk, and never before.
 *
 * A gutter column exists as soon as its extension does, whether or not any
 * line has a marker for it. Installing the layer on open therefore charged
 * every file the width of a chip that was not there: an empty strip on every
 * note the user opened by hand, and in live preview — which drops the
 * line-number gutter so the buffer reads as prose — the only gutter on screen.
 *
 * Hunk-free is the common case, so the layer waits for one. Once up it cannot
 * be taken down in place (CodeMirror has no way to remove an appended
 * extension), so a file whose hunks are all resolved keeps an empty column
 * until the host next reconfigures. That column collapses to nothing on its
 * own: its width is the chips' width, and there are none.
 */
export function syncReviewLayer(
  view: EditorView,
  hunks: ReviewHunkMark[],
  opts: ReviewDecorationOptions,
): void {
  const installed = view.state.field(reviewHunksField, false) !== undefined;
  if (!installed) {
    if (hunks.length === 0) return;
    ensureReviewLayer(view, opts);
  }
  applyReviewHunks(view, hunks);
}
