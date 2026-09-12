import { Component, JSX, createContext, createSignal, useContext } from 'solid-js';
import { navStack, type NavStack } from '@/components/mobile/NavStack';

/** One pushed page: a title for the bar, and what fills the body. */
export interface SettingsPage {
  id: string;
  title: string;
  body: () => JSX.Element;
  /**
   * True when the body renders table rows rather than blocks.
   *
   * Every settings section is written as `<tr>`s, because the desktop lays
   * them out in a two-column table. A page carrying them needs a table to be
   * legal HTML; a page of navigation rows must NOT have one.
   */
  rows?: boolean;
}

export interface SettingsStack {
  /** The pages above the root, oldest first. */
  pages(): SettingsPage[];
  push(page: SettingsPage): void;
  /** Drop the top page. Answers false at the root, where there is none. */
  pop(): boolean;
  /** Drop every page, without touching history beyond what it owns. */
  reset(): void;
}

interface Entry {
  page: SettingsPage;
  release: () => void;
}

/**
 * The drill-down a phone navigates settings by.
 *
 * A phone gets ONE list at a time: the root names the categories, a tap opens
 * one, and back returns. It is the shape iOS, Android and Obsidian all use,
 * and it is not the desktop's — a 216 px section list beside a form does not
 * fit 412 px, and flattening it to a strip of tabs (what this replaced) hides
 * every label but two.
 *
 * Each page takes a history entry, so the phone's own back button walks the
 * levels before it leaves the app. A page closed by its own back control takes
 * its entry with it; see `NavStack`.
 */
export function createSettingsStack(back: NavStack = navStack()): SettingsStack {
  const [entries, setEntries] = createSignal<Entry[]>([]);

  return {
    pages: () => entries().map((e) => e.page),

    push(page) {
      let entry: Entry | undefined;
      const release = back.push(() => {
        // The browser went back. Drop this page and anything above it — a
        // gesture can skip levels, and those pages are gone either way.
        setEntries((held) => {
          const at = entry ? held.indexOf(entry) : -1;
          return at === -1 ? held : held.slice(0, at);
        });
      });
      entry = { page, release };
      setEntries((held) => [...held, entry!]);
    },

    pop() {
      const held = entries();
      const top = held[held.length - 1];
      if (!top) return false;
      setEntries(held.slice(0, -1));
      // Consumes the history entry. The layer is already spliced out, so the
      // popstate this causes closes nothing a second time.
      top.release();
      return true;
    },

    reset() {
      const held = entries();
      setEntries([]);
      // Newest first, so each `history.back()` unwinds the entry it owns.
      for (let i = held.length - 1; i >= 0; i -= 1) held[i].release();
    },
  };
}

const StackContext = createContext<SettingsStack | null>(null);

export const SettingsStackProvider: Component<{
  stack: SettingsStack;
  children: JSX.Element;
}> = (props) => (
  <StackContext.Provider value={props.stack}>{props.children}</StackContext.Provider>
);

/**
 * The drill-down a section sits in, or null on the desktop.
 *
 * Null is the ordinary case, not an error: the desktop shows one section as a
 * whole page, so a section that would push a sub-page renders it inline there
 * instead. A section MUST handle null rather than assume a phone.
 */
export function useSettingsStack(): SettingsStack | null {
  return useContext(StackContext);
}
