import { windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';
import { focusTabInPlace, openTabDockedRight } from './session-actions';
import { iconForContentType } from './tab-icons';

/**
 * First-message handoff for lazy session creation: the draft surface stores
 * the typed message here BEFORE `crucible:open-session` mounts the real
 * ChatProvider, which consumes it and sends through the normal optimistic
 * path. In-memory on purpose — tab metadata is persisted with the layout,
 * and a persisted first message would re-send on every reload.
 */
const pendingFirstMessages = new Map<string, string>();

export function setPendingFirstMessage(sessionId: string, message: string): void {
  pendingFirstMessages.set(sessionId, message);
}

/** Non-destructive read — for RENDERING the optimistic turn. A provider that
 * remounts mid-handoff must still see the message; only the dispatcher
 * consumes. */
export function peekPendingFirstMessage(sessionId: string): string | undefined {
  return pendingFirstMessages.get(sessionId);
}

/** Destructive take — call at DISPATCH time only. First caller wins; a
 * concurrent (e.g. zombie pre-remount) dispatcher gets undefined and must
 * skip, so the message can never be sent twice. */
export function consumePendingFirstMessage(sessionId: string): string | undefined {
  const message = pendingFirstMessages.get(sessionId);
  pendingFirstMessages.delete(sessionId);
  return message;
}

/**
 * Prefill handoff for the Home composer: the landing page stashes the text the
 * user typed BEFORE `openDraftSession` mounts the DraftSessionPanel, which
 * consumes it into its message box on mount (the user reviews and hits enter —
 * it is never auto-sent). Separate from `pendingFirstMessages`, which is the
 * post-send handoff into the real ChatProvider. In-memory on purpose.
 */
let pendingDraftPrefill: string | null = null;

export function setDraftPrefill(text: string): void {
  pendingDraftPrefill = text;
}

/** Destructive take — call once at mount. Clears so a later plain draft-open
 * (e.g. the "+ new session" affordances) starts with an empty box. */
export function consumeDraftPrefill(): string | null {
  const text = pendingDraftPrefill;
  pendingDraftPrefill = null;
  return text;
}

let draftCounter = 0;

function findDraftTab(): { groupId: string; tab: Tab } | null {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    const tab = group.tabs.find((t) => t.contentType === 'chat-draft');
    if (tab) return { groupId, tab };
  }
  return null;
}

/**
 * Open (or focus) a draft session tab — the lazy-creation surface. Nothing
 * touches the daemon until the first message is sent; the draft panel then
 * creates the session and closes itself.
 */
export function openDraftSession(): void {
  const existing = findDraftTab();
  if (existing) {
    focusTabInPlace(existing.groupId, existing.tab.id);
    return;
  }

  const tabId = `tab-draft-${++draftCounter}`;
  const opened = openTabDockedRight({
    id: tabId,
    title: 'New Session',
    contentType: 'chat-draft',
    icon: iconForContentType('chat-draft'),
    metadata: { draftTabId: tabId },
  });
  if (!opened) {
    console.error('openDraftSession: no pane available — cannot open draft tab');
  }
}

/** Close a draft tab wherever it lives (used after the real session opens). */
export function closeDraftTab(tabId: string): void {
  for (const [groupId, group] of Object.entries(windowStore.tabGroups)) {
    if (group.tabs.some((t) => t.id === tabId)) {
      windowActions.removeTab(groupId, tabId);
      return;
    }
  }
}
