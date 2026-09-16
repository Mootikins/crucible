import { Component } from 'solid-js';
import { windowStore } from '@/windowing/store';
import type { EdgePanelPosition } from '@/windowing/model/types';
import { Ribbon } from './Ribbon';
import { DockedBody } from './DockedBody';

/**
 * One rail: the ribbon at the window edge, and the body that grows out of it
 * toward the centre. Left: [ribbon][body]; right: [body][ribbon].
 *
 * `data-edge-mode` names the presentation. `docked` and `strip` share one
 * tree: the body stays mounted in `strip`, clipped to zero width, so a toggle
 * slides it and never remounts it. `flyout` and `hidden` present as `strip`
 * until their own presentations land; they branch here.
 */
export const EdgeHost: Component<{ position: EdgePanelPosition }> = (props) => {
  const panel = () => windowStore.edgePanels[props.position];
  return (
    <div
      class="flex flex-row bg-shell-bg overflow-hidden"
      data-testid={`edge-host-${props.position}`}
      data-edge-mode={panel().mode}
    >
      {props.position === 'left' && <Ribbon position="left" />}
      <DockedBody position={props.position} />
      {props.position !== 'left' && <Ribbon position={props.position} />}
    </div>
  );
};
