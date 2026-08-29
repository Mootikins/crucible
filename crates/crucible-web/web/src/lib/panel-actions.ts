import { findEdgePanelForGroup, windowActions, windowStore } from '@/stores/windowStore';
import { collectLeafGroupIds, primaryEdgeGroupId } from '@/stores/windowStoreInternals';
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

/** Content that belongs to a conversation, not to the editor. */
const SESSION_CONTENT = new Set(['chat', 'chat-draft']);

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
  const centre = collectLeafGroupIds(windowStore.layout)
    .map((id) => windowStore.tabGroups[id])
    .filter((g): g is NonNullable<typeof g> => !!g);

  const holdsEditorContent = (g: { tabs: { contentType: string }[] }) =>
    g.tabs.some((t) => !SESSION_CONTENT.has(t.contentType));

  return (
    centre.find(holdsEditorContent)?.id ??
    centre.find((g) => !g.tabs.some((t) => SESSION_CONTENT.has(t.contentType)))?.id ??
    centre[0]?.id ??
    null
  );
}

/** The first pane in the centre tiling — what a new pane splits off from. */
export function firstCenterPaneId(): string | null {
  function findFirst(node: LayoutNode): string | null {
    if (node.type === 'pane') return node.id;
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
    const groupId = primaryEdgeGroupId(windowStore, pos);
    if (!groupId) {
      console.error(`openPanelTab: edge panel '${pos}' has no tab group — cannot open '${contentType}'`);
      return;
    }
    windowActions.addTab(groupId, tab);
    windowActions.setEdgePanelCollapsed(pos, false);
  }
}
