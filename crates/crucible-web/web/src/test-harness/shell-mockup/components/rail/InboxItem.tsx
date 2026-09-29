/** One thing that waits in the inbox: the session, what it waits for, and the answers. */
import type { Component, JSX } from 'solid-js';
import { Ident } from '../primitives/Ident';

export interface InboxItemProps {
  title: string;
  color: string;
  text: JSX.Element;
  /** The answer buttons. */
  children: JSX.Element;
}

export const InboxItem: Component<InboxItemProps> = (props) => (
  <div class="mk-item">
    <div class="mk-who">
      <Ident color={props.color} />
      {props.title}
    </div>
    <div class="mk-quiet">{props.text}</div>
    <div class="mk-acts">{props.children}</div>
  </div>
);
