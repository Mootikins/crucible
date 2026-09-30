/**
 * A tree row that opens nothing: an entry of a project listing, or the empty
 * line of a folder. A folder entry shows a closed caret.
 */
import { Show, type Component } from 'solid-js';
import { Caret } from '../primitives/Caret';

export const StaticRow: Component<{ name: string; dir?: boolean; quiet?: boolean }> = (props) => (
  <div
    class="mk-trow"
    classList={{ 'mk-dir': !!props.dir, 'mk-quiet': !!props.quiet }}
  >
    <Show when={props.dir}>
      <Caret open={false} />
    </Show>
    <span class="mk-t">{props.name}</span>
  </div>
);
