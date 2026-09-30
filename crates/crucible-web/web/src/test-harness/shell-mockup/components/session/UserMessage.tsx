/**
 * A user message: a bubble on the right, and under it a row with its time,
 * copy and edit (`Message.tsx` in the real app). The row always holds its
 * room, and shows on hover and on focus, so nothing moves when it shows.
 */
import type { Component } from 'solid-js';
import { Pencil } from 'lucide-solid';
import { CopyIconButton } from '../primitives/CopyIconButton';
import { IconButton } from '../primitives/IconButton';
import { Markdown } from '../primitives/Markdown';
import type { WikilinkEvents } from '../primitives/wikilinks';

export interface UserMessageProps {
  text: string;
  time: string;
  links: WikilinkEvents;
  /** Put the text back into the composer, to change it and send it again. */
  onEdit: (text: string) => void;
}

export const UserMessage: Component<UserMessageProps> = (props) => (
  <div class="mk-user">
    <Markdown class="mk-bubble" source={props.text} links={props.links} />
    <div class="mk-meta-row mk-turnacts hov">
      <span>{props.time}</span>
      <CopyIconButton label="Copy message" text={() => props.text} />
      <IconButton label="Edit message" onClick={() => props.onEdit(props.text)}>
        <Pencil class="mk-i" />
      </IconButton>
    </div>
  </div>
);
