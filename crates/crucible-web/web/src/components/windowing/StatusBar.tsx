import { Component } from 'solid-js';
import { createDraggable, createDroppable } from '@thisbeyond/solid-dnd';
import { windowStore } from '@/stores/windowStore';
import { IconLayout } from './icons';

export const StatusBar: Component = () => {
  const totalTabs = () =>
    Object.values(windowStore.tabGroups).reduce(
      (sum, group) => sum + group.tabs.length,
      0
    );
  const minimizedCount = () =>
    windowStore.floatingWindows.filter((w) => w.isMinimized).length;

  const newFloatingId = 'newFloating';
  const dropNewFloatingId = 'dropNewFloating';
  // SolidJS directives: variables referenced via use:draggable / use:droppable in JSX
  const draggable = createDraggable(newFloatingId, { type: 'newFloating' });
  void draggable;
  const droppable = createDroppable(dropNewFloatingId, { type: 'newFloating' });
  void droppable;

  return (
    <div class="flex items-center justify-between px-2 h-5 bg-zinc-950 border-t border-zinc-800 text-[10px] text-zinc-500 select-none">
      <div class="flex items-center gap-3">
        <span>Ready</span>
        <span>{totalTabs()} tabs</span>
        {minimizedCount() > 0 && (
          <span class="text-amber-500">{minimizedCount()} minimized</span>
        )}
      </div>
      <div class="flex items-center gap-3">
        <div
          use:droppable
          class="flex items-center gap-2 px-2 py-1 rounded"
        >
          <div
            use:draggable
            class="flex items-center gap-2 px-2 py-1 text-xs text-zinc-500 hover:text-zinc-300 cursor-grab active:cursor-grabbing transition-colors"
          >
            <IconLayout class="w-3.5 h-3.5" />
            <span>New Window</span>
          </div>
        </div>
        <div class="w-px h-3 bg-zinc-800" />
        <span>UTF-8</span>
        <span>TypeScript</span>
      </div>
    </div>
  );
};
