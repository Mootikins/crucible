import { Component, createSignal, createEffect, onCleanup } from 'solid-js';
import { produce, unwrap } from 'solid-js/store';
import { Pane } from './Pane';
import { setStore, updateSplitRatio, windowStore } from '@/stores/windowStore';
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

  const handlePointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    setIsDragging(true);
    const startX = e.clientX;
    const startY = e.clientY;
    const startRatio = localRatio();
    const dir = split().direction;
    const splitId = split().id;

    const handlePointerMove = (e: PointerEvent) => {
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

    const handlePointerUp = (e: PointerEvent) => {
      const ratio = localRatio();
      const currentLayout = unwrap(windowStore.layout) as LayoutNode;
      const newLayout = updateSplitRatio(currentLayout, splitId, ratio);
      setStore(produce((s) => { s.layout = newLayout; }));
      setIsDragging(false);
      el.releasePointerCapture(e.pointerId);
      document.removeEventListener('pointermove', handlePointerMove);
      document.removeEventListener('pointerup', handlePointerUp);
      cleanupRef = null;
    };

    document.addEventListener('pointermove', handlePointerMove);
    document.addEventListener('pointerup', handlePointerUp);
    cleanupRef = () => {
      document.removeEventListener('pointermove', handlePointerMove);
      document.removeEventListener('pointerup', handlePointerUp);
    };
  };

  return (
    <div
      ref={(el) => (containerRef = el)}
      classList={{
        'relative flex h-full w-full': true,
        'flex-row': split().direction === 'horizontal',
        'flex-col': split().direction !== 'horizontal',
      }}
    >
      <div
        class="relative z-0 overflow-hidden min-w-0 min-h-0"
        style={{ flex: `${effectiveRatio()} 1 0` }}
      >
        {split().first.type === 'pane' ? (
          <Pane paneId={split().first.id} />
        ) : (
          <SplitPane node={split().first} />
        )}
      </div>
      <div
        data-testid="resize-splitter"
        data-split-id={split().id}
        classList={{
          'group relative flex-shrink-0 z-10 pointer-events-auto transition-colors': true,
          'w-2 cursor-col-resize': split().direction === 'horizontal',
          'h-2 cursor-row-resize': split().direction !== 'horizontal',
          'bg-blue-500': isDragging(),
          'bg-zinc-800 hover:bg-zinc-700 active:bg-blue-500': !isDragging(),
        }}
        on:pointerdown={handlePointerDown}
      >
        <div
          classList={{
            'absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity text-zinc-500': true,
          }}
        >
          {split().direction === 'horizontal' ? (
            <IconGripVertical class="w-1 h-4" />
          ) : (
            <IconGripHorizontal class="w-4 h-1" />
          )}
        </div>
      </div>
      <div
        class="relative z-0 overflow-hidden min-w-0 min-h-0"
        style={{ flex: `${1 - effectiveRatio()} 1 0` }}
      >
        {split().second.type === 'pane' ? (
          <Pane paneId={split().second.id} />
        ) : (
          <SplitPane node={split().second} />
        )}
      </div>
    </div>
  );
};
