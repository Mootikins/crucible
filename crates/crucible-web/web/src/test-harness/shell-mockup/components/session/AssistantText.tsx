/**
 * An assistant answer, with its time and token count under it. Only the last
 * answer shows that line at rest; the others show it on hover.
 */
import { Show, type Component } from 'solid-js';
import { Markdown } from '../primitives/Markdown';
import type { WikilinkEvents } from '../primitives/wikilinks';

export interface AssistantTextProps {
  md: string;
  elapsed?: string;
  tokens?: string;
  last: boolean;
  links: WikilinkEvents;
}

export const AssistantText: Component<AssistantTextProps> = (props) => (
  <div class="mk-aturn">
    <Markdown class="mk-atext" source={props.md} links={props.links} />
    <Show when={props.elapsed}>
      <div class="mk-meta-row" classList={{ hov: !props.last }}>
        <span title={props.tokens ? `${props.tokens} tokens` : undefined}>{props.elapsed}</span>
      </div>
    </Show>
  </div>
);
