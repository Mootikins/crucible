/**
 * A heading inside a panel. `panel` is the heading of a popover or a card;
 * `group` is the quiet label over a group of list rows.
 */
import type { Component, JSX } from 'solid-js';

export const SectionLabel: Component<{ kind?: 'panel' | 'group'; children: JSX.Element }> = (props) => (
  <div class={props.kind === 'group' ? 'mk-grouplabel' : 'mk-ph'}>{props.children}</div>
);
