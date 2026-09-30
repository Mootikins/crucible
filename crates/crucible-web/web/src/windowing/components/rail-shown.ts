import { createStore } from 'solid-js/store';
import type { EdgePanelPosition } from '@/windowing/model/types';

/**
 * The on-screen state of each rail, from its slide.
 *
 * `shown`: some of the rail's body is on screen: the rail is open, or it
 * still slides shut. It comes on at the start of an opening slide and goes off
 * at the end of a closing slide. The ribbon highlight and `data-edge-shown`
 * follow it.
 *
 * `progress`: how far the rail is open, from 0 (shut) to 1 (open), on every
 * frame of the slide. The edge host writes it as `--wm-edge-progress`, so a
 * theme can fade or mix a colour in step with the slide.
 *
 * DockedBody owns the slide; the Ribbon and the EdgeHost read the result.
 */
const [rails, setRails] = createStore<Record<EdgePanelPosition, { shown: boolean; progress: number }>>({
  left: { shown: false, progress: 0 },
  right: { shown: false, progress: 0 },
});

export const railShown = (position: EdgePanelPosition): boolean => rails[position].shown;
export const railProgress = (position: EdgePanelPosition): number => rails[position].progress;
export const setRailShown = (position: EdgePanelPosition, value: boolean): void => {
  setRails(position, 'shown', value);
};
export const setRailProgress = (position: EdgePanelPosition, value: number): void => {
  setRails(position, 'progress', value);
};
