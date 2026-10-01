import { statusBarStore } from '@/stores/statusBarStore';
import { windowActions, windowStore } from '@/stores/windowStore';
import { collectLeafGroupIds, collectPanes, edgeLeaf } from '@/windowing';
import type { LayoutNode, EdgePanelPosition } from '@/types/windowTypes';
import { getGlobalRegistry, type PanelDefinition } from './panel-registry';
import { iconForPanelId } from './tab-icons';
import { tabHost } from './tab-host';
import { terminalAllowed } from './terminal-availability';
import type { Tab, TabContentType } from '@/types/windowTypes';
import {
  diffsetKey,
  diffsetTitle,
  type DiffFocus,
  type DiffFocusRequest,
  type DiffsetSource,
} from './diffset';


/** Content that belongs to a conversation, not to the editor. */
const SESSION_CONTENT = new Set(['chat', 'chat-draft']);

/** Navigation follows its Sessions panel when the rails swap. */
export function sessionsSide(): EdgePanelPosition {
  const railHolds = (side: EdgePanelPosition, contentType: string) =>
    collectLeafGroupIds(windowStore.edgePanels[side].layout).some((id) =>
      windowStore.tabGroups[id]?.tabs.some((t) => t.contentType === contentType),
    );
  for (const side of ['left', 'right'] as const) {
    if (railHolds(side, 'sessions')) return side;
  }
  // If Sessions moved to the centre, Files still locates navigation.
  for (const side of ['left', 'right'] as const) {
    if (railHolds(side, 'files')) return side;
  }
  return 'left';
}

export function filesSide(): EdgePanelPosition {
  return sessionsSide() === 'left' ? 'right' : 'left';
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
  // A centre/rail swap moves the editor's actual group. Follow that group
  // instead of placing a new document beside the conversations it left.
  const edgeGroups = (['left', 'right'] as const).flatMap((side) =>
    collectLeafGroupIds(windowStore.edgePanels[side].layout),
  );
  const movedEditor = edgeGroups.find((id) => windowStore.tabGroups[id]?.tabs.some(
    (tab) => ['file', 'canvas', 'base'].includes(tab.contentType),
  ));
  if (movedEditor) return movedEditor;
  const centreGroups = collectLeafGroupIds(windowStore.layout);
  if (centreGroups.some((id) => windowStore.tabGroups[id]?.tabs.some(
    (tab) => SESSION_CONTENT.has(tab.contentType) || tab.contentType === 'terminal',
  ))) {
    const emptyEditor = edgeGroups.find((id) => windowStore.tabGroups[id]?.tabs.length === 0);
    if (emptyEditor) return emptyEditor;
  }
  // The pane next to the files rail first: that is where a file belongs.
  const edge = edgeLeaf(windowStore.layout, filesSide());
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
const NOT_REOPENABLE = new Set<string>(['file', 'chat', 'chat-draft', 'diff']);

/**
 * Closed or individually folded panels — what "Re-add pane" offers.
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
    .filter((def) => {
      const existing = findTabByContentType(def.id as TabContentType);
      if (!existing) return true;
      return [windowStore.layout, windowStore.edgePanels.left.layout, windowStore.edgePanels.right.layout]
        .flatMap(collectPanes).some(pane => pane.tabGroupId === existing.groupId && pane.collapsed);
    });
}

export function findTabByContentType(
  contentType: TabContentType,
): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.contentType === contentType);
    if (tab) return { groupId, tab };
  }
  return null;
}

/**
 * The target of a panel that opens one tab for each target, such as one tab
 * for each diffset. The metadata reaches the panel as its props.
 */
export interface PanelTarget {
  /** The tab id is `tab-${contentType}-${key}`. */
  key: string;
  /** Absent: the registered title of the panel. */
  title?: string;
  metadata?: Record<string, unknown>;
}

/** The tab id of one target of a panel. */
function targetTabId(contentType: TabContentType, key: string): string {
  return `tab-${contentType}-${key}`;
}

/**
 * Open a registered panel as a tab (command-palette / gear entry point).
 *
 * Focuses the existing tab when one is already open (singleton panels —
 * there is no reason for two Settings tabs), otherwise creates the tab in
 * the panel's registered default zone. Collapsed edge panels are expanded
 * so the result is always visible.
 *
 * With a target, the panel has one tab for each target key, and the existing
 * tab is the tab of that key.
 */
export function openPanelTab(contentType: TabContentType, target?: PanelTarget): void {
  const host = tabHost();
  const id = target ? targetTabId(contentType, target.key) : `tab-${contentType}`;
  const existing = target
    ? host.find((t) => t.id === id)
    : host.find((t) => t.contentType === contentType);
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
    id,
    title: target?.title ?? def.title,
    contentType,
    icon: iconForPanelId(contentType),
    ...(target?.metadata ? { metadata: target.metadata } : {}),
  };

  if (!host.open(tab, { placement: 'zone' })) {
    console.error(`openPanelTab: nowhere to open '${contentType}' (zone '${def.defaultZone}')`);
  }
}

/** The chat of a new diff tab: the session of a record, else the caller's. */
function chatOf(source: DiffsetSource, session?: string): string | undefined {
  if (source.kind === 'session_record') return source.session;
  return session ?? statusBarStore.activeSessionId() ?? undefined;
}

/** The last sequence number of a focus request. */
let focusSeq = 0;

/**
 * Open the diff pane of one diffset, or focus its tab when it is open.
 *
 * With a focus target, the pane scrolls to that file and expands it. An open
 * tab gets the new target in its metadata, and the mounted pane reads it.
 *
 * The pane names one chat, which takes the comments that the user writes in
 * it. A session record names its own session. Any other diffset takes
 * `session` from the caller that opened it from a chat, and else the session
 * that is active now. The tab keeps that choice in its metadata, so the pane
 * does not follow the focus of the user from chat to chat.
 */
export function openDiff(source: DiffsetSource, focus?: DiffFocus, session?: string): void {
  const key = diffsetKey(source);
  const request: DiffFocusRequest | undefined = focus ? { ...focus, seq: ++focusSeq } : undefined;
  const chat = chatOf(source, session);
  const metadata = {
    source,
    ...(chat ? { session: chat } : {}),
    ...(request ? { focus: request } : {}),
  };
  if (request) {
    const host = tabHost();
    const existing = host.find((t) => t.id === targetTabId('diff', key));
    if (existing) {
      host.update(existing.id, { metadata: { ...existing.metadata, ...metadata } });
      host.activate(existing.id);
      return;
    }
  }
  openPanelTab('diff', { key, title: diffsetTitle(source), metadata });
}

/** Swap the document areas while leaving the terminal's rail subtree in place. */
export function swapConversationAndEditor(): void {
  const candidates = (node: LayoutNode): LayoutNode[] => {
    const hasTerminal = collectLeafGroupIds(node).some(id =>
      windowStore.tabGroups[id]?.tabs.some(tab => tab.contentType === 'terminal'));
    if (!hasTerminal) return [node];
    return node.type === 'split' ? [...candidates(node.first), ...candidates(node.second)] : [];
  };
  const priority = (node: LayoutNode): number => {
    const groups = collectLeafGroupIds(node).map(id => windowStore.tabGroups[id]);
    if (groups.some(group => group?.tabs.some(tab =>
      SESSION_CONTENT.has(tab.contentType) || ['file', 'canvas', 'base'].includes(tab.contentType)))) return 2;
    return groups.some(group => group?.tabs.length === 0) ? 1 : 0;
  };
  const target = candidates(windowStore.edgePanels.right.layout)
    .sort((a, b) => priority(b) - priority(a))[0];
  if (!target) return;
  windowActions.swapCentreWithEdge('right', target.id);
  const needsRail = collectPanes(windowStore.edgePanels.right.layout).some(pane =>
    windowStore.tabGroups[pane.tabGroupId ?? '']?.tabs.some(tab =>
      tab.contentType !== 'terminal' || !pane.collapsed));
  if (!needsRail) windowActions.setEdgePanelCollapsed('right', true);
}
