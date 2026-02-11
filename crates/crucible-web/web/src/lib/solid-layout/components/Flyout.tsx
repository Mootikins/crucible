import { type Component, createMemo, createEffect, onCleanup, Show } from "solid-js";
import { render } from "solid-js/web";
import { DockLocation } from "../../flexlayout/core/DockLocation";
import { computeFlyoutRect } from "../../flexlayout/core/flyout-rect";
import { Action } from "../../flexlayout/model/Action";
import type { TabNode } from "../../flexlayout/model/TabNode";
import type { IJsonBorderNode, IJsonTabNode } from "../../flexlayout/types";
import { useLayoutContext } from "../context";
import { computeInsets } from "./BorderLayout";

export function findFlyoutBorder(
  borders: IJsonBorderNode[],
): { border: IJsonBorderNode; tab: IJsonTabNode; tabIndex: number } | undefined {
  for (const border of borders) {
    const flyoutTabId = border.flyoutTabId as string | undefined;
    if (!flyoutTabId) continue;

    const children = (border.children ?? []) as IJsonTabNode[];
    const tabIndex = children.findIndex((c) => c.id === flyoutTabId);
    if (tabIndex === -1) continue;

    return { border, tab: children[tabIndex], tabIndex };
  }
  return undefined;
}

export function toDockLocation(location: string): DockLocation {
  switch (location) {
    case "left": return DockLocation.LEFT;
    case "right": return DockLocation.RIGHT;
    case "top": return DockLocation.TOP;
    case "bottom": return DockLocation.BOTTOM;
    default: return DockLocation.LEFT;
  }
}

export function buildFlyoutRect(
  border: IJsonBorderNode,
  _tabIndex: number,
  layoutWidth: number,
  layoutHeight: number,
  borders: IJsonBorderNode[],
  tabButtonRect?: { x: number; y: number; width: number; height: number },
): { x: number; y: number; width: number; height: number } {
  const location = toDockLocation(border.location ?? "left");
  const primarySize = (border.size as number) ?? 200;
  const insets = computeInsets(borders);

  const rect = computeFlyoutRect({
    primarySize,
    location,
    layoutWidth,
    layoutHeight,
    insets,
    tabButtonRect,
  });

  return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
}

export function flyoutEdgeName(location: string): string {
  if (location === "left") return "left";
  if (location === "right") return "right";
  if (location === "top") return "top";
  return "bottom";
}

export function isAutoHide(border: IJsonBorderNode): boolean {
  return border.enableAutoHide === true;
}

export interface FlyoutProps {}

export const Flyout: Component<FlyoutProps> = () => {
  const ctx = useLayoutContext();
  const mapClass = (cls: string) => ctx.classNameMapper?.(cls) ?? cls;

  let panelRef: HTMLDivElement | undefined;
  let contentContainer: HTMLDivElement | undefined;
  let disposeFn: (() => void) | undefined;
  let currentTabId: string | undefined;

  const allBorders = createMemo(
    (): IJsonBorderNode[] => (ctx.bridge.store.borders ?? []) as IJsonBorderNode[],
  );

  const flyoutInfo = createMemo(() => findFlyoutBorder(allBorders()));

  const isVisible = createMemo(() => flyoutInfo() !== undefined);

  const flyoutRect = createMemo(() => {
    const info = flyoutInfo();
    if (!info) return undefined;

    const layoutRect = ctx.bridge.store.global?.layoutRect as
      | { width: number; height: number }
      | undefined;
    const layoutWidth = layoutRect?.width ?? 1000;
    const layoutHeight = layoutRect?.height ?? 800;

    const borderPath = `/border/${info.border.id ?? `border_${info.border.location}`}`;
    const buttonPath = `${borderPath}/tb${info.tabIndex}`;
    let tabButtonRect: { x: number; y: number; width: number; height: number } | undefined;

    if (typeof document !== "undefined") {
      const buttonEl = document.querySelector(
        `[data-layout-path="${buttonPath}"]`,
      ) as HTMLElement | null;
      if (buttonEl) {
        const domRect = buttonEl.getBoundingClientRect();
        tabButtonRect = {
          x: domRect.x,
          y: domRect.y,
          width: domRect.width,
          height: domRect.height,
        };
      }
    }

    return buildFlyoutRect(
      info.border,
      info.tabIndex,
      layoutWidth,
      layoutHeight,
      allBorders(),
      tabButtonRect,
    );
  });

  const edgeName = createMemo(() => {
    const info = flyoutInfo();
    if (!info) return "left";
    return flyoutEdgeName(info.border.location ?? "left");
  });

  const borderId = createMemo(() => {
    const info = flyoutInfo();
    return info?.border.id ?? undefined;
  });

  createEffect(() => {
    const info = flyoutInfo();
    if (!info || !panelRef) {
      if (disposeFn) {
        disposeFn();
        disposeFn = undefined;
      }
      if (contentContainer) {
        contentContainer.remove();
        contentContainer = undefined;
      }
      currentTabId = undefined;
      return;
    }

    const tabId = info.tab.id;
    if (tabId === currentTabId) return;

    if (disposeFn) {
      disposeFn();
      disposeFn = undefined;
    }
    if (contentContainer) {
      contentContainer.remove();
    }

    contentContainer = document.createElement("div");
    contentContainer.style.cssText = "width: 100%; height: 100%;";

    const modelNode = tabId ? (ctx.model.getNodeById(tabId) as TabNode | undefined) : undefined;
    if (modelNode) {
      disposeFn = render(() => ctx.factory(modelNode), contentContainer);
    }

    panelRef.appendChild(contentContainer);
    currentTabId = tabId;
  });

  createEffect(() => {
    const info = flyoutInfo();
    if (!info || !panelRef) return;
    if (!isAutoHide(info.border)) return;

    let autoHideTimer: ReturnType<typeof setTimeout> | undefined;

    const handleMouseLeave = () => {
      autoHideTimer = setTimeout(() => {
        const bId = info.border.id;
        if (bId) {
          ctx.doAction(Action.closeFlyout(bId));
        }
      }, 300);
    };

    const handleMouseEnter = () => {
      if (autoHideTimer) {
        clearTimeout(autoHideTimer);
        autoHideTimer = undefined;
      }
    };

    panelRef.addEventListener("mouseleave", handleMouseLeave);
    panelRef.addEventListener("mouseenter", handleMouseEnter);

    onCleanup(() => {
      if (autoHideTimer) clearTimeout(autoHideTimer);
      panelRef?.removeEventListener("mouseleave", handleMouseLeave);
      panelRef?.removeEventListener("mouseenter", handleMouseEnter);
    });
  });

  onCleanup(() => {
    if (disposeFn) {
      disposeFn();
      disposeFn = undefined;
    }
  });

  const handleBackdropClick = (e: MouseEvent) => {
    e.stopPropagation();
    const bId = borderId();
    if (bId) {
      ctx.doAction(Action.closeFlyout(bId));
    }
  };

  const handlePanelPointerDown = (e: PointerEvent) => {
    e.stopPropagation();
  };

  return (
    <Show when={isVisible()}>
      <div
        class={mapClass("flexlayout__flyout_backdrop")}
        data-layout-path="/flyout/backdrop"
        onPointerDown={handleBackdropClick}
        style={{
          position: "absolute",
          top: "0",
          left: "0",
          width: "100%",
          height: "100%",
          "z-index": "900",
        }}
      />
      <div
        ref={panelRef}
        class={mapClass("flexlayout__flyout")}
        data-layout-path="/flyout/panel"
        data-edge={edgeName()}
        onPointerDown={handlePanelPointerDown}
        style={{
          position: "absolute",
          left: `${flyoutRect()?.x ?? 0}px`,
          top: `${flyoutRect()?.y ?? 0}px`,
          width: `${Math.max(0, flyoutRect()?.width ?? 0)}px`,
          height: `${Math.max(0, flyoutRect()?.height ?? 0)}px`,
          "z-index": "901",
          overflow: "hidden",
        }}
      />
    </Show>
  );
};
