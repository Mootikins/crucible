/** The quiet line that a list shows when it has nothing to show. */
import type { Component, JSX } from 'solid-js';
import { Dynamic } from 'solid-js/web';

export const EmptyText: Component<{ as?: 'div' | 'p'; pad?: boolean; children: JSX.Element }> = (props) => (
  <Dynamic component={props.as ?? 'div'} class={props.pad ? 'mk-quiet mk-pad' : 'mk-quiet'}>
    {props.children}
  </Dynamic>
);
