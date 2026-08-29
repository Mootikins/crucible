import { Component, Show, createSignal, createEffect, onCleanup } from 'solid-js';
import { Pane } from './Pane';
import { windowActions } from '@/stores/windowStore';
import type { LayoutNode } from '@/types/windowTypes';
import { isCollapsedLeaf, paneFlex } from '@/lib/pane-collapse';

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

  const firstCollapsed = () => isCollapsedLeaf(split().first);
  const secondCollapsed = () => isCollapsedLeaf(split().second);
  // `splitRatio` is NOT touched while a side is collapsed: it is what the pane
  // opens back to. The splitter is therefore inert instead — a drag that moved
  // an invisible ratio would silently rewrite the restore size.
  const locked = () => firstCollapsed() || secondCollapsed();

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
      // Root-aware commit: this split may live in the center tiling or
      // inside an edge panel's tree.
      windowActions.commitSplitRatio(splitId, localRatio());
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
        style={{ flex: paneFlex(split().first, effectiveRatio()) }}
      >
        <SplitPane node={split().first} />
      </div>
      {/* 1px visible line; the after: pseudo extends the pointer target ±4px
          so the thin separator is still comfortable to grab. Locked against a
          collapsed side it stays as the separator and drops both the grab
          target and the resize cursor. */}
      <div
        data-testid="resize-splitter"
        data-split-id={split().id}
        data-locked={locked() ? 'true' : undefined}
        classList={{
          'relative flex-shrink-0 z-10 pointer-events-auto transition-colors': true,
          'after:content-[\'\'] after:absolute': !locked(),
          'w-px': split().direction === 'horizontal',
          'h-px': split().direction !== 'horizontal',
          'cursor-col-resize after:inset-y-0 after:-inset-x-1':
            split().direction === 'horizontal' && !locked(),
          'cursor-row-resize after:inset-x-0 after:-inset-y-1':
            split().direction !== 'horizontal' && !locked(),
          'bg-primary': isDragging(),
          'bg-control': locked() && !isDragging(),
          'bg-control hover:bg-hover-wash': !locked() && !isDragging(),
        }}
        on:pointerdown={(e) => {
          if (locked()) return;
          handlePointerDown(e);
        }}
      />
      <div
        class="relative z-0 overflow-hidden min-w-0 min-h-0"
        style={{ flex: paneFlex(split().second, 1 - effectiveRatio()) }}
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
