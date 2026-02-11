import type { Accessor } from "solid-js";
import { useDndContext } from "./DndContext";

export interface UseDraggableResult {
  onPointerDown: (event: PointerEvent) => void;
  isDragging: Accessor<boolean>;
}

export function useDraggable(nodeId: Accessor<string>): UseDraggableResult {
  const dnd = useDndContext();

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    dnd.startDrag(nodeId(), event);
  }

  const isDragging = (): boolean => {
    const info = dnd.dragInfo();
    return info !== null && info.nodeId === nodeId();
  };

  return { onPointerDown, isDragging };
}

export function buildDraggableProps(nodeId: string, dnd: ReturnType<typeof useDndContext>) {
  return {
    onPointerDown: (event: PointerEvent) => {
      if (event.button !== 0) return;
      dnd.startDrag(nodeId, event);
    },
  };
}
