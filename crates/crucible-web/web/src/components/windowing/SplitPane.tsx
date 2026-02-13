import { Component, createSignal, createEffect, onCleanup } from 'solid-js';
import { Pane } from './Pane';
import { windowStore, setStore, updateSplitRatio } from '@/stores/windowStore';
import type { LayoutNode } from '@/types/windowTypes';
import { IconGripVertical, IconGripHorizontal } from './icons';

export const SplitPane: Component<{ node: LayoutNode }> = (props) => {
  const node = () => props.node;
  if (node().type === 'pane') {
    return <Pane paneId={node().id} />;
  }

  const split = () => node() as Extract<LayoutNode, { type: 'split' }>;
  const [localRatio, setLocalRatio] = createSignal(split().splitRatio);
  const [isDragging, setIsDragging] = createSignal(false);
  let containerRef: HTMLDivElement;
  let cleanupRef: (() => void) | null = null;

  createEffect(() => {
    if (!isDragging()) {
      setLocalRatio(split().splitRatio);
    }
  });

  const effectiveRatio = () => (isDragging() ? localRatio() : split().splitRatio);

  onCleanup(() => {
    if (cleanupRef) {
      cleanupRef();
      cleanupRef = null;
    }
    setIsDragging(false);
  });

  const handleMouseDown = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(true);
    const startX = e.clientX;
    const startY = e.clientY;
    const startRatio = localRatio();
    const dir = split().direction;
    const splitId = split().id;

    const handleMouseMove = (e: MouseEvent) => {
      const rect = containerRef?.getBoundingClientRect();
      if (!rect) return;
      let newRatio: number;
      if (dir === 'horizontal') {
        newRatio = startRatio + (e.clientX - startX) / rect.width;
      } else {
        newRatio = startRatio + (e.clientY - startY) / rect.height;
      }
      setLocalRatio(Math.max(0.1, Math.min(0.9, newRatio)));
    };

    const handleMouseUp = () => {
      const ratio = localRatio();
      setIsDragging(false);
      const currentLayout = windowStore.layout;
      const newLayout = updateSplitRatio(currentLayout, splitId, ratio);
      setStore('layout', newLayout);
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      cleanupRef = null;
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    cleanupRef = () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  };

  const s = split();
  const dir = s.direction;
  const isHoriz = dir === 'horizontal';
  const ratio = effectiveRatio();

  return (
    <div
      ref={(el) => (containerRef = el)}
      classList={{
        'flex h-full w-full': true,
        'flex-row': isHoriz,
        'flex-col': !isHoriz,
      }}
    >
      <div
        class="overflow-hidden min-w-0 min-h-0"
        style={{ flex: `${ratio} 1 0` }}
      >
        {s.first.type === 'pane' ? (
          <Pane paneId={s.first.id} />
        ) : (
          <SplitPane node={s.first} />
        )}
      </div>
      <div
        classList={{
          'group relative flex-shrink-0 z-10 pointer-events-auto transition-colors': true,
          'w-1 cursor-col-resize': isHoriz,
          'h-1 cursor-row-resize': !isHoriz,
          'bg-blue-500': isDragging(),
          'bg-zinc-800 hover:bg-zinc-700 active:bg-blue-500': !isDragging(),
        }}
        onMouseDown={handleMouseDown}
      >
        <div
          classList={{
            'absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity text-zinc-500': true,
          }}
        >
          {isHoriz ? (
            <IconGripVertical class="w-3 h-6" />
          ) : (
            <IconGripHorizontal class="w-6 h-3" />
          )}
        </div>
      </div>
      <div
        class="overflow-hidden min-w-0 min-h-0"
        style={{ flex: `${1 - ratio} 1 0` }}
      >
        {s.second.type === 'pane' ? (
          <Pane paneId={s.second.id} />
        ) : (
          <SplitPane node={s.second} />
        )}
      </div>
    </div>
  );
};
