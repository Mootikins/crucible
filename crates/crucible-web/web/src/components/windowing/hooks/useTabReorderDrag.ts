import { createEffect, createSignal } from 'solid-js';
import { useDragDropContext } from '@thisbeyond/solid-dnd';
import type { DragSource } from '@/types/windowTypes';
import type { ReorderState } from '../TabBar';

export interface TabReorderDragOptions {
  groupId: () => string;
  containerRef: () => HTMLDivElement | undefined;
}

export interface TabReorderDragResult {
  insertIdx: () => number | null;
}

let pendingReorder: ReorderState = null;

export function getPendingReorder(): ReorderState {
  return pendingReorder;
}

export function clearPendingReorder(): void {
  pendingReorder = null;
}

export function computeInsertIndex(
  containerEl: HTMLElement,
  pointerX: number,
  draggedTabId?: string,
): { logical: number; display: number } | null {
  const tabEls = containerEl.querySelectorAll('[data-tab-id]');
  let logicalIndex = 0;
  for (let i = 0; i < tabEls.length; i++) {
    const el = tabEls[i] as HTMLElement;
    if (draggedTabId && el.dataset.tabId === draggedTabId) continue;
    const rect = el.getBoundingClientRect();
    if (pointerX < rect.left + rect.width / 2) return { logical: logicalIndex, display: i };
    logicalIndex++;
  }
  return { logical: logicalIndex, display: tabEls.length };
}

export function useTabReorderDrag(opts: TabReorderDragOptions): TabReorderDragResult {
  const [insertIdx, setInsertIdx] = createSignal<number | null>(null);
  const dndCtx = useDragDropContext();

  const isSameBarDrag = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return false;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' && data.sourceGroupId === opts.groupId();
  };

  const draggedTabId = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return undefined;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' ? data.tab.id : undefined;
  };

  createEffect(() => {
    const containerEl = opts.containerRef();
    if (!isSameBarDrag() || !containerEl) {
      setInsertIdx(null);
      pendingReorder = null;
      return;
    }

    const sensor = dndCtx?.[0]?.active?.sensor;
    const x = sensor?.coordinates?.current?.x;
    const y = sensor?.coordinates?.current?.y;
    if (x != null && y != null) {
      const rect = containerEl.getBoundingClientRect();
      const VERTICAL_TOLERANCE = 8;
      const inBounds =
        x >= rect.left &&
        x <= rect.right &&
        y >= rect.top - VERTICAL_TOLERANCE &&
        y <= rect.bottom + VERTICAL_TOLERANCE;
      if (!inBounds) {
        setInsertIdx(null);
        pendingReorder = null;
        return;
      }

      const result = computeInsertIndex(containerEl, x, draggedTabId());
      setInsertIdx(result?.display ?? null);
      if (result != null) {
        pendingReorder = { groupId: opts.groupId(), insertIndex: result.logical };
      }
    } else {
      setInsertIdx(null);
      pendingReorder = null;
    }
  });

  createEffect(() => {
    if (!dndCtx?.[0]?.active?.draggable) {
      setInsertIdx(null);
      pendingReorder = null;
    }
  });

  return { insertIdx };
}
