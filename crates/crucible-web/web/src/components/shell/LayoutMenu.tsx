import { Component, For, Show, createMemo, createSignal } from 'solid-js';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { windowActions } from '@/stores/windowStore';
import { closedPanels, openPanelTab } from '@/lib/panel-actions';
import { useResetLayout } from '@/lib/query/layout';
import { notificationActions } from '@/stores/notificationStore';
import { menuContent, menuItem, menuSeparator, menuTrigger } from '@/components/ui/menu-style';
import { ChevronRight, MoreHorizontal, RefreshCw } from '@/lib/icons';
import type { TabContentType } from '@/types/windowTypes';

/** `add:<panel id>` and the one reset, in one flat value space per menu. */
const ADD = 'add:';

/**
 * The rail kebab: put a closed pane back, or start the layout over.
 *
 * The windowing system lets a user close anything, and until this existed it
 * gave nothing back — a closed panel was reachable only from the command
 * palette, which you have to know the name of, and a layout the user had
 * broken could only be repaired from a settings page they had to find while
 * looking at the broken layout. The two repairs now sit ON the thing they
 * repair.
 *
 * Re-add lists only what is CLOSED: a menu that offers to open what is
 * already open reads as a list of panels, not as a repair, and `openPanelTab`
 * would just focus the existing tab. Reset asks first, because it is the one
 * action here that throws away work the user did arranging panes.
 *
 * Project actions used to live behind this button. They moved into the
 * sessions rail, beside the projects they act on — see `ProjectMenu`.
 */
export const LayoutMenu: Component = () => {
  const resetMutation = useResetLayout();
  const [busy, setBusy] = createSignal(false);
  const closed = createMemo(() => closedPanels());

  const reAdd = (value: string) => {
    if (!value.startsWith(ADD)) return;
    openPanelTab(value.slice(ADD.length) as TabContentType);
  };

  const reset = async () => {
    if (busy()) return;
    if (
      !window.confirm(
        'Reset the pane layout? Panes, tabs and rails return to what a fresh install ships with. Sessions and notes are untouched.',
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      // The server copy FIRST: the store write below triggers the layout
      // auto-save, so deleting afterwards races it and can leave the old
      // layout on disk to come back on the next load.
      await resetMutation.mutateAsync();
      windowActions.resetLayoutToDefaults();
      notificationActions.addNotification('info', 'Pane layout reset to defaults');
    } catch (err) {
      notificationActions.addNotification(
        'error',
        err instanceof Error ? err.message : 'Could not reset the layout',
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <Menu.Root>
      <Menu.Trigger
        data-testid="layout-menu"
        aria-label="Layout menu"
        class={menuTrigger}
      >
        <MoreHorizontal class="w-3.5 h-3.5" />
      </Menu.Trigger>
      <Portal>
        <Menu.Positioner>
          <Menu.Content data-testid="layout-menu-popout" class={`${menuContent} max-w-[18rem]`}>
            {/* Its own Root, which is how Ark nests a menu: the submenu owns
                its items and its own `onSelect`. */}
            <Menu.Root onSelect={(d) => reAdd(d.value)}>
              <Menu.TriggerItem data-testid="layout-readd" class={`${menuItem} justify-between`}>
                <span>Re-add pane</span>
                <ChevronRight class="w-3 h-3 shrink-0 text-muted-dark" />
              </Menu.TriggerItem>
              <Portal>
                <Menu.Positioner>
                  <Menu.Content
                    data-testid="layout-readd-popout"
                    class={`${menuContent} max-w-[18rem]`}
                  >
                    <For each={closed()}>
                      {(def) => (
                        <Menu.Item
                          value={`${ADD}${def.id}`}
                          data-testid={`layout-readd-${def.id}`}
                          class={menuItem}
                        >
                          <span class="truncate">{def.title}</span>
                        </Menu.Item>
                      )}
                    </For>
                    <Show when={closed().length === 0}>
                      <p class="px-3 py-4 text-center text-floor text-muted-dark">
                        Every panel is open
                      </p>
                    </Show>
                  </Menu.Content>
                </Menu.Positioner>
              </Portal>
            </Menu.Root>

            <hr class={menuSeparator} />
            <Menu.Item
              value="reset"
              data-testid="layout-reset"
              class={menuItem}
              onClick={() => void reset()}
            >
              <RefreshCw class="w-3 h-3 shrink-0 text-muted-dark" />
              <span>Reset layout</span>
            </Menu.Item>
          </Menu.Content>
        </Menu.Positioner>
      </Portal>
    </Menu.Root>
  );
};
