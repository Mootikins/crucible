/** Completed turns have one action row; interim text leaves no empty controls slot. */
import { Show, type Component } from 'solid-js';
import { RefreshCw } from '@/lib/icons';
import { CopyIconButton } from '../primitives/CopyIconButton';
import { IconButton } from '../primitives/IconButton';
import { Markdown } from '../primitives/Markdown';
import type { WikilinkEvents } from '../primitives/wikilinks';

export interface AssistantTextProps {
  md: string;
  elapsed?: string;
  tokens?: string;
  last: boolean;
  /** All assistant text in this completed turn; absent while tools continue. */
  copyText?: string;
  links: WikilinkEvents;
  /** Ask the model again for the last answer. */
  onRegenerate: () => void;
}

export const AssistantText: Component<AssistantTextProps> = (props) => (
  <div class="mk-aturn">
    <Markdown class="mk-atext" source={props.md} links={props.links} />
    <Show when={props.copyText}>
      <div class="mk-meta-row mk-turnacts" classList={{ hov: !props.last }}>
        <CopyIconButton label="Copy response" text={() => props.copyText!} />
        <Show when={props.last}>
          <IconButton label="Regenerate response" onClick={() => props.onRegenerate()}>
            <RefreshCw class="mk-i" />
          </IconButton>
        </Show>
        <Show when={props.elapsed}>
          <span title={props.tokens ? `${props.tokens} tokens` : undefined}>{props.elapsed}</span>
        </Show>
      </div>
    </Show>
  </div>
);
