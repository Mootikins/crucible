import { produce } from 'solid-js/store';
import type {
  DragSource,
  DropTarget,
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

export interface LayoutActionDependencies {
  moveTab(
    sourceGroupId: string,
    targetGroupId: string,
    tabId: string,
    insertIndex?: number
  ): void;
  createTabGroup(paneId?: string): string;
  createFloatingWindow(
    tabGroupId: string,
    x: number,
    y: number,
    width?: number,
    height?: number
  ): string;
}

export interface LayoutActions {
  setActivePane(paneId: string | null): void;
  toggleEdgePanel(position: EdgePanelPosition): void;
  setEdgePanelCollapsed(position: EdgePanelPosition, collapsed: boolean): void;
  setEdgePanelActiveTab(position: EdgePanelPosition, tabId: string | null): void;
  setEdgePanelSize(position: EdgePanelPosition, size: number): void;
  openFlyout(position: EdgePanelPosition, tabId: string): void;
  closeFlyout(): void;
  startDrag(source: DragSource): void;
  setDropTarget(target: DropTarget | null): void;
  endDrag(): void;
  executeDrop(): void;
  getTabGroup(groupId: string): TabGroup | undefined;
  getPaneTabGroupId(paneId: string): string | null;
  findPaneById(paneId: string): ReturnType<typeof findPaneInLayout>;
  exportLayout(): SerializedLayout;
  importLayout(json: SerializedLayout): void;
}

export function createLayoutActions(
  context: WindowStoreContext,
  dependencies: LayoutActionDependencies
): LayoutActions {
  const { store, setStore } = context;
  const { moveTab, createTabGroup, createFloatingWindow } = dependencies;

  const endDrag = () => {
    setStore('dragState', null);
  };

  const setActivePane = (paneId: string | null) => {
    setStore('activePaneId', paneId);
    setStore('focusedRegion', 'center');
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

  const startDrag = (source: DragSource) => {
    setStore({
      dragState: { isDragging: true, source, target: null },
    });
  };

  const setDropTarget = (target: DropTarget | null) => {
    if (!store.dragState) return;
    setStore('dragState', 'target', target);
  };

  const executeDrop = () => {
    const state = store;
    if (!state.dragState?.source || !state.dragState?.target) return;
    const { source, target } = state.dragState;
    if (source.type === 'tab') {
      if (target.type === 'pane') {
        const pane = findPaneInLayout(state.layout, target.paneId);
        if (pane) {
          const existingId = pane.tabGroupId;
          if (existingId) {
            moveTab(source.sourceGroupId, existingId, source.tab.id);
          } else {
            const newGroupId = createTabGroup(pane.id);
            moveTab(source.sourceGroupId, newGroupId, source.tab.id);
          }
        }
      } else if (target.type === 'tabGroup') {
        moveTab(
          source.sourceGroupId,
          target.groupId,
          source.tab.id,
          target.insertIndex
        );
      } else if (target.type === 'edgePanel') {
        const position = target.panelId as EdgePanelPosition;
        const panel = state.edgePanels[position];
        if (panel) {
          const edgeGroupId = panel.tabGroupId;
          moveTab(source.sourceGroupId, edgeGroupId, source.tab.id);
        }
      } else if (target.type === 'newFloating') {
        const newGroupId = createTabGroup();
        moveTab(source.sourceGroupId, newGroupId, source.tab.id);
        createFloatingWindow(newGroupId, 100, 100, 400, 300);
      }
    }
    endDrag();
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
        s.dragState = null;
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
    startDrag,
    setDropTarget,
    endDrag,
    executeDrop,
    getTabGroup,
    getPaneTabGroupId,
    findPaneById,
    exportLayout,
    importLayout,
  };
}
