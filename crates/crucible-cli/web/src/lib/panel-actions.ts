import { findEdgePanelForGroup, windowActions, windowStore } from '@/stores/windowStore';
import { getGlobalRegistry } from './panel-registry';
import { iconForContentType } from './tab-icons';
import type { LayoutNode, Tab, TabContentType } from '@/types/windowTypes';

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
    const pos = findEdgePanelForGroup(existing.groupId);
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
    icon: iconForContentType(contentType),
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
