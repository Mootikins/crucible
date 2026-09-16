import type { Component } from 'solid-js';
import type { Tab, WindowState } from '../model/types';
import type { LayoutCodecHooks } from '../model/serializer';
import type { ShortcutAction } from '../shortcuts';

/**
 * Every decision the window manager does not own.
 *
 * Each member is required. A policy that forgets one is a type error, not a
 * silent default: the core has no opinion about what a rail holds, which tab
 * may close, or what a chord does outside the layout.
 *
 * The members use method syntax on purpose. TypeScript checks method
 * parameters bivariantly, so a `WindowPolicy<'a' | 'b'>` is a
 * `WindowPolicy<string>`, and the core stores one without a cast.
 */
export interface WindowPolicy<C extends string = string> {
  /** The state a fresh profile, and a reset, start from. */
  seed(): WindowState<C>;
  /** False keeps the tab. Every close path calls it. */
  mayCloseTab(state: WindowState<C>, groupId: string, tabId: string): boolean;
  /** Change a restored or reset draft before it becomes the store. */
  repairLayout(draft: WindowState<C>): void;
  /** The tab that took focus, or undefined when the focused pane has none. */
  onActiveTabChange(tab: Tab<C> | undefined): void;
  /**
   * The icon of a tab of this type. The core calls it for each tab that a
   * stored layout brings back, because no layout stores an icon.
   */
  iconFor(contentType: C): Component<{ class?: string }> | undefined;
  /**
   * Why the tab cannot work here, or null when it can. A reason greys the
   * tab out, and the ribbon shows the reason in its tooltip.
   */
  unavailableReason(tab: Tab<C>): string | null;
  /** The reader hooks for a stored layout: legacy upgrade and prune. */
  layoutHooks: LayoutCodecHooks<C>;
  /** The chords that the keyboard loop matches, in priority order. */
  shortcuts: readonly ShortcutAction[];
  /** Handle an action that the core does not own. True when it consumed the action. */
  onShortcut(action: string, e: KeyboardEvent): boolean;
}
