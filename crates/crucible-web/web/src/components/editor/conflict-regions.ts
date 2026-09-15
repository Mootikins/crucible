/**
 * Conflict regions, marked up inside the buffer the user resolves them in.
 *
 * A merge the daemon could not finish answers with the merged text and one
 * region per span the two writers changed differently. The merged text takes
 * OURS in every region, so the document already reads as this device's writing
 * and every region is a place where the other writer said something else.
 *
 * Each region gets a block widget ABOVE its lines: the two texts, with the
 * words that differ marked, and the three choices GitHub's web conflict editor
 * settled on — keep mine, keep theirs, keep both. A choice replaces the
 * region's lines in the document and drops the region; the regions below it
 * move with the document, because their spans are mapped through every change
 * rather than recomputed from line numbers that no longer hold.
 *
 * Nothing here writes a note. The view above decides when the document is
 * settled, and `lib/conflicts.ts` writes it. This module owns the marking and
 * the replacement, the way `review-decorations.ts` owns the review's.
 */
import { StateEffect, StateField, type EditorState, type Extension, type Text } from '@codemirror/state';
import { Decoration, EditorView, WidgetType, type DecorationSet } from '@codemirror/view';
import { diffWords } from 'diff';
import type { MergeRegion } from '@/lib/api';

/** What a person can do with one region. Never "accept": that word is the review's. */
export type ConflictChoice = 'mine' | 'theirs' | 'both';

/** One region, with the document positions it currently occupies. */
export interface TrackedRegion {
  /** Its index in the answer. Stable for the life of the view. */
  id: number;
  base: string;
  ours: string;
  theirs: string;
  /** Document position of the region's first line. */
  from: number;
  /** Document position just past its last line. Equal to `from` for a span our side deleted. */
  to: number;
}

const setRegions = StateEffect.define<MergeRegion[]>();
const dropRegion = StateEffect.define<number>();

/**
 * Where a region's lines sit in the merged text.
 *
 * The daemon's lines are 1-based and end-exclusive and point into the merged
 * text (`crucible_core::note_merge::Region`), so the span runs from the start
 * of `start_line` to the start of `end_line` — which is past the last line's
 * newline. A region whose end is past the document ends at the document.
 */
function spanOf(doc: Text, region: MergeRegion): { from: number; to: number } {
  const start = Math.min(Math.max(region.start_line, 1), doc.lines);
  const from = doc.line(start).from;
  const end = Math.max(region.end_line, start);
  const to = end > doc.lines ? doc.length : doc.line(end).from;
  return { from, to: Math.max(to, from) };
}

function track(doc: Text, regions: MergeRegion[]): TrackedRegion[] {
  return regions.map((region, id) => ({
    id,
    base: region.base,
    ours: region.ours,
    theirs: region.theirs,
    ...spanOf(doc, region),
  }));
}

/**
 * The regions still open, with their live positions.
 *
 * Positions are MAPPED through every change rather than recomputed: the user
 * types in this buffer, and a region three screens down must not move because
 * a line above it grew. Recomputing from the answer's line numbers would put
 * every widget below the first edit in the wrong place.
 */
const conflictRegionsField = StateField.define<TrackedRegion[]>({
  create: () => [],
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setRegions)) return track(tr.state.doc, e.value);
    let next = value;
    for (const e of tr.effects) {
      if (e.is(dropRegion)) next = next.filter((r) => r.id !== e.value);
    }
    if (!tr.docChanged) return next;
    return next.map((r) => ({
      ...r,
      from: tr.changes.mapPos(r.from, -1),
      to: tr.changes.mapPos(r.to, 1),
    }));
  },
});

/** Put the answer's regions into a mounted view. Call once, after it exists. */
export function seedConflictRegions(view: EditorView, regions: MergeRegion[]): void {
  view.dispatch({ effects: setRegions.of(regions) });
}

/** The regions nobody has chosen for yet. Empty on a view with no layer. */
export function openConflictRegions(state: EditorState): TrackedRegion[] {
  return state.field(conflictRegionsField, false) ?? [];
}

/**
 * The text a choice puts in a region's place.
 *
 * Keeping both puts ours first, because the merged text already reads in that
 * order and a reader's eye is on it. A newline is invented only when ours has
 * none and both sides have text: without it the two runs would join into one
 * line that neither writer wrote.
 */
export function regionChoiceText(
  region: { ours: string; theirs: string },
  choice: ConflictChoice,
): string {
  if (choice === 'mine') return region.ours;
  if (choice === 'theirs') return region.theirs;
  if (region.ours === '') return region.theirs;
  if (region.theirs === '') return region.ours;
  return region.ours.endsWith('\n') ? region.ours + region.theirs : `${region.ours}\n${region.theirs}`;
}

/**
 * Settle one region: write the chosen text over its lines and drop it.
 *
 * The change and the drop go in ONE transaction, so no update ever sees a
 * region whose span names text that is no longer there.
 */
export function resolveConflictRegion(
  view: EditorView,
  id: number,
  choice: ConflictChoice,
): void {
  const region = openConflictRegions(view.state).find((r) => r.id === id);
  if (!region) return;
  view.dispatch({
    changes: { from: region.from, to: region.to, insert: regionChoiceText(region, choice) },
    effects: dropRegion.of(id),
  });
}

/** One side of a region, with the words the other side does not have marked. */
function sideElement(region: TrackedRegion, side: 'mine' | 'theirs'): HTMLElement {
  const row = document.createElement('div');
  row.className = 'cm-conflict-side';
  row.dataset.testid = `region-${side}-${region.id}`;
  const name = document.createElement('span');
  name.className = 'cm-conflict-name';
  name.textContent = side === 'mine' ? 'Mine' : 'Theirs';
  row.appendChild(name);
  const text = document.createElement('span');
  text.className = 'cm-conflict-text';
  // Word level inside the region, because the merge is line level: two edits
  // to one line are a region even when they touch different words, and this
  // is what shows the user where they actually differ.
  for (const part of diffWords(region.ours, region.theirs)) {
    if (side === 'mine' ? part.added : part.removed) continue;
    const span = document.createElement('span');
    span.textContent = part.value;
    if (part.added || part.removed) span.className = 'cm-conflict-word';
    text.appendChild(span);
  }
  row.appendChild(text);
  return row;
}

const CHOICES: { choice: ConflictChoice; label: string }[] = [
  { choice: 'mine', label: 'Keep mine' },
  { choice: 'theirs', label: 'Keep theirs' },
  { choice: 'both', label: 'Keep both' },
];

class RegionWidget extends WidgetType {
  constructor(private readonly region: TrackedRegion) {
    super();
  }

  /** One widget per region id: the span moves, the choice does not. */
  eq(other: RegionWidget): boolean {
    return other.region.id === this.region.id;
  }

  /** The buttons are the widget's own; the editor must not read their clicks. */
  ignoreEvent(): boolean {
    return true;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement('div');
    box.className = 'cm-conflict-region';
    box.dataset.testid = `conflict-region-${this.region.id}`;
    box.appendChild(sideElement(this.region, 'mine'));
    box.appendChild(sideElement(this.region, 'theirs'));

    const controls = document.createElement('div');
    controls.className = 'cm-conflict-controls';
    for (const { choice, label } of CHOICES) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = label;
      button.dataset.testid = `keep-${choice}-${this.region.id}`;
      // 44 px unconditionally, the way the merge view's controls are: this is
      // a widget the editor builds once, so a size that read the shell at
      // build time would go stale.
      button.className =
        'min-w-11 min-h-11 rounded border border-hairline px-2 text-floor ' +
        'text-shell-ink hover:bg-hover-wash';
      button.addEventListener('mousedown', (e) => e.preventDefault());
      button.addEventListener('click', (e) => {
        e.preventDefault();
        resolveConflictRegion(view, this.region.id, choice);
      });
      controls.appendChild(button);
    }
    box.appendChild(controls);
    return box;
  }
}

function widgets(state: EditorState): DecorationSet {
  const open = [...openConflictRegions(state)].sort((a, b) => a.from - b.from);
  return Decoration.set(
    open.map((region) =>
      Decoration.widget({ widget: new RegionWidget(region), block: true, side: -1 }).range(
        region.from,
      ),
    ),
    true,
  );
}

/** Tokens, not literals, so the box tracks the shell palette. */
const conflictTheme = EditorView.baseTheme({
  '.cm-conflict-region': {
    margin: '4px 0',
    padding: '4px 6px',
    borderRadius: 'var(--cru-radius-md)',
    border: '1px solid var(--color-attention)',
    backgroundColor: 'color-mix(in srgb, var(--color-attention) 8%, transparent)',
    fontSize: 'var(--cru-font-floor)',
  },
  '.cm-conflict-side': { display: 'flex', gap: '6px', alignItems: 'baseline' },
  '.cm-conflict-name': {
    flex: '0 0 auto',
    textTransform: 'uppercase',
    letterSpacing: '0.04em',
    color: 'var(--color-muted)',
  },
  '.cm-conflict-text': { whiteSpace: 'pre-wrap', wordBreak: 'break-word' },
  '.cm-conflict-word': {
    backgroundColor: 'color-mix(in srgb, var(--color-attention) 30%, transparent)',
    // 3px, from the token. The literal here was 2px and named no step in the
    // scale, so one word wash rounded differently from every other chip.
    borderRadius: 'var(--cru-radius-sm)',
  },
  '.cm-conflict-controls': { display: 'flex', gap: '6px', marginTop: '4px' },
});

export interface ConflictRegionOptions {
  /** Told the open regions whenever they change: the counter and Save read it. */
  onChange?: (open: TrackedRegion[]) => void;
}

/** The whole layer: the tracked regions, their widgets and their theme. */
export function conflictRegions(opts: ConflictRegionOptions = {}): Extension {
  return [
    conflictRegionsField,
    EditorView.decorations.compute([conflictRegionsField], widgets),
    EditorView.updateListener.of((update) => {
      const before = update.startState.field(conflictRegionsField, false);
      const after = update.state.field(conflictRegionsField, false);
      if (before !== after && after !== undefined) opts.onChange?.(after);
    }),
    conflictTheme,
  ];
}
