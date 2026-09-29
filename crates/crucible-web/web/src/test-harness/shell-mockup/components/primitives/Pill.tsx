/**
 * A small rounded label. Each kind has one look in mockup.css:
 * `tag` is a note tag, `count` is a review count on a tree row, `state` is a
 * hunk state ("Your edit") and `badge` is the count on a rail button.
 */
import type { Component, JSX } from 'solid-js';

export type PillKind = 'tag' | 'count' | 'state' | 'badge';

const CLASS: Record<PillKind, string> = {
  tag: 'mk-tag',
  count: 'mk-owe',
  state: 'mk-state',
  badge: 'mk-badge',
};

export const Pill: Component<{ kind: PillKind; title?: string; children: JSX.Element }> = (props) => (
  <span class={CLASS[props.kind]} title={props.title}>
    {props.children}
  </span>
);
