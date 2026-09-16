import { Component, For, Show, createResource, createSignal, onCleanup } from 'solid-js';
import { getSurfaces } from '@/lib/api';
import type { Surface, SurfaceRow } from '@/lib/api';
import { surfaceEvents } from '@/lib/query/sse';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';

/**
 * Panels plugins declared, drawn by the browser in its own idiom.
 *
 * The plugin states what is true — a row's status is a declared mark, never a
 * glyph — and this component decides what that looks like here. The TUI decides
 * separately, and the two are allowed to differ: that split is the whole reason
 * the mark is a stated vocabulary instead of a character the plugin picks.
 *
 * Nothing in this file knows what any plugin's rows mean. A row is
 * `{id, text, detail, mark}`, so a plugin shipped tomorrow gets a panel with no
 * change here.
 */

/** A dot, not a letter: the mark is a status, and colour carries it. */
const MARK_CLASS: Record<string, string> = {
  busy: 'bg-primary',
  blocked: 'bg-warn',
  ok: 'bg-ok',
  failed: 'bg-error',
};

/**
 * An unknown mark renders as empty space rather than a placeholder.
 *
 * A build that does not know a status should say nothing about it, not assert
 * that something is wrong.
 */
const MarkDot: Component<{ mark?: string | null }> = (props) => (
  <span
    class={`inline-block w-2 h-2 rounded-full shrink-0 ${
      props.mark && MARK_CLASS[props.mark] ? MARK_CLASS[props.mark] : 'bg-transparent'
    }`}
    aria-label={props.mark ?? undefined}
  />
);

const SurfaceRowItem: Component<{ row: SurfaceRow }> = (props) => (
  <li class="flex items-baseline gap-2 px-2 py-1 text-sm">
    <MarkDot mark={props.row.mark} />
    <span class="text-shell-ink truncate">{props.row.text}</span>
    <Show when={props.row.detail}>
      <span class="text-xs text-muted truncate">{props.row.detail}</span>
    </Show>
  </li>
);

export const SurfacesPanel: Component = () => {
  const [surfaces, { refetch, mutate }] = createResource(getSurfaces);
  const [selected, setSelected] = createSignal<string | null>(null);

  // A change says only that a surface moved, so refetch rather than patch: the
  // event carries a version and no rows, which is what keeps an unbounded
  // surface off an event channel.
  //
  // A withdrawal is the exception, and the daemon marks it rather than leaving
  // this layer to work it out. The surface is gone, so there is no content to
  // fetch and asking for it would spend a round trip to be told what the event
  // already said. Drop it here, and drop a selection that pointed at it —
  // otherwise a plugin that later re-declares the same name silently steals the
  // panel back. A re-declare announces, so the refetch below restores it.
  //
  // One source for the stream, whatever the count of panels: `surfaceEvents()`
  // is the shared root of `lib/query/sse.ts`, so a second panel joins the
  // stream the first one opened rather than starting another.
  const unsubscribe = surfaceEvents().subscribe((event) => {
    if (event.withdrawn) {
      mutate((prev) => (prev ?? []).filter((s) => s.name !== event.name));
      if (selected() === event.name) setSelected(null);
      return;
    }
    void refetch();
  });
  onCleanup(unsubscribe);

  const shown = (): Surface | undefined => {
    const all = surfaces() ?? [];
    const pick = selected();
    return all.find((s) => s.name === pick) ?? all[0];
  };

  return (
    <PanelShell>
      <PanelHeader title={shown()?.title ?? 'Surfaces'} />

      {/* More than one surface: a chooser. One: its title is already the header. */}
      <Show when={(surfaces() ?? []).length > 1}>
        <div class="flex gap-1 px-2 py-1 border-b border-hairline overflow-x-auto">
          <For each={surfaces()}>
            {(s) => (
              <button
                type="button"
                class={`px-2 py-0.5 text-xs rounded whitespace-nowrap ${
                  shown()?.name === s.name
                    ? 'bg-surface-elevated text-shell-ink'
                    : 'text-muted hover:text-shell-ink'
                }`}
                onClick={() => setSelected(s.name)}
              >
                {s.title}
              </button>
            )}
          </For>
        </div>
      </Show>

      <div class="flex-1 overflow-y-auto">
        <Show
          when={shown()}
          fallback={
            <div class="flex flex-col items-center justify-center h-full px-4 text-center">
              <p class="text-sm text-muted-dark">No plugin surfaces</p>
              <p class="text-xs text-muted-dark mt-1">
                A plugin declares one with <code>cru.surface.declare</code>
              </p>
            </div>
          }
        >
          {(surface) => (
            <Show
              when={surface().rows.length > 0}
              fallback={<p class="p-3 text-sm text-muted-dark">Nothing here yet</p>}
            >
              <ul class="py-1">
                <For each={surface().rows}>{(row) => <SurfaceRowItem row={row} />}</For>
              </ul>
            </Show>
          )}
        </Show>
      </div>
    </PanelShell>
  );
};
