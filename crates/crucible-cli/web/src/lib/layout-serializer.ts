import type {
  LayoutNode,
  TabGroup,
  EdgePanel,
  EdgePanelPosition,
  FloatingWindow,
  Tab,
  TabContentType,
} from '@/types/windowTypes';

export interface SerializedLayout {
  version: number;
  layout: LayoutNode;
  tabGroups: Record<string, SerializedTabGroup>;
  edgePanels: Record<EdgePanelPosition, SerializedEdgePanel>;
  floatingWindows: FloatingWindow[];
}

interface SerializedTabGroup {
  id: string;
  tabs: SerializedTab[];
  activeTabId: string | null;
}

interface SerializedTab {
  id: string;
  title: string;
  contentType: TabContentType;
  isModified?: boolean;
  isPinned?: boolean;
  metadata?: Record<string, unknown>;
}

interface SerializedEdgePanel {
  id: string;
  tabGroupId: string;
  isCollapsed: boolean;
  width?: number;
  height?: number;
}

function stripIcon(tab: Tab): SerializedTab {
  const { icon: _icon, ...rest } = tab;
  void _icon;
  return rest;
}

export function serializeLayout(state: {
  layout: LayoutNode;
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, EdgePanel>;
  floatingWindows: FloatingWindow[];
}): SerializedLayout {
  const serializedGroups: Record<string, SerializedTabGroup> = {};
  for (const [id, group] of Object.entries(state.tabGroups)) {
    serializedGroups[id] = {
      id: group.id,
      tabs: group.tabs.map(stripIcon),
      activeTabId: group.activeTabId,
    };
  }

  const serializedEdgePanels = {} as Record<EdgePanelPosition, SerializedEdgePanel>;
  for (const [pos, panel] of Object.entries(state.edgePanels)) {
    serializedEdgePanels[pos as EdgePanelPosition] = {
      id: panel.id,
      tabGroupId: panel.tabGroupId,
      isCollapsed: panel.isCollapsed,
      width: panel.width,
      height: panel.height,
    };
  }

  return {
    version: 2,
    layout: JSON.parse(JSON.stringify(state.layout)) as LayoutNode,
    tabGroups: serializedGroups,
    edgePanels: serializedEdgePanels,
    floatingWindows: state.floatingWindows.map((w) => ({ ...w })),
  };
}

function migrateV1toV2(v1: any): SerializedLayout {
  const newTabGroups = { ...v1.tabGroups };

  // Migrate each edge panel
  for (const pos of ['left', 'right', 'bottom'] as const) {
    const panel = v1.edgePanels[pos];
    if (panel.tabs && Array.isArray(panel.tabs)) {
      // Create new tab group from v1 inline tabs
      const groupId = `edge-${pos}-${Date.now()}`;
      const tabs = panel.tabs.map((tab: any) => {
        const { panelPosition: _panelPosition, ...rest } = tab;
        void _panelPosition;
        return rest;
      });

      newTabGroups[groupId] = {
        id: groupId,
        tabs,
        activeTabId: panel.activeTabId ?? (tabs.length > 0 ? tabs[0].id : null),
      };

      // Replace inline tabs with tabGroupId reference
      v1.edgePanels[pos] = {
        id: panel.id,
        tabGroupId: groupId,
        isCollapsed: panel.isCollapsed,
        width: panel.width,
        height: panel.height,
      };
    }
  }

  return {
    ...v1,
    version: 2,
    tabGroups: newTabGroups,
  };
}

export function deserializeLayout(json: SerializedLayout): {
  layout: LayoutNode;
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, EdgePanel>;
  floatingWindows: FloatingWindow[];
} {
  // Auto-migrate v1 to v2
  let layout = json;
  if (layout.version === 1) {
    layout = migrateV1toV2(layout as any);
  }

  if (layout.version !== 2) {
    throw new Error(`Unsupported layout version: ${layout.version}`);
  }

  const tabGroups: Record<string, TabGroup> = {};
  for (const [id, group] of Object.entries(layout.tabGroups)) {
    tabGroups[id] = {
      id: group.id,
      tabs: group.tabs.map((t) => ({ ...t })),
      activeTabId: group.activeTabId,
    };
  }

  const edgePanels = {} as Record<EdgePanelPosition, EdgePanel>;
  for (const [pos, panel] of Object.entries(layout.edgePanels)) {
    edgePanels[pos as EdgePanelPosition] = {
      id: panel.id,
      tabGroupId: panel.tabGroupId,
      isCollapsed: panel.isCollapsed,
      width: panel.width,
      height: panel.height,
    };
  }

  return {
    layout: layout.layout,
    tabGroups,
    edgePanels,
    floatingWindows: layout.floatingWindows.map((w) => ({ ...w })),
  };
}
