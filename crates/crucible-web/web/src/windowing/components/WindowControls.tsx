import { Component, Show, createContext, onCleanup, useContext, type JSX } from 'solid-js';
import { windowStore, windowActions } from '@/windowing/store';
import type { FloatingChrome, FloatingWindow } from '@/windowing/model/types';
import { confirmTabClose } from '@/windowing/model/tab-guards';
import { IconClose, IconLayout, IconMinimize, IconMaximize, IconPin, IconTabBar } from './icons';

/**
 * The controls of one floating window. They act on the whole window, that
 * is, on its tab group.
 *
 * `FloatingChrome` decides where they sit: in the title bar, in the actions
 * area of the window's tab bar, or in the content of a window without a tab
 * bar. Each place renders these components, so each control has one
 * implementation.
 */

const btn = 'wm-floating-btn';

/** The window that `windowId` names, or undefined after it closes. */
const windowOf = (windowId: string): FloatingWindow | undefined =>
  windowStore.floatingWindows.find((w) => w.id === windowId);

/**
 * The pin of a transient (hover) window. A pin promotes the window to a
 * normal window, so the button goes away after the click.
 */
export const WindowPinButton: Component<{ windowId: string }> = (props) => (
  <Show when={windowOf(props.windowId)?.transient}>
    <button
      type="button"
      class={btn}
      data-testid="float-pin"
      onClick={(e) => {
        e.stopPropagation();
        windowActions.pinFloatingWindow(props.windowId);
      }}
      title="Pin (keep open when the cursor leaves)"
    >
      <IconPin class="w-3 h-3" />
    </button>
  </Show>
);

/** The tab bar toggle, dock, roll up, maximize or restore, and close. */
export const WindowActionButtons: Component<{ windowId: string }> = (props) => {
  const w = () => windowOf(props.windowId);
  // Closing the window closes its tabs: the same unsaved-changes contract as
  // every other path that closes a tab (confirmTabClose per modified tab).
  const handleClose = () => {
    const win = w();
    if (!win) return;
    const tabs = windowStore.tabGroups[win.tabGroupId]?.tabs ?? [];
    for (const tab of tabs.filter((t) => t.isModified)) {
      if (!confirmTabClose(tab)) return;
    }
    windowActions.closeFloatingWindow(props.windowId);
  };
  return (
    <>
      <button
        type="button"
        class={btn}
        data-testid="float-tabbar-toggle"
        onClick={() =>
          windowActions.updateFloatingWindow(props.windowId, {
            showTabBar: w()?.showTabBar === false,
          })
        }
        title={w()?.showTabBar === false ? 'Show tab bar' : 'Hide tab bar'}
      >
        <IconTabBar class="w-3 h-3" />
      </button>
      <button
        type="button"
        class={btn}
        data-testid="float-dock"
        onClick={() => windowActions.dockFloatingWindow(props.windowId)}
        title="Dock back into the layout"
      >
        <IconLayout class="w-3 h-3" />
      </button>
      <button
        type="button"
        class={btn}
        data-testid="float-minimize"
        onClick={() => windowActions.minimizeFloatingWindow(props.windowId)}
        title="Roll up into the window bar"
      >
        <IconMinimize class="w-3 h-3" />
      </button>
      <button
        type="button"
        class={btn}
        data-testid="float-maximize"
        onClick={() =>
          w()?.isMaximized
            ? windowActions.restoreFloatingWindow(props.windowId)
            : windowActions.maximizeFloatingWindow(props.windowId)
        }
        title={w()?.isMaximized ? 'Restore previous size' : 'Maximize'}
      >
        <IconMaximize class="w-3 h-3" />
      </button>
      <button
        type="button"
        class={btn}
        data-testid="float-close"
        onClick={handleClose}
        title="Close (closes its tabs)"
      >
        <IconClose class="w-3 h-3" />
      </button>
    </>
  );
};

/**
 * Every control of one floating window, in one group. A tab bar and a tabless
 * content use this component. The title bar puts the pin before the title
 * instead, so it renders the two halves itself.
 */
export const WindowControls: Component<{ windowId: string }> = (props) => (
  <div class="wm-window-controls flex items-center" data-testid="window-controls">
    <WindowPinButton windowId={props.windowId} />
    <WindowActionButtons windowId={props.windowId} />
  </div>
);

// ── The floating window context ─────────────────────────────────────────

/** What a component inside a floating window knows about that window. */
export interface FloatingWindowHandle {
  /** The id of the floating window. */
  id: string;
  /**
   * `WindowControls` for this window. A tabless content renders it in its
   * own nav bar. While it is mounted, the window draws no fallback title bar.
   */
  controls: Component;
  /** Where the window puts its controls. See `FloatingChrome`. */
  chrome: () => FloatingChrome;
  /** True when the window shows its tab bar. */
  hasTabBar: () => boolean;
}

const FloatingWindowCtx = createContext<FloatingWindowHandle | null>(null);

/**
 * Give the descendants of a floating window its handle. `onClaim` hears each
 * mount of `controls` and returns the release for its cleanup.
 */
export function FloatingWindowProvider(props: {
  windowId: string;
  onClaim: () => () => void;
  children: JSX.Element;
}): JSX.Element {
  const id = props.windowId;
  const handle: FloatingWindowHandle = {
    id,
    controls: () => {
      onCleanup(props.onClaim());
      return <WindowControls windowId={id} />;
    },
    chrome: () => windowStore.floatingChrome,
    hasTabBar: () => windowOf(id)?.showTabBar !== false,
  };
  return <FloatingWindowCtx.Provider value={handle}>{props.children}</FloatingWindowCtx.Provider>;
}

/**
 * The floating window that holds the caller, or null for a docked component.
 *
 * A content that shows one document in a window without a tab bar puts the
 * controls in its own nav bar, and marks the bar as a drag handle:
 *
 * ```tsx
 * const fw = useFloatingWindow();
 * <nav data-wm-drag-handle>
 *   <Show when={fw && fw.chrome() === 'merged' && !fw.hasTabBar()}>
 *     <fw.controls />
 *   </Show>
 * </nav>
 * ```
 */
export function useFloatingWindow(): FloatingWindowHandle | null {
  return useContext(FloatingWindowCtx);
}

/** The attribute that marks a drag handle of a floating window. */
export const DRAG_HANDLE_ATTR = 'data-wm-drag-handle';

/** Targets that keep their own pointer behaviour inside a drag handle. */
const INTERACTIVE =
  'button, input, textarea, select, a, [role="button"], [contenteditable=""], [contenteditable="true"], [data-tab-id]';

/**
 * True when a press on `target` starts a drag of the window whose root is
 * `root`. The press must land in a drag handle of that window, and not on an
 * interactive element inside the handle.
 */
export function startsWindowDrag(target: EventTarget | null, root: HTMLElement): boolean {
  if (!(target instanceof Element)) return false;
  const handle = target.closest(`[${DRAG_HANDLE_ATTR}]`);
  if (!handle || !root.contains(handle)) return false;
  const interactive = target.closest(INTERACTIVE);
  return !interactive || !handle.contains(interactive);
}
