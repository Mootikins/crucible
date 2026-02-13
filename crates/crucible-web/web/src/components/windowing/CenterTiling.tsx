import { Component, Show } from 'solid-js';
import { produce, unwrap } from 'solid-js/store';
import { SplitPane } from './SplitPane';
import { setStore, updateSplitRatio, windowStore } from '@/stores/windowStore';
import type { LayoutNode } from '@/types/windowTypes';

/**
 * Center tiling region: user-configurable binary tree of splits and panes
 * (Dockview-style). Resize via splitter dividers; tabs can be dragged between
 * panes and dropped on edges to create new splits.
 */
export const CenterTiling: Component = () => {
  const layout = () => windowStore.layout;
  const setRatio = (ratio: number) => {
    const current = unwrap(windowStore.layout) as LayoutNode;
    const newLayout = updateSplitRatio(current, 'split-root', ratio);
    setStore(produce((s) => { s.layout = newLayout; }));
  };
  return (
    <div class="flex-1 overflow-hidden min-h-0 relative">
      <Show when={import.meta.env.DEV}>
        <div class="absolute top-1 right-1 z-20 flex gap-1">
          <button
            type="button"
            class="px-2 py-0.5 text-xs bg-zinc-700 hover:bg-zinc-600 rounded"
            onClick={() => setRatio(0.25)}
          >
            Set ratio 0.25
          </button>
          <button
            type="button"
            class="px-2 py-0.5 text-xs bg-zinc-700 hover:bg-zinc-600 rounded"
            onClick={() => setRatio(0.5)}
          >
            Set ratio 0.5
          </button>
        </div>
      </Show>
      <SplitPane node={layout()} />
    </div>
  );
};
