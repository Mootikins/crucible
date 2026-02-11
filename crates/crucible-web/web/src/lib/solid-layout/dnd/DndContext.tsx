import {
  createContext,
  useContext,
  createSignal,
  onCleanup,
  type Accessor,
  type ParentComponent,
} from "solid-js";
import type { Model } from "../../flexlayout/model/Model";
import type { Node } from "../../flexlayout/model/Node";
import type { IDraggable } from "../../flexlayout/model/IDraggable";
import type { DropInfo } from "../../flexlayout/core/DropInfo";
import type { LayoutAction } from "../../flexlayout/model/Action";
import { Action } from "../../flexlayout/model/Action";
import { CLASSES } from "../../flexlayout/core/Types";
import type { ClassNameMapper } from "../classes";
import { mapClass, cn } from "../classes";

export interface DragInfo {
  nodeId: string;
  nodeName: string;
  nodeType: string;
}

export interface DropTarget {
  nodeId: string;
  /** "top" | "bottom" | "left" | "right" | "center" */
  location: string;
  /** Position index within target for tab reordering */
  index: number;
  className: string;
  rect: { x: number; y: number; width: number; height: number };
}

export interface DragPosition {
  x: number;
  y: number;
}

export interface DndContextValue {
  dragInfo: Accessor<DragInfo | null>;
  dropTarget: Accessor<DropTarget | null>;
  dragPosition: Accessor<DragPosition | null>;
  isDragging: Accessor<boolean>;
  startDrag: (nodeId: string, event: PointerEvent) => void;
  cancelDrag: () => void;
}

const DndCtx = createContext<DndContextValue>();

export function useDndContext(): DndContextValue {
  const ctx = useContext(DndCtx);
  if (!ctx) {
    throw new Error("useDndContext must be used within a <DndProvider>");
  }
  return ctx;
}

export const DRAG_THRESHOLD = 5;

export function buildOutlineClass(
  dropInfo: { className: string; rect: { width: number } },
  mapper?: ClassNameMapper,
): string {
  let cls = mapClass(dropInfo.className, mapper);
  if (dropInfo.rect.width <= 5) {
    cls += " " + mapClass(CLASSES.FLEXLAYOUT__OUTLINE_RECT + "_tab_reorder", mapper);
  }
  return cls;
}

export function buildGhostClass(mapper?: ClassNameMapper): string {
  return cn(
    mapClass(CLASSES.FLEXLAYOUT__TAB_BUTTON, mapper),
    mapClass(CLASSES.FLEXLAYOUT__TAB_BUTTON + "--selected", mapper),
  );
}

export function isDroppableNode(node: Node | undefined): node is (Node & IDraggable) {
  if (!node) return false;
  const type = (node as any).getType?.();
  return type === "tab" || type === "tabset";
}

export function computeRelativePosition(
  clientX: number,
  clientY: number,
  containerRect: { left: number; top: number },
): DragPosition {
  return {
    x: clientX - containerRect.left,
    y: clientY - containerRect.top,
  };
}

export function exceedsThreshold(
  startX: number,
  startY: number,
  currentX: number,
  currentY: number,
  threshold: number = DRAG_THRESHOLD,
): boolean {
  const dx = currentX - startX;
  const dy = currentY - startY;
  return Math.abs(dx) > threshold || Math.abs(dy) > threshold;
}

export function findDropTarget(
  model: Model,
  dragNode: Node & IDraggable,
  x: number,
  y: number,
): DropInfo | undefined {
  const root = model.getRoot();
  if (!root) return undefined;

  let di = root.findDropTargetNode("main", dragNode, x, y);
  if (!di) {
    di = model.getBorderSet().findDropTargetNode(dragNode, x, y);
  }
  return di;
}

export function dropInfoToTarget(info: DropInfo): DropTarget {
  return {
    nodeId: info.node.getId(),
    location: info.location.getName(),
    index: info.index,
    className: info.className,
    rect: {
      x: info.rect.x,
      y: info.rect.y,
      width: info.rect.width,
      height: info.rect.height,
    },
  };
}

export function buildMoveAction(
  dragNodeId: string,
  target: DropTarget,
): LayoutAction {
  return Action.moveNode(
    dragNodeId,
    target.nodeId,
    target.location,
    target.index,
  );
}

export interface DndProviderProps {
  model: Model;
  doAction: (action: LayoutAction) => void;
  layoutRef: () => HTMLDivElement | undefined;
  classNameMapper?: ClassNameMapper;
}

export const DndProvider: ParentComponent<DndProviderProps> = (props) => {
  const [dragInfo, setDragInfo] = createSignal<DragInfo | null>(null);
  const [dropTarget, setDropTarget] = createSignal<DropTarget | null>(null);
  const [dragPosition, setDragPosition] = createSignal<DragPosition | null>(null);

  let pendingDrag: {
    nodeId: string;
    startClientX: number;
    startClientY: number;
  } | null = null;

  const isDragging = (): boolean => dragInfo() !== null;

  function startDrag(nodeId: string, event: PointerEvent) {
    const node = props.model.getNodeById(nodeId);
    if (!node || !isDroppableNode(node)) return;
    if (!(node as any).isEnableDrag?.()) return;

    pendingDrag = {
      nodeId,
      startClientX: event.clientX,
      startClientY: event.clientY,
    };

    document.addEventListener("pointermove", onPointerMove);
    document.addEventListener("pointerup", onPointerUp);
    document.addEventListener("keydown", onKeyDown);
  }

  function activateDrag(nodeId: string) {
    const node = props.model.getNodeById(nodeId);
    if (!node) return;

    setDragInfo({
      nodeId,
      nodeName: (node as any).getName?.() ?? nodeId,
      nodeType: (node as any).getType?.() ?? "tab",
    });
  }

  function cancelDrag() {
    clearDrag();
  }

  function clearDrag() {
    pendingDrag = null;
    setDragInfo(null);
    setDropTarget(null);
    setDragPosition(null);

    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerup", onPointerUp);
    document.removeEventListener("keydown", onKeyDown);
  }

  function onPointerMove(event: PointerEvent) {
    const layoutEl = props.layoutRef();
    if (!layoutEl) return;

    const containerRect = layoutEl.getBoundingClientRect();
    const pos = computeRelativePosition(event.clientX, event.clientY, containerRect);

    if (pendingDrag && !isDragging()) {
      if (
        exceedsThreshold(
          pendingDrag.startClientX,
          pendingDrag.startClientY,
          event.clientX,
          event.clientY,
        )
      ) {
        activateDrag(pendingDrag.nodeId);
      } else {
        return;
      }
    }

    if (!isDragging()) return;

    setDragPosition(pos);

    const info = dragInfo();
    if (!info) return;

    const dragNode = props.model.getNodeById(info.nodeId);
    if (!dragNode || !isDroppableNode(dragNode)) {
      setDropTarget(null);
      return;
    }

    const di = findDropTarget(props.model, dragNode, pos.x, pos.y);
    if (di) {
      setDropTarget(dropInfoToTarget(di));
    } else {
      setDropTarget(null);
    }
  }

  function onPointerUp(_event: PointerEvent) {
    const info = dragInfo();
    const target = dropTarget();

    if (info && target) {
      props.doAction(buildMoveAction(info.nodeId, target));
    }

    clearDrag();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      cancelDrag();
    }
  }

  onCleanup(() => {
    clearDrag();
  });

  const value: DndContextValue = {
    dragInfo,
    dropTarget,
    dragPosition,
    isDragging,
    startDrag,
    cancelDrag,
  };

  return (
    <DndCtx.Provider value={value}>
      {props.children}
    </DndCtx.Provider>
  );
};
