import { Component, Show, createMemo } from 'solid-js';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { EdgePanelPosition } from '@/types/windowTypes';
import { IconClose } from './icons';

const FLYOUT_WIDTH = 280;
const FLYOUT_HEIGHT = 280;
const COLLAPSED_STRIP_W = 40;
const COLLAPSED_STRIP_H = 36;

function flyoutPositionStyle(position: EdgePanelPosition): Record<string, string> {
  switch (position) {
    case 'left':
      return { left: `${COLLAPSED_STRIP_W}px`, top: '0', width: `${FLYOUT_WIDTH}px`, height: '100%' };
    case 'right':
      return { right: `${COLLAPSED_STRIP_W}px`, top: '0', width: `${FLYOUT_WIDTH}px`, height: '100%' };
    case 'bottom':
      return { left: `${COLLAPSED_STRIP_W}px`, bottom: `${COLLAPSED_STRIP_H}px`, right: `${COLLAPSED_STRIP_W}px`, height: `${FLYOUT_HEIGHT}px` };
  }
}

export const FlyoutPanel: Component = () => {
  const flyout = () => windowStore.flyoutState;
  const isOpen = () => flyout()?.isOpen ?? false;
  const panelPosition = () => flyout()?.panelPosition ?? 'left';

  const panel = () =>
    flyout() ? windowStore.edgePanels[flyout()!.panelPosition] : null;

  const activeTab = () => {
    const f = flyout();
    const p = panel();
    if (!f || !p) return null;
    return windowStore.tabGroups[p.tabGroupId]?.tabs.find((t) => t.id === f.tabId) ?? null;
  };

  const posStyle = createMemo(() => flyoutPositionStyle(panelPosition()));

  return (
    <Show when={isOpen()}>
      <div
        class="absolute inset-0 z-40"
        onClick={() => windowActions.closeFlyout()}
      >
        <div
          data-testid="flyout-panel"
          class="absolute bg-zinc-900 border border-zinc-700 rounded-lg shadow-xl flex flex-col z-50"
          style={posStyle()}
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
