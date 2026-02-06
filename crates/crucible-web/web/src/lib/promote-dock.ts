import type { DrawerState, Zone } from './drawer-state';
import type { PanelRegistry } from './panel-registry';
import { removePanel, addPanel } from './drawer-state';

export interface DockviewPanelApi {
  id: string;
  title?: string;
}

export interface DockviewApi {
  addPanel(options: {
    id: string;
    component: string;
    title: string;
    position?: { referenceGroup?: unknown; direction?: string };
  }): DockviewPanelApi;
  removePanel(panel: DockviewPanelApi): void;
  getPanel(id: string): DockviewPanelApi | undefined;
}

export const CRUCIBLE_PANEL_MIME = 'application/x-crucible-panel';

export function promoteToCenter(
  panelId: string,
  drawerState: DrawerState,
  dockviewApi: DockviewApi,
  registry: PanelRegistry,
): DrawerState {
  if (!drawerState.panels.includes(panelId)) {
    return drawerState;
  }

  const def = registry.get(panelId);
  const title = def?.title ?? panelId;

  const newState = removePanel(drawerState, panelId);

  dockviewApi.addPanel({
    id: panelId,
    component: panelId,
    title,
  });

  return newState;
}

export function dockToDrawer(
  panelId: string,
  _targetZone: Zone,
  dockviewApi: DockviewApi,
  drawerState: DrawerState,
): DrawerState {
  const panel = dockviewApi.getPanel(panelId);
  if (!panel) {
    return drawerState;
  }

  dockviewApi.removePanel(panel);

  let newState = addPanel(drawerState, panelId);

  if (newState.mode === 'hidden') {
    newState = { ...newState, mode: 'iconStrip' };
  }

  return newState;
}

export function handleCenterDragOver(event: {
  nativeEvent: DragEvent;
  accept: () => void;
}): void {
  const types = event.nativeEvent.dataTransfer?.types;
  if (types && types.includes(CRUCIBLE_PANEL_MIME)) {
    event.accept();
  }
}

export function handleCenterDrop(
  event: {
    nativeEvent: DragEvent;
    position?: string;
    group?: unknown;
  },
  dockviewApi: DockviewApi,
  registry: PanelRegistry,
  drawerStates: Record<Zone, DrawerState>,
  setDrawerState: (zone: Zone, state: DrawerState) => void,
): void {
  const panelId = event.nativeEvent.dataTransfer?.getData(CRUCIBLE_PANEL_MIME);
  if (!panelId) return;

  const def = registry.get(panelId);
  const title = def?.title ?? panelId;

  dockviewApi.addPanel({
    id: panelId,
    component: panelId,
    title,
    position: event.group
      ? { referenceGroup: event.group, direction: event.position }
      : undefined,
  });

  const zones: Zone[] = ['left', 'right', 'bottom'];
  for (const zone of zones) {
    const state = drawerStates[zone];
    if (state.panels.includes(panelId)) {
      setDrawerState(zone, removePanel(state, panelId));
      break;
    }
  }
}
