import { Component, For, Show } from 'solid-js';
import { windowStore, windowActions } from '@/windowing/store';
import { IconLayout } from './icons';

export const MinimizedBar: Component = () => {
  const minimized = () =>
    windowStore.floatingWindows.filter((w) => w.isMinimized);

  return (
    <Show when={minimized().length > 0}>
      <div class="wm-minimized-bar fixed bottom-6 left-1/2 -translate-x-1/2 flex items-center z-50">
        <For each={minimized()}>
          {(w) => (
            <button
              type="button"
              class="wm-minimized-btn flex items-center"
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
