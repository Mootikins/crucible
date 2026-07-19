import { windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';
import { findFirstCenterPaneGroupId } from './panel-actions';
import { iconForContentType } from './tab-icons';

export function findTabByFilePath(filePath: string): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.metadata?.filePath === filePath);
    if (tab) return { groupId, tab };
  }
  return null;
}

export function openFileInEditor(filePath: string, fileName?: string): void {
  openFileInGroup(findFirstCenterPaneGroupId(), filePath, fileName);
}

/**
 * Open a file as a tab in a SPECIFIC tab group (drag-a-file-onto-a-pane).
 * Falls back to activating an existing tab wherever it lives — one file, one
 * tab, matching `openFileInEditor`.
 */
export function openFileInGroup(
  groupId: string | null,
  filePath: string,
  fileName?: string,
): void {
  const existing = findTabByFilePath(filePath);
  if (existing) {
    windowActions.setActiveTab(existing.groupId, existing.tab.id);
    return;
  }
  if (!groupId) return;

  const newTab: Tab = {
    id: `tab-file-${filePath}`,
    // Last-resort basename fallback: a falsy caller value would otherwise
    // mint a tab literally titled "undefined" (save prompts included).
    title: fileName || filePath.split('/').pop() || filePath,
    contentType: 'file',
    icon: iconForContentType('file'),
    metadata: { filePath },
  };

  windowActions.addTab(groupId, newTab);
}

/**
 * Close every open file tab at `absPath` (or, for a trashed directory, any
 * tab under it). The file is already gone from disk — the tabs would show
 * stale, unsavable content.
 */
export function closeTabsUnder(absPath: string, isDir: boolean): void {
  const prefix = `${absPath}/`;
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    for (const tab of [...group.tabs]) {
      const fp = tab.metadata?.filePath;
      if (typeof fp !== 'string') continue;
      if (fp === absPath || (isDir && fp.startsWith(prefix))) {
        windowActions.removeTab(groupId, tab.id);
      }
    }
  }
}
