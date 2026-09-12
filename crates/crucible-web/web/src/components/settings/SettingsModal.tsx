import { Component, For, Show, createEffect, createResource, createSignal, onCleanup } from 'solid-js';
import { isCompact } from '@/stores/deviceStore';
import { Portal, Dynamic } from 'solid-js/web';
import { X } from '@/lib/icons';
import { settingsSections, settingsGroups } from './sections';
import { getPluginOptions } from '@/lib/api';
import { WithoutSectionHeaders } from './primitives';
import { MobileSettings } from './MobileSettings';
import { createSettingsStack, SettingsStackProvider } from './settings-nav';

/**
 * Settings, as a modal with its sections down the left — on a desktop.
 *
 * It used to be a TAB in the centre tiling area, which made it a peer of the
 * work: it took a pane, it could be split beside a session, it survived a
 * reload, and closing it was a tab close rather than a dismissal. None of that
 * matches what changing a setting is — a brief detour you return from. Worse,
 * as a tab it was one uninterrupted scroll of nine sections, so finding the
 * terminal font meant scrolling past every model and plugin row.
 *
 * The shape is Obsidian's, for the reason Obsidian uses it: a left list turns
 * "scroll until you see it" into "read nine labels", and it keeps the section
 * you are in named on screen while you change it.
 *
 * A PHONE gets a different shape entirely, not this one squeezed: one list,
 * drilled into, with a back control. See `MobileSettings`.
 */
export const SettingsModal: Component<{ open: boolean; onClose: () => void }> = (props) => {
  const [activeId, setActiveId] = createSignal(settingsSections()[0].id);
  const stack = createSettingsStack();
  let panelRef: HTMLDivElement | undefined;

  /**
   * The plugins' declared trees, fetched once the dialog opens.
   *
   * Keyed on `props.open` so a closed dialog costs nothing: describing every
   * plugin runs each tree's function-valued fields — `oci` shells out to find
   * its installed runtimes — so this is a real cost, not a cheap GET.
   *
   * A failed fetch leaves the plugin group absent rather than erroring the
   * dialog. The app's OWN settings must stay reachable when the plugin host is
   * unhappy; that is when a user most needs them.
   */
  const [trees, { refetch }] = createResource(
    () => (props.open ? 'open' : null),
    async () => {
      try {
        return await getPluginOptions();
      } catch {
        return {};
      }
    },
  );

  // Wrapped rather than passed straight through: `refetch` answers with the
  // resource's value, and the row's `onChanged` contract is "resolves once the
  // reloaded tree is in hand" — a value, not a signal.
  const reload = async () => {
    await refetch();
  };
  const sections = () => settingsSections(trees(), reload);
  const active = () => sections().find((s) => s.id === activeId()) ?? sections()[0];

  createEffect(() => {
    if (!props.open) return;
    // Focus the panel, not the first control: landing on a control means the
    // first keystroke edits a setting the user has not looked at yet.
    queueMicrotask(() => panelRef?.focus());

    // The compact shell installs its own, because Escape there means "up one
    // level" and only closes at the root. Two handlers would close the dialog
    // from the first keystroke at any depth.
    if (isCompact()) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      // Claim it: a settings dialog is the innermost thing on screen, and the
      // shell also listens for Escape.
      e.stopPropagation();
      props.onClose();
    };
    document.addEventListener('keydown', onKey, true);
    onCleanup(() => document.removeEventListener('keydown', onKey, true));
  });

  return (
    <Show when={props.open}>
      <Portal>
        <div
          class={`cru-anim-fade fixed inset-0 z-[60] flex items-center justify-center bg-shell-bg/70 backdrop-blur-[2px] ${
            isCompact() ? 'p-0' : 'p-6'
          }`}
          onClick={props.onClose}
          data-testid="settings-modal-backdrop"
        >
          <div
            ref={panelRef}
            tabindex={-1}
            role="dialog"
            aria-modal="true"
            aria-label="Settings"
            data-testid="settings-modal"
            // Stop a click INSIDE the dialog from reaching the backdrop's
            // dismiss — including a drag that starts on a slider and releases
            // outside, which `click` on the backdrop would otherwise catch.
            onClick={(e) => e.stopPropagation()}
            // A phone cannot hold the two-column form: the section list alone
            // is 216 px of a 412 px screen. It takes the whole screen and
            // navigates by drilling in instead.
            class={`cru-anim-pop overflow-hidden border-hairline-strong bg-shell-panel outline-none ${
              isCompact()
                ? 'flex h-dvh w-screen flex-col'
                : 'grid h-[min(38rem,85vh)] w-[min(56rem,94vw)] grid-cols-[13.5rem_1fr] rounded-2xl border shadow-2xl'
            }`}
          >
            <Show when={isCompact()}>
              <SettingsStackProvider stack={stack}>
                <MobileSettings
                  sections={sections()}
                  stack={stack}
                  onChanged={reload}
                  onClose={props.onClose}
                />
              </SettingsStackProvider>
            </Show>

            <Show when={!isCompact()}>
            {/* ── The section list ─────────────────────────────────────── */}
            <nav class="flex flex-col overflow-y-auto border-r border-hairline bg-surface-base py-3">
              <For each={settingsGroups(sections())}>
                {(group) => (
                  <>
                    <div class="px-4 pb-1 pt-3 text-floor font-semibold uppercase tracking-wider text-muted-dark first:pt-0">
                      {group.group}
                    </div>
                    <For each={group.sections}>
                      {(section) => (
                        <button
                          type="button"
                          onClick={() => setActiveId(section.id)}
                          data-testid={`settings-nav-${section.id}`}
                          aria-current={activeId() === section.id ? 'page' : undefined}
                          classList={{
                            'focus-ring mx-2 flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-reading transition-colors':
                              true,
                            // A fill, not a coloured edge bar: the selected row
                            // has to read at a glance without adding a second
                            // accent to a panel the ember already governs.
                            'bg-control text-shell-ink': activeId() === section.id,
                            'text-muted hover:bg-hover-wash hover:text-shell-body':
                              activeId() !== section.id,
                          }}
                        >
                          <Dynamic
                            component={section.icon}
                            class={`w-3.5 h-3.5 flex-none ${
                              activeId() === section.id ? 'text-primary' : 'text-muted-dark'
                            }`}
                          />
                          <span class="truncate">{section.label}</span>
                        </button>
                      )}
                    </For>
                  </>
                )}
              </For>
            </nav>

            {/* ── The section ──────────────────────────────────────────── */}
            <div class="flex min-w-0 flex-col">
              <header class="flex flex-none items-center justify-between border-b border-hairline px-5 py-3">
                <h2 class="text-sm font-semibold text-shell-ink">{active().label}</h2>
                <button
                  type="button"
                  onClick={props.onClose}
                  aria-label="Close settings"
                  data-testid="settings-modal-close"
                  class="focus-ring flex h-7 w-7 items-center justify-center rounded-full text-muted-dark transition-colors hover:bg-hover-wash hover:text-shell-ink"
                >
                  <X class="h-4 w-4" />
                </button>
              </header>
              <div class="min-h-0 flex-1 overflow-y-auto px-5 pb-6">
                {/* Sections are rows, so they need a table to live in. Keyed by
                    id so switching sections remounts rather than reconciling
                    two unrelated row sets into each other. */}
                <WithoutSectionHeaders>
                  <table class="w-full">
                    <tbody>
                      <Show when={active()} keyed>
                        {(section) => (
                          <Dynamic
                            component={section.render}
                            // Every section is offered both. Most ignore them:
                            // the plugin list reloads the trees after an
                            // install, and the app-config pane dismisses the
                            // dialog when it opens a pinned line in the editor
                            // behind it.
                            onChanged={reload}
                            onClose={props.onClose}
                          />
                        )}
                      </Show>
                    </tbody>
                  </table>
                </WithoutSectionHeaders>
              </div>
            </div>
            </Show>
          </div>
        </div>
      </Portal>
    </Show>
  );
};
