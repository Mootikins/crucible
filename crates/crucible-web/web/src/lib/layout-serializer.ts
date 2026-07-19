import type {
  LayoutNode,
  TabGroup,
  EdgePanel,
  EdgePanelPosition,
  FloatingWindow,
  Tab,
  TabContentType,
} from '@/types/windowTypes';
import { iconForContentType } from './tab-icons';
import { getGlobalRegistry } from './panel-registry';

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
    version: 3,
    layout: JSON.parse(JSON.stringify(state.layout)) as LayoutNode,
    tabGroups: serializedGroups,
    edgePanels: serializedEdgePanels,
    floatingWindows: state.floatingWindows.map((w) => ({ ...w })),
  };
}

/** Group ids reachable from the layout tree, edge panels, and floating windows. */
function referencedGroupIds(layout: SerializedLayout): Set<string> {
  const ids = new Set<string>();
  const walk = (node: any) => {
    if (!node) return;
    if (node.type === 'pane') {
      if (node.tabGroupId) ids.add(node.tabGroupId);
    } else if (node.type === 'split') {
      walk(node.first);
      walk(node.second);
    }
  };
  walk(layout.layout);
  for (const panel of Object.values(layout.edgePanels)) ids.add(panel.tabGroupId);
  for (const w of layout.floatingWindows) ids.add((w as { tabGroupId: string }).tabGroupId);
  return ids;
}

// Drop tabs whose content type is no longer registered — the panel roster
// shrank (removed placeholder panels), and a persisted layout would otherwise
// resurrect ghost tabs that render "Unknown content type". Empty groups that
// nothing references are dropped; a referenced group may go empty (renders an
// empty state) so its pane/edge/window reference stays valid.
function migrateV2toV3(v2: SerializedLayout): SerializedLayout {
  const registry = getGlobalRegistry();
  // If the registry isn't populated yet (defensive — registerPanels() runs
  // before layout load in practice), skip pruning rather than nuke every tab.
  if (registry.list().length === 0) {
    return { ...v2, version: 3 };
  }
  const isKnown = (contentType: string) => registry.get(contentType) !== undefined;

  const tabGroups: Record<string, SerializedTabGroup> = {};
  for (const [id, group] of Object.entries(v2.tabGroups)) {
    const tabs = group.tabs.filter((t) => isKnown(t.contentType));
    const activeTabId =
      group.activeTabId && tabs.some((t) => t.id === group.activeTabId)
        ? group.activeTabId
        : (tabs[0]?.id ?? null);
    tabGroups[id] = { id: group.id, tabs, activeTabId };
  }

  const referenced = referencedGroupIds(v2);
  for (const [id, group] of Object.entries(tabGroups)) {
    if (group.tabs.length === 0 && !referenced.has(id)) {
      delete tabGroups[id];
    }
  }

  return { ...v2, version: 3, tabGroups };
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
  // Auto-migrate forward: v1 → v2 (edge-panel tab groups) → v3 (prune tabs
  // whose content type is no longer registered).
  let layout = json;
  if (layout.version === 1) {
    layout = migrateV1toV2(layout as any);
  }
  if (layout.version === 2) {
    layout = migrateV2toV3(layout);
  }

  if (layout.version !== 3) {
    throw new Error(`Unsupported layout version: ${layout.version}`);
  }

  const tabGroups: Record<string, TabGroup> = {};
  for (const [id, group] of Object.entries(layout.tabGroups)) {
    tabGroups[id] = {
      id: group.id,
      // Icons are components and never survive serialization — resolve them
      // from the content type on the way back in.
      tabs: group.tabs.map((t) => ({ ...t, icon: iconForContentType(t.contentType) })),
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
