/**
 * The one CodeMirror merge-view setup for every diff surface.
 *
 * The diff pane shows one file of a diffset: a read-only diff with the
 * language highlight, the theme and the word-level change highlight. Every
 * diff surface gets that setup from this helper.
 * A diff is a code view: the prose features of the note editor stay off.
 */
import type { Extension, Range } from '@codemirror/state';
import {
  EditorState,
  Facet,
  RangeSetBuilder,
  StateEffect,
  StateField,
  Text,
} from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from '@codemirror/view';
import { type Chunk, getChunks, unifiedMergeView } from '@codemirror/merge';
import { getLanguageExtension } from '@/components/editor/CodeMirrorEditor';
import { editorThemeExtension } from '@/components/editor/editor-theme';
import { theme } from '@/lib/theme';

/** The unchanged-line collapse of `@codemirror/merge`. */
export interface MergeCollapse {
  /** The unchanged lines that stay visible next to a change. */
  margin: number;
  /** The smallest run of unchanged lines that collapses. */
  minSize?: number;
}

export interface MergeViewSetup {
  /** The base text. The editor document is the changed text. */
  original: string;
  /** The file path. It selects the language highlight. */
  path: string;
  /**
   * The element for each chunk control. The caller owns the click. Absent:
   * the view draws no control, because CodeMirror's own control edits only
   * the browser's copy of the text.
   */
  controls?: (type: 'accept' | 'reject') => HTMLElement;
  /**
   * True: the caller mounts a `MergeView` with two editors. The helper then
   * gives only the extensions for each editor, and no unified view.
   */
  split?: boolean;
  /** Soft-wrap long lines. The default is on. */
  wrap?: boolean;
  /** Absent: every unchanged line stays visible. */
  collapse?: MergeCollapse;
  /** Hide the empty line after a final newline. See `hidesFinalNewline`. */
  hideFinalNewline?: boolean;
  /**
   * A header row for each hunk, which hides and shows the hunk. It needs
   * `collapse`, because the collapse decides where a hunk ends. Absent: no
   * header.
   */
  hunks?: HunkControl;
}

/** What the hunk headers ask of their owner. The owner keeps the state. */
interface HunkControl {
  /** The current text. A split view needs it to count the lines of the other side. */
  current: string;
  /** The owner hides or shows the hunk, then sends `setHiddenHunks`. */
  onToggle: (label: string) => void;
}

const tint = (color: string, percent: number) =>
  `color-mix(in srgb, var(${color}) ${percent}%, transparent)`;

/**
 * The diff colors of the shell theme. A row is tinted, and a changed word gets
 * a stronger tint of the same color. `@codemirror/merge` marks a changed word
 * with a thin underline and gives a row only a faint tint. That is hard to read
 * on the dark shell, so this theme replaces both.
 *
 * The library writes its rules in a base theme with a light and a dark form,
 * which gives a selector of up to four classes. `EditorView.theme` has no
 * light or dark form, so each selector here adds `.cm-editor` and repeats its
 * last class. Each rule then has one class more than the library's rule.
 */
const diffTheme = EditorView.theme({
  '&.cm-editor.cm-merge-b .cm-changedLine': { backgroundColor: tint('--color-ok', 12) },
  '&.cm-editor.cm-merge-a .cm-changedLine, &.cm-editor .cm-deletedChunk': {
    backgroundColor: tint('--color-error', 12),
  },
  // The changed side draws its own word marks (`wordMarks`), so the library's
  // marks on the changed side carry no tint.
  '&.cm-editor.cm-merge-b .cm-changedText.cm-changedText': { background: 'none' },
  '&.cm-editor .cm-wordChange': {
    background: tint('--color-ok', 32),
    borderRadius: 'var(--cru-radius-sm)',
  },
  '&.cm-editor.cm-merge-a .cm-changedText.cm-changedText, &.cm-editor .cm-deletedChunk .cm-deletedText.cm-deletedText':
    { background: tint('--color-error', 32), borderRadius: 'var(--cru-radius-sm)' },
  '&.cm-editor .cm-changedLineGutter.cm-changedLineGutter': { background: 'var(--color-ok)' },
  '&.cm-editor .cm-deletedLineGutter.cm-deletedLineGutter, &.cm-editor.cm-merge-a .cm-changedLineGutter.cm-changedLineGutter':
    { background: 'var(--color-error)' },
  // The fold of unchanged lines is a quiet label in the UI font, between two
  // hairlines, on the background of the file header, as T3 Code draws its
  // separator. The library draws a grey gradient in a literal colour and two
  // "⦚" marks. A band of its own colour made each fold a second header.
  '&.cm-editor .cm-collapsedLines.cm-collapsedLines': {
    display: 'flex',
    alignItems: 'center',
    gap: '0.75em',
    padding: '0.25em 0.75em',
    background: 'none',
    color: 'var(--color-muted-dark)',
    fontFamily: 'var(--cru-font-ui)',
    fontSize: 'var(--cru-font-floor)',
  },
  '&.cm-editor .cm-collapsedLines.cm-collapsedLines:hover': { color: 'var(--color-shell-ink)' },
  '&.cm-editor .cm-collapsedLines.cm-collapsedLines::before, &.cm-editor .cm-collapsedLines.cm-collapsedLines::after':
    { content: '""', flex: '1', height: '1px', margin: '0', background: 'var(--color-hairline)' },
});

/**
 * Whether the diff hides the empty line after the final newline of a text.
 *
 * CodeMirror shows an empty line after a final newline. That line is not a
 * line of the file, so the diff hides it. When only one of two texts ends in
 * a newline, the diff keeps the line, so that the change of the newline shows.
 * The texts stay whole: a diff of cut texts marks the last line as changed
 * when text is added after it.
 */
export function hidesFinalNewline(base: string, current: string): boolean {
  const ends = (text: string) => text.endsWith('\n');
  return base === '' || current === '' || ends(base) === ends(current);
}

/** An empty block in the place of a line. */
class NoLine extends WidgetType {
  eq(): boolean {
    return true;
  }

  toDOM(): HTMLElement {
    const el = document.createElement('div');
    el.className = 'cm-diff-no-line';
    return el;
  }
}

const noLine = Decoration.replace({ block: true, widget: new NoLine() });

/**
 * Replaces the empty last line with an empty block. A removed chunk at the end
 * of the text is a block of its own, so it stays in view.
 */
const finalNewline = EditorView.decorations.compute(['doc'], (state) => {
  const end = state.doc.length;
  if (end === 0 || state.doc.sliceString(end - 1) !== '\n') return Decoration.none;
  return Decoration.set(noLine.range(end, end));
});

const wordChange = Decoration.mark({ class: 'cm-wordChange' });

/** A character that makes a word. A bracket or a comma does not. */
const WORD_CHAR = /[\p{L}\p{N}_]/u;

/**
 * The changed words of the changed side, as marks.
 *
 * The library marks every changed character. That also marks the indentation
 * of a line, and every character of a line that is new as a whole, where the
 * row tint already says that it is new. These marks leave out both. A line is
 * new as a whole when the text that it keeps from the base has no word
 * character: only brackets, spaces or punctuation match.
 */
function buildWordMarks(state: EditorState): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = state.doc;
  const chunks = getChunks(state);
  // Only the changed side. The base side of a split view keeps the library's
  // marks, which the theme tints red.
  if (!chunks || chunks.side !== 'b') return Decoration.none;
  for (const chunk of chunks.chunks) {
    const end = Math.min(chunk.toB, doc.length);
    for (let pos = chunk.fromB; pos < end;) {
      const line = doc.lineAt(pos);
      pos = line.to + 1;
      // The changed ranges of this line, in document positions.
      const ranges = chunk.changes
        .map((c) => [
          Math.max(chunk.fromB + c.fromB, line.from),
          Math.min(chunk.fromB + c.toB, line.to),
        ])
        .filter(([from, to]) => from < to);
      if (ranges.length === 0) continue;
      let kept = '';
      let at = line.from;
      for (const [from, to] of ranges) {
        kept += doc.sliceString(at, from);
        at = to;
      }
      kept += doc.sliceString(at, line.to);
      if (!WORD_CHAR.test(kept)) continue;
      for (const [from, to] of ranges) {
        const text = doc.sliceString(from, to);
        const start = from + (text.length - text.trimStart().length);
        const stop = to - (text.length - text.trimEnd().length);
        if (start < stop) builder.add(start, stop, wordChange);
      }
    }
  }
  return builder.finish();
}

/**
 * The word marks, rebuilt when the chunks change. A split view gives its
 * editors their chunks by an effect after they mount, so a decoration that
 * follows only the document would miss them.
 */
const wordMarks = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    private chunks: unknown;

    constructor(view: EditorView) {
      this.chunks = getChunks(view.state)?.chunks;
      this.decorations = buildWordMarks(view.state);
    }

    update(update: ViewUpdate) {
      const chunks = getChunks(update.state)?.chunks;
      if (!update.docChanged && chunks === this.chunks) return;
      this.chunks = chunks;
      this.decorations = buildWordMarks(update.state);
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

/**
 * One hunk: the changed lines and their context lines, as a patch shows them.
 * The unchanged lines between two hunks fold.
 */
export interface DiffHunk {
  /** `@@ -base,count +current,count @@`. The label also names the hunk. */
  label: string;
  /** The first position of the hunk in this editor. */
  from: number;
  /** The last position of the hunk in this editor. */
  to: number;
}

/** The start line and the line count of a hunk on one side, as a patch writes them. */
const patchRange = (start: number, count: number) => `${count === 0 ? start - 1 : start},${count}`;

/**
 * The number of lines of a chunk on one side. A chunk covers whole lines: its
 * end is the start of the next line, or one past the end of the text.
 */
function chunkLines(doc: Text, from: number, to: number): number {
  if (from >= to) return 0;
  const last = to > doc.length ? doc.lines : doc.lineAt(to).number - 1;
  return last - doc.lineAt(from).number + 1;
}

/** The setup of the hunk headers of one editor. */
interface HunkSetup {
  collapse: MergeCollapse;
  hideFinalNewline: boolean;
  /**
   * The two texts. Each editor counts the lines of the other side. The split
   * view has no unified view, so an editor cannot read the base text from one.
   */
  base: Text;
  current: Text;
  onToggle: (label: string) => void;
}

const hunkSetup = Facet.define<HunkSetup, HunkSetup | null>({
  combine: (values) => values[0] ?? null,
});

/**
 * The hunks of one editor, with the rule of `collapseUnchanged` in
 * `@codemirror/merge`: a run of unchanged lines folds when it has `minSize`
 * lines or more after `margin` lines stay next to each change. Two chunks with
 * no fold between them are one hunk.
 */
export function diffHunks(state: EditorState, collapse: MergeCollapse): DiffHunk[] {
  const found = getChunks(state);
  if (!found) return [];
  const isA = found.side === 'a';
  const setup = state.facet(hunkSetup);
  const doc = state.doc;
  const other = (isA ? setup?.current : setup?.base) ?? Text.empty;
  const { margin } = collapse;
  const minSize = collapse.minSize ?? 4;
  const own = (c: Chunk) =>
    isA ? chunkLines(doc, c.fromA, c.toA) : chunkLines(doc, c.fromB, c.toB);
  const theirs = (c: Chunk) =>
    isA ? chunkLines(other, c.fromB, c.toB) : chunkLines(other, c.fromA, c.toA);
  // The empty line after a final newline is not a line of the file.
  let lastLine = doc.lines;
  if (setup?.hideFinalNewline && lastLine > 1 && doc.line(lastLine).length === 0) lastLine--;

  const hunks: DiffHunk[] = [];
  // Each chunk adds its line difference: the other side minus this side.
  let deltaBefore = 0;
  let deltaIn = 0;
  let open = false;
  let start = 1;
  const close = (end: number, toEnd: boolean) => {
    const last = Math.max(start, end);
    const count = Math.max(0, end - start + 1);
    const [base, current] = isA
      ? [
          [start, count],
          [start + deltaBefore, count + deltaIn],
        ]
      : [
          [start + deltaBefore, count + deltaIn],
          [start, count],
        ];
    hunks.push({
      label: `@@ -${patchRange(base[0], base[1])} +${patchRange(current[0], current[1])} @@`,
      from: doc.line(start).from,
      // A hunk at the end of the text covers the empty last line too: a
      // removed last line of the base sits there.
      to: toEnd ? doc.length : doc.line(last).to,
    });
    deltaBefore += deltaIn;
    deltaIn = 0;
    open = false;
  };
  let prevLine = 1;
  for (let i = 0; ; i++) {
    const chunk = i < found.chunks.length ? found.chunks[i] : null;
    const collapseFrom = i ? prevLine + margin : 1;
    const collapseTo = chunk
      ? doc.lineAt(isA ? chunk.fromA : chunk.fromB).number - 1 - margin
      : doc.lines;
    if (collapseTo - collapseFrom + 1 >= minSize) {
      if (open) close(collapseFrom - 1, false);
      start = collapseTo + 1;
    }
    if (!chunk) {
      if (open) close(lastLine, true);
      break;
    }
    open = true;
    deltaIn += theirs(chunk) - own(chunk);
    prevLine = doc.lineAt(Math.min(doc.length, isA ? chunk.toA : chunk.toB)).number;
  }
  return hunks;
}

/**
 * Gives the editor the hidden hunks, as a test on the label. The owner keeps
 * the state, so that it survives a new editor for the same file.
 */
export const setHiddenHunks = StateEffect.define<(label: string) => boolean>();

const CHEVRON_PATH = 'm6 9 6 6 6-6';

/** The header row of one hunk: a chevron and the patch range, in a button. */
class HunkHeader extends WidgetType {
  constructor(
    readonly label: string,
    readonly hidden: boolean,
  ) {
    super();
  }

  eq(other: HunkHeader): boolean {
    return other.label === this.label && other.hidden === this.hidden;
  }

  toDOM(view: EditorView): HTMLElement {
    const row = document.createElement('div');
    row.className = 'cm-diff-hunk';
    const button = row.appendChild(document.createElement('button'));
    button.type = 'button';
    button.className = 'cm-diff-hunk-toggle';
    button.dataset.testid = 'diff-hunk-toggle';
    button.setAttribute('aria-expanded', String(!this.hidden));
    button.title = this.hidden ? 'Show this hunk' : 'Hide this hunk';
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('aria-hidden', 'true');
    svg.setAttribute('class', 'cm-diff-hunk-chevron');
    const path = svg.appendChild(document.createElementNS('http://www.w3.org/2000/svg', 'path'));
    path.setAttribute('d', CHEVRON_PATH);
    button.append(svg, document.createTextNode(this.label));
    const label = this.label;
    button.addEventListener('click', () => view.state.facet(hunkSetup)?.onToggle(label));
    return row;
  }

  // The button owns its clicks and keys. The editor must not take them.
  ignoreEvent(): boolean {
    return true;
  }
}

interface HunkState {
  hidden: (label: string) => boolean;
  chunks: unknown;
  decorations: DecorationSet;
}

/**
 * A block replace that also covers the block widgets at its two ends.
 *
 * An inclusive block replace starts after a block widget that sits before its
 * first position. The removed row of a change on line 1 is such a widget, at
 * position 0, so no start position can cover it. `@codemirror/view` gives the
 * wider reach only to the viewport gaps that it draws itself, through the
 * `isBlockGap` field of the spec. The field is not in the typings. The test
 * "a hidden hunk on the first line" in `merge-view.test.ts` fails when a new
 * version of the library drops it.
 */
const COVERS_END_BLOCKS = { isBlockGap: true, inclusive: true } as Parameters<
  typeof Decoration.replace
>[0];

function hunkDecorations(state: EditorState, hidden: (label: string) => boolean): DecorationSet {
  const setup = state.facet(hunkSetup);
  if (!setup) return Decoration.none;
  const ranges: Range<Decoration>[] = [];
  for (const hunk of diffHunks(state, setup.collapse)) {
    const header = new HunkHeader(hunk.label, hidden(hunk.label));
    if (header.hidden) {
      // The header takes the place of the hunk, with the removed rows and the
      // comments at either end of it.
      ranges.push(
        Decoration.replace({ ...COVERS_END_BLOCKS, widget: header, block: true }).range(
          hunk.from,
          hunk.to,
        ),
      );
    } else {
      // Before the removed-line block of a chunk on the first line (side -1).
      ranges.push(Decoration.widget({ widget: header, block: true, side: -2 }).range(hunk.from));
    }
  }
  return Decoration.set(ranges, true);
}

/**
 * The hunk headers and the hidden hunks. A block decoration must come from the
 * state, not from a view plugin. The chunks of a split view arrive by an
 * effect after the editor mounts, so the field follows them too.
 */
const hunkField = StateField.define<HunkState>({
  create(state) {
    const hidden = () => false;
    return {
      hidden,
      chunks: getChunks(state)?.chunks,
      decorations: hunkDecorations(state, hidden),
    };
  },
  update(value, tr) {
    let { hidden } = value;
    for (const effect of tr.effects) if (effect.is(setHiddenHunks)) hidden = effect.value;
    const chunks = getChunks(tr.state)?.chunks;
    if (hidden === value.hidden && chunks === value.chunks && !tr.docChanged) return value;
    return { hidden, chunks, decorations: hunkDecorations(tr.state, hidden) };
  },
  provide: (field) => EditorView.decorations.from(field, (value) => value.decorations),
});

const hunkTheme = EditorView.theme({
  '.cm-diff-hunk': { display: 'flex' },
  '.cm-diff-hunk-toggle': {
    display: 'flex',
    flex: '1',
    alignItems: 'center',
    gap: '0.5em',
    padding: '0.25em 0.5em',
    // No rule: the fold above the hunk, or the file header, separates it.
    border: 'none',
    background: 'transparent',
    color: 'var(--color-muted-dark)',
    font: 'inherit',
    fontSize: 'var(--cru-font-floor)',
    textAlign: 'left',
    cursor: 'pointer',
  },
  '.cm-diff-hunk-toggle:hover': {
    background: 'var(--color-hover-wash)',
    color: 'var(--color-shell-ink)',
  },
  '.cm-diff-hunk-toggle:focus-visible': {
    outline: '1px solid var(--color-focus-ring)',
    outlineOffset: '-1px',
  },
  '.cm-diff-hunk-chevron': {
    width: '1.1em',
    height: '1.1em',
    flexShrink: '0',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: '2',
    strokeLinecap: 'round',
    strokeLinejoin: 'round',
    transition: 'transform 120ms ease-out',
  },
  '.cm-diff-hunk-toggle[aria-expanded="false"] .cm-diff-hunk-chevron': {
    transform: 'rotate(-90deg)',
  },
});

/** The extensions of the hunk headers. */
function hunkExtensions(setup: MergeViewSetup): Extension {
  if (!setup.hunks || !setup.collapse) return [];
  return [
    hunkSetup.of({
      collapse: setup.collapse,
      hideFinalNewline: !!setup.hideFinalNewline,
      base: Text.of(setup.original.split('\n')),
      current: Text.of(setup.hunks.current.split('\n')),
      onToggle: setup.hunks.onToggle,
    }),
    hunkField,
    hunkTheme,
  ];
}

/** The extensions of one read-only diff editor. */
export function mergeViewExtensions(setup: MergeViewSetup): Extension[] {
  const editor: Extension[] = [
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    setup.wrap === false ? [] : EditorView.lineWrapping,
    editorThemeExtension(theme()),
    getLanguageExtension(setup.path) ?? [],
    diffTheme,
    setup.hideFinalNewline ? finalNewline : [],
    wordMarks,
    hunkExtensions(setup),
  ];
  if (setup.split) return editor;
  return [
    ...editor,
    unifiedMergeView({
      original: setup.original,
      mergeControls: setup.controls ?? false,
      collapseUnchanged: setup.collapse,
      highlightChanges: true,
      // A removed line is a row of its own, as in a patch. An inline diff hides
      // a small removal inside the new line.
      allowInlineDiffs: false,
      gutter: true,
    }),
  ];
}
