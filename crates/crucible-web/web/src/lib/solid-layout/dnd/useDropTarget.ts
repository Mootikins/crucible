import type { Accessor } from "solid-js";
import { useDndContext, type DropTarget } from "./DndContext";

export interface UseDropTargetResult {
  isDropTarget: Accessor<boolean>;
  dropLocation: Accessor<string | null>;
  dropIndex: Accessor<number | null>;
  currentDropTarget: Accessor<DropTarget | null>;
}

export function useDropTarget(nodeId: Accessor<string>): UseDropTargetResult {
  const dnd = useDndContext();

  const isDropTarget = (): boolean => {
    const target = dnd.dropTarget();
    return target !== null && target.nodeId === nodeId();
  };

  const dropLocation = (): string | null => {
    const target = dnd.dropTarget();
    if (target && target.nodeId === nodeId()) {
      return target.location;
    }
    return null;
  };

  const dropIndex = (): number | null => {
    const target = dnd.dropTarget();
    if (target && target.nodeId === nodeId()) {
      return target.index;
    }
    return null;
  };

  const currentDropTarget = (): DropTarget | null => {
    const target = dnd.dropTarget();
    if (target && target.nodeId === nodeId()) {
      return target;
    }
    return null;
  };

  return { isDropTarget, dropLocation, dropIndex, currentDropTarget };
}

export function isTargetNode(target: DropTarget | null, nodeId: string): boolean {
  return target !== null && target.nodeId === nodeId;
}

export function getDropLocationForNode(target: DropTarget | null, nodeId: string): string | null {
  if (target && target.nodeId === nodeId) {
    return target.location;
  }
  return null;
}
