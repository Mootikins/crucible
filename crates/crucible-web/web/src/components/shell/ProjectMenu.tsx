import { Component, For, Show } from 'solid-js';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { treeSectionHeader } from '@/components/tree/tree-style';
import { menuContent, menuItem, menuSeparator } from '@/components/ui/menu-style';
import { PROJECT_PARAM } from '@/lib/project-url';
import { Check, ExternalLink, MoreHorizontal } from '@/lib/icons';

/** This page, addressed to one project — see `PROJECT_PARAM`. */
function windowUrlForProject(path: string): string {
  const url = new URL(window.location.href);
  url.searchParams.set(PROJECT_PARAM, path);
  return url.toString();
}

/** `pin:<path>` and `window:<path>` in one flat value space, so one `onSelect`
 * routes both without a second menu. */
const PIN = 'pin:';
const NEW_WINDOW = 'window:';

/**
 * The titlebar kebab: pin a project, or open one in a second window.
 *
 * Everything here is low-frequency, which is why it is buried behind a kebab
 * while the session switcher sits in the open. Pinning happens IN THIS WINDOW.
 * Switching projects must not spawn a window: two windows means two layouts
 * and two things to track — worth it when you deliberately want them side by
 * side, actively bad as the automatic outcome of a switch. `New window` is
 * therefore a separate, quieter row per project.
 *
 * Built on Ark's Menu, like `FileTreeContextMenu`. A hand-rolled popout stood
 * here first: it declared `role="menu"` and `role="menuitem"` and then
 * implemented no keyboard navigation at all — an ARIA contract it could not
 * honour, and one an unstyled menu primitive satisfies for free (roving focus,
 * typeahead, arrow keys, Escape restoring focus to the trigger, viewport-aware
 * placement that survives a resize).
 */
export const ProjectMenu: Component = () => {
  const { projects, currentProject, selectProject } = useProjectSafe();

  const onSelect = (value: string) => {
    if (value.startsWith(PIN)) {
      void selectProject(value.slice(PIN.length));
      return;
    }
    if (value.startsWith(NEW_WINDOW)) {
      window.open(windowUrlForProject(value.slice(NEW_WINDOW.length)), '_blank', 'noopener');
    }
  };

  return (
    <Menu.Root onSelect={(d) => onSelect(d.value)}>
      <Menu.Trigger
        data-testid="project-menu"
        aria-label="Project menu"
        class="inline-flex items-center justify-center h-6 w-6 rounded border border-hairline text-muted hover:text-shell-ink hover:bg-hover-wash transition-colors"
      >
        <MoreHorizontal class="w-3.5 h-3.5" />
      </Menu.Trigger>
      <Portal>
        <Menu.Positioner>
          <Menu.Content data-testid="project-menu-popout" class={`${menuContent} max-w-[18rem]`}>
            <div class={treeSectionHeader}>Pin a project</div>
            <For each={projects()}>
              {(p) => (
                <Menu.Item value={`${PIN}${p.path}`} data-testid={`project-pin-${p.path}`} class={menuItem}>
                  <Check
                    class="w-3 h-3 shrink-0"
                    classList={{ 'opacity-0': currentProject()?.path !== p.path }}
                  />
                  <span class="truncate">{p.name}</span>
                </Menu.Item>
              )}
            </For>
            <Show when={projects().length === 0}>
              <p class="px-3 py-4 text-center text-floor text-muted-dark">No projects registered</p>
            </Show>

            <Show when={projects().length > 0}>
              <hr class={menuSeparator} />
              <div class={treeSectionHeader}>Open in a new window</div>
              <For each={projects()}>
                {(p) => (
                  <Menu.Item
                    value={`${NEW_WINDOW}${p.path}`}
                    data-testid={`project-new-window-${p.path}`}
                    class={menuItem}
                  >
                    <ExternalLink class="w-3 h-3 shrink-0 text-muted-dark" />
                    <span class="truncate">{p.name}</span>
                  </Menu.Item>
                )}
              </For>
            </Show>
          </Menu.Content>
        </Menu.Positioner>
      </Portal>
    </Menu.Root>
  );
};
