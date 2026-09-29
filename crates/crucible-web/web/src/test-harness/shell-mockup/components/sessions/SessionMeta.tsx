/** The short state text at the end of a session row. Only an idle row shows its time. */
import type { Component } from 'solid-js';
import type { SessionRowView } from './types';

function meta(s: Pick<SessionRowView, 'status' | 'time' | 'pending'>): { text: string; cls: string } {
  if (s.status === 'need') return { text: 'Needs you', cls: 'attn' };
  if (s.status === 'run') return { text: 'Working', cls: 'run' };
  if (s.status === 'owe') return { text: `${s.pending} to review`, cls: 'attn' };
  return { text: s.time, cls: '' };
}

export const SessionMeta: Component<{ session: SessionRowView }> = (props) => (
  <span class={`mk-meta ${meta(props.session).cls}`}>{meta(props.session).text}</span>
);
