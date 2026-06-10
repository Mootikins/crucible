import { createStore } from 'solid-js/store';
import { createFloatingWindowActions } from './floatingWindowActions';
import {
  createInitialState,
  findEdgePanelForGroup as findEdgePanelForGroupInState,
} from './windowStoreInternals';
import { createLayoutActions } from './layoutActions';
import { createTabActions } from './tabActions';
import type { WindowState } from './windowStoreTypes';

export type { PaneDropPosition, WindowState } from './windowStoreTypes';
export { updateSplitRatio } from './windowStoreInternals';

const initialState = createInitialState();
const [store, setStore] = createStore<WindowState>(initialState);

export { store as windowStore, setStore };

export function findEdgePanelForGroup(groupId: string) {
  return findEdgePanelForGroupInState(store, groupId);
}

const context = { store, setStore };
const tabActions = createTabActions(context);
const floatingWindowActions = createFloatingWindowActions(context);
const layoutActions = createLayoutActions(context, {
  moveTab: tabActions.moveTab,
  createTabGroup: tabActions.createTabGroup,
  createFloatingWindow: floatingWindowActions.createFloatingWindow,
});

export const windowActions = {
  ...tabActions,
  ...layoutActions,
  ...floatingWindowActions,
};

if (typeof window !== 'undefined') {
  (window as unknown as Record<string, unknown>).__windowActions = windowActions;
  (window as unknown as Record<string, unknown>).__windowStore = store;
}
