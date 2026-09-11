import { windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';
import { editorGroupId } from './panel-actions';
import { iconForContentType } from './tab-icons';
import { recordRecentFile } from './recent-files';
import { pendingDiffActions } from '@/stores/pendingDiffStore';

export function findTabByFilePath(filePath: string): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.metadata?.filePath === filePath);
    if (tab) return { groupId, tab };
  }
  return null;
}

export function openFileInEditor(filePath: string, fileName?: string): void {
  // The EDITOR group, not the first centre leaf. A session opens as a pane to
  // the left of the editor, so "first" became the conversation and files
  // opened on top of the chat.
  openFileInGroup(editorGroupId(), filePath, fileName);
}

/**
 * Open (or focus, if already open) a file and overlay a proposed edit as an
 * inline diff in the real editor buffer: `original` is the current content,
 * `proposed` is what the agent wants. The file's editor renders the proposed
 * content diffed against the original (unified merge view), so a pending edit
 * is reviewed in place. Registering the diff before opening means an
 * already-open tab picks it up reactively too.
 */
export function openFileWithDiff(
  filePath: string,
  original: string,
  proposed: string,
  fileName?: string,
): void {
  pendingDiffActions.set(filePath, { original, proposed });
  openFileInEditor(filePath, fileName);
}

/**
 * Open a file in the editor and scroll to one line.
 *
 * The route out of a locked setting: a settings control the user's `init.lua`
 * pins names the file and the line that pins it, and this opens that line. A
 * lock with no route out is a dead end, which is why the jump is part of the
 * lock rather than an extra.
 *
 * A file already open is scrolled rather than opened twice — the editor reads
 * `scrollToLine` off the tab, so the metadata is updated on the existing tab.
 */
export function openFileAtLine(filePath: string, line: number, fileName?: string): void {
  const existing = findTabByFilePath(filePath);
  if (existing) {
    windowActions.updateTab(existing.groupId, existing.tab.id, {
      metadata: { ...existing.tab.metadata, filePath, scrollToLine: line },
    });
    windowActions.setActiveTab(existing.groupId, existing.tab.id);
    return;
  }
  const groupId = editorGroupId();
  if (!groupId) return;
  const title = fileName || filePath.split('/').pop() || filePath;
  windowActions.addTab(groupId, {
    id: `tab-file-${filePath}`,
    title,
    contentType: contentTypeForPath(filePath),
    icon: iconForContentType(contentTypeForPath(filePath)),
    metadata: { filePath, scrollToLine: line },
  });
  recordRecentFile(filePath, title);
}

/**
 * Open a file as a tab in a SPECIFIC tab group (drag-a-file-onto-a-pane).
 * Falls back to activating an existing tab wherever it lives — one file, one
 * tab, matching `openFileInEditor`.
 */
/**
 * Which panel opens a path. A `.canvas` is a spatial document, not text, so it
 * routes to the canvas editor rather than the file viewer.
 */
function contentTypeForPath(filePath: string): 'file' | 'canvas' {
  return /\.canvas$/i.test(filePath) ? 'canvas' : 'file';
}

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
    contentType: contentTypeForPath(filePath),
    icon: iconForContentType(contentTypeForPath(filePath)),
    metadata: { filePath },
  };

  windowActions.addTab(groupId, newTab);
  recordRecentFile(filePath, newTab.title);
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
