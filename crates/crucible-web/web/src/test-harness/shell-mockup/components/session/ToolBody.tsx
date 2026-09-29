/** The details under an open tool line, on a thread line. */
import { Show, type Component, type JSX } from 'solid-js';

export const ToolBody: Component<{ open: boolean; children: JSX.Element }> = (props) => (
  <Show when={props.open}>
    <div class="mk-tlbody">{props.children}</div>
  </Show>
);
