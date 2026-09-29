/** One hunk in the review: its state or the review buttons, then the lines. A decided hunk fades. */
import { Show, type Component } from 'solid-js';
import { DecisionButtons } from '../primitives/DecisionButtons';
import { MiniDiff } from '../primitives/MiniDiff';
import { Pill } from '../primitives/Pill';
import type { HunkView } from './types';

const DONE_TEXT = { pending: '', accepted: 'Accepted', rejected: 'Rejected', absent: 'Rejected' } as const;

export const ChangeChunk: Component<{ hunk: HunkView; onDecide: (accept: boolean) => void }> = (props) => (
  <div class="mk-chunk" classList={{ done: props.hunk.state !== 'pending' }}>
    <div class="mk-chead">
      <Show when={props.hunk.external} fallback={<span class="mk-quiet">{DONE_TEXT[props.hunk.state]}</span>}>
        <Pill kind="state">Your edit</Pill>
      </Show>
      <span class="mk-grow" />
      <Show when={props.hunk.state === 'pending' && !props.hunk.external}>
        <DecisionButtons onReject={() => props.onDecide(false)} onAccept={() => props.onDecide(true)} />
      </Show>
    </div>
    <MiniDiff del={props.hunk.del} add={props.hunk.add} />
  </div>
);
