/** A user message: a bubble on the right, with its time under it on hover. */
import type { Component } from 'solid-js';
import { Markdown } from '../primitives/Markdown';
import type { WikilinkEvents } from '../primitives/wikilinks';

export const UserMessage: Component<{ text: string; time: string; links: WikilinkEvents }> = (props) => (
  <div class="mk-user">
    <Markdown class="mk-bubble" source={props.text} links={props.links} />
    <div class="mk-meta-row hov">{props.time}</div>
  </div>
);
