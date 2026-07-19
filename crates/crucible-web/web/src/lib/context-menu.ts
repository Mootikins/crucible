/**
 * Shared rules for the app's custom right-click menus.
 *
 * The override is SELECTIVE by design: we only replace the native menu where
 * we have something better to offer, and never on copyable content — images
 * and external links keep the browser menu ("Copy image", "Save image as…"
 * cannot be replicated from JS). Shift+right-click always yields the native
 * menu (Firefox does this natively; the capture guard mirrors it in Chrome).
 */

/** True when this contextmenu event must fall through to the browser menu. */
export function shouldUseNativeMenu(e: MouseEvent): boolean {
  if (e.shiftKey) return true;
  const target = e.target;
  if (!(target instanceof Element)) return false;
  return target.closest('img, a[href], [data-native-menu]') !== null;
}

export type TabCloseMode = 'close' | 'close-others' | 'close-right';

/**
 * The tabs a close action removes (pure — the caller still runs each through
 * the dirty-tab confirm guard).
 */
export function tabsToClose<T extends { id: string }>(
  tabs: T[],
  tabId: string,
  mode: TabCloseMode,
): T[] {
  const idx = tabs.findIndex((t) => t.id === tabId);
  if (idx === -1) return [];
  switch (mode) {
    case 'close':
      return [tabs[idx]];
    case 'close-others':
      return tabs.filter((t) => t.id !== tabId);
    case 'close-right':
      return tabs.slice(idx + 1);
  }
}

/**
 * Capture-phase guard for a custom-menu region: events that must go native
 * (Shift, images, links) are stopped before any ark context trigger inside
 * sees them, so the browser menu appears. Use as a `ref` callback.
 */
export function attachNativeMenuGuard(el: HTMLElement): void {
  el.addEventListener(
    'contextmenu',
    (e) => {
      if (shouldUseNativeMenu(e)) e.stopPropagation();
    },
    { capture: true },
  );
}
