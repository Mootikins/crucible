/**
 * One review hunk as a CodeMirror unified merge view.
 *
 * The view shows `after_content` with `before_content` as the original, so a
 * deleted line is red and an inserted one is green, with a word-level
 * highlight inside a changed line. It is read-only and display only.
 *
 * The view draws no controls. The daemon no longer accepts or reverts a
 * hunk, so there is no decision for a control to send.
 *
 * `DiffViewer` stays for the tool card and the permission prompt, where a diff
 * is shown and nothing is disposed.
 */
import { Component, onCleanup } from 'solid-js';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { mergeViewExtensions } from '@/lib/merge-view';
import type { ComposedHunk } from '@/lib/review-types';

export interface HunkMergeViewProps {
  hunk: ComposedHunk;
}

export const HunkMergeView: Component<HunkMergeViewProps> = (props) => {
  let view: EditorView | undefined;

  const mount = (el: HTMLDivElement) => {
    if (view) return;
    view = new EditorView({
      state: EditorState.create({
        doc: props.hunk.after_content,
        extensions: mergeViewExtensions({
          original: props.hunk.before_content,
          path: props.hunk.path,
          collapse: { margin: 3 },
        }),
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
