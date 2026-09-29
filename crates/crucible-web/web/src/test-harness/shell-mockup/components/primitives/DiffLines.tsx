/**
 * The removed lines, then the added lines, of one change, as block spans.
 * The parent `pre` gives the look: `MiniDiff` for a review, or the
 * permission card's own frame. In the real app the lines come from a hunk's
 * `before_content` and `after_content`, or from a `FileDiff`.
 */
import { For, type Component } from 'solid-js';

export interface DiffLinesProps {
  del?: readonly string[];
  add?: readonly string[];
}

export const DiffLines: Component<DiffLinesProps> = (props) => (
  <>
    <For each={props.del ?? []}>{(l) => <span class="d">{l || ' '}</span>}</For>
    <For each={props.add ?? []}>{(l) => <span class="a">{l || ' '}</span>}</For>
  </>
);
