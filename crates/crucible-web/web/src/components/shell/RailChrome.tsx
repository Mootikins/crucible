import { Component, Show, createSignal, type JSX } from 'solid-js';
import { RibbonCommand, ribbonBtn, type EdgePanelPosition } from '@/windowing';
import { Bell, Moon, Settings, Sun } from '@/lib/icons';
import { LayoutMenu } from '@/components/shell/LayoutMenu';
import { OfflineBadge } from '@/components/OfflineBadge';
import { NotificationCenter } from '@/components/NotificationCenter';
import { applyTheme, theme } from '@/lib/theme';
import { notificationStore } from '@/stores/notificationStore';

/**
 * The notification bell, at the bottom of the right ribbon.
 *
 * Its own component rather than a `RibbonCommand` because it carries an unread
 * badge and owns a popout, neither of which that shape supports. Keeps
 * `data-testid="corner-bell"` from its previous home so existing locators
 * resolve.
 */
const RibbonBell: Component = () => {
  const [open, setOpen] = createSignal(false);
  const unreadCount = () => notificationStore.notificationCount();
  let bellRef: HTMLButtonElement | undefined;

  return (
    <>
      <button
        type="button"
        ref={bellRef}
        data-testid="corner-bell"
        class={`${ribbonBtn} relative w-10 h-9 flex-none`}
        classList={{ 'text-shell-body': open() }}
        title="Notifications"
        aria-label="Toggle notifications"
        onClick={() => setOpen(!open())}
      >
        <Bell class="w-4 h-4" />
        <Show when={unreadCount() > 0}>
          {/* The count was 8px — three steps under the app's 11px floor, and
              unreadable at a glance, which is the badge's only job. It reads
              the floor now, and the badge grew to hold it: a 15px pill that
              still clears the 16px icon it sits on. `tabular-nums` keeps 1 and
              9 the same width, so the badge does not twitch as the count
              climbs. */}
          <span class="absolute -top-0.5 -right-0.5 px-1 min-w-[15px] text-center rounded-full bg-error text-white text-floor font-semibold leading-[15px] tabular-nums">
            {unreadCount() > 99 ? '99+' : unreadCount()}
          </span>
        </Show>
      </button>
      <NotificationCenter open={open()} onClose={() => setOpen(false)} anchor={bellRef} />
    </>
  );
};

/**
 * The head of a rail, under its toggle: the layout actions on the left rail.
 *
 * Layout actions put a closed pane back, or start the layout over. They sit on
 * the rail because the rail IS the layout: the two repairs belong on the thing
 * they repair, not on a settings page the user has to find while looking at
 * what they broke. Project actions used to sit here. They moved into the
 * sessions pane, beside the projects they act on.
 */
export function railHead(position: EdgePanelPosition): JSX.Element {
  return position === 'left' ? (
    <div class="flex-none w-10 h-9 flex items-center justify-center border-b border-hairline">
      <LayoutMenu />
    </div>
  ) : null;
}

/**
 * The tail of a rail, at its far end.
 *
 * Left: the offline badge, the theme and settings, the shell-wide toggles,
 * together at the bottom-left like Obsidian's gear. Right: the notification
 * bell. The bell used to float in the centre pane's bottom-right corner, where
 * it sat over the document and vanished with the rest of the transient chip
 * cluster. The rail is rendered outside the slide clip frame, so the bell
 * survives a collapsed panel.
 */
export function railTail(position: EdgePanelPosition): JSX.Element {
  if (position === 'right') return <RibbonBell />;
  return (
    <>
      <OfflineBadge />
      <RibbonCommand
        title={theme() === 'light' ? 'Switch to dark theme' : 'Switch to light theme'}
        testId="ribbon-cmd-theme"
        onClick={() => applyTheme(theme() === 'light' ? 'dark' : 'light')}
      >
        <Show when={theme() === 'light'} fallback={<Sun class="w-4 h-4" />}>
          <Moon class="w-4 h-4" />
        </Show>
      </RibbonCommand>
      <RibbonCommand
        title="Settings"
        testId="ribbon-cmd-settings"
        // A dialog, not a tab. Changing a setting is a detour you return
        // from; it never wanted a pane, a split or a place in the layout.
        onClick={() => window.dispatchEvent(new CustomEvent('crucible:open-settings'))}
      >
        <Settings class="w-4 h-4" />
      </RibbonCommand>
    </>
  );
}
