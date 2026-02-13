import { Component, createSignal, createEffect, onCleanup } from 'solid-js';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { FloatingWindow as FloatingWindowType } from '@/types/windowTypes';
import { TabBar } from './TabBar';
import { IconClose, IconMinimize } from './icons';

export const FloatingWindow: Component<{ window: FloatingWindowType }> = (props) => {
  const w = () => props.window;
  const group = () => windowStore.tabGroups[w().tabGroupId];
  const tabs = () => group()?.tabs ?? [];
  const [isDragging, setIsDragging] = createSignal(false);
  const [dragStart, setDragStart] = createSignal({ x: 0, y: 0, windowX: 0, windowY: 0 });

  const handleTitleMouseDown = (e: MouseEvent) => {
    if (w().isMaximized) return;
    e.preventDefault();
    setIsDragging(true);
    setDragStart({
      x: e.clientX,
      y: e.clientY,
      windowX: w().x,
      windowY: w().y,
    });
    windowActions.bringToFront(w().id);
  };

  createEffect(() => {
    if (!isDragging()) return;
    const move = (e: MouseEvent) => {
      const start = dragStart();
      windowActions.updateFloatingWindow(w().id, {
        x: Math.max(0, start.windowX + (e.clientX - start.x)),
        y: Math.max(0, start.windowY + (e.clientY - start.y)),
      });
    };
    const up = () => setIsDragging(false);
    document.addEventListener('mousemove', move);
    document.addEventListener('mouseup', up);
    onCleanup(() => {
      document.removeEventListener('mousemove', move);
      document.removeEventListener('mouseup', up);
    });
  });

  return (
    <div
      class="absolute flex flex-col bg-zinc-900 border border-zinc-700 rounded-lg shadow-xl overflow-hidden"
      style={{
        left: `${w().x}px`,
        top: `${w().y}px`,
        width: `${w().width}px`,
        height: `${w().height}px`,
        'z-index': `${w().zIndex}`,
      }}
      onMouseDown={() => windowActions.bringToFront(w().id)}
    >
      <div
        class="flex items-center justify-between px-2 py-1 bg-zinc-800 border-b border-zinc-700 cursor-grab active:cursor-grabbing select-none"
        onMouseDown={handleTitleMouseDown}
      >
        <span class="text-xs font-medium text-zinc-300 truncate">
          {w().title ?? (tabs()[0]?.title ?? 'Window')}
        </span>
        <div class="flex items-center gap-0.5">
          <button
            type="button"
            class="p-1 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-700"
            onClick={() => windowActions.minimizeFloatingWindow(w().id)}
            title="Minimize"
          >
            <IconMinimize class="w-3 h-3" />
          </button>
          <button
            type="button"
            class="p-1 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-700"
            onClick={() => windowActions.removeFloatingWindow(w().id)}
            title="Close"
          >
            <IconClose class="w-3 h-3" />
          </button>
        </div>
      </div>
      <div class="flex-1 flex flex-col min-h-0">
        <TabBar groupId={w().tabGroupId} paneId="" />
        <div class="flex-1 bg-zinc-900 overflow-auto p-2 text-xs text-zinc-400">
          {tabs().length > 0 ? (
            <span>Content for {tabs()[0]?.title}</span>
          ) : (
            <span>No tabs</span>
          )}
        </div>
      </div>
    </div>
  );
};
