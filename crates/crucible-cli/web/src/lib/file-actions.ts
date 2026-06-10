import { windowActions, windowStore } from '@/stores/windowStore';
import type { LayoutNode, Tab } from '@/types/windowTypes';

export function findTabByFilePath(filePath: string): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.metadata?.filePath === filePath);
    if (tab) return { groupId, tab };
  }
  return null;
}

function findFirstCenterPaneGroupId(): string | null {
  function findFirst(node: LayoutNode): string | null {
    if (node.type === 'pane') return node.tabGroupId ?? null;
    return findFirst(node.first) || findFirst(node.second);
  }

  return findFirst(windowStore.layout);
}

export function openFileInEditor(filePath: string, fileName: string): void {
  const existing = findTabByFilePath(filePath);
  if (existing) {
    windowActions.setActiveTab(existing.groupId, existing.tab.id);
    return;
  }

  const groupId = findFirstCenterPaneGroupId();
  if (!groupId) return;

  const newTab: Tab = {
    id: `tab-file-${filePath}`,
    title: fileName,
    contentType: 'file',
    metadata: { filePath },
  };

  windowActions.addTab(groupId, newTab);
}
