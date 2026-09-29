/**
 * The small marks of a session state: `need` is a filled dot, `run` is a
 * spinner and `owe` is a ring. An idle session shows its identity colour
 * instead (see `Ident`).
 */
import type { Component } from 'solid-js';

export type MarkKind = 'need' | 'run' | 'owe';

export const StatusMark: Component<{ status: MarkKind; title?: string }> = (props) => (
  <span class={`mk-mark-${props.status}`} title={props.title} />
);
