import { isCompact } from '@/stores/deviceStore';
import { isBasePath } from './markdown-path';
import { windowActions, windowStore } from '@/stores/windowStore';
import { collectPanes } from '@/windowing';
import type { Tab } from '@/types/windowTypes';

import { iconForContentType } from './tab-icons';
import { recordRecentFile } from './recent-files';
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
  const preferred = `tab-file-${filePath}`;
  const id = tabHost().find((tab) => tab.id === preferred) ? `${preferred}-${crypto.randomUUID()}` : preferred;
  return {
    id,
    // Last-resort basename fallback: a falsy caller value would otherwise
    // mint a tab literally titled "undefined" (save prompts included).
    title: fileName || filePath.split('/').pop() || filePath,
    contentType,
    icon: iconForContentType(contentType),
    metadata: { filePath },
  };
}

export interface FileOpenOptions {
  where?: 'here' | 'tab' | 'split';
  tabId?: string;
  line?: number;
}

export function fileOpenOptionsForEvent(event: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }): FileOpenOptions {
  return { where: event.shiftKey ? 'split' : event.ctrlKey || event.metaKey ? 'tab' : 'here' };
}

export interface FileHistory {
  entries: { path: string; title: string }[];
  at: number;
}

export function fileHistory(tab: Tab): FileHistory {
  const raw = tab.metadata?.fileHistory as FileHistory | undefined;
  if (raw && Array.isArray(raw.entries) && raw.entries.every((e) => e !== null && typeof e === 'object' && typeof e.path === 'string' && typeof e.title === 'string') && Number.isInteger(raw.at) && raw.at >= 0 && raw.at < raw.entries.length) return raw;
  const path = tab.metadata?.filePath;
  return { entries: typeof path === 'string' ? [{ path, title: tab.title }] : [], at: 0 };
}

export function fileHistoryPaths(tab: Tab): string[] {
  const retained = tab.metadata?.fileHistoryRetained;
  return [...new Set([...fileHistory(tab).entries.map((entry) => entry.path), ...(Array.isArray(retained) ? retained.filter((path): path is string => typeof path === 'string') : [])])];
}

export function goFileHistory(tabId: string, step: -1 | 1): void {
  const host = tabHost();
  const tab = host.find((t) => t.id === tabId);
  if (!tab) return;
  const history = fileHistory(tab);
  const at = history.at + step;
  const entry = history.entries[at];
  if (!entry) return;
  host.update(tab.id, { title: entry.title, metadata: { ...tab.metadata, filePath: entry.path, fileHistory: { ...history, at }, scrollToLine: undefined, scrollToNote: undefined } });
  host.activate(tab.id);
}

export function openFileInEditor(filePath: string, fileName?: string, options: FileOpenOptions = {}): void {
  const host = tabHost();
  const active = options.tabId ? host.find((t) => t.id === options.tabId) : host.activeTab();
  if (options.where === 'here' && active?.contentType === 'file' && contentTypeForPath(filePath) === 'file') {
    if (active.metadata?.filePath === filePath) {
      if (options.line !== undefined) host.update(active.id, { metadata: { ...active.metadata, scrollToLine: options.line } });
      return;
    }
    const history = fileHistory(active);
    const entries = [...history.entries.slice(0, history.at + 1), { path: filePath, title: fileName || filePath.split('/').pop() || filePath }];
    host.update(active.id, { title: entries.at(-1)!.title, metadata: { ...active.metadata, filePath, fileHistoryRetained: [...new Set([...fileHistoryPaths(active), filePath])], fileHistory: { entries, at: entries.length - 1 }, scrollToLine: options.line, scrollToNote: undefined } });
    host.activate(active.id);
    recordRecentFile(filePath, entries.at(-1)!.title);
    return;
  }
  const existing = host.find((t) => t.metadata?.filePath === filePath);
  if (existing && options.where !== 'tab' && options.where !== 'split') {
    if (options.line !== undefined) host.update(existing.id, { metadata: { ...existing.metadata, scrollToLine: options.line } });
    host.activate(existing.id);
    return;
  }
  const tab = fileTab(filePath, fileName);
  if (options.line !== undefined) tab.metadata = { ...tab.metadata, scrollToLine: options.line };
  if (options.where === 'split' && !isCompact()) {
    const originPane = options.tabId ? [windowStore.layout, windowStore.edgePanels.left.layout, windowStore.edgePanels.right.layout]
      .flatMap(collectPanes).find(pane => windowStore.tabGroups[pane.tabGroupId ?? '']?.tabs.some(tab => tab.id === options.tabId)) : undefined;
    const pane = originPane?.id ?? windowStore.activePaneId;
    if (pane && windowActions.openTabInNewPane(pane, 'right', tab)) {
      recordRecentFile(filePath, tab.title);
      return;
    }
  }
  if (host.open(tab, { placement: 'editor' })) recordRecentFile(filePath, tab.title);
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
  openFileInEditor(filePath, fileName, { line });
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
function contentTypeForPath(filePath: string): 'file' | 'canvas' | 'base' {
  if (isBasePath(filePath)) return 'base';
  return /\.canvas$/i.test(filePath) ? 'canvas' : 'file';
}

export function openFileInGroup(groupId: string | null, filePath: string, fileName?: string): void {
  const existing = findTabByFilePath(filePath);
  if (existing) {
    windowActions.setActiveTab(existing.groupId, existing.tab.id);
    return;
  }
  if (!groupId) return;

  const newTab = fileTab(filePath, fileName);

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
