import type {
  LayoutNode,
  TabGroup,
  EdgePanel,
  EdgePanelPosition,
  EdgePanelTab,
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

interface SerializedEdgePanelTab extends SerializedTab {
  panelPosition: EdgePanelPosition;
}

interface SerializedEdgePanel {
  id: string;
  position: EdgePanelPosition;
  tabs: SerializedEdgePanelTab[];
  activeTabId: string | null;
  isCollapsed: boolean;
  width?: number;
  height?: number;
}

function stripIcon(tab: Tab): SerializedTab {
  const { icon: _icon, ...rest } = tab;
  void _icon;
  return rest;
}

function stripEdgePanelTabIcon(tab: EdgePanelTab): SerializedEdgePanelTab {
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
      position: panel.position,
      tabs: panel.tabs.map(stripEdgePanelTabIcon),
      activeTabId: panel.activeTabId,
      isCollapsed: panel.isCollapsed,
      width: panel.width,
      height: panel.height,
    };
  }

  return {
    version: 1,
    layout: JSON.parse(JSON.stringify(state.layout)) as LayoutNode,
    tabGroups: serializedGroups,
    edgePanels: serializedEdgePanels,
    floatingWindows: state.floatingWindows.map((w) => ({ ...w })),
  };
}

export function deserializeLayout(json: SerializedLayout): {
  layout: LayoutNode;
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, EdgePanel>;
  floatingWindows: FloatingWindow[];
} {
  if (json.version !== 1) {
    throw new Error(`Unknown layout version: ${json.version}`);
  }

  const tabGroups: Record<string, TabGroup> = {};
  for (const [id, group] of Object.entries(json.tabGroups)) {
    tabGroups[id] = {
      id: group.id,
      tabs: group.tabs.map((t) => ({ ...t })),
      activeTabId: group.activeTabId,
    };
  }

  const edgePanels = {} as Record<EdgePanelPosition, EdgePanel>;
  for (const [pos, panel] of Object.entries(json.edgePanels)) {
    edgePanels[pos as EdgePanelPosition] = {
      id: panel.id,
      position: panel.position,
      tabs: panel.tabs.map((t) => ({ ...t })),
      activeTabId: panel.activeTabId,
      isCollapsed: panel.isCollapsed,
      width: panel.width,
      height: panel.height,
    };
  }

  return {
    layout: json.layout,
    tabGroups,
    edgePanels,
    floatingWindows: json.floatingWindows.map((w) => ({ ...w })),
  };
}
