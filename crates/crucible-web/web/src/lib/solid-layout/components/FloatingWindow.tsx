import { type Component, type JSX, onCleanup } from "solid-js";
import { CLASSES } from "../../flexlayout/core/Types";

export type ResizeEdge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

export interface FloatingWindowRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface FloatingWindowProps {
  windowId: string;
  rect: FloatingWindowRect;
  title: string;
  zIndex: number;
  children?: JSX.Element;
  onMove?: (windowId: string, x: number, y: number) => void;
  onResize?: (windowId: string, rect: FloatingWindowRect) => void;
  onFocus?: (windowId: string) => void;
  onClose?: (windowId: string) => void;
  onDock?: (windowId: string) => void;
  classNameMapper?: (defaultClassName: string) => string;
}

export const MIN_WIDTH = 150;
export const MIN_HEIGHT = 80;
export const MIN_VIEWPORT_VISIBILITY = 100;

export const RESIZE_EDGES: ReadonlyArray<{
  edge: ResizeEdge;
  className: string;
  extraClass?: string;
}> = [
  { edge: "n", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_N },
  { edge: "s", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_S },
  { edge: "e", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_E },
  { edge: "w", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_W },
  { edge: "nw", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_NW },
  { edge: "ne", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_NE },
  { edge: "sw", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_SW },
  {
    edge: "se",
    className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_SE,
    extraClass: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_HANDLE,
  },
];

/**
 * Compute the new rect after applying a resize drag delta for the given edge.
 * Enforces MIN_WIDTH/MIN_HEIGHT constraints; when dragging a left/top edge
 * past minimum, the position clamps so the opposite edge stays fixed.
 */
export function computeResizedRect(
  dx: number,
  dy: number,
  start: { x: number; y: number; width: number; height: number },
  edge: ResizeEdge,
): FloatingWindowRect {
  const resizesLeft = edge === "w" || edge === "nw" || edge === "sw";
  const resizesRight = edge === "e" || edge === "ne" || edge === "se";
  const resizesTop = edge === "n" || edge === "nw" || edge === "ne";
  const resizesBottom = edge === "s" || edge === "sw" || edge === "se";

  let x = start.x;
  let y = start.y;
  let width = start.width;
  let height = start.height;

  if (resizesRight) {
    width = Math.max(MIN_WIDTH, start.width + dx);
  }
  if (resizesBottom) {
    height = Math.max(MIN_HEIGHT, start.height + dy);
  }
  if (resizesLeft) {
    const proposedWidth = start.width - dx;
    if (proposedWidth >= MIN_WIDTH) {
      width = proposedWidth;
      x = start.x + dx;
    } else {
      width = MIN_WIDTH;
      x = start.x + (start.width - MIN_WIDTH);
    }
  }
  if (resizesTop) {
    const proposedHeight = start.height - dy;
    if (proposedHeight >= MIN_HEIGHT) {
      height = proposedHeight;
      y = start.y + dy;
    } else {
      height = MIN_HEIGHT;
      y = start.y + (start.height - MIN_HEIGHT);
    }
  }

  return { x, y, width, height };
}

export function clampToViewport(
  rect: FloatingWindowRect,
  containerWidth: number,
  containerHeight: number,
  minVisible: number = MIN_VIEWPORT_VISIBILITY,
): FloatingWindowRect {
  let { x, y } = rect;
  const { width, height } = rect;

  const maxX = containerWidth - minVisible;
  const minX = -(width - minVisible);
  const maxY = containerHeight - minVisible;
  const minY = -(height - minVisible);

  x = Math.max(minX, Math.min(maxX, x));
  y = Math.max(minY, Math.min(maxY, y));

  return { x, y, width, height };
}

export function buildResizeHandleClass(
  def: { className: string; extraClass?: string },
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  const base = map(def.className);
  return def.extraClass ? `${base} ${map(def.extraClass)}` : base;
}

export function buildPanelClass(
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  return map(CLASSES.FLEXLAYOUT__FLOATING_PANEL);
}

export function buildTitleBarClass(
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  return map(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR);
}

export function buildTitleClass(
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  return map(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR_TITLE);
}

export function buildButtonsClass(
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  return map(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR_BUTTONS);
}

export function buildContentClass(
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  return map(CLASSES.FLEXLAYOUT__FLOATING_PANEL_CONTENT);
}

export function buildPanelStyle(
  rect: FloatingWindowRect,
  zIndex: number,
): JSX.CSSProperties {
  return {
    position: "absolute",
    left: `${rect.x}px`,
    top: `${rect.y}px`,
    width: `${rect.width}px`,
    height: `${rect.height}px`,
    "z-index": String(zIndex),
  };
}

export const FloatingWindow: Component<FloatingWindowProps> = (props) => {
  const map = (cls: string) => props.classNameMapper?.(cls) ?? cls;

  const onPanelPointerDown = () => {
    props.onFocus?.(props.windowId);
  };

  const onTitleBarPointerDown = (event: PointerEvent) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest(`.${map(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR_BUTTONS)}`)) {
      return;
    }

    event.preventDefault();
    props.onFocus?.(props.windowId);

    const startX = event.clientX;
    const startY = event.clientY;
    const startRectX = props.rect.x;
    const startRectY = props.rect.y;

    const onMove = (moveEvent: PointerEvent) => {
      const dx = moveEvent.clientX - startX;
      const dy = moveEvent.clientY - startY;
      props.onMove?.(props.windowId, startRectX + dx, startRectY + dy);
    };

    const onUp = () => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };

    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);

    onCleanup(() => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    });
  };

  const onResizePointerDown = (event: PointerEvent, edge: ResizeEdge) => {
    event.preventDefault();
    event.stopPropagation();
    props.onFocus?.(props.windowId);

    const startX = event.clientX;
    const startY = event.clientY;
    const startRect = {
      x: props.rect.x,
      y: props.rect.y,
      width: props.rect.width,
      height: props.rect.height,
    };

    const onMove = (moveEvent: PointerEvent) => {
      const dx = moveEvent.clientX - startX;
      const dy = moveEvent.clientY - startY;
      const newRect = computeResizedRect(dx, dy, startRect, edge);
      props.onResize?.(props.windowId, newRect);
    };

    const onUp = () => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };

    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);

    onCleanup(() => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    });
  };

  return (
    <div
      class={buildPanelClass(props.classNameMapper)}
      style={buildPanelStyle(props.rect, props.zIndex)}
      data-window-id={props.windowId}
      onPointerDown={onPanelPointerDown}
    >
      <div
        class={buildTitleBarClass(props.classNameMapper)}
        onPointerDown={onTitleBarPointerDown}
      >
        <div class={buildTitleClass(props.classNameMapper)}>
          {props.title}
        </div>
        <div class={buildButtonsClass(props.classNameMapper)}>
          {props.onDock && (
            <button
              type="button"
              title="Dock"
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                props.onDock?.(props.windowId);
              }}
            >
              ⇱
            </button>
          )}
          {props.onClose && (
            <button
              type="button"
              title="Close"
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                props.onClose?.(props.windowId);
              }}
            >
              ✕
            </button>
          )}
        </div>
      </div>

      <div class={buildContentClass(props.classNameMapper)}>
        {props.children}
      </div>

      {RESIZE_EDGES.map((def) => (
        <div
          class={buildResizeHandleClass(def, props.classNameMapper)}
          onPointerDown={(e) => onResizePointerDown(e, def.edge)}
        />
      ))}
    </div>
  );
};
