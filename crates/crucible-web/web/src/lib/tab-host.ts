import { edgeCenterPane, filesSide } from './panel-actions';
import { isCompact } from '@/stores/deviceStore';
import { findEdgePanelForGroup, windowActions, windowStore } from '@/stores/windowStore';
import { primaryEdgeGroupId } from '@/stores/windowStoreInternals';
import { tabStack, tabStackActions } from '@/stores/tabStackStore';
import { getGlobalRegistry } from '@/lib/panel-registry';
import { editorGroupId, findFirstCenterPaneGroupId } from '@/lib/panel-actions';
import { openTabBesideEditor } from '@/lib/session-actions';
import type { Tab } from '@/types/windowTypes';

/**
 * The one way to reach the shell's tabs, whichever shell is drawn.
 *
 * The desktop keeps tabs in `windowStore`, inside panes and groups; the phone
 * keeps a flat stack in `tabStackStore`. Everything that opens, finds, renames,
 * marks or closes a tab goes through here, so no caller has to ask which shell
 * it is in. Before this existed, more than ten places wrote to `windowStore`
 * directly — including the one that marks a buffer unsaved, which on a phone
 * meant a tab could close over unsaved work without asking.
 *
 * Callers name tabs by id. Each host resolves its own grouping.
 */

/** Where a NEW tab goes on a shell that has more than one place to put it. */
type TabPlacement =
  /** With the editor. */
  | 'editor'
  /** Beside the editor — the session pane. */
  | 'beside-editor'
  /** The panel's registered default zone. */
  | 'zone';

export interface TabHost {
  list(): Tab[];
  find(pred: (tab: Tab) => boolean): Tab | null;
  /** The tab the user is looking at, or null. */
  activeTab(): Tab | null;
  /** Open a tab and focus it. Answers false when there is nowhere to put it. */
  open(tab: Tab, opts?: { placement?: TabPlacement }): boolean;
  activate(tabId: string): void;
  update(tabId: string, patch: Partial<Tab>): void;
  remove(tabId: string): void;
}

// ── The desktop host: tabs live in panes and groups ────────────────────────

/** The group holding `tabId`, which the window store needs for every write. */
function groupOf(tabId: string): string | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    if (group.tabs.some((t) => t.id === tabId)) return groupId;
  }
  return null;
}

const windowTabHost: TabHost = {
  list() {
    return Object.values(windowStore.tabGroups).flatMap((g) => g.tabs);
  },

  find(pred) {
    return windowTabHost.list().find(pred) ?? null;
  },

  activeTab() {
    const paneId = windowStore.activePaneId;
    const pane = paneId ? windowActions.findPaneById(paneId) : null;
    const group = pane?.tabGroupId ? windowStore.tabGroups[pane.tabGroupId] : null;
    return group?.tabs.find((t) => t.id === group.activeTabId) ?? null;
  },

  open(tab, opts) {
    const placement = opts?.placement ?? 'editor';
    if (placement === 'beside-editor') {
      // Left of the editor, splitting the centre if there is no session pane.
      return openBesideEditor(tab);
    }
    if (placement === 'zone') return openInDefaultZone(tab);
    const groupId = editorGroup();
    if (!groupId) {
      // The centre holds only conversations. A file gets its own pane on the
      // files side of the pane at that edge, never a tab on top of a chat.
      const edge = edgeCenterPane(filesSide());
      if (!edge) return false;
      return windowActions.openTabInNewPane(edge.paneId, filesSide(), tab) !== null;
    }
    windowActions.addTab(groupId, tab);
    windowActions.setActiveTab(groupId, tab.id);
    return true;
  },

  activate(tabId) {
    const groupId = groupOf(tabId);
    if (!groupId) return;
    // An edge panel needs expanding first, or the focus lands out of sight.
    const pos = findEdgePanelForGroup(groupId);
    if (pos) {
      windowActions.setEdgePanelCollapsed(pos, false);
      windowActions.setEdgePanelActiveTab(pos, tabId);
    } else {
      windowActions.setActiveTab(groupId, tabId);
    }
  },

  update(tabId, patch) {
    const groupId = groupOf(tabId);
    if (groupId) windowActions.updateTab(groupId, tabId, patch);
  },

  remove(tabId) {
    const groupId = groupOf(tabId);
    if (groupId) windowActions.removeTab(groupId, tabId);
  },
};

/** The centre group the editor lives in, or null when the centre is all
 * conversations (the caller then opens a new pane on the files side). */
function editorGroup(): string | null {
  return editorGroupId();
}

/** Left of the editor, where a conversation belongs. */
function openBesideEditor(tab: Tab): boolean {
  return openTabBesideEditor(tab);
}

/** The zone the panel registered itself for. An edge zone is expanded, or the
 * tab lands where nobody can see it. */
function openInDefaultZone(tab: Tab): boolean {
  const zone = getGlobalRegistry().get(tab.contentType)?.defaultZone ?? 'center';
  if (zone === 'center') {
    const groupId = findFirstCenterPaneGroupId();
    if (!groupId) return false;
    windowActions.addTab(groupId, tab);
    windowActions.setActiveTab(groupId, tab.id);
    return true;
  }
  const groupId = primaryEdgeGroupId(windowStore, zone);
  if (!groupId) return false;
  windowActions.addTab(groupId, tab);
  windowActions.setEdgePanelCollapsed(zone, false);
  windowActions.setEdgePanelActiveTab(zone, tab.id);
  return true;
}

// ── The compact host: one flat stack, no placement ─────────────────────────

const stackTabHost: TabHost = {
  list: () => tabStack.tabs,
  find: (pred) => tabStack.tabs.find(pred) ?? null,
  activeTab: () => tabStackActions.activeTab(),
  // Placement is a desktop word. A phone has one surface, so it is ignored.
  open: (tab) => {
    tabStackActions.open(tab);
    return true;
  },
  activate: (tabId) => tabStackActions.activate(tabId),
  update: (tabId, patch) => tabStackActions.update(tabId, patch),
  remove: (tabId) => tabStackActions.remove(tabId),
};

/** The host of the shell this page drew. Fixed for the life of the page. */
export function tabHost(): TabHost {
  return isCompact() ? stackTabHost : windowTabHost;
}
