import { windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';
import { sessionPane } from './session-actions';
import { findFirstCenterPaneGroupId } from './panel-actions';
import { iconForContentType } from './tab-icons';

/**
 * First-message handoff for lazy session creation: the draft surface stores
 * the typed message here BEFORE `crucible:open-session` mounts the real
 * ChatProvider, which consumes it and sends through the normal optimistic
 * path. In-memory on purpose — tab metadata is persisted with the layout,
 * and a persisted first message would re-send on every reload.
 */
const pendingFirstMessages = new Map<string, string>();

export function setPendingFirstMessage(sessionId: string, message: string): void {
  pendingFirstMessages.set(sessionId, message);
}

export function consumePendingFirstMessage(sessionId: string): string | undefined {
  const message = pendingFirstMessages.get(sessionId);
  pendingFirstMessages.delete(sessionId);
  return message;
}

let draftCounter = 0;

function findDraftTab(): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.contentType === 'chat-draft');
    if (tab) return { groupId, tab };
  }
  return null;
}

/**
 * Open (or focus) a draft session tab — the lazy-creation surface. Nothing
 * touches the daemon until the first message is sent; the draft panel then
 * creates the session and closes itself. Docks in the right edge panel,
 * same as real chat tabs (session-actions.openSessionInChat).
 */
export function openDraftSession(): void {
  const existing = findDraftTab();
  if (existing) {
    const target = sessionPane();
    if (target && target.groupId === existing.groupId) {
      windowActions.setEdgePanelCollapsed('right', false);
      windowActions.setEdgePanelActiveTab('right', existing.tab.id);
    } else {
      windowActions.setActiveTab(existing.groupId, existing.tab.id);
    }
    return;
  }

  const target = sessionPane();
  const groupId = target?.groupId ?? findFirstCenterPaneGroupId();
  if (!groupId) {
    console.error('openDraftSession: no pane available — cannot open draft tab');
    return;
  }

  const tabId = `tab-draft-${++draftCounter}`;
  const newTab: Tab = {
    id: tabId,
    title: 'New Session',
    contentType: 'chat-draft',
    icon: iconForContentType('chat'),
    metadata: { draftTabId: tabId },
  };

  windowActions.addTab(groupId, newTab);
  if (target) {
    windowActions.setEdgePanelCollapsed('right', false);
    windowActions.setEdgePanelActiveTab('right', newTab.id);
  }
}

/** Close a draft tab wherever it lives (used after the real session opens). */
export function closeDraftTab(tabId: string): void {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    if (group.tabs.some((t) => t.id === tabId)) {
      windowActions.removeTab(groupId, tabId);
      return;
    }
  }
}
