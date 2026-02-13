import { Component, Show, onMount, onCleanup } from 'solid-js';
import { windowStore, windowActions } from '@/stores/windowStore';
import { IconClose } from './icons';

export const FlyoutPanel: Component = () => {
  const flyout = () => windowStore.flyoutState;
  const isOpen = () => flyout()?.isOpen ?? false;
  const panel = () =>
    flyout() ? windowStore.edgePanels[flyout()!.panelPosition] : null;
  const activeTab = () => {
    const f = flyout();
    const p = panel();
    if (!f || !p) return null;
    return p.tabs.find((t) => t.id === f.tabId) ?? null;
  };

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') windowActions.closeFlyout();
    };
    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  return (
    <Show when={isOpen()}>
      <div class="fixed inset-0 z-40 flex items-center justify-center bg-black/30">
        <div
          class="bg-zinc-900 border border-zinc-700 rounded-lg shadow-xl w-[400px] max-h-[80vh] flex flex-col"
          onClick={(e) => e.stopPropagation()}
        >
          <div class="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
            <span class="text-sm font-medium text-zinc-200">
              {activeTab()?.title ?? 'Panel'}
            </span>
            <button
              type="button"
              class="p-1 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800"
              onClick={() => windowActions.closeFlyout()}
            >
              <IconClose class="w-4 h-4" />
            </button>
          </div>
          <div class="flex-1 overflow-auto p-3 text-xs text-zinc-400">
            {activeTab()?.title} content in flyout
          </div>
        </div>
      </div>
    </Show>
  );
};
