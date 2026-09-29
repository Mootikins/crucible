/** The added and removed line counts of an edit. A zero count shows nothing. */
import { Show, type Component } from 'solid-js';

export const DiffStat: Component<{ add: number; del: number }> = (props) => (
  <span class="mk-stat">
    <Show when={props.add}>
      <span class="a">+{props.add}</span>
    </Show>
    <Show when={props.del}>
      <span class="d">−{props.del}</span>
    </Show>
  </span>
);
