/**
 * The mockup's window moves, all through the core's own actions: open a note
 * in the centre, peek at one while the session covers the centre, float a
 * hover editor over a wikilink, and open the review.
 */
import { windowActions, windowStore } from '@/windowing/store';
import { collectLeafGroupIds, firstLeafGroupId } from '@/windowing/model/tree';
import type { Tab } from '@/windowing/model/types';
import { state } from './state';
import type { MockType } from './policy';

const basename = (p: string) => p.split('/').pop() ?? p;
const noteTab = (path: string): Tab<MockType> => ({
  id: `note:${path}`,
  title: basename(path),
  contentType: 'note',
  metadata: { path },
});

/** The centre group that holds a tab, if any. */
function centreGroupHolding(tabId: string): string | null {
  return (
    collectLeafGroupIds(windowStore.layout).find((id) =>
      windowStore.tabGroups[id]?.tabs.some((t) => t.id === tabId),
    ) ?? null
  );
}

/** The centre group that has focus, else the first one. */
function editorGroup(): string | null {
  const active = windowStore.activePaneId ? windowActions.getPaneTabGroupId(windowStore.activePaneId) : null;
  if (active && collectLeafGroupIds(windowStore.layout).includes(active)) return active;
  return firstLeafGroupId(windowStore.layout);
}

/**
 * Open a note as a centre tab.
 *
 * From the session while it covers the centre, and with the toggle exit, the
 * note opens as a PEEK instead: the session keeps the centre. With the
 * centre-focus exit the tab takes focus, which gives the centre back.
 */
export function openNote(path: string, opts: { fromSession?: boolean } = {}) {
  if (opts.fromSession && windowStore.expandedEdge && windowStore.expandExit === 'toggle') {
    peek(path);
    return;
  }
  const tab = noteTab(path);
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
  const tab: Tab<MockType> = { id: `changes:${sid}`, title: 'Changes', contentType: 'changes', metadata: { sid } };
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
  }, 220);
}
