import { Component, Show } from 'solid-js';
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
 *
 * With `ribbonPlacement: 'panel'`, the ribbon rides inside the rail, on its
 * inside edge (see DockedBody), so the edge holds nothing of its own.
 */
export const EdgeHost: Component<{ position: EdgePanelPosition }> = (props) => {
  const panel = () => windowStore.edgePanels[props.position];
  const expanded = () => windowStore.expandedEdge === props.position;
  const edgeRibbon = () => (
    <Show when={windowStore.ribbonPlacement === 'edge'}>
      <Ribbon position={props.position} />
    </Show>
  );
  return (
    <div
      class="wm-edge-host flex flex-row overflow-hidden"
      classList={{ 'flex-1 min-w-0': expanded() }}
      data-testid={`edge-host-${props.position}`}
      data-edge-mode={panel().mode}
      data-edge-expanded={expanded() ? '' : undefined}
      data-ribbon-placement={windowStore.ribbonPlacement}
    >
      {props.position === 'left' && edgeRibbon()}
      <DockedBody position={props.position} />
      {props.position !== 'left' && edgeRibbon()}
    </div>
  );
};
