/**
 * The details of one call. An edit shows its lines and, while it waits, the
 * review buttons; any other call shows its output.
 */
import { Show, type Component } from 'solid-js';
import { Eye } from 'lucide-solid';
import { Button } from '../primitives/Button';
import { DecisionButtons } from '../primitives/DecisionButtons';
import { MiniDiff } from '../primitives/MiniDiff';
import type { ToolHunkView } from './types';

export interface ToolDetailsProps {
  hunk?: ToolHunkView;
  out?: string;
  onShow: () => void;
  onDecide: (accept: boolean) => void;
}

export const ToolDetails: Component<ToolDetailsProps> = (props) => (
  <Show when={props.hunk} fallback={<div class="mk-out">{props.out}</div>}>
    {(h) => (
      <>
        <MiniDiff del={h().del} add={h().add} />
        <Show when={h().state === 'pending'}>
          <div class="mk-tlacts">
            <Button variant="ghost" onClick={() => props.onShow()}>
              <Eye class="mk-i" />
              Show in note
            </Button>
            <span class="mk-grow" />
            <DecisionButtons onReject={() => props.onDecide(false)} onAccept={() => props.onDecide(true)} />
          </div>
        </Show>
      </>
    )}
  </Show>
);
