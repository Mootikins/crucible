import {
  type Component,
  createMemo,
  createSignal,
  For,
  Show,
  onMount,
  onCleanup,
} from "solid-js";
import { render } from "solid-js/web";
import { CLASSES } from "../../flexlayout/core/Types";
import { Action } from "../../flexlayout/model/Action";
import type { IJsonBorderNode, IJsonTabNode } from "../../flexlayout/types";
import type { TabNode } from "../../flexlayout/model/TabNode";
import { useLayoutContext } from "../context";
import { isHorizontalBorder, isStartEdge } from "./Border";
import {
  buildBorderButtonClass,
  getDockIcon,
} from "./BorderStrip";

export function resolveVisibleIndices(
  border: IJsonBorderNode,
): number[] {
  const explicit = border.visibleTabs;
  if (Array.isArray(explicit) && explicit.length > 0) {
    return explicit;
  }
  const selected = typeof border.selected === "number" ? border.selected : -1;
  return selected >= 0 ? [selected] : [];
}

export function isTileHorizontal(location: string): boolean {
  return !isHorizontalBorder(location);
}

export function ensureTileWeights(
  existing: number[],
  count: number,
): number[] {
  if (existing.length === count) return existing;
  return Array(count).fill(1);
}

export function edgeResizeCursor(location: string): string {
  return isHorizontalBorder(location) ? "ns-resize" : "ew-resize";
}

export function tileSplitterCursor(location: string): string {
  return isTileHorizontal(location) ? "ew-resize" : "ns-resize";
}

export function buildBorderContentClass(
  mapper: (cls: string) => string,
): string {
  return mapper(CLASSES.FLEXLAYOUT__BORDER_TAB_CONTENTS);
}

export function buildBorderTabBarClass(
  mapper: (cls: string) => string,
): string {
  return mapper(CLASSES.FLEXLAYOUT__BORDER_TABBAR);
}

export function buildTileHostClass(
  mapper: (cls: string) => string,
): string {
  return `${mapper(CLASSES.FLEXLAYOUT__TAB)} ${mapper(CLASSES.FLEXLAYOUT__TAB_BORDER)}`;
}

export function buildTileSplitterClass(
  location: string,
  mapper: (cls: string) => string,
): string {
  const orient = isTileHorizontal(location) ? "horz" : "vert";
  return `${mapper(CLASSES.FLEXLAYOUT__SPLITTER)} ${mapper(CLASSES.FLEXLAYOUT__SPLITTER_ + orient)}`;
}

export function buildEdgeSplitterClass(
  location: string,
  mapper: (cls: string) => string,
): string {
  const orient = isHorizontalBorder(location) ? "vert" : "horz";
  return `${mapper(CLASSES.FLEXLAYOUT__SPLITTER)} ${mapper(CLASSES.FLEXLAYOUT__SPLITTER_ + orient)} ${mapper(CLASSES.FLEXLAYOUT__SPLITTER_BORDER)}`;
}

export function clampSize(
  value: number,
  minSize: number,
  maxSize: number,
): number {
  return Math.max(minSize, Math.min(maxSize, value));
}

export interface BorderContentProps {
  borderNode: IJsonBorderNode;
  location: string;
  path: string;
}

export const BorderContent: Component<BorderContentProps> = (props) => {
  const ctx = useLayoutContext();
  const mapClass = (cls: string) => ctx.classNameMapper?.(cls) ?? cls;

  const dockState = createMemo(
    () => (props.borderNode.dockState as string) ?? "expanded",
  );

  const selected = createMemo(() => {
    const sel = props.borderNode.selected;
    return typeof sel === "number" ? sel : -1;
  });

  const isContentVisible = createMemo(
    () => dockState() === "expanded" && selected() !== -1,
  );

  const horizontal = createMemo(() => isHorizontalBorder(props.location));

  const tabs = createMemo(
    (): IJsonTabNode[] => (props.borderNode.children ?? []) as IJsonTabNode[],
  );

  const visibleIndices = createMemo(() =>
    resolveVisibleIndices(props.borderNode),
  );

  const visibleNodes = createMemo(() =>
    visibleIndices()
      .map((i) => tabs()[i])
      .filter((t): t is IJsonTabNode => t != null),
  );

  const isTiled = createMemo(() => visibleNodes().length > 1);
  const tileHoriz = createMemo(() => isTileHorizontal(props.location));

  const borderSize = createMemo(
    () => (props.borderNode.size as number | undefined) ?? 200,
  );
  const borderMinSize = createMemo(
    () => (props.borderNode.minSize as number | undefined) ?? 30,
  );
  const borderMaxSize = createMemo(
    () => (props.borderNode.maxSize as number | undefined) ?? 99999,
  );

  const splitterSize = createMemo(() => {
    const globalSplitterSize = (ctx.bridge.store.global as any)?.splitterSize;
    return typeof globalSplitterSize === "number" ? globalSplitterSize : 8;
  });

  const sizeStyle = createMemo(() => {
    if (!isContentVisible()) return {};
    const size = borderSize();
    const min = borderMinSize();
    const max = borderMaxSize();
    if (horizontal()) {
      return {
        height: `${size}px`,
        "min-height": `${min}px`,
        "max-height": `${max}px`,
      };
    }
    return {
      width: `${size}px`,
      "min-width": `${min}px`,
      "max-width": `${max}px`,
    };
  });

  const [tileWeights, setTileWeights] = createSignal<number[]>([]);

  const currentWeights = createMemo(() => {
    const count = visibleNodes().length;
    return ensureTileWeights(tileWeights(), count);
  });

  const onEdgeSashPointerDown = (e: PointerEvent) => {
    e.stopPropagation();
    e.preventDefault();

    const borderId = props.borderNode.id;
    if (!borderId) return;

    const startPos = horizontal() ? e.clientY : e.clientX;
    const startSize = borderSize();
    const min = borderMinSize();
    const max = borderMaxSize();
    const isStart = isStartEdge(props.location);

    const onMove = (moveEvent: PointerEvent) => {
      const currentPos = horizontal() ? moveEvent.clientY : moveEvent.clientX;
      const delta = currentPos - startPos;
      const newSize = isStart ? startSize + delta : startSize - delta;
      const clamped = clampSize(newSize, min, max);
      ctx.doAction(Action.adjustBorderSplit(borderId, clamped));
    };

    const onUp = () => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };

    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  };

  const onTileSplitterPointerDown = (
    splitterIndex: number,
    e: PointerEvent,
    containerEl: HTMLElement,
  ) => {
    e.stopPropagation();
    e.preventDefault();

    const th = tileHoriz();
    const startPos = th ? e.clientX : e.clientY;
    const weights = [...currentWeights()];
    const beforeIdx = splitterIndex;
    const afterIdx = splitterIndex + 1;

    const tiles =
      containerEl.querySelectorAll<HTMLElement>("[data-border-tile]");
    const beforeEl = tiles[beforeIdx];
    const afterEl = tiles[afterIdx];
    if (!beforeEl || !afterEl) return;

    const beforePx = th ? beforeEl.offsetWidth : beforeEl.offsetHeight;
    const afterPx = th ? afterEl.offsetWidth : afterEl.offsetHeight;
    const totalPx = beforePx + afterPx;
    const totalWeight = (weights[beforeIdx] ?? 1) + (weights[afterIdx] ?? 1);
    const minPx = 30;

    const onMove = (moveEvent: PointerEvent) => {
      const pos = th ? moveEvent.clientX : moveEvent.clientY;
      const delta = pos - startPos;
      const newBefore = Math.max(minPx, Math.min(totalPx - minPx, beforePx + delta));
      const newAfter = totalPx - newBefore;

      const next = [...weights];
      next[beforeIdx] = (newBefore / totalPx) * totalWeight;
      next[afterIdx] = (newAfter / totalPx) * totalWeight;
      setTileWeights(next);
    };

    const onUp = () => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };

    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  };

  const handleDockToggle = () => {
    const borderId = props.borderNode.id;
    if (!borderId) return;
    const newState = dockState() === "collapsed" ? "expanded" : "collapsed";
    ctx.doAction(Action.setDockState(borderId, newState));
  };

  const handleTabClick = (tabId: string) => {
    ctx.doAction(Action.selectTab(tabId));
  };

  const renderTabContent = (
    nodeId: string,
    container: HTMLDivElement,
  ): (() => void) | undefined => {
    const modelNode = ctx.model.getNodeById(nodeId) as TabNode | undefined;
    if (!modelNode) return undefined;
    return render(() => ctx.factory(modelNode), container);
  };

  const TabBar: Component<{
    tabNodes: IJsonTabNode[];
    showToolbar: boolean;
    tileIndex?: number;
  }> = (barProps) => {
    const enableDock = createMemo(
      () => props.borderNode.enableDock !== false,
    );

    return (
      <div
        class={buildBorderTabBarClass(mapClass)}
        data-border-tabbar="true"
        data-layout-path={`${props.path}/tabbar${barProps.tileIndex != null ? `/${barProps.tileIndex}` : ""}`}
        style={{
          display: "flex",
          "align-items": "center",
          cursor: edgeResizeCursor(props.location),
        }}
        onPointerDown={onEdgeSashPointerDown}
      >
        <div
          style={{
            display: "flex",
            flex: "1",
            "overflow-x": "auto",
            "align-items": "center",
            "padding-left": "4px",
          }}
        >
          <For each={barProps.tabNodes}>
            {(tab, index) => {
              const childIndex = createMemo(() => {
                const allTabs = tabs();
                return allTabs.indexOf(tab);
              });
              const isTabSelected = createMemo(
                () => childIndex() === selected(),
              );

              return (
                <>
                  <div
                    class={buildBorderButtonClass(
                      props.location,
                      isTabSelected(),
                      mapClass,
                      tab.className as string | undefined,
                    )}
                    data-layout-path={`${props.path}/tb${childIndex()}`}
                    data-state={isTabSelected() ? "selected" : "unselected"}
                    onClick={() => tab.id && handleTabClick(tab.id)}
                    onPointerDown={(e) => e.stopPropagation()}
                  >
                    <span class={mapClass(CLASSES.FLEXLAYOUT__BORDER_BUTTON_CONTENT)}>
                      {tab.name ?? ""}
                    </span>
                  </div>
                  <Show when={index() < barProps.tabNodes.length - 1}>
                    <div class={mapClass(CLASSES.FLEXLAYOUT__BORDER_TAB_DIVIDER)} />
                  </Show>
                </>
              );
            }}
          </For>
        </div>

        <Show when={barProps.showToolbar && enableDock()}>
          <div
            style={{
              display: "flex",
              "align-items": "center",
              padding: "0 4px",
            }}
          >
            <button
              type="button"
              class={mapClass(CLASSES.FLEXLAYOUT__BORDER_DOCK_BUTTON)}
              title={dockState() === "collapsed" ? "Expand" : "Collapse"}
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                handleDockToggle();
              }}
            >
              {getDockIcon(dockState(), props.location)}
            </button>
          </div>
        </Show>
      </div>
    );
  };

  const SingleContent: Component = () => {
    let hostRef: HTMLDivElement | undefined;
    let disposeFn: (() => void) | undefined;

    onMount(() => {
      if (!hostRef) return;
      const node = visibleNodes()[0];
      if (node?.id) {
        disposeFn = renderTabContent(node.id, hostRef);
      }
    });

    onCleanup(() => {
      disposeFn?.();
    });

    return (
      <div
        style={{
          display: "flex",
          "flex-direction": horizontal() ? "row" : "column",
          flex: "1 1 0%",
          "min-width": "0",
          "min-height": "0",
          overflow: "hidden",
        }}
      >
        <TabBar tabNodes={tabs()} showToolbar={true} />
        <div
          style={{
            display: "flex",
            flex: "1",
            "min-width": "0",
            "min-height": "0",
          }}
        >
          <div
            ref={hostRef}
            class={buildTileHostClass(mapClass)}
            data-border-tile="0"
            style={{
              flex: "1",
              position: "relative",
              overflow: "hidden",
            }}
          />
        </div>
      </div>
    );
  };

  const TiledContent: Component = () => {
    let containerRef: HTMLDivElement | undefined;
    const disposeFns: (() => void)[] = [];

    onCleanup(() => {
      for (const fn of disposeFns) fn();
      disposeFns.length = 0;
    });

    return (
      <div
        ref={containerRef}
        style={{
          display: "flex",
          flex: "1",
          "min-width": "0",
          "min-height": "0",
          "flex-direction": tileHoriz() ? "row" : "column",
        }}
      >
        <For each={visibleNodes()}>
          {(node, index) => {
            const weights = currentWeights;
            const weight = createMemo(() => weights()[index()] ?? 1);
            const totalWeight = createMemo(() =>
              weights().reduce((s, w) => s + w, 0) || 1,
            );
            const pct = createMemo(
              () => (weight() / totalWeight()) * 100,
            );
            const splitterCount = createMemo(
              () => Math.max(0, visibleNodes().length - 1),
            );
            const splitterDeduction = createMemo(() =>
              splitterCount() > 0
                ? splitterSize() * (splitterCount() / visibleNodes().length)
                : 0,
            );

            let tileHostRef: HTMLDivElement | undefined;

            onMount(() => {
              if (!tileHostRef || !node.id) return;
              const dispose = renderTabContent(node.id, tileHostRef);
              if (dispose) disposeFns.push(dispose);
            });

            const showToolbar = createMemo(() => index() === 0);

            return (
              <>
                <Show when={index() > 0}>
                  <div
                    class={buildTileSplitterClass(props.location, mapClass)}
                    data-border-tile-splitter={String(index() - 1)}
                    style={{
                      cursor: tileSplitterCursor(props.location),
                      "flex-shrink": "0",
                      ...(tileHoriz()
                        ? {
                            width: `${splitterSize()}px`,
                            "min-width": `${splitterSize()}px`,
                          }
                        : {
                            height: `${splitterSize()}px`,
                            "min-height": `${splitterSize()}px`,
                          }),
                    }}
                    onPointerDown={(e) => {
                      if (containerRef) {
                        onTileSplitterPointerDown(index() - 1, e, containerRef);
                      }
                    }}
                  />
                </Show>

                <div
                  data-border-tile={String(index())}
                  style={{
                    display: "flex",
                    "flex-direction": horizontal() ? "row" : "column",
                    flex: `0 0 calc(${pct()}% - ${splitterDeduction()}px)`,
                    "min-width": "0",
                    "min-height": "0",
                    overflow: "hidden",
                  }}
                >
                  <TabBar
                    tabNodes={[node]}
                    showToolbar={showToolbar()}
                    tileIndex={index()}
                  />
                  <div
                    ref={tileHostRef}
                    class={buildTileHostClass(mapClass)}
                    data-border-tile-host={String(index())}
                    style={{
                      flex: "1",
                      position: "relative",
                      overflow: "hidden",
                    }}
                  />
                </div>
              </>
            );
          }}
        </For>
      </div>
    );
  };

  const EdgeSplitter: Component = () => (
    <div
      class={buildEdgeSplitterClass(props.location, mapClass)}
      data-layout-path={`${props.path}/s-1`}
      style={{
        cursor: edgeResizeCursor(props.location),
        "flex-shrink": "0",
        ...(horizontal()
          ? {
              height: `${splitterSize()}px`,
              "min-height": `${splitterSize()}px`,
            }
          : {
              width: `${splitterSize()}px`,
              "min-width": `${splitterSize()}px`,
            }),
      }}
      onPointerDown={onEdgeSashPointerDown}
    />
  );

  return (
    <Show when={isContentVisible()}>
      {(() => {
        const startEdge = isStartEdge(props.location);

        const contentPanel = () => (
          <div
            class={buildBorderContentClass(mapClass)}
            data-layout-path={`${props.path}/content`}
            data-border-content="true"
            data-state={dockState()}
            style={{
              display: "flex",
              "flex-direction": horizontal() ? "column" : "row",
              ...sizeStyle(),
            }}
          >
            <Show when={!isTiled()} fallback={<TiledContent />}>
              <SingleContent />
            </Show>
          </div>
        );

        return (
          <>
            <Show
              when={startEdge}
              fallback={
                <>
                  <EdgeSplitter />
                  {contentPanel()}
                </>
              }
            >
              {contentPanel()}
              <EdgeSplitter />
            </Show>
          </>
        );
      })()}
    </Show>
  );
};
