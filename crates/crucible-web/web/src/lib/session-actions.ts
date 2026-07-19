import { windowActions, windowStore } from '@/stores/windowStore';
import type { LayoutNode, Tab } from '@/types/windowTypes';
import { findFirstCenterPaneGroupId } from './panel-actions';
import { iconForContentType } from './tab-icons';

export function findTabBySessionId(sessionId: string): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.metadata?.sessionId === sessionId);
    if (tab) return { groupId, tab };
  }
  return null;
}

/** Rightmost pane of the center tiling (descend `.second` through splits). */
function rightmostPane(node: LayoutNode): { id: string; tabGroupId: string | null } {
  let cur = node;
  while (cur.type === 'split') cur = cur.second;
  return { id: cur.id, tabGroupId: cur.tabGroupId ?? null };
}

/**
 * The pane sessions open in: the RIGHT side of the center tiling. The center
 * stays the editing surface (Obsidian-with-copilot shape); chat lives in a
 * side-by-side pane, created on demand by splitting the rightmost pane when
 * the layout has no horizontal split yet.
 */
export function sessionPane(): { paneId: string; groupId: string } | null {
  const root = windowStore.layout;
  if (root.type === 'split' && root.direction === 'horizontal') {
    const right = rightmostPane(root);
    if (right.tabGroupId) return { paneId: right.id, groupId: right.tabGroupId };
  }
  // Single pane (or vertical stack): make the right pane. splitPane keeps the
  // original tabs in `first` and yields an empty `second` (the right side).
  const target = rightmostPane(root);
  windowActions.splitPane(target.id, 'horizontal');
  const after = rightmostPane(windowStore.layout);
  if (!after.tabGroupId) return null;
  return { paneId: after.id, groupId: after.tabGroupId };
}

export function openSessionInChat(sessionId: string, sessionTitle: string): void {
  const existing = findTabBySessionId(sessionId);
  if (existing) {
    windowActions.setActiveTab(existing.groupId, existing.tab.id);
    return;
  }

  // Sessions open in the right pane by default; fall back to the first
  // center group only if the layout walk failed entirely.
  const target = sessionPane();
  const groupId = target?.groupId ?? findFirstCenterPaneGroupId();
  if (!groupId) {
    console.error('openSessionInChat: no pane available — cannot open chat tab');
    return;
  }

  const newTab: Tab = {
    id: `tab-chat-${sessionId}`,
    title: sessionTitle || 'Chat',
    contentType: 'chat',
    icon: iconForContentType('chat'),
    metadata: { sessionId },
  };

  windowActions.addTab(groupId, newTab);
  if (target) windowActions.setActivePane(target.paneId);
}
