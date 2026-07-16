import { Component, Show, createSignal, createEffect, onCleanup } from 'solid-js';
import { produce, unwrap } from 'solid-js/store';
import { Pane } from './Pane';
import { setStore, updateSplitRatio, windowStore } from '@/stores/windowStore';
import type { LayoutNode } from '@/types/windowTypes';

const SplitPaneInner: Component<{ node: Extract<LayoutNode, { type: 'split' }> }> = (props) => {
  const split = () => props.node;
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
        <SplitPane node={split().first} />
      </div>
      {/* 1px visible line; the after: pseudo extends the pointer target ±4px
          so the thin separator is still comfortable to grab. */}
      <div
        data-testid="resize-splitter"
        data-split-id={split().id}
        classList={{
          'relative flex-shrink-0 z-10 pointer-events-auto transition-colors after:content-[\'\'] after:absolute': true,
          'w-px cursor-col-resize after:inset-y-0 after:-inset-x-1': split().direction === 'horizontal',
          'h-px cursor-row-resize after:inset-x-0 after:-inset-y-1': split().direction !== 'horizontal',
          'bg-primary': isDragging(),
          'bg-zinc-800 hover:bg-zinc-600': !isDragging(),
        }}
        on:pointerdown={handlePointerDown}
      />
      <div
        class="relative z-0 overflow-hidden min-w-0 min-h-0"
        style={{ flex: `${1 - effectiveRatio()} 1 0` }}
      >
        <SplitPane node={split().second} />
      </div>
    </div>
  );
};

export const SplitPane: Component<{ node: LayoutNode }> = (props) => {
  return (
    <Show
      when={props.node.type === 'split' ? props.node : undefined}
      fallback={
        // Keyed: a layout restore (server /api/layout) replaces pane ids under
        // a surviving component instance. Pane registers its solid-dnd
        // droppables with the id captured at mount — without a remount every
        // pane drop carries the stale boot-time id and silently no-ops.
        <Show when={props.node.id} keyed>
          {(paneId) => <Pane paneId={paneId} />}
        </Show>
      }
    >
      {(split) => <SplitPaneInner node={split()} />}
    </Show>
  );
};
