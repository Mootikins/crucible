import { findEdgePanelForGroup, windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';
import { findFirstCenterPaneGroupId, firstCenterPaneId } from './panel-actions';
import { iconForContentType } from './tab-icons';

export function findTabBySessionId(sessionId: string): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.metadata?.sessionId === sessionId);
    if (tab) return { groupId, tab };
  }
  return null;
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

/** Content that makes a pane a session pane. */
const SESSION_CONTENT = new Set(['chat', 'chat-draft']);

/**
 * The centre pane sessions live in, resolved BY ROLE.
 *
 * A session is a PEER OF THE EDITOR, not a sidebar. Cursor's agents window is
 * the model: a nav rail with the session list, then the conversation and the
 * editor side by side sharing the main area, with the file tree beyond the
 * editor. A session is a working surface with its own composer and its own
 * width, so docking it in a rail either starved it or starved the tree.
 *
 * By role, never by side or by a stored id, because panes split, move and
 * swap. `findTabBySessionId` already honours wherever the user dragged a
 * session to; this only decides where a session with no home goes.
 */
export function sessionPane(): { groupId: string } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    if (findEdgePanelForGroup(groupId)) continue; // centre only
    if (group.tabs.some((t) => SESSION_CONTENT.has(t.contentType))) return { groupId };
  }
  return null;
}

/**
 * Put a session beside the editor, splitting the centre if it has no session
 * pane yet.
 *
 * LEFT of the editor, matching the agents-window arrangement: the conversation
 * is what you read and steer from, the file it changes sits to its right.
 * Falls back to adding a tab to the first centre group when the layout has no
 * pane to split (a shell with nothing open).
 */
export function openTabBesideEditor(tab: Tab): boolean {
  const existing = sessionPane();
  if (existing) {
    windowActions.addTab(existing.groupId, tab);
    windowActions.setActiveTab(existing.groupId, tab.id);
    return true;
  }

  const editorPaneId = firstCenterPaneId();
  if (editorPaneId) return windowActions.openTabInNewPane(editorPaneId, 'left', tab) !== null;

  const groupId = findFirstCenterPaneGroupId();
  if (!groupId) return false;
  windowActions.addTab(groupId, tab);
  windowActions.setActiveTab(groupId, tab.id);
  return true;
}

export function openSessionInChat(sessionId: string, sessionTitle: string): void {
  const existing = findTabBySessionId(sessionId);
  if (existing) {
    focusTabInPlace(existing.groupId, existing.tab.id);
    return;
  }

  const opened = openTabBesideEditor({
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
