/**
 * An assistant answer, and under it a row with copy, regenerate (on the last
 * answer only, as in `AssistantTurn.tsx`) and the time the turn took. Only
 * the last answer shows that row at rest; the others show it on hover and
 * on focus. The row always holds its room, so nothing moves when it shows.
 */
import { Show, type Component } from 'solid-js';
import { RefreshCw } from 'lucide-solid';
import { CopyIconButton } from '../primitives/CopyIconButton';
import { IconButton } from '../primitives/IconButton';
import { Markdown } from '../primitives/Markdown';
import type { WikilinkEvents } from '../primitives/wikilinks';

export interface AssistantTextProps {
  md: string;
  elapsed?: string;
  tokens?: string;
  last: boolean;
  links: WikilinkEvents;
  /** Ask the model again for the last answer. */
  onRegenerate: () => void;
}

export const AssistantText: Component<AssistantTextProps> = (props) => (
  <div class="mk-aturn">
    <Markdown class="mk-atext" source={props.md} links={props.links} />
    <div class="mk-meta-row mk-turnacts" classList={{ hov: !props.last }}>
      <CopyIconButton label="Copy response" text={() => props.md} />
      <Show when={props.last}>
        <IconButton label="Regenerate response" onClick={() => props.onRegenerate()}>
          <RefreshCw class="mk-i" />
        </IconButton>
      </Show>
      <Show when={props.elapsed}>
        <span title={props.tokens ? `${props.tokens} tokens` : undefined}>{props.elapsed}</span>
      </Show>
    </div>
  </div>
);
