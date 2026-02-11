import { type Component, type JSX, For } from "solid-js";
import { Portal } from "solid-js/web";
import {
  FloatingWindow,
  clampToViewport,
  type FloatingWindowRect,
} from "./FloatingWindow";

export interface FloatingWindowEntry {
  windowId: string;
  rect: FloatingWindowRect;
  title: string;
  content?: JSX.Element;
}

export interface OverlayContainerProps {
  windows: FloatingWindowEntry[];
  zOrder: string[];
  containerWidth: number;
  containerHeight: number;
  onMove?: (windowId: string, x: number, y: number) => void;
  onResize?: (windowId: string, rect: FloatingWindowRect) => void;
  onFocus?: (windowId: string) => void;
  onClose?: (windowId: string) => void;
  onDock?: (windowId: string) => void;
  classNameMapper?: (defaultClassName: string) => string;
  portalTarget?: HTMLElement;
}

export const Z_INDEX_BASE = 1000;

export function getZIndex(windowId: string, zOrder: string[]): number {
  const index = zOrder.indexOf(windowId);
  return Z_INDEX_BASE + (index >= 0 ? index : 0);
}

export function bringToFront(windowId: string, zOrder: string[]): string[] {
  const filtered = zOrder.filter((id) => id !== windowId);
  filtered.push(windowId);
  return filtered;
}

export function syncZOrder(windowIds: string[], currentOrder: string[]): string[] {
  const existing = new Set(windowIds);
  const order = currentOrder.filter((id) => existing.has(id));

  for (const id of windowIds) {
    if (!order.includes(id)) {
      order.push(id);
    }
  }

  return order;
}

export const OverlayContainer: Component<OverlayContainerProps> = (props) => {
  const handleMove = (windowId: string, x: number, y: number) => {
    const entry = props.windows.find((w) => w.windowId === windowId);
    if (!entry) return;

    const clamped = clampToViewport(
      { x, y, width: entry.rect.width, height: entry.rect.height },
      props.containerWidth,
      props.containerHeight,
    );
    props.onMove?.(windowId, clamped.x, clamped.y);
  };

  const handleResize = (windowId: string, rect: FloatingWindowRect) => {
    const clamped = clampToViewport(
      rect,
      props.containerWidth,
      props.containerHeight,
    );
    props.onResize?.(windowId, clamped);
  };

  const handleFocus = (windowId: string) => {
    props.onFocus?.(windowId);
  };

  const content = () => (
    <For each={props.windows}>
      {(entry) => (
        <FloatingWindow
          windowId={entry.windowId}
          rect={entry.rect}
          title={entry.title}
          zIndex={getZIndex(entry.windowId, props.zOrder)}
          onMove={handleMove}
          onResize={handleResize}
          onFocus={handleFocus}
          onClose={props.onClose}
          onDock={props.onDock}
          classNameMapper={props.classNameMapper}
        >
          {entry.content}
        </FloatingWindow>
      )}
    </For>
  );

  return props.portalTarget ? (
    <Portal mount={props.portalTarget}>{content()}</Portal>
  ) : (
    content()
  );
};
