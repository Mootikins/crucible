/**
 * The change that a permission request asks for, as the current app's
 * `DiffViewer` shows it: the path with the added and removed counts, then
 * each line with its old and new line numbers and its mark. The rows are
 * `DiffLine`s from `lib/diff-stats.ts`, the same analysis the viewer uses.
 */
import { For, type Component } from 'solid-js';
import type { DiffLine } from '@/lib/diff-stats';
import { DiffStat } from './DiffStat';

/** One row of the diff. The real request carries `diffs` (`FileDiff`); `analyzeDiff` makes the rows. */
export type DiffRowView = DiffLine;

export interface PermissionDiffProps {
  path: string;
  add: number;
  del: number;
  rows: readonly DiffRowView[];
}

const MARK = { add: '+', remove: '−', context: '' } as const;

export const PermissionDiff: Component<PermissionDiffProps> = (props) => (
  <div class="mk-pdiff">
    <div class="mk-pdhead">
      <span class="mk-t">{props.path}</span>
      <DiffStat add={props.add} del={props.del} />
    </div>
    <div class="mk-pdrows">
      <For each={props.rows}>
        {(r) => (
          <div class={`mk-pdrow ${r.type}`}>
            <span class="n">{r.oldLineNum ?? ''}</span>
            <span class="n">{r.newLineNum ?? ''}</span>
            <span class="m">{MARK[r.type]}</span>
            <span class="c">{r.content || ' '}</span>
          </div>
        )}
      </For>
    </div>
  </div>
);
