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
  const existing = findTabByFilePath(filePath);
  if (existing) {
    windowActions.setActiveTab(existing.groupId, existing.tab.id);
    return;
  }

  const groupId = findFirstCenterPaneGroupId();
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
