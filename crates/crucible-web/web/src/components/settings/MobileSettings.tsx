import { Component, For, Show, createEffect, onCleanup } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { ChevronLeft, ChevronRight, X } from '@/lib/icons';
import { settingsGroups, type SettingsSection, type SettingsSectionProps } from './sections';
import { WithoutSectionHeaders } from './primitives';
import type { SettingsPage, SettingsStack } from './settings-nav';

/**
 * One row that opens something else. The phone's whole navigation vocabulary.
 *
 * `h-14` is the touch target, and the chevron is what says the row leads
 * somewhere rather than toggling in place.
 */
export const SettingsNavRow: Component<{
  label: string;
  detail?: string;
  icon?: Component<{ class?: string }>;
  testId?: string;
  onSelect: () => void;
}> = (props) => (
  <button
    type="button"
    data-testid={props.testId}
    onClick={() => props.onSelect()}
    class="focus-ring flex h-14 w-full items-center gap-3 px-3 text-left transition-colors hover:bg-hover-wash"
  >
    <Show when={props.icon}>
      <Dynamic component={props.icon!} class="h-4 w-4 flex-none text-muted-dark" />
    </Show>
    <span class="min-w-0 flex-1">
      <span class="block truncate text-reading text-shell-ink">{props.label}</span>
      <Show when={props.detail}>
        <span class="block truncate text-floor text-muted-dark">{props.detail}</span>
      </Show>
    </span>
    <ChevronRight class="h-4 w-4 flex-none text-muted-dark" />
  </button>
);

/** A group of rows as one card, the way both phone platforms draw a list. */
export const SettingsNavGroup: Component<{ label?: string; children: Element | unknown }> = (
  props,
) => (
  <section class="mb-5">
    <Show when={props.label}>
      <h3 class="px-3 pb-1.5 text-floor font-semibold uppercase tracking-wider text-muted-dark">
        {props.label}
      </h3>
    </Show>
    {/* `divide-y` rather than a border per row: the last row must not draw a
        rule against the card's own edge. */}
    <div class="divide-y divide-hairline overflow-hidden rounded-lg border border-hairline bg-surface-elevated">
      {props.children as Element}
    </div>
  </section>
);

/** The page a section becomes when it is opened from the root list. */
export function sectionPage(
  section: SettingsSection,
  props: SettingsSectionProps,
): SettingsPage {
  return {
    id: section.id,
    title: section.label,
    rows: true,
    body: () => <Dynamic component={section.render} onChanged={props.onChanged} onClose={props.onClose} />,
  };
}

/**
 * Settings on a phone: one list, drilled into.
 *
 * The root names every category. A tap opens one, the bar's back control (and
 * the phone's own back button) returns, and a section deep enough to have
 * sub-categories pushes again — the daemon's configuration tree does exactly
 * that. See `settings-nav.tsx` for why this shape and not the desktop's.
 */
export const MobileSettings: Component<{
  sections: SettingsSection[];
  stack: SettingsStack;
  onChanged: () => void | Promise<unknown>;
  onClose: () => void;
}> = (props) => {
  const top = () => {
    const pages = props.stack.pages();
    return pages[pages.length - 1] ?? null;
  };

  // This shell owns its own Escape, because Escape here means "up one level"
  // and only means "close" at the root. The dialog does not install its own
  // handler while a phone is showing.
  createEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.stopPropagation();
      if (!props.stack.pop()) props.onClose();
    };
    document.addEventListener('keydown', onKey, true);
    onCleanup(() => document.removeEventListener('keydown', onKey, true));
  });

  const close = () => {
    // Give back every history entry the drill-down took, or the phone's back
    // button would walk levels of a dialog that is no longer on screen.
    props.stack.reset();
    props.onClose();
  };

  return (
    <>
      <header
        class="flex h-14 flex-none items-center gap-1 border-b border-hairline bg-surface-elevated px-2"
        style={{ 'padding-top': 'var(--inset-top)', 'box-sizing': 'content-box' }}
      >
        <Show
          when={top()}
          fallback={<div class="w-1" />}
        >
          <button
            type="button"
            onClick={() => props.stack.pop()}
            aria-label="Back"
            data-testid="settings-back"
            class="focus-ring flex h-11 w-11 flex-none items-center justify-center rounded text-muted-dark transition-colors hover:bg-hover-wash hover:text-shell-ink"
          >
            <ChevronLeft class="h-5 w-5" />
          </button>
        </Show>
        <h2 class="min-w-0 flex-1 truncate px-1 text-reading font-semibold text-shell-ink">
          {top()?.title ?? 'Settings'}
        </h2>
        <button
          type="button"
          onClick={close}
          aria-label="Close settings"
          data-testid="settings-modal-close"
          class="focus-ring flex h-11 w-11 flex-none items-center justify-center rounded text-muted-dark transition-colors hover:bg-hover-wash hover:text-shell-ink"
        >
          <X class="h-5 w-5" />
        </button>
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto px-3 py-4">
        {/* Every level stays MOUNTED and all but the top are hidden.
            Rendering only the top unmounted the section that owns the data:
            drilling into a config group disposed `AppConfigSettingsSection`,
            so its inline save error had no reader, and every press of Back
            remounted it and refetched the whole tree with a loading flash. */}
        <div class={top() ? 'hidden' : ''}>
          <RootList
            sections={props.sections}
            stack={props.stack}
            onChanged={props.onChanged}
            onCloseAll={close}
          />
        </div>
        <For each={props.stack.pages()}>
          {(page, index) => (
            <div class={index() === props.stack.pages().length - 1 ? '' : 'hidden'}>
              <Show
                when={page.rows}
                fallback={<div class="flex flex-col gap-1">{page.body()}</div>}
              >
                {/* Sections are written as table rows; they need a table. */}
                <WithoutSectionHeaders>
                  <table class="w-full">
                    <tbody>{page.body()}</tbody>
                  </table>
                </WithoutSectionHeaders>
              </Show>
            </div>
          )}
        </For>
      </div>
    </>
  );
};

/** Every category, grouped, each opening its own page. */
const RootList: Component<{
  sections: SettingsSection[];
  stack: SettingsStack;
  onChanged: () => void | Promise<unknown>;
  /** Dismiss the whole dialog AND give its history entries back. */
  onCloseAll: () => void;
}> = (props) => (
  <For each={settingsGroups(props.sections)}>
    {(group) => (
      <SettingsNavGroup label={group.group}>
        <For each={group.sections}>
          {(section) => (
            <SettingsNavRow
              label={section.label}
              icon={section.icon}
              testId={`settings-nav-${section.id}`}
              onSelect={() =>
                props.stack.push(
                  sectionPage(section, { onChanged: props.onChanged, onClose: props.onCloseAll }),
                )
              }
            />
          )}
        </For>
      </SettingsNavGroup>
    )}
  </For>
);
