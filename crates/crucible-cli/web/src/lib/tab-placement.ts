/**
 * Place a group-less tab (DragSource 'newTab') into any window-system drop
 * target. This is what lets non-window surfaces — wikilink hover cards
 * today, anything carrying a Tab payload tomorrow — participate in the same
 * drag-and-drop abstraction as panes, tabs, and edge panels.
 */
import { windowStore, windowActions } from '@/stores/windowStore';
import type { DropTarget, Tab } from '@/types/windowTypes';

/** Focus the tab if some group already holds its id. */
function focusExisting(tabId: string): boolean {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    if (group.tabs.some((t) => t.id === tabId)) {
      windowActions.setActiveTab(groupId, tabId);
      return true;
    }
  }
  return false;
}

export function placeNewTab(target: DropTarget, tab: Tab): void {
  // Same dedupe rule as openFileInEditor: one tab per identity, focus wins.
  if (focusExisting(tab.id)) return;

  switch (target.type) {
    case 'pane': {
      const groupId =
        windowActions.getPaneTabGroupId(target.paneId) ??
        windowActions.createTabGroup(target.paneId);
      windowActions.addTab(groupId, tab);
      if (target.position && target.position !== 'center') {
        // Reuse the move machinery for directional splits: the tab now has a
        // source group, so a split-drop behaves exactly like a tab drag.
        windowActions.splitPaneAndDrop(target.paneId, target.position, groupId, tab.id);
      }
      break;
    }
    case 'tabGroup': {
      windowActions.addTab(target.groupId, tab, target.insertIndex);
      break;
    }
    case 'edgePanel': {
      const panel = windowStore.edgePanels[target.panelId];
      if (!panel) return;
      windowActions.addTab(panel.tabGroupId, tab, target.insertIndex);
      if (panel.isCollapsed) {
        windowActions.setEdgePanelCollapsed(target.panelId, false);
      }
      break;
    }
    case 'newFloating': {
      const groupId = windowActions.createTabGroup();
      windowActions.addTab(groupId, tab);
      windowActions.createFloatingWindow(groupId, 100, 100, 400, 300);
      break;
    }
  }
}
