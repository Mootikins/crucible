/**
 * The row of one root in the tree: a kiln, a project or the session folder.
 * A root that the active session does not attach is dim; an attached root
 * shows a bar in the session's colour.
 */
import { Show, type Component } from 'solid-js';
import { Caret } from '../primitives/Caret';

export interface RootRowProps {
  label: string;
  /** What the root is: "kiln", "project" or "workspace". */
  kind: string;
  open: boolean;
  attached: boolean;
  color: string;
  onToggle: () => void;
}

export const RootRow: Component<RootRowProps> = (props) => (
  <button type="button" class="mk-rootrow" classList={{ dim: !props.attached }} onClick={() => props.onToggle()}>
    <Caret open={props.open} />
    <span>{props.label}</span>
    <span class="mk-kind">{props.kind}</span>
    <span class="mk-grow" />
    <Show when={props.attached}>
      <span class="mk-sessionbar" style={{ background: props.color }} title="A root of the active session" />
    </Show>
  </button>
);
