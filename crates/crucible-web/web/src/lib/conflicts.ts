/**
 * The conflicts waiting on a person, as the surfaces read them.
 *
 * A conflict is an outbox entry the daemon could neither take nor merge
 * (`lib/offline/outbox.ts`). The entry holds the three texts and the hash a
 * resolution must be written against, so this store is a READ of the outbox,
 * never a second copy: a reload rebuilds it from the queue, and a resolution
 * that lands removes the entry rather than a row beside it.
 *
 * Global rather than context-bound for the reason `review-store.ts` is: its
 * consumers do not share a provider. The offline badge is in the phone's
 * chrome, the More sheet is in the shell, and the Changes panel renders in the
 * right edge region.
 */
import { createStore } from 'solid-js/store';
import { openPanelTab } from '@/lib/panel-actions';
import type { TabContentType } from '@/types/windowTypes';
import {
  pendingConflicts,
  resolveConflict,
  type Conflicted,
  type WriteOutcome,
} from '@/lib/offline/sync';

/**
 * The registered panel that draws a conflict, as `openPanelTab` names it.
 *
 * The registry keys on a plain string and the tab store on `TabContentType`,
 * which no registered panel outside the layout's own set belongs to. The cast
 * is the same one the phone's overflow menu makes (`MobileShell.tsx`).
 */
const CONFLICTS_PANEL = 'conflicts' as TabContentType;

const [state, setState] = createStore<{ rows: Conflicted[]; selected: string | null }>({
  rows: [],
  selected: null,
});

export const conflictStore = {
  /** Every conflict, oldest first. Reactive. */
  list(): Conflicted[] {
    return state.rows;
  },
  /** How many wait. What the badge and the More sheet count. */
  count(): number {
    return state.rows.length;
  },
  /** The conflict for a note, or null. */
  get(path: string): Conflicted | null {
    return state.rows.find((row) => row.path === path) ?? null;
  },
  /**
   * The note a surface asked to settle, or null for the list.
   *
   * It lives here rather than in the panel because three surfaces point at
   * one conflict — the badge, the phone's More sheet and the Changes panel —
   * and none of them draws it.
   */
  selected(): string | null {
    return state.selected;
  },
};

export const conflictActions = {
  /** Say which conflict the panel should draw. `null` draws the list. */
  select(path: string | null): void {
    setState('selected', path);
  },

  /** Read the outbox again. Cheap: it is one indexed list of a short queue. */
  async refresh(): Promise<void> {
    setState('rows', await pendingConflicts());
  },

  /**
   * The conflict for a note, read fresh.
   *
   * A surface opens a conflict by path — from a notification, a badge or the
   * panel — and the row it was listed with may be a drain old by then.
   */
  async open(path: string): Promise<Conflicted | null> {
    await conflictActions.refresh();
    return conflictStore.get(path);
  },

  /**
   * Write the text a person chose.
   *
   * The write carries the hash and the text the conflict was settled FROM, so
   * a note that moved again is refused rather than overwritten; the entry then
   * stays and the caller shows it again. The store is re-read either way,
   * because both answers change what waits.
   */
  async resolve(path: string, text: string): Promise<WriteOutcome> {
    try {
      return await resolveConflict(path, text);
    } finally {
      await conflictActions.refresh();
    }
  },
};

/**
 * Show a conflict, or the list of them, wherever this device draws panels.
 *
 * One door for every surface that counts conflicts but does not draw them.
 * On a phone `openPanelTab` stacks the tab; on a desktop it opens it in the
 * centre, beside the note the writing belongs to.
 */
export function openConflict(path: string | null = null): void {
  conflictActions.select(path);
  openPanelTab(CONFLICTS_PANEL);
}

/** Test seam: forget every row, the way `__resetReviewStore` does. */
export function __resetConflictStore(): void {
  setState({ rows: [], selected: null });
}
