/**
 * The one CodeMirror merge-view setup for every diff surface.
 *
 * `HunkMergeView` shows one review hunk. The diff pane shows one file of a
 * diffset. Both show a read-only diff with the language highlight, the theme
 * and the word-level change highlight, so both get it from this helper.
 * A diff is a code view: the prose features of the note editor stay off.
 */
import type { Extension } from '@codemirror/state';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { unifiedMergeView } from '@codemirror/merge';
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

/** The extensions of one read-only diff editor. */
export function mergeViewExtensions(setup: MergeViewSetup): Extension[] {
  const editor: Extension[] = [
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    setup.wrap === false ? [] : EditorView.lineWrapping,
    editorThemeExtension(theme()),
    getLanguageExtension(setup.path) ?? [],
  ];
  if (setup.split) return editor;
  return [
    ...editor,
    unifiedMergeView({
      original: setup.original,
      mergeControls: setup.controls ?? false,
      collapseUnchanged: setup.collapse,
      highlightChanges: true,
      allowInlineDiffs: true,
      gutter: true,
    }),
  ];
}
