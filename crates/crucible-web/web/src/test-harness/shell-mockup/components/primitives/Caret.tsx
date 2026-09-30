/** The disclosure mark of a tree row: down when the row is open, right when it is closed. */
import { Show, type Component } from 'solid-js';
import { ChevronDown, ChevronRight } from '@/lib/icons';

export const Caret: Component<{ open: boolean }> = (props) => (
  <Show when={props.open} fallback={<ChevronRight class="mk-i" />}>
    <ChevronDown class="mk-i" />
  </Show>
);
