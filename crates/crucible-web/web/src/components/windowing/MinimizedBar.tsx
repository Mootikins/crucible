import { Component, For, Show } from 'solid-js';
import { windowStore, windowActions } from '@/stores/windowStore';
import { IconLayout } from './icons';

export const MinimizedBar: Component = () => {
  const minimized = () =>
    windowStore.floatingWindows.filter((w) => w.isMinimized);

  return (
    <Show when={minimized().length > 0}>
      <div class="fixed bottom-6 left-1/2 -translate-x-1/2 flex items-center gap-1 p-1 bg-zinc-800/95 rounded-lg border border-zinc-700 shadow-xl backdrop-blur-sm z-50">
        <For each={minimized()}>
          {(w) => (
            <button
              type="button"
              class="flex items-center gap-1.5 px-2 py-1 rounded bg-zinc-700/50 hover:bg-zinc-700 text-zinc-300 text-xs transition-colors"
              onClick={() => windowActions.restoreFloatingWindow(w.id)}
            >
              <IconLayout class="w-3 h-3" />
              <span class="max-w-[100px] truncate">{w.title ?? 'Window'}</span>
            </button>
          )}
        </For>
      </div>
    </Show>
  );
};
