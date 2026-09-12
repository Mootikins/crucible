import { createStore } from 'solid-js/store';
import type { Tab, TabContentType } from '@/types/windowTypes';
import { iconForContentType } from '@/lib/tab-icons';

/**
 * The compact shell's tabs: one flat stack, no panes and no groups.
 *
 * It is the phone's answer to `windowStore`, and deliberately much smaller: a
 * phone shows one tab at a time, so panes, splits, floating windows and drop
 * targets have nothing to model. Both stores are reached through the same
 * `TabHost` seam, so the rest of the app never asks which shell it is in.
 *
 * It persists in THIS BROWSER. The desktop layout lives on the daemon and is
 * shared by every desktop, so a phone must never write there — see
 * `lib/shell-boot.ts` and the decision log, 2026-09-11.
 */

export const COMPACT_TABS_KEY = 'crucible:compactTabs';

interface TabStackState {
  tabs: Tab[];
  activeTabId: string | null;
}

/** A tab as it survives JSON: no icon, because an icon is a component. */
interface StoredTab {
  id: string;
  title: string;
  contentType: TabContentType;
  metadata?: Record<string, unknown>;
  isPinned?: boolean;
}

const empty = (): TabStackState => ({ tabs: [], activeTabId: null });

/** Read the saved tabs, rebuilding each icon from its content type. */
export function loadCompactTabs(): TabStackState {
  try {
    const raw = localStorage.getItem(COMPACT_TABS_KEY);
    if (!raw) return empty();
    const parsed = JSON.parse(raw) as { tabs?: StoredTab[]; activeTabId?: string | null };
    const tabs: Tab[] = [];
    for (const stored of parsed.tabs ?? []) {
      if (!stored || typeof stored.id !== 'string') continue;
      const icon = iconForContentType(stored.contentType);
      // A content type this build no longer draws is dropped, not guessed at.
      if (!icon) continue;
      tabs.push({
        id: stored.id,
        title: stored.title ?? stored.id,
        contentType: stored.contentType,
        icon,
        metadata: stored.metadata,
        isPinned: stored.isPinned,
      });
    }
    const activeTabId = tabs.some((t) => t.id === parsed.activeTabId)
      ? (parsed.activeTabId ?? null)
      : (tabs[tabs.length - 1]?.id ?? null);
    return { tabs, activeTabId };
  } catch {
    return empty(); // private mode, or storage a human edited
  }
}

const [tabStack, setTabStack] = createStore<TabStackState>(loadCompactTabs());

/**
 * Where the user has been, most recent first. Back walks this, not the order
 * the tabs were opened, and each tab at most once per walk.
 */
let visits: string[] = tabStack.activeTabId ? [tabStack.activeTabId] : [];
let walked: Set<string> | null = null;

function persist(): void {
  try {
    const stored: StoredTab[] = tabStack.tabs.map((t) => ({
      id: t.id,
      title: t.title,
      contentType: t.contentType,
      metadata: t.metadata as Record<string, unknown> | undefined,
      isPinned: t.isPinned,
    }));
    localStorage.setItem(
      COMPACT_TABS_KEY,
      JSON.stringify({ tabs: stored, activeTabId: tabStack.activeTabId }),
    );
  } catch {
    /* private mode: this session keeps its tabs in memory */
  }
}

function visit(id: string): void {
  visits = [id, ...visits.filter((v) => v !== id)];
  // A deliberate pick ends the current back walk, so back starts again here.
  walked = null;
}

export { tabStack };

export const tabStackActions = {
  activeTab(): Tab | null {
    return tabStack.tabs.find((t) => t.id === tabStack.activeTabId) ?? null;
  },

  open(tab: Tab): void {
    if (!tabStack.tabs.some((t) => t.id === tab.id)) {
      setTabStack('tabs', (tabs) => [...tabs, tab]);
    }
    setTabStack('activeTabId', tab.id);
    visit(tab.id);
    persist();
  },

  activate(id: string): void {
    if (!tabStack.tabs.some((t) => t.id === id)) return;
    setTabStack('activeTabId', id);
    visit(id);
    persist();
  },

  update(id: string, patch: Partial<Tab>): void {
    const at = tabStack.tabs.findIndex((t) => t.id === id);
    if (at === -1) return;
    setTabStack('tabs', at, patch);
    persist();
  },

  remove(id: string): void {
    const at = tabStack.tabs.findIndex((t) => t.id === id);
    if (at === -1) return;
    setTabStack('tabs', (tabs) => tabs.filter((t) => t.id !== id));
    visits = visits.filter((v) => v !== id);
    if (tabStack.activeTabId === id) {
      // The tab the user came from, not the neighbour in the list.
      const next = visits.find((v) => tabStack.tabs.some((t) => t.id === v)) ?? null;
      setTabStack('activeTabId', next);
    }
    persist();
  },

  /**
   * Move to the previously visited tab. Answers false when the walk has
   * already reached every tab, which is when the browser may leave the app.
   */
  back(): boolean {
    walked ??= new Set(tabStack.activeTabId ? [tabStack.activeTabId] : []);
    const next = visits.find((v) => !walked!.has(v) && tabStack.tabs.some((t) => t.id === v));
    if (!next) return false;
    walked.add(next);
    setTabStack('activeTabId', next);
    visits = [next, ...visits.filter((v) => v !== next)];
    persist();
    return true;
  },

  reset(): void {
    setTabStack(empty());
    visits = [];
    walked = null;
    persist();
  },
};
