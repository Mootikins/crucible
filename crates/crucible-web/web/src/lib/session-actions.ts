import { findEdgePanelForGroup, windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';
import { findFirstCenterPaneGroupId } from './panel-actions';
import { iconForContentType } from './tab-icons';

export function findTabBySessionId(sessionId: string): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.metadata?.sessionId === sessionId);
    if (tab) return { groupId, tab };
  }
  return null;
}

/**
 * Sessions live in the RIGHT EDGE PANEL (the collapsible sidebar), not the
 * center tiling — the center stays the editing surface. Returns the right
 * panel's tab group (addTab materializes it if a stale layout lost it).
 */
export function sessionPane(): { groupId: string } | null {
  const groupId = windowStore.edgePanels.right.tabGroupId;
  return groupId ? { groupId } : null;
}

/** Focus an existing tab in place — wherever the user has put it (edge panel or a pane). */
export function focusTabInPlace(groupId: string, tabId: string): void {
  const pos = findEdgePanelForGroup(groupId);
  if (pos) {
    windowActions.setEdgePanelCollapsed(pos, false);
    windowActions.setEdgePanelActiveTab(pos, tabId);
  } else {
    windowActions.setActiveTab(groupId, tabId);
  }
}

/**
 * Add a tab docked in the right edge panel (where sessions live), falling
 * back to the first center group only if the layout has no right panel group
 * at all. Returns false when no pane exists to host the tab.
 */
export function openTabDockedRight(tab: Tab): boolean {
  const target = sessionPane();
  const groupId = target?.groupId ?? findFirstCenterPaneGroupId();
  if (!groupId) return false;

  windowActions.addTab(groupId, tab);
  if (target) {
    windowActions.setEdgePanelCollapsed('right', false);
    windowActions.setEdgePanelActiveTab('right', tab.id);
  }
  return true;
}

export function openSessionInChat(sessionId: string, sessionTitle: string): void {
  const existing = findTabBySessionId(sessionId);
  if (existing) {
    focusTabInPlace(existing.groupId, existing.tab.id);
    return;
  }

  const opened = openTabDockedRight({
    id: `tab-chat-${sessionId}`,
    title: sessionTitle || 'Chat',
    contentType: 'chat',
    icon: iconForContentType('chat'),
    metadata: { sessionId },
  });
  if (!opened) {
    console.error('openSessionInChat: no pane available — cannot open chat tab');
  }
}
