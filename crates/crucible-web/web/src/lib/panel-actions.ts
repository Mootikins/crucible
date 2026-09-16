import { windowStore } from '@/stores/windowStore';
import { collectLeafGroupIds } from '@/windowing/model/tree';
import type { EdgePanelPosition } from '@/types/windowTypes';
import { getGlobalRegistry, type PanelDefinition } from './panel-registry';
import { iconForPanelId } from './tab-icons';
import { tabHost } from './tab-host';
import { terminalAllowed } from './terminal-availability';
import type { LayoutNode, Tab, TabContentType } from '@/types/windowTypes';

/** First pane group in the center tiling — where center-zone tabs open. */
export function findFirstCenterPaneGroupId(): string | null {
  function findFirst(node: LayoutNode): string | null {
    if (node.type === 'pane') return node.tabGroupId ?? null;
    return findFirst(node.first) || findFirst(node.second);
  }

  return findFirst(windowStore.layout);
}

/** Content that belongs to a conversation, not to the editor. */
const SESSION_CONTENT = new Set(['chat', 'chat-draft']);

/**
 * The rail that holds the session list. A session opens in the centre pane
 * NEXT TO this rail, and a file in the pane next to the other rail, so the
 * centre reads as sessions | editor with the two rails outside them. The
 * rule follows the rails, so a swap flips it with them.
 */
export function sessionsSide(): EdgePanelPosition {
  const railHolds = (side: EdgePanelPosition, contentType: string) =>
    collectLeafGroupIds(windowStore.edgePanels[side].layout).some((id) =>
      windowStore.tabGroups[id]?.tabs.some((t) => t.contentType === contentType),
    );
  for (const side of ['left', 'right'] as const) {
    if (railHolds(side, 'sessions')) return side;
  }
  // No sessions tab (the user closed it): the files rail names the other side.
  for (const side of ['left', 'right'] as const) {
    if (railHolds(side, 'files')) return side === 'left' ? 'right' : 'left';
  }
  return 'left';
}

export function filesSide(): EdgePanelPosition {
  return sessionsSide() === 'left' ? 'right' : 'left';
}

/**
 * The centre pane at one edge: the leftmost or rightmost leaf. A stacked
 * split has no left or right, so both halves count as the same edge and the
 * top one wins.
 */
export function edgeCenterPane(side: EdgePanelPosition): { paneId: string; groupId: string | null } | null {
  function walk(node: LayoutNode): { paneId: string; groupId: string | null } | null {
    if (node.type === 'pane') return { paneId: node.id, groupId: node.tabGroupId ?? null };
    if (node.direction === 'horizontal') return walk(side === 'left' ? node.first : node.second);
    return walk(node.first);
  }
  return walk(windowStore.layout);
}

function groupHolds(groupId: string | null, pred: (contentType: string) => boolean): boolean {
  const g = groupId ? windowStore.tabGroups[groupId] : undefined;
  return !!g && g.tabs.some((t) => pred(t.contentType));
}

/** True when the group holds nothing, or nothing that is a conversation. */
function groupIsEditorRoom(groupId: string | null): boolean {
  return !groupHolds(groupId, (c) => SESSION_CONTENT.has(c));
}

/**
 * The centre group a FILE belongs in, resolved by role.
 *
 * Not simply the first centre leaf. A session opens as a pane to the LEFT of
 * the editor, so the first leaf became the conversation — and files started
 * opening on top of the chat instead of beside it. The same defect as keying
 * sessions to a literal side, in the other direction.
 *
 * Prefers a pane that already holds editor content, then any centre pane that
 * is not a conversation, and falls back to the first leaf only when the centre
 * is nothing but conversations (opening a file there beats not opening it).
 */
export function editorGroupId(): string | null {
  // The pane next to the files rail first: that is where a file belongs.
  const edge = edgeCenterPane(filesSide());
  if (edge?.groupId && groupIsEditorRoom(edge.groupId)) return edge.groupId;

  const centre = collectLeafGroupIds(windowStore.layout)
    .map((id) => windowStore.tabGroups[id])
    .filter((g): g is NonNullable<typeof g> => !!g);

  const holdsEditorContent = (g: { tabs: { contentType: string }[] }) =>
    g.tabs.some((t) => !SESSION_CONTENT.has(t.contentType));

  // Any editor pane, then any pane that is not a conversation. When the
  // centre is nothing but conversations there is NO editor group: the caller
  // opens a new pane on the files side instead of putting a file on a chat.
  return (
    centre.find(holdsEditorContent)?.id ??
    centre.find((g) => !g.tabs.some((t) => SESSION_CONTENT.has(t.contentType)))?.id ??
    null
  );
}

/**
 * Panels a user may ask for BY NAME — the ones the layout menu and the
 * palette can open on their own.
 *
 * A file tab names a file and a chat tab names a session, so neither means
 * anything without one: "Open File" would open an empty viewer. They are
 * registered because something else (the tree, the session list) opens them
 * with an argument.
 */
const NOT_REOPENABLE = new Set<string>(['file', 'chat', 'chat-draft']);

/**
 * Registered panels with no tab open anywhere — what "Re-add pane" offers.
 *
 * Reads the live tab groups, so it is reactive: closing a panel puts it back
 * in the list. The terminal drops out where the client cannot run one (a
 * remote without the `remote_shell` opt-in), because re-adding it would open
 * a panel that only explains itself.
 */
export function closedPanels(): PanelDefinition[] {
  return getGlobalRegistry()
    .list()
    .filter((def) => !NOT_REOPENABLE.has(def.id))
    .filter((def) => def.id !== 'terminal' || terminalAllowed())
    .filter((def) => findTabByContentType(def.id as TabContentType) === null);
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
  const host = tabHost();
  const existing = host.find((t) => t.contentType === contentType);
  if (existing) {
    host.activate(existing.id);
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
    icon: iconForPanelId(contentType),
  };

  if (!host.open(tab, { placement: 'zone' })) {
    console.error(`openPanelTab: nowhere to open '${contentType}' (zone '${def.defaultZone}')`);
  }
}
