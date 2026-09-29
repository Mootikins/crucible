/** A root row, and its contents while it is open. */
import { Show, type Component, type JSX } from 'solid-js';
import { RootRow, type RootRowProps } from './RootRow';

export const TreeRoot: Component<RootRowProps & { children: JSX.Element }> = (props) => (
  <>
    <RootRow
      label={props.label}
      kind={props.kind}
      open={props.open}
      attached={props.attached}
      color={props.color}
      onToggle={props.onToggle}
    />
    <Show when={props.open}>{props.children}</Show>
  </>
);
