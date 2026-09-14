import { windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';

import { iconForContentType } from './tab-icons';
import { recordRecentFile } from './recent-files';
import { pendingDiffActions } from '@/stores/pendingDiffStore';
import { tabHost } from './tab-host';

export function findTabByFilePath(filePath: string): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.metadata?.filePath === filePath);
    if (tab) return { groupId, tab };
  }
  return null;
}

/** The tab a file opens as, on either shell. */
function fileTab(filePath: string, fileName?: string): Tab {
  const contentType = contentTypeForPath(filePath);
  return {
    id: `tab-file-${filePath}`,
    // Last-resort basename fallback: a falsy caller value would otherwise
    // mint a tab literally titled "undefined" (save prompts included).
    title: fileName || filePath.split('/').pop() || filePath,
    contentType,
    icon: iconForContentType(contentType),
    metadata: { filePath },
  };
}

export function openFileInEditor(filePath: string, fileName?: string): void {
  // Through the host: on the desktop this is the EDITOR group, not the first
  // centre leaf (a session opens to its left, so "first" became the chat); on
  // a phone it is the one content surface.
  const host = tabHost();
  const existing = host.find((t) => t.metadata?.filePath === filePath);
  if (existing) {
    host.activate(existing.id);
    return;
  }
  const tab = fileTab(filePath, fileName);
  if (host.open(tab, { placement: 'editor' })) recordRecentFile(filePath, tab.title);
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
  const host = tabHost();
  const existing = host.find((t) => t.metadata?.filePath === filePath);
  if (existing) {
    host.update(existing.id, {
      metadata: { ...existing.metadata, filePath, scrollToLine: line },
    });
    host.activate(existing.id);
    return;
  }
  const tab = fileTab(filePath, fileName);
  tab.metadata = { filePath, scrollToLine: line };
  if (host.open(tab, { placement: 'editor' })) recordRecentFile(filePath, tab.title);
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
  const host = tabHost();
  for (const tab of [...host.list()]) {
    const fp = tab.metadata?.filePath;
    if (typeof fp !== 'string') continue;
    if (fp === absPath || (isDir && fp.startsWith(prefix))) host.remove(tab.id);
  }
}
