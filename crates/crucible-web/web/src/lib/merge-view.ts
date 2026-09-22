/**
 * The one CodeMirror merge-view setup for every diff surface.
 *
 * The diff pane shows one file of a diffset: a read-only diff with the
 * language highlight, the theme and the word-level change highlight. Every
 * diff surface gets that setup from this helper.
 * A diff is a code view: the prose features of the note editor stay off.
 */
import type { Extension } from '@codemirror/state';
import { EditorState, RangeSetBuilder } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from '@codemirror/view';
import { getChunks, unifiedMergeView } from '@codemirror/merge';
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
  // The fold of unchanged lines is a quiet label in the UI font. The library
  // draws a grey gradient in a literal colour and two "⦚" marks.
  '&.cm-editor .cm-collapsedLines.cm-collapsedLines': {
    padding: '0.25em 0',
    background: 'var(--color-hover-wash)',
    color: 'var(--color-muted-dark)',
    fontFamily: 'var(--cru-font-ui)',
    fontSize: 'var(--cru-font-floor)',
    textAlign: 'center',
  },
  '&.cm-editor .cm-collapsedLines.cm-collapsedLines:hover': { color: 'var(--color-shell-ink)' },
  '&.cm-editor .cm-collapsedLines.cm-collapsedLines::before, &.cm-editor .cm-collapsedLines.cm-collapsedLines::after':
    { content: 'none' },
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
