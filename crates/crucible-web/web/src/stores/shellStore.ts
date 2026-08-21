import { createSignal } from 'solid-js';
import type { Tab, TabContentType } from '@/types/windowTypes';
import { windowStore, windowActions } from '@/stores/windowStore';
import { statusBarStore } from '@/stores/statusBarStore';
import { openPanelTab } from '@/lib/panel-actions';
import { findTabBySessionId } from '@/lib/session-actions';

// ── Shell surface state ──────────────────────────────────────────────────
// The shell has three navigable surfaces: Inbox (everything waiting on
// you), Session (chat), and Edit (the vault flow). There is no landing
// page — a fresh shell opens the new-session draft. Surfaces are not a
// router — they map onto tabs in the window manager; this store tracks
// which surface the focused center tab belongs to and provides the
// header's navigation actions.

export type ShellSurface = 'inbox' | 'session' | 'edit';

const [activeSurface, setActiveSurface] = createSignal<ShellSurface>('session');

/** Which surface a tab belongs to; null for tabs that don't change the
 * surface (settings, terminal, edge-panel tools…). */
export function surfaceForContentType(contentType: TabContentType): ShellSurface | null {
  switch (contentType) {
    case 'chat':
      return 'session';
    case 'file':
      return 'edit';
    case 'inbox':
      return 'inbox';
    default:
      return null;
  }
}

/** Called from tab-focus sync (tabActions/layoutActions) so the header pill
 * and status bar always reflect the tab the user is looking at. */
export function syncShellSurface(tab: Tab | undefined | null): void {
  if (!tab) return;
  const surface = surfaceForContentType(tab.contentType);
  if (surface) setActiveSurface(surface);
}

function focusMostRecentTabOfType(contentType: TabContentType): boolean {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.contentType === contentType);
    if (tab) {
      windowActions.setActiveTab(groupId, tab.id);
      return true;
    }
  }
  return false;
}

function goInbox(): void {
  openPanelTab('inbox');
}

/** Focus the active session's chat tab; fall back to any open chat tab;
 * otherwise start a fresh session (SessionContext owns creation). */
function goSession(): void {
  const sessionId = statusBarStore.activeSessionId();
  if (sessionId) {
    const existing = findTabBySessionId(sessionId);
    if (existing) {
      windowActions.setActiveTab(existing.groupId, existing.tab.id);
      return;
    }
  }
  if (focusMostRecentTabOfType('chat')) return;
  window.dispatchEvent(new CustomEvent('crucible:new-session'));
}

/** Focus the editor: most recent file tab, else an empty editor tab plus
 * the notes tree so there is something to open. */
function goEdit(): void {
  if (focusMostRecentTabOfType('file')) return;
  openPanelTab('files');
  openPanelTab('file');
}

export const shellStore = {
  activeSurface,
} as const;

export const shellActions = {
  goInbox,
  goSession,
  goEdit,
  setActiveSurface,
} as const;
