/**
 * One agent edit, inline in the note: who made it, then the review buttons,
 * then the removed and the added text. A reject asks once, in place, because
 * it reverts on disk.
 */
import { Show, type Component } from 'solid-js';
import { DecisionButtons } from '../primitives/DecisionButtons';
import { Ident } from '../primitives/Ident';
import { Markdown } from '../primitives/Markdown';
import type { HunkAuthor, HunkView } from './types';

export interface HunkBlockProps {
  hunk: HunkView;
  author: HunkAuthor;
  onDecide: (accept: boolean) => void;
}

export const HunkBlock: Component<HunkBlockProps> = (props) => (
  <div class="mk-hunk" data-hunk={props.hunk.id}>
    <div class="mk-hhead">
      <Ident color={props.author.color} />
      <span class="mk-who">{props.author.title}</span>
      <span class="mk-grow" />
      <Show when={!props.hunk.external}>
        <DecisionButtons
          primary
          check={false}
          confirmReject="Revert on disk?"
          onReject={() => props.onDecide(false)}
          onAccept={() => props.onDecide(true)}
        />
      </Show>
    </div>
    <Show when={props.hunk.del.length}>
      <Markdown class="mk-del" source={props.hunk.del.join('\n')} />
    </Show>
    <Show when={props.hunk.add.length}>
      <Markdown class="mk-add" source={props.hunk.add.join('\n')} />
    </Show>
  </div>
);
