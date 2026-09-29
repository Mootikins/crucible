/** One session in the list: its mark, its title and its state. */
import type { Component } from 'solid-js';
import { SessionMark } from './SessionMark';
import { SessionMeta } from './SessionMeta';
import type { SessionRowView } from './types';

export interface SessionRowProps {
  session: SessionRowView;
  current: boolean;
  onOpen: (id: string) => void;
}

export const SessionRow: Component<SessionRowProps> = (props) => (
  <button
    type="button"
    class="mk-srow"
    aria-current={props.current}
    title={props.session.title}
    onClick={() => props.onOpen(props.session.id)}
  >
    <SessionMark status={props.session.status} color={props.session.color} />
    <span class="mk-t">{props.session.title}</span>
    <SessionMeta session={props.session} />
  </button>
);
