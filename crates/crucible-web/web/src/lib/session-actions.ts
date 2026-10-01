import { windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';
import { edgeLeaf, collectLeafGroupIds } from '@/windowing';
import { sessionsSide } from './panel-actions';
import { iconForContentType } from './tab-icons';
import { tabHost } from './tab-host';

/** Content that makes a pane a session pane. */
const SESSION_CONTENT = new Set(['chat', 'chat-draft']);

/** The conversation rail follows the navigation rail when the sides swap. */
function conversationSide() {
  return sessionsSide() === 'left' ? 'right' : 'left';
}

export function sessionPane(): { groupId: string } | null {
  const centreGroups = collectLeafGroupIds(windowStore.layout);
  const centreHasConversations = centreGroups.some((id) => windowStore.tabGroups[id]?.tabs.some(
    (tab) => SESSION_CONTENT.has(tab.contentType) || tab.contentType === 'terminal',
  ));
  const groups = [
    ...(centreHasConversations ? centreGroups : []),
    ...collectLeafGroupIds(windowStore.edgePanels[conversationSide()].layout),
  ].map(id => windowStore.tabGroups[id]).filter(group => group &&
    !group.tabs.some(tab => tab.contentType === 'sessions' || tab.contentType === 'files'));
  // Supporting tabs do not stop a user-arranged pane from being a conversation
  // pane. Prefer an existing conversation over an unused group in the layout.
  const group = groups.find(group => group.tabs.some(tab => SESSION_CONTENT.has(tab.contentType)))
    ?? groups.find(group => group.tabs.length === 0);
  if (group) return { groupId: group.id };
  return null;
}

/** Reuse the conversation pane, or split the rail without replacing its content. */
export function openTabInSessionRail(tab: Tab): boolean {
  const side = conversationSide();
  const existing = sessionPane();
  if (existing) {
    if (collectLeafGroupIds(windowStore.edgePanels[side].layout).includes(existing.groupId)) {
      windowActions.setEdgeMode(side, 'docked');
    }
    windowActions.addTab(existing.groupId, tab);
    tabHost().activate(tab.id);
    return true;
  }
  windowActions.setEdgeMode(side, 'docked');
  const edge = edgeLeaf(windowStore.edgePanels[side].layout, 'left');
  return windowActions.openTabInNewPane(edge.paneId, 'top', tab) !== null;
}

export function openSessionInChat(sessionId: string, sessionTitle: string): void {
  const host = tabHost();
  const existing = host.find((t) => t.metadata?.sessionId === sessionId);
  if (existing) {
    // Activating an existing tab preserves its buffer, scroll and user placement.
    host.activate(existing.id);
    return;
  }

  const opened = host.open({
    id: `tab-chat-${sessionId}`,
    title: sessionTitle || 'Chat',
    contentType: 'chat',
    icon: iconForContentType('chat'),
    metadata: { sessionId },
  }, { placement: 'session-rail' });
  if (!opened) {
    console.error('openSessionInChat: no pane available — cannot open chat tab');
  }
}
