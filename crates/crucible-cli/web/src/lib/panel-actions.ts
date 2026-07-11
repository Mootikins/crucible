import { windowActions, windowStore } from '@/stores/windowStore';
import { getGlobalRegistry } from './panel-registry';
import type { EdgePanelPosition, LayoutNode, Tab, TabContentType } from '@/types/windowTypes';

const EDGE_POSITIONS: readonly EdgePanelPosition[] = ['left', 'right', 'bottom'];

/** First pane group in the center tiling — where center-zone tabs open. */
export function findFirstCenterPaneGroupId(): string | null {
  function findFirst(node: LayoutNode): string | null {
    if (node.type === 'pane') return node.tabGroupId ?? null;
    return findFirst(node.first) || findFirst(node.second);
  }

  return findFirst(windowStore.layout);
}

export function findTabByContentType(
  contentType: TabContentType
): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.contentType === contentType);
    if (tab) return { groupId, tab };
  }
  return null;
}

function edgePositionForGroup(groupId: string): EdgePanelPosition | null {
  for (const pos of EDGE_POSITIONS) {
    if (windowStore.edgePanels[pos].tabGroupId === groupId) return pos;
  }
  return null;
}

/**
 * Open a registered panel as a tab (command-palette / gear entry point).
 *
 * Focuses the existing tab when one is already open (singleton panels —
 * there is no reason for two Settings tabs), otherwise creates the tab in
 * the panel's registered default zone. Collapsed edge panels are expanded
 * so the result is always visible.
 */
export function openPanelTab(contentType: TabContentType): void {
  const existing = findTabByContentType(contentType);
  if (existing) {
    const pos = edgePositionForGroup(existing.groupId);
    if (pos) {
      windowActions.setEdgePanelCollapsed(pos, false);
      windowActions.setEdgePanelActiveTab(pos, existing.tab.id);
    } else {
      windowActions.setActiveTab(existing.groupId, existing.tab.id);
    }
    return;
  }

  const def = getGlobalRegistry().get(contentType);
  if (!def) {
    console.error(`openPanelTab: no registered panel for content type '${contentType}'`);
    return;
  }

  const tab: Tab = {
    id: `tab-${contentType}`,
    title: def.title,
    contentType,
  };

  if (def.defaultZone === 'center') {
    const groupId = findFirstCenterPaneGroupId();
    if (!groupId) {
      console.error(`openPanelTab: no center pane group found — cannot open '${contentType}'`);
      return;
    }
    windowActions.addTab(groupId, tab);
  } else {
    const pos = def.defaultZone;
    windowActions.addTab(windowStore.edgePanels[pos].tabGroupId, tab);
    windowActions.setEdgePanelCollapsed(pos, false);
  }
}
