import { produce } from 'solid-js/store';
import type {
  EdgePanel as EdgePanelType,
  EdgePanelPosition,
  TabGroup,
} from '@/types/windowTypes';
import type { SerializedLayout } from '@/lib/layout-serializer';
import {
  deserializeLayout,
  serializeLayout,
} from '@/lib/layout-serializer';
import type { WindowStoreContext } from './windowStoreInternals';
import {
  findPaneInLayout,
  findFirstPane,
} from './windowStoreInternals';
import { statusBarActions } from './statusBarStore';
import { syncShellSurface } from './shellStore';

export interface LayoutActions {
  setActivePane(paneId: string | null): void;
  toggleEdgePanel(position: EdgePanelPosition): void;
  setEdgePanelCollapsed(position: EdgePanelPosition, collapsed: boolean): void;
  setEdgePanelActiveTab(position: EdgePanelPosition, tabId: string | null): void;
  setEdgePanelSize(position: EdgePanelPosition, size: number): void;
  openFlyout(position: EdgePanelPosition, tabId: string): void;
  closeFlyout(): void;
  getTabGroup(groupId: string): TabGroup | undefined;
  getPaneTabGroupId(paneId: string): string | null;
  findPaneById(paneId: string): ReturnType<typeof findPaneInLayout>;
  exportLayout(): SerializedLayout;
  importLayout(json: SerializedLayout): void;
}

export function createLayoutActions(context: WindowStoreContext): LayoutActions {
  const { store, setStore } = context;

  const setActivePane = (paneId: string | null) => {
    setStore('activePaneId', paneId);
    setStore('focusedRegion', 'center');
    // Focusing a pane makes its visible chat the target of session-scoped
    // commands (Ctrl+K clear, switch-model) — see syncActiveSession in
    // tabActions for the tab-activation half.
    if (paneId) {
      const pane = findPaneInLayout(store.layout, paneId);
      const group = pane?.tabGroupId ? store.tabGroups[pane.tabGroupId] : null;
      const activeTab = group?.tabs.find((t) => t.id === group.activeTabId);
      const sessionId = activeTab?.metadata?.sessionId;
      if (typeof sessionId === 'string') {
        statusBarActions.setActiveSessionId(sessionId);
      }
      syncShellSurface(activeTab);
    }
  };

  const toggleEdgePanel = (position: EdgePanelPosition) => {
    setStore(
      produce((s) => {
        s.edgePanels[position].isCollapsed = !s.edgePanels[position].isCollapsed;
        s.flyoutState = null;
      })
    );
  };

  const setEdgePanelCollapsed = (
    position: EdgePanelPosition,
    collapsed: boolean
  ) => {
    setStore('edgePanels', position, 'isCollapsed', collapsed);
  };

  const setEdgePanelActiveTab = (
    position: EdgePanelPosition,
    tabId: string | null
  ) => {
    const groupId = store.edgePanels[position].tabGroupId;
    setStore('tabGroups', groupId, 'activeTabId', tabId);
    setStore('focusedRegion', position);
  };

  const setEdgePanelSize = (position: EdgePanelPosition, size: number) => {
    const isVertical = position === 'left' || position === 'right';
    const clamped = isVertical
      ? Math.max(120, Math.min(600, size))
      : Math.max(100, Math.min(500, size));
    setStore(
      produce((s) => {
        if (isVertical) {
          s.edgePanels[position].width = clamped;
        } else {
          s.edgePanels[position].height = clamped;
        }
      })
    );
  };

  const openFlyout = (position: EdgePanelPosition, tabId: string) => {
    setStore({
      flyoutState: { isOpen: true, position, tabId },
    });
  };

  const closeFlyout = () => {
    setStore('flyoutState', null);
  };

  const getTabGroup = (groupId: string) => {
    return store.tabGroups[groupId];
  };

  const getPaneTabGroupId = (paneId: string): string | null => {
    const pane = findPaneInLayout(store.layout, paneId);
    return pane?.tabGroupId ?? null;
  };

  const findPaneById = (paneId: string) => {
    return findPaneInLayout(store.layout, paneId);
  };

  const exportLayout = (): SerializedLayout => {
    return serializeLayout({
      layout: store.layout,
      tabGroups: { ...store.tabGroups },
      edgePanels: { ...store.edgePanels } as Record<EdgePanelPosition, EdgePanelType>,
      floatingWindows: [...store.floatingWindows],
    });
  };

  const importLayout = (json: SerializedLayout) => {
    const restored = deserializeLayout(json);
    setStore(
      produce((s) => {
        s.layout = restored.layout;
        s.tabGroups = restored.tabGroups;
        s.edgePanels = restored.edgePanels as Record<EdgePanelPosition, EdgePanelType>;
        s.floatingWindows = restored.floatingWindows;
        s.activePaneId = null;
        s.focusedRegion = 'center';
        s.flyoutState = null;
        s.nextZIndex = 100;
        const firstPane = findFirstPane(s.layout);
        if (firstPane) s.activePaneId = firstPane.id;
      })
    );
  };

  return {
    setActivePane,
    toggleEdgePanel,
    setEdgePanelCollapsed,
    setEdgePanelActiveTab,
    setEdgePanelSize,
    openFlyout,
    closeFlyout,
    getTabGroup,
    getPaneTabGroupId,
    findPaneById,
    exportLayout,
    importLayout,
  };
}
