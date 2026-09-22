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
import { Decoration, EditorView } from '@codemirror/view';
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
  '&.cm-editor.cm-merge-b .cm-insertOnly .cm-changedText.cm-changedText': {
    background: 'none',
  },
  '&.cm-editor.cm-merge-b .cm-changedText.cm-changedText': {
    background: tint('--color-ok', 32),
    borderRadius: 'var(--cru-radius-sm)',
  },
  '&.cm-editor.cm-merge-a .cm-changedText.cm-changedText, &.cm-editor .cm-deletedChunk .cm-deletedText.cm-deletedText':
    { background: tint('--color-error', 32), borderRadius: 'var(--cru-radius-sm)' },
  '&.cm-editor .cm-changedLineGutter.cm-changedLineGutter': { background: 'var(--color-ok)' },
  '&.cm-editor .cm-deletedLineGutter.cm-deletedLineGutter, &.cm-editor.cm-merge-a .cm-changedLineGutter.cm-changedLineGutter':
    { background: 'var(--color-error)' },
});

const insertOnlyLine = Decoration.line({ class: 'cm-insertOnly' });

/**
 * Mark each line of a chunk that removes nothing. Every word of such a line is
 * new, so a word tint only repeats the row tint. The theme turns it off there.
 */
const insertOnlyLines = EditorView.decorations.compute(['doc'], (state) => {
  const builder = new RangeSetBuilder<Decoration>();
  for (const chunk of getChunks(state)?.chunks ?? []) {
    if (chunk.fromA !== chunk.toA) continue;
    for (let pos = chunk.fromB; pos < chunk.toB && pos <= state.doc.length;) {
      const line = state.doc.lineAt(pos);
      builder.add(line.from, line.from, insertOnlyLine);
      pos = line.to + 1;
    }
  }
  return builder.finish();
});

/** The extensions of one read-only diff editor. */
export function mergeViewExtensions(setup: MergeViewSetup): Extension[] {
  const editor: Extension[] = [
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    setup.wrap === false ? [] : EditorView.lineWrapping,
    editorThemeExtension(theme()),
    getLanguageExtension(setup.path) ?? [],
    diffTheme,
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
    insertOnlyLines,
  ];
}
