/**
 * One review hunk as a CodeMirror unified merge view.
 *
 * The view shows `after_content` with `before_content` as the original, so a
 * deleted line is red and an inserted one is green, with a word-level
 * highlight inside a changed line. It is read-only and display only.
 *
 * The controls the merge view draws beside a chunk do NOT call CodeMirror's
 * own `action`. That action edits the browser's copy of the text and leaves
 * the disk as it was. The daemon decides a hunk: Accept records a state and
 * Reject rewrites the file. The buttons therefore call the same review actions
 * as the row's buttons in `ChangesPanel`, through the same confirm for Reject.
 *
 * `DiffViewer` stays for the tool card and the permission prompt, where a diff
 * is shown and nothing is disposed.
 */
import { Component, onCleanup } from 'solid-js';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { unifiedMergeView } from '@codemirror/merge';
import { getLanguageExtension } from './editor/CodeMirrorEditor';
import { editorThemeExtension } from './editor/editor-theme';
import { theme } from '@/lib/theme';
import type { ComposedHunk } from '@/lib/review-types';

export interface HunkMergeViewProps {
  hunk: ComposedHunk;
  onAccept: () => void;
  /**
   * Absent for an external hunk. The view then draws no Reject control, for
   * the reason the row draws no Reject button: a revert of the user's own edit
   * would report that an agent edit was undone.
   */
  onReject?: () => void;
}

/** The two words the review vocabulary uses. Never Keep. */
const CONTROL_LABEL: Record<'accept' | 'reject', string> = {
  accept: 'Accept',
  reject: 'Reject',
};

export const HunkMergeView: Component<HunkMergeViewProps> = (props) => {
  let view: EditorView | undefined;

  const renderControls = (type: 'accept' | 'reject'): HTMLElement => {
    const onClick = type === 'accept' ? props.onAccept : props.onReject;
    if (!onClick) {
      const gap = document.createElement('span');
      gap.hidden = true;
      return gap;
    }
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = CONTROL_LABEL[type];
    button.dataset.testid = `merge-${type}-${props.hunk.id}`;
    button.className =
      'min-w-11 min-h-11 rounded border border-hairline px-2 text-floor text-shell-ink ' +
      'hover:bg-hover-wash disabled:opacity-50 ' +
      (type === 'accept' ? 'hover:text-ok' : 'hover:text-error');
    // Stop CodeMirror from reading the click as an edit of its own.
    button.addEventListener('mousedown', (e) => e.preventDefault());
    button.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      onClick();
    });
    return button;
  };

  const mount = (el: HTMLDivElement) => {
    if (view) return;
    view = new EditorView({
      state: EditorState.create({
        doc: props.hunk.after_content,
        extensions: [
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          EditorView.lineWrapping,
          editorThemeExtension(theme()),
          getLanguageExtension(props.hunk.path) ?? [],
          unifiedMergeView({
            original: props.hunk.before_content,
            mergeControls: renderControls,
            collapseUnchanged: { margin: 3 },
            highlightChanges: true,
            allowInlineDiffs: true,
            gutter: true,
          }),
        ],
      }),
      parent: el,
    });
  };

  onCleanup(() => {
    view?.destroy();
    view = undefined;
  });

  return (
    <div
      class="rounded border border-hairline overflow-hidden text-xs"
      data-testid="hunk-merge"
      ref={mount}
    />
  );
};
