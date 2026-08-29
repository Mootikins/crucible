import { windowActions, windowStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';
import { focusTabInPlace, openTabBesideEditor } from './session-actions';
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
 *
 * `workspace` pre-selects the project the session will act in. It is what
 * makes "New session" a PROJECT action: the sessions tree offers the row per
 * project, and the draft must open already aimed at that one.
 *
 * One draft surface at a time. A second call RETARGETS the open draft instead
 * of focusing it unchanged — focusing a draft still aimed at the previous
 * project would silently discard the project the user just picked.
 */
export function openDraftSession(opts: { workspace?: string } = {}): void {
  const existing = findDraftTab();
  if (existing) {
    if (opts.workspace !== undefined) {
      windowActions.updateTab(existing.groupId, existing.tab.id, {
        metadata: { ...existing.tab.metadata, workspace: opts.workspace },
      });
    }
    focusTabInPlace(existing.groupId, existing.tab.id);
    return;
  }

  const tabId = `tab-draft-${++draftCounter}`;
  const opened = openTabBesideEditor({
    id: tabId,
    title: 'New Session',
    contentType: 'chat-draft',
    icon: iconForContentType('chat-draft'),
    // ALWAYS define `workspace`, even as undefined. A panel's props are keyed
    // from the metadata present when it mounts (`reactiveMetadataProps`), so a
    // key omitted here gets no reactive channel and can never be written to
    // later. The spread guarded on truthiness dropped it for `''` — the
    // ribbon's explicit "no project" — and that draft could then never be
    // retargeted at a project, which is the common path.
    metadata: { draftTabId: tabId, workspace: opts.workspace },
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
