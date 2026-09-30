/**
 * The details of one call. An edit shows its lines and, while it waits, the
 * review buttons; any other call shows its output.
 */
import { Show, type Component } from 'solid-js';
import { Eye } from '@/lib/icons';
import { Button } from '../primitives/Button';
import { MiniDiff } from '../primitives/MiniDiff';
import type { ToolHunkView } from './types';

export interface ToolDetailsProps {
  hunk?: ToolHunkView;
  out?: string;
  onShow: () => void;
}

export const ToolDetails: Component<ToolDetailsProps> = (props) => (
  <Show when={props.hunk} fallback={<div class="mk-out">{props.out}</div>}>
    {(h) => (
      <>
        <MiniDiff del={h().del} add={h().add} />
        <Show when={h().state !== 'absent'}>
          <div class="mk-tlacts">
            <Button variant="ghost" onClick={() => props.onShow()}>
              <Eye class="mk-i" />
              Open diff
            </Button>
            <span class="mk-grow" />
          </div>
        </Show>
      </>
    )}
  </Show>
);
