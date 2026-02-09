import { Component, createEffect, createMemo, createSignal, For, JSX } from "solid-js";
import { Orientation } from "../flexlayout/core/Orientation";
import { DockLocation } from "../flexlayout/core/DockLocation";
import { Rect } from "../flexlayout/core/Rect";
import { CLASSES } from "../flexlayout/core/Types";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { TabNode } from "../flexlayout/model/TabNode";
import { Action } from "../flexlayout/model/Action";
import { BorderButton } from "./BorderButton";
import { Splitter } from "./Splitter";
import type { ILayoutContext, ITabSetRenderValues } from "./Layout";

export interface IBorderTabProps {
    layout: ILayoutContext;
    border: BorderNode;
    show: boolean;
}

function resolveVisibleTabs(border: BorderNode): number[] {
    const explicit = border.getVisibleTabs();
    if (explicit.length > 0) return explicit;
    const sel = border.getSelected();
    return sel >= 0 ? [sel] : [];
}

/**
 * Tiling direction is perpendicular to the border's dock orientation.
 * - Top/bottom borders (VERT orientation) → tiles side-by-side → horizontal
 * - Left/right borders (HORZ orientation) → tiles stacked → vertical
 */
function tilingIsHorizontal(border: BorderNode): boolean {
    return border.getOrientation() === Orientation.VERT;
}

export const BorderTab: Component<IBorderTabProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;

    const [tileWeights, setTileWeights] = createSignal<number[]>([]);

    const isContentVisible = (): boolean => {
        if (!props.show) return false;
        const state = props.border.getDockState();
        return state === "expanded";
    };

    const visibleIndices = createMemo(() => {
        void props.layout.getRevision();
        return resolveVisibleTabs(props.border);
    });

    const visibleNodes = createMemo((): TabNode[] => {
        void props.layout.getRevision();
        const children = props.border.getChildren();
        return visibleIndices().map((i) => children[i] as TabNode).filter(Boolean);
    });

    const isTiled = createMemo(() => visibleNodes().length > 1);
    const tileHorizontal = createMemo(() => tilingIsHorizontal(props.border));

    createEffect(() => {
        const count = visibleNodes().length;
        if (count > 0) {
            setTileWeights(Array(count).fill(1));
        }
    });

    // When tiling, clear contentRect so Layout.tsx absolute overlays
    // don't double-render — tiled content is rendered inline below.
    createEffect(() => {
        void props.layout.getRevision();
        if (selfRef && isContentVisible()) {
            if (isTiled()) {
                props.border.setContentRect(Rect.empty());
            } else {
                props.border.setContentRect(props.layout.getBoundingClientRect(selfRef));
            }
        }
    });

    const style = (): Record<string, any> => {
        void props.layout.getRevision();
        const s: Record<string, any> = {};

        if (props.border.getOrientation() === Orientation.HORZ) {
            s.width = props.border.getSize() + "px";
            s["min-width"] = props.border.getMinSize() + "px";
            s["max-width"] = props.border.getMaxSize() + "px";
        } else {
            s.height = props.border.getSize() + "px";
            s["min-height"] = props.border.getMinSize() + "px";
            s["max-height"] = props.border.getMaxSize() + "px";
        }

        s.display = isContentVisible() ? "flex" : "none";
        s["flex-direction"] = tileHorizontal() ? "row" : "column";

        return s;
    };

    const splitterSize = () => props.border.getModel().getSplitterSize();

    const tileStyle = (index: number): Record<string, any> => {
        const weights = tileWeights();
        const w = weights[index] ?? 1;
        const totalWeight = weights.reduce((a, b) => a + b, 0) || 1;
        const pct = (w / totalWeight) * 100;
        const splitterCount = Math.max(0, weights.length - 1);
        const splitterDeduction = splitterCount > 0
            ? splitterSize() * (splitterCount / weights.length)
            : 0;
        return {
            flex: `0 0 calc(${pct}% - ${splitterDeduction}px)`,
            overflow: "hidden",
            position: "relative",
        };
    };

    const onSplitterPointerDown = (splitterIndex: number, event: PointerEvent) => {
        event.stopPropagation();
        event.preventDefault();

        const isHorz = tileHorizontal();
        const startPos = isHorz ? event.clientX : event.clientY;
        const weights = [...tileWeights()];
        const beforeIdx = splitterIndex;
        const afterIdx = splitterIndex + 1;

        if (!selfRef) return;
        const tileEls = selfRef.querySelectorAll<HTMLElement>("[data-border-tile]");
        const beforeEl = tileEls[beforeIdx];
        const afterEl = tileEls[afterIdx];
        if (!beforeEl || !afterEl) return;

        const beforeSize = isHorz ? beforeEl.offsetWidth : beforeEl.offsetHeight;
        const afterSize = isHorz ? afterEl.offsetWidth : afterEl.offsetHeight;
        const totalSize = beforeSize + afterSize;
        const totalWeight = weights[beforeIdx] + weights[afterIdx];
        const minPx = 30;

        const onMove = (e: PointerEvent) => {
            const currentPos = isHorz ? e.clientX : e.clientY;
            const delta = currentPos - startPos;

            const newBeforeSize = Math.max(minPx, Math.min(totalSize - minPx, beforeSize + delta));
            const newAfterSize = totalSize - newBeforeSize;

            const newWeights = [...weights];
            newWeights[beforeIdx] = (newBeforeSize / totalSize) * totalWeight;
            newWeights[afterIdx] = (newAfterSize / totalSize) * totalWeight;
            setTileWeights(newWeights);
        };

        const onUp = () => {
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);
        };

        document.addEventListener("pointermove", onMove);
        document.addEventListener("pointerup", onUp);
    };

    const internalSplitterStyle = (): Record<string, any> => {
        const isHorz = tileHorizontal();
        const sz = splitterSize();
        return {
            cursor: isHorz ? "ew-resize" : "ns-resize",
            "flex-shrink": "0",
            ...(isHorz
                ? { width: sz + "px", "min-width": sz + "px" }
                : { height: sz + "px", "min-height": sz + "px" }),
        };
    };

    const horizontal = () => props.border.getOrientation() === Orientation.HORZ;
    const className = props.layout.getClassName(CLASSES.FLEXLAYOUT__BORDER_TAB_CONTENTS);
    const cm = props.layout.getClassName;

    const location = props.border.getLocation();
    const isBeforeSplitter = location === DockLocation.LEFT || location === DockLocation.TOP;

    const isExpanded = (): boolean => {
        return props.border.getDockState() === "expanded";
    };

    const onDockToggle = (event: MouseEvent) => {
        event.stopPropagation();
        const current = props.border.getDockState();
        let next: "expanded" | "collapsed" | "hidden";
        if (current === "expanded") {
            next = "collapsed";
        } else if (current === "collapsed") {
            next = "hidden";
        } else {
            next = "expanded";
        }
        props.layout.doAction(Action.setDockState(props.border.getId(), next));
    };

    const dockIcon = (): string => {
        const state = props.border.getDockState();
        const loc = props.border.getLocation();
        if (state === "hidden") {
            if (loc === DockLocation.LEFT) return "▶";
            if (loc === DockLocation.RIGHT) return "◀";
            if (loc === DockLocation.TOP) return "▼";
            return "▲";
        }
        if (loc === DockLocation.LEFT) return "◀";
        if (loc === DockLocation.RIGHT) return "▶";
        if (loc === DockLocation.TOP) return "▲";
        return "▼";
    };

    const dockTitle = (): string => {
        const state = props.border.getDockState();
        if (state === "expanded") return "Collapse";
        if (state === "collapsed") return "Hide";
        return "Expand";
    };

    const expandedTabButtons = (): JSX.Element[] => {
        const buttons: JSX.Element[] = [];
        const children = props.border.getChildren();
        for (let i = 0; i < children.length; i++) {
            const isSelected = props.border.getSelected() === i;
            const child = children[i] as TabNode;
            buttons.push(
                <BorderButton
                    layout={props.layout}
                    border={props.border.getLocation().getName()}
                    node={child}
                    path={props.border.getPath() + "/tb" + i}
                    selected={isSelected}
                />,
            );
            if (i < children.length - 1) {
                buttons.push(
                    <div class={cm(CLASSES.FLEXLAYOUT__BORDER_TAB_DIVIDER)} />,
                );
            }
        }
        return buttons;
    };

    const expandedToolbarButtons = (): JSX.Element[] => {
        const buttons: JSX.Element[] = [];
        let stickyButtons: JSX.Element[] = [];
        const renderState: ITabSetRenderValues = {
            leading: undefined,
            buttons,
            stickyButtons,
            overflowPosition: undefined,
        };
        props.layout.customizeTabSet(props.border, renderState);

        if (props.border.isEnableDock()) {
            renderState.buttons.push(
                <button
                    data-layout-path={props.border.getPath() + "/button/dock"}
                    title={dockTitle()}
                    class={cm(CLASSES.FLEXLAYOUT__BORDER_DOCK_BUTTON)}
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={onDockToggle}
                >
                    {dockIcon()}
                </button>,
            );
        }

        return renderState.buttons;
    };

    const tabBar = () => (
        <div
            class={cm(CLASSES.FLEXLAYOUT__BORDER_TABBAR)}
            data-border-tabbar
        >
            <div style={{ display: "flex", flex: "1", "overflow-x": "auto", "align-items": "center", "padding-left": "4px" }}>
                {expandedTabButtons()}
            </div>
            <div style={{ display: "flex", "align-items": "center", "padding": "0 4px" }}>
                {expandedToolbarButtons()}
            </div>
        </div>
    );

    const flatItems = createMemo(() => {
        void props.layout.getRevision();
        const nodes = visibleNodes();
        const items: Array<
            | { type: "tile"; node: TabNode; index: number }
            | { type: "splitter"; index: number }
        > = [];
        for (let i = 0; i < nodes.length; i++) {
            if (i > 0) {
                items.push({ type: "splitter", index: i - 1 });
            }
            items.push({ type: "tile", node: nodes[i], index: i });
        }
        return items;
    });

    const contentArea = () => {
        const expandedWrapperStyle = (): Record<string, any> => {
            void props.layout.getRevision();
            const s: Record<string, any> = {
                display: isContentVisible() ? "flex" : "none",
                "flex-direction": "column",
            };
            if (props.border.getOrientation() === Orientation.HORZ) {
                s.width = props.border.getSize() + "px";
                s["min-width"] = props.border.getMinSize() + "px";
                s["max-width"] = props.border.getMaxSize() + "px";
            } else {
                s.height = props.border.getSize() + "px";
                s["min-height"] = props.border.getMinSize() + "px";
                s["max-height"] = props.border.getMaxSize() + "px";
            }
            return s;
        };

        if (isExpanded()) {
            return (
                <div style={expandedWrapperStyle()} data-border-content>
                    {tabBar()}
                    <div
                        ref={selfRef}
                        style={{ ...style(), display: isContentVisible() ? "flex" : "none", flex: "1", width: "auto", height: "auto", "min-width": "0", "min-height": "0", "max-width": "none", "max-height": "none" }}
                        class={className}
                    >
                        <For each={flatItems()}>
                            {(item) => {
                                if (item.type === "splitter") {
                                    const splitterIdx = item.index;
                                    return (
                                        <div
                                            class={
                                                cm(CLASSES.FLEXLAYOUT__SPLITTER) +
                                                " " +
                                                cm(CLASSES.FLEXLAYOUT__SPLITTER_ + (tileHorizontal() ? "horz" : "vert"))
                                            }
                                            style={internalSplitterStyle()}
                                            data-border-tile-splitter={splitterIdx}
                                            onPointerDown={(e: PointerEvent) => onSplitterPointerDown(splitterIdx, e)}
                                        />
                                    );
                                } else {
                                    const tileIdx = item.index;
                                    return (
                                        <div
                                            class={cm(CLASSES.FLEXLAYOUT__TAB) + " " + cm(CLASSES.FLEXLAYOUT__TAB_BORDER)}
                                            style={tileStyle(tileIdx)}
                                            data-border-tile={tileIdx}
                                        >
                                            {props.layout.factory(item.node)}
                                        </div>
                                    );
                                }
                            }}
                        </For>
                    </div>
                </div>
            );
        }

        return (
            <div ref={selfRef} style={style()} class={className} data-border-content>
                <For each={flatItems()}>
                    {(item) => {
                        if (item.type === "splitter") {
                            const splitterIdx = item.index;
                            return (
                                <div
                                    class={
                                        cm(CLASSES.FLEXLAYOUT__SPLITTER) +
                                        " " +
                                        cm(CLASSES.FLEXLAYOUT__SPLITTER_ + (tileHorizontal() ? "horz" : "vert"))
                                    }
                                    style={internalSplitterStyle()}
                                    data-border-tile-splitter={splitterIdx}
                                    onPointerDown={(e: PointerEvent) => onSplitterPointerDown(splitterIdx, e)}
                                />
                            );
                        } else {
                            const tileIdx = item.index;
                            return (
                                <div
                                    class={cm(CLASSES.FLEXLAYOUT__TAB) + " " + cm(CLASSES.FLEXLAYOUT__TAB_BORDER)}
                                    style={tileStyle(tileIdx)}
                                    data-border-tile={tileIdx}
                                >
                                    {props.layout.factory(item.node)}
                                </div>
                            );
                        }
                    }}
                </For>
            </div>
        );
    };

    const edgeSplitter = () =>
        isContentVisible() ? (
            <Splitter layout={props.layout} node={props.border as any} index={0} horizontal={horizontal()} />
        ) : null;

    if (isBeforeSplitter) {
        return (
            <>
                {contentArea()}
                {edgeSplitter()}
            </>
        );
    } else {
        return (
            <>
                {edgeSplitter()}
                {contentArea()}
            </>
        );
    }
};
