import { createStore } from 'solid-js/store';
import type { EdgePanelPosition } from '@/windowing/model/types';

/**
 * Whether any part of a rail's body is on screen: the rail is open, or it
 * still slides shut. A rail that opens shows at the start of its slide; a rail
 * that closes stays shown until the end of its slide. The ribbon highlight and
 * `data-edge-shown` follow this, so a theme's colours grow out of the icon on
 * open and leave with the body on close.
 *
 * DockedBody owns the slide, and the Ribbon beside it reads the result.
 */
const [shown, setShown] = createStore<Record<EdgePanelPosition, boolean>>({ left: false, right: false });

export const railShown = (position: EdgePanelPosition): boolean => shown[position];
export const setRailShown = (position: EdgePanelPosition, value: boolean): void => {
  setShown(position, value);
};
