import { windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';
import { findFirstCenterPaneGroupId, edgeCenterPane, sessionsSide } from './panel-actions';
import { iconForContentType } from './tab-icons';
import { tabHost } from './tab-host';

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
 * swap. `tabHost().find` already honours wherever the user dragged a
 * session to; this only decides where a session with no home goes.
 */
export function sessionPane(): { groupId: string } | null {
  // The pane next to the sessions rail, when it is a conversation pane or
  // empty. A session pane that was dragged elsewhere is not the default.
  const edge = edgeCenterPane(sessionsSide());
  if (edge?.groupId) {
    const group = windowStore.tabGroups[edge.groupId];
    const tabs = group?.tabs ?? [];
    // A pane that mixes a chat with a file is an editor pane with a stray
    // chat in it, not the session pane: a session gets its own pane beside it.
    const sessionsOnly = tabs.length > 0 && tabs.every((t) => SESSION_CONTENT.has(t.contentType));
    if (sessionsOnly || tabs.length === 0) return { groupId: edge.groupId };
  }
  return null;
}

/**
 * Put a session in the centre pane beside the SESSIONS RAIL, splitting the
 * centre if that pane holds an editor.
 *
 * The side follows the rail, so a swap flips it. Falls back to adding a tab
 * to the first centre group when the layout has no pane to split (a shell
 * with nothing open).
 */
export function openTabBesideEditor(tab: Tab): boolean {
  const existing = sessionPane();
  if (existing) {
    windowActions.addTab(existing.groupId, tab);
    windowActions.setActiveTab(existing.groupId, tab.id);
    return true;
  }

  // The edge pane holds editor content: split it, new pane on the rail side.
  const edge = edgeCenterPane(sessionsSide());
  if (edge) return windowActions.openTabInNewPane(edge.paneId, sessionsSide(), tab) !== null;

  const groupId = findFirstCenterPaneGroupId();
  if (!groupId) return false;
  windowActions.addTab(groupId, tab);
  windowActions.setActiveTab(groupId, tab.id);
  return true;
}

export function openSessionInChat(sessionId: string, sessionTitle: string): void {
  const host = tabHost();
  const existing = host.find((t) => t.metadata?.sessionId === sessionId);
  if (existing) {
    // A session ALWAYS reads beside the sessions rail. One that was dragged
    // elsewhere (a rail, the editor pane) moves back on reopen; the phone
    // stack has no panes, so there it only comes to the front. The move
    // remounts the transcript, which a deliberate reopen can afford.
    const group = Object.entries(windowStore.tabGroups).find(([, g]) =>
      g.tabs.some((t) => t.id === existing.id),
    )?.[0];
    const target = sessionPane();
    if (!group || (target && group === target.groupId)) {
      host.activate(existing.id);
      return;
    }
    // Remove, then open; when the open fails, the tab goes back where it
    // was, so a reopen can never lose a session.
    const tab: Tab = { ...existing };
    windowActions.removeTab(group, existing.id);
    if (openTabBesideEditor(tab)) {
      host.activate(tab.id);
      return;
    }
    const fallback = windowStore.tabGroups[group] ? group : findFirstCenterPaneGroupId();
    if (fallback) {
      windowActions.addTab(fallback, tab);
      windowActions.setActiveTab(fallback, tab.id);
    }
    return;
  }

  const opened = host.open({
    id: `tab-chat-${sessionId}`,
    title: sessionTitle || 'Chat',
    contentType: 'chat',
    icon: iconForContentType('chat'),
    metadata: { sessionId },
  }, { placement: 'beside-editor' });
  if (!opened) {
    console.error('openSessionInChat: no pane available — cannot open chat tab');
  }
}
