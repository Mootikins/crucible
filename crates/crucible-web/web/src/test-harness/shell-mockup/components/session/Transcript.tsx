/** The transcript column: each block is an item or a folded group of quiet calls. */
import { For, type Component } from 'solid-js';
import type { WikilinkEvents } from '../primitives/wikilinks';
import type { Block } from './blocks';
import { ToolGroup } from './ToolGroup';
import { TranscriptItemView } from './TranscriptItemView';
import type { ToolLineHandlers, TurnHandlers } from './types';

export const Transcript: Component<{ blocks: Block[]; links: WikilinkEvents; tools: ToolLineHandlers; turn: TurnHandlers }> = (props) => (
  <div class="mk-tinner">
    <For each={props.blocks}>
      {(b) =>
        b.kind === 'group' ? (
          <ToolGroup items={b.items} tools={props.tools} />
        ) : (
          <TranscriptItemView it={b.it} last={b.last} copyText={b.copyText} links={props.links} tools={props.tools} turn={props.turn} />
        )
      }
    </For>
  </div>
);
