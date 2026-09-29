/** A settings row: a label with an optional hint on the left, the control on the right. */
import { Show, type Component, type JSX } from 'solid-js';

export const FieldRow: Component<{ label: string; hint?: string; children: JSX.Element }> = (props) => (
  <div class="mk-tb-row">
    <div class="mk-tb-label">
      {props.label}
      <Show when={props.hint}>
        <small>{props.hint}</small>
      </Show>
    </div>
    {props.children}
  </div>
);
