/**
 * The mockup's window moves, all through the core's own actions: open a note
 * in the centre, peek at one while the session covers the centre, float a
 * hover editor over a wikilink, and open the review.
 */
import { windowActions, windowStore } from '@/windowing/store';
import { collectLeafGroupIds, firstLeafGroupId } from '@/windowing/model/tree';
import type { Tab } from '@/windowing/model/types';
import { setState, state } from './state';
import { ICONS, type MockType } from './policy';
import { basename } from './components/path';
let nextDoc = 1;
const noteTab = (path: string): Tab<MockType> => ({
  id: `doc:${nextDoc++}`,
  title: basename(path),
  contentType: 'note',
  icon: ICONS.note,
  metadata: { path },
});

/** Run a layout change as a view transition, so an expand morphs instead of jumping. */
export function withTransition(change: () => void) {
  const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown };
  if (doc.startViewTransition && !matchMedia('(prefers-reduced-motion: reduce)').matches) doc.startViewTransition(change);
  else change();
}

/** Put the caret in the session's composer (Ctrl+L, the note's ask button, New session). */
export function focusComposer() {
  document.querySelector<HTMLTextAreaElement>('.mk-composer textarea')?.focus();
}

/** The centre group that holds a tab, if any. */
function centreGroupHolding(tabId: string): string | null {
  return (
    collectLeafGroupIds(windowStore.layout).find((id) =>
      windowStore.tabGroups[id]?.tabs.some((t) => t.id === tabId),
    ) ?? null
  );
}

/** The group that holds a tab anywhere in the layout, if any. */
function groupHolding(tabId: string): string | null {
  return centreGroupHolding(tabId) ?? Object.values(windowStore.tabGroups).find((g) => g.tabs.some((t) => t.id === tabId))?.id ?? null;
}

/** Where a document opens when sessions take the centre: the right rail's first pane. */
function railDocGroup(): string | null {
  return firstLeafGroupId(windowStore.edgePanels.right.layout);
}

/** The centre group that has focus, else the first one. */
function editorGroup(): string | null {
  const active = windowStore.activePaneId ? windowActions.getPaneTabGroupId(windowStore.activePaneId) : null;
  if (active && collectLeafGroupIds(windowStore.layout).includes(active)) return active;
  return firstLeafGroupId(windowStore.layout);
}

/** Where a document opens: in the tab that has focus, a new tab, or a new split. */
export type OpenWhere = 'here' | 'tab' | 'split';

/** The place a click asks for: Ctrl/Cmd for a new tab, and with Shift for a split. */
export const whereFor = (e: MouseEvent): OpenWhere =>
  e.ctrlKey || e.metaKey ? (e.shiftKey ? 'split' : 'tab') : e.button === 1 ? 'tab' : 'here';

/**
 * Open a note, as a web browser opens a link.
 *
 * `here` (a plain click) navigates the document tab that has focus, and adds
 * the note to that tab's history; back and forward walk it. A new tab and a
 * new split are explicit (`tab`, `split`). A group that shows no document
 * gets a new tab.
 *
 * From the session while it covers the centre, and with the toggle exit, the
 * note opens as a PEEK instead: the session keeps the centre. With the
 * centre-focus exit the tab takes focus, which gives the centre back.
 */
export function openNote(path: string, opts: { fromSession?: boolean; where?: OpenWhere } = {}) {
  const where = opts.where ?? 'here';
  if (opts.fromSession && where === 'here' && windowStore.expandedEdge && windowStore.expandExit === 'toggle') {
    peek(path);
    return;
  }
  // Sessions take the centre: a document opens in the right rail instead.
  const inRail = state.spawn === 'sessions';
  let group = inRail ? railDocGroup() : editorGroup();
  if (!group) return;
  if (inRail) windowActions.setEdgePanelCollapsed('right', false);
  if (where === 'split' && !inRail) {
    const paneId = collectPaneIdsHolding(group);
    if (paneId) {
      const before = new Set(collectLeafGroupIds(windowStore.layout));
      windowActions.splitPane(paneId, 'horizontal');
      group = collectLeafGroupIds(windowStore.layout).find((id) => !before.has(id) && !windowStore.tabGroups[id]?.tabs.length) ?? group;
    }
  }
  const g = windowStore.tabGroups[group];
  const active = g?.tabs.find((t) => t.id === g.activeTabId);
  if (where === 'here' && active?.contentType === 'note') {
    navigate(group, active.id, path);
    return;
  }
  const tab = noteTab(path);
  windowActions.addTab(group, tab);
  windowActions.setActiveTab(group, tab.id);
}

/** The pane that shows a group, if the centre holds it. */
function collectPaneIdsHolding(groupId: string): string | null {
  const walk = (n: typeof windowStore.layout): string | null =>
    n.type === 'pane' ? (n.tabGroupId === groupId ? n.id : null) : walk(n.first) ?? walk(n.second);
  return walk(windowStore.layout);
}

/** Show a path in a tab without a history move. */
function show(groupId: string, tabId: string, path: string) {
  windowActions.updateTab(groupId, tabId, { title: basename(path), metadata: { path } });
}

/** Navigate a document tab: drop the forward entries, then push the path. */
function navigate(groupId: string, tabId: string, path: string) {
  const tab = windowStore.tabGroups[groupId]?.tabs.find((t) => t.id === tabId);
  const current = tab?.metadata?.path as string | undefined;
  if (current === path) return;
  const h = state.history[tabId] ?? { stack: current ? [current] : [], at: current ? 0 : -1 };
  const stack = [...h.stack.slice(0, h.at + 1), path];
  setState('history', tabId, { stack, at: stack.length - 1 });
  show(groupId, tabId, path);
}

/** The group that holds a tab, anywhere in the layout. */
const groupOf = (tabId: string) => groupHolding(tabId);

export const canGoBack = (tabId: string) => (state.history[tabId]?.at ?? 0) > 0;
export const canGoForward = (tabId: string) => {
  const h = state.history[tabId];
  return !!h && h.at < h.stack.length - 1;
};

/** Walk a tab's history by one step: -1 is back, 1 is forward. */
export function goHistory(tabId: string, step: -1 | 1) {
  const h = state.history[tabId];
  const group = groupOf(tabId);
  if (!h || !group) return;
  const at = h.at + step;
  if (at < 0 || at >= h.stack.length) return;
  setState('history', tabId, 'at', at);
  show(group, tabId, h.stack[at]!);
}

/**
 * Open a session. Documents first: the right rail shows it. Sessions first:
 * it opens as a centre tab, one tab for each session.
 */
export function openSession(sid: string) {
  setState('active', sid);
  if (state.spawn === 'docs') return;
  const tab: Tab<MockType> = {
    id: `session:${sid}`,
    title: state.sessions[sid]!.title,
    contentType: 'session',
    icon: ICONS.session,
    metadata: { sid },
  };
  const holder = centreGroupHolding(tab.id);
  if (holder) {
    windowActions.setActiveTab(holder, tab.id);
    return;
  }
  const group = editorGroup();
  if (!group) return;
  windowActions.addTab(group, tab);
  windowActions.setActiveTab(group, tab.id);
}

/** A note in a floating window over the right side: the click-only peek. */
export function peek(path: string) {
  const existing = windowStore.floatingWindows.find((w) =>
    windowStore.tabGroups[w.tabGroupId]?.tabs.some((t) => t.id === `peek:${path}`),
  );
  if (existing) {
    windowActions.bringToFront(existing.id);
    return;
  }
  const group = windowActions.createTabGroup();
  windowActions.addTab(group, { ...noteTab(path), id: `peek:${path}` });
  const width = Math.min(520, Math.round(window.innerWidth * 0.36));
  windowActions.createFloatingWindow(group, window.innerWidth - width - 56, 44, width, window.innerHeight - 96, {
    title: `Peek · ${basename(path)}`,
    showTabBar: false,
  });
}

/** The review of one session, as a centre tab. */
export function openChanges(sid = state.active) {
  const tab: Tab<MockType> = { id: `changes:${sid}`, title: 'Changes', contentType: 'changes', icon: ICONS.changes, metadata: { sid } };
  const holder = centreGroupHolding(tab.id);
  if (holder) {
    windowActions.setActiveTab(holder, tab.id);
    return;
  }
  const group = editorGroup();
  if (!group) return;
  windowActions.addTab(group, tab);
  windowActions.setActiveTab(group, tab.id);
}

// ---------------------------------------------------------------- hover editor
// Obsidian's Hover Editor, on the core's transient floating windows: a hover
// over a wikilink floats the note; it closes when the pointer leaves both the
// link and the window, unless the window was pinned (the core's pin button
// clears `transient`).
let openTimer: number | undefined;
let closeTimer: number | undefined;
let hoverWindow: string | null = null;

function closeHover() {
  if (!hoverWindow) return;
  const w = windowStore.floatingWindows.find((f) => f.id === hoverWindow);
  if (w?.transient) windowActions.removeFloatingWindow(w.id);
  hoverWindow = null;
}

export function hoverStart(anchor: HTMLElement, path: string) {
  window.clearTimeout(closeTimer);
  window.clearTimeout(openTimer);
  openTimer = window.setTimeout(() => {
    closeHover();
    const r = anchor.getBoundingClientRect();
    const group = windowActions.createTabGroup();
    windowActions.addTab(group, { ...noteTab(path), id: `hover:${path}:${Date.now()}` });
    const width = 440;
    const height = 320;
    const x = Math.max(8, Math.min(r.left, window.innerWidth - width - 8));
    const y = r.bottom + 6 + height > window.innerHeight ? Math.max(8, r.top - height - 6) : r.bottom + 6;
    hoverWindow = windowActions.createFloatingWindow(group, x, y, width, height, {
      transient: true,
      showTabBar: false,
      title: basename(path),
    });
  }, 380);
}

export function hoverEnd() {
  window.clearTimeout(openTimer);
  window.clearTimeout(closeTimer);
  closeTimer = window.setTimeout(() => {
    const el = hoverWindow ? document.querySelector(`[data-window-id="${hoverWindow}"]`) : null;
    if (el?.matches(':hover')) {
      el.addEventListener('mouseleave', () => hoverEnd(), { once: true });
      return;
    }
    closeHover();
    // Long enough to move the pointer from the link into the popup, or to
    // come back after a short slip past its edge.
  }, 700);
}
