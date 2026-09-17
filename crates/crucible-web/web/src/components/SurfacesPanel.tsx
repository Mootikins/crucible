import { Component, For, Show, createSignal, onCleanup } from 'solid-js';
import type { Surface, SurfaceRow } from '@/lib/types';
import { surfaceEvents } from '@/lib/query/sse';
import { useSurfaces } from '@/lib/query/surfaces';
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
  // One entry for every panel on screen, and the stream's route keeps it
  // current. The panel used to hold its own resource and correct it by hand,
  // which gave a second panel a second fetch and a second answer.
  const surfaces = useSurfaces();
  const all = (): Surface[] => surfaces.data ?? [];

  // The chooser's pick, as the pair that identifies a surface. The name alone
  // is not one: two plugins may declare `sessions`, and a pick stored as a bare
  // name follows whichever of them the roster happens to list first.
  const [selected, setSelected] = createSignal<{ plugin: string; name: string } | null>(null);

  // The panel no longer answers the frame with a fetch or a patch: the cache
  // write belongs to the route, which runs inside the shared root
  // (`lib/query/routes/surfaces.ts`). What is left here is the one thing the
  // route cannot own, because it is this browser's display state — a pick that
  // named the surface that is gone. Dropping it matters because a plugin that
  // later re-declares the same name would otherwise silently take the panel
  // back from whatever the user had selected.
  //
  // The subscription is still required whatever the handler does: the root
  // counts its subscribers, and with none it closes the `EventSource` and no
  // panel hears anything. One source for the stream, whatever the count of
  // panels — `surfaceEvents()` is the shared root of `lib/query/sse.ts`, so a
  // second panel joins the stream the first one opened.
  const unsubscribe = surfaceEvents().subscribe((event) => {
    if (!event.withdrawn) return;
    const pick = selected();
    if (pick && pick.plugin === event.plugin && pick.name === event.name) setSelected(null);
  });
  onCleanup(unsubscribe);

  const shown = (): Surface | undefined => {
    const list = all();
    const pick = selected();
    return (
      list.find((s) => pick !== null && s.plugin === pick.plugin && s.name === pick.name) ?? list[0]
    );
  };

  return (
    <PanelShell>
      <PanelHeader title={shown()?.title ?? 'Surfaces'} />

      {/* More than one surface: a chooser. One: its title is already the header. */}
      <Show when={all().length > 1}>
        <div class="flex gap-1 px-2 py-1 border-b border-hairline overflow-x-auto">
          <For each={all()}>
            {(s) => (
              <button
                type="button"
                class={`px-2 py-0.5 text-xs rounded whitespace-nowrap ${
                  shown()?.plugin === s.plugin && shown()?.name === s.name
                    ? 'bg-surface-elevated text-shell-ink'
                    : 'text-muted hover:text-shell-ink'
                }`}
                onClick={() => setSelected({ plugin: s.plugin, name: s.name })}
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
