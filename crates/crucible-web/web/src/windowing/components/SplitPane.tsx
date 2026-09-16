import { Component, Show, createEffect, createMemo, createSignal, onCleanup } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import { Pane } from './Pane';
import type { LayoutNode } from '@/windowing/model/types';
import { isCollapsedLeaf, splitFlex } from '@/windowing/model/pane-collapse';
import { subtreeHasTabs } from '@/windowing/model/pane-content';
import { findSplitInLayout } from '@/windowing/model/pane-boundaries';
import { windowStore } from '@/windowing/store';
import { startSplitDrag } from '@/windowing/components/split-drag';

/**
 * Stands in for the splitter inside the keyed row. Its `id` never changes, so
 * `Key` never moves it and the two halves reorder around it.
 */
const SPLITTER = { id: '\u0000splitter' } as const;

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

  // A side with no tabs yields its share to the side that has them.
  //
  // A 50% split against a void is not a layout, it is a hole: at 1280px the
  // centre gave an empty pane 460px it could not use while the pane beside it
  // squeezed its content into 459px. Only ONE side may yield, and only to a
  // side that actually holds content — two empty panes keep their ratio,
  // because neither has a better claim on the space than the other.
  //
  // Centre tiling only. A rail's panes are a fixed stack that the user does
  // not fill with new tabs, so an empty slot there is not the same state.
  // (The ribbon follows either way — it MEASURES each pane's box rather than
  // recomputing it from `splitRatio`.)
  const inCenter = createMemo(() => findSplitInLayout(windowStore.layout, split().id) !== null);
  const firstHasTabs = () => subtreeHasTabs(windowStore.tabGroups, split().first);
  const secondHasTabs = () => subtreeHasTabs(windowStore.tabGroups, split().second);
  const firstYields = () => inCenter() && !firstHasTabs() && secondHasTabs();
  const secondYields = () => inCenter() && !secondHasTabs() && firstHasTabs();

  // Both halves at once: the growing half's flex factor depends on whether the
  // other half is pinned to a fixed basis.
  const flex = () =>
    splitFlex(split().first, split().second, effectiveRatio(), {
      first: firstYields(),
      second: secondYields(),
    });

  // `splitRatio` is NOT touched while a side is collapsed or yields: it is what
  // the pane opens back to. The splitter is therefore inert instead — a drag
  // that moved an invisible ratio would silently rewrite the restore size.
  const locked = () =>
    firstCollapsed() || secondCollapsed() || firstYields() || secondYields();

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
    setIsDragging(true);
    cleanupRef = startSplitDrag({
      event: e,
      splitId: split().id,
      direction: split().direction === 'horizontal' ? 'horizontal' : 'vertical',
      startRatio: localRatio(),
      getContainerRect: () => containerRef?.getBoundingClientRect() ?? null,
      onPreview: setLocalRatio,
      onEnd: () => {
        setIsDragging(false);
        cleanupRef = null;
      },
    });
  };

  // The row is a KEYED LIST, not fixed JSX slots, so a mirror MOVES the two
  // halves instead of rebuilding them. `swapSidePanels` reverses every
  // horizontal split; with the halves in fixed slots each one received the
  // other's node, and Solid answered the only way it can for a slot whose
  // content changed — tear down, build again. Every panel in the flipped
  // subtree lost its state and loaded its content again from empty.
  //
  // Keyed by node id, the same reversal is a REORDER: `Key` moves the
  // existing DOM node, and the component instance inside it survives.
  //
  // The splitter rides in the list on a constant key, so it stays BETWEEN the
  // halves in the DOM and the halves move around it. Keeping it in the
  // markup instead would have forced DOM order away from visual order.
  const rowItems = (): Array<LayoutNode | typeof SPLITTER> => [
    split().first,
    SPLITTER,
    split().second,
  ];

  return (
    <div
      ref={(el) => (containerRef = el)}
      classList={{
        'relative flex h-full w-full': true,
        'flex-row': split().direction === 'horizontal',
        'flex-col': split().direction !== 'horizontal',
      }}
    >
      <Key each={rowItems()} by="id">
        {(item, index) => (
          <Show
            when={item() === SPLITTER ? undefined : (item() as LayoutNode)}
            fallback={
              /* 1px visible line; the after: pseudo extends the pointer target
                 ±4px so the thin separator is still comfortable to grab.
                 Locked against a collapsed side it stays as the separator and
                 drops both the grab target and the resize cursor. */
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
            }
          >
            {(node) => (
              <div
                class="relative z-0 overflow-hidden min-w-0 min-h-0"
                style={{ flex: index() === 0 ? flex().first : flex().second }}
              >
                <SplitPane node={node()} />
              </div>
            )}
          </Show>
        )}
      </Key>
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
