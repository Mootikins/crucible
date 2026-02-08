import { Component, createMemo, For, Show } from "solid-js";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { LayoutWindow } from "../flexlayout/model/LayoutWindow";
import { Rect } from "../flexlayout/core/Rect";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import { Row } from "./Row";
import type { ILayoutContext } from "./Layout";
import { RowNode } from "../flexlayout/model/RowNode";

export interface IFloatingPanelProps {
    layoutWindow: LayoutWindow;
    layoutContext: ILayoutContext;
    onBringToFront: (windowId: string) => void;
    onContentRef?: (el: HTMLDivElement) => void;
    zIndex?: number;
}

export const FloatingPanel: Component<IFloatingPanelProps> = (props) => {
    let panelRef: HTMLDivElement | undefined;

    const rect = (): Rect => {
        void props.layoutContext.getRevision();
        return props.layoutWindow.rect;
    };

    const tabNodes = createMemo(() => {
        void props.layoutContext.getRevision();
        const tabs: Array<{ node: TabNode; parent: TabSetNode | BorderNode }> = [];
        const root = props.layoutWindow.root;
        if (!root) return tabs;

        const visitNode = (n: any) => {
            if (n instanceof TabNode) {
                tabs.push({ node: n, parent: n.getParent() as TabSetNode | BorderNode });
            }
            for (const child of n.getChildren()) {
                visitNode(child);
            }
        };
        visitNode(root);
        return tabs;
    });

    let dragStartPos = { x: 0, y: 0, rectX: 0, rectY: 0 };

    function createOutlineDiv(r: Rect): HTMLDivElement {
        const div = document.createElement("div");
        div.className = props.layoutContext.getClassName(CLASSES.FLEXLAYOUT__FLOATING_PANEL_OUTLINE);
        div.style.left = r.x + "px";
        div.style.top = r.y + "px";
        div.style.width = r.width + "px";
        div.style.height = r.height + "px";
        return div;
    }

    const onFloatDragStart = (e: PointerEvent) => {
        e.preventDefault();
        props.onBringToFront(props.layoutWindow.windowId);

        const r = rect();
        dragStartPos = { x: e.clientX, y: e.clientY, rectX: r.x, rectY: r.y };
        const isRealtime = props.layoutContext.isRealtimeResize();

        let outline: HTMLDivElement | undefined;
        if (!isRealtime) {
            const layoutRoot = props.layoutContext.getLayoutRootDiv();
            if (layoutRoot) {
                outline = createOutlineDiv(r);
                layoutRoot.appendChild(outline);
            }
        }

        const onMove = (ev: PointerEvent) => {
            const dx = ev.clientX - dragStartPos.x;
            const dy = ev.clientY - dragStartPos.y;
            const newX = dragStartPos.rectX + dx;
            const newY = dragStartPos.rectY + dy;

            if (isRealtime) {
                const r = rect();
                props.layoutContext.doAction(
                    Action.moveWindow(
                        props.layoutWindow.windowId,
                        newX,
                        newY,
                        r.width,
                        r.height,
                    ),
                );
            } else if (outline) {
                outline.style.left = newX + "px";
                outline.style.top = newY + "px";
            }
        };

        const onUp = () => {
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);

            if (!isRealtime && outline) {
                const r = rect();
                props.layoutContext.doAction(
                    Action.moveWindow(
                        props.layoutWindow.windowId,
                        parseFloat(outline.style.left),
                        parseFloat(outline.style.top),
                        r.width,
                        r.height,
                    ),
                );
                const layoutRoot = props.layoutContext.getLayoutRootDiv();
                if (layoutRoot) layoutRoot.removeChild(outline);
            }
        };

        document.addEventListener("pointermove", onMove);
        document.addEventListener("pointerup", onUp);
    };

    const onFloatDock = () => {
        const root = props.layoutWindow.root;
        if (!root) return;
        const children = root.getChildren();
        if (children.length > 0 && children[0] instanceof TabSetNode) {
            props.layoutContext.doAction(
                Action.dockTabset(children[0].getId(), "center"),
            );
        }
    };

    const onFloatClose = () => {
        props.layoutContext.doAction(
            Action.closeWindow(props.layoutWindow.windowId),
        );
    };

    props.layoutContext.onFloatDragStart = onFloatDragStart;
    props.layoutContext.onFloatDock = onFloatDock;
    props.layoutContext.onFloatClose = onFloatClose;

    const MIN_WIDTH = 150;
    const MIN_HEIGHT = 80;

    type ResizeEdge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

    let resizeStart = { x: 0, y: 0, rectX: 0, rectY: 0, width: 0, height: 0 };

    function computeResizedRect(
        dx: number,
        dy: number,
        resizesLeft: boolean,
        resizesRight: boolean,
        resizesTop: boolean,
        resizesBottom: boolean,
    ): { x: number; y: number; w: number; h: number } {
        let newX = resizeStart.rectX;
        let newY = resizeStart.rectY;
        let newWidth = resizeStart.width;
        let newHeight = resizeStart.height;

        if (resizesRight) {
            newWidth = Math.max(MIN_WIDTH, resizeStart.width + dx);
        }
        if (resizesBottom) {
            newHeight = Math.max(MIN_HEIGHT, resizeStart.height + dy);
        }
        if (resizesLeft) {
            const proposedWidth = resizeStart.width - dx;
            if (proposedWidth >= MIN_WIDTH) {
                newWidth = proposedWidth;
                newX = resizeStart.rectX + dx;
            } else {
                newWidth = MIN_WIDTH;
                newX = resizeStart.rectX + (resizeStart.width - MIN_WIDTH);
            }
        }
        if (resizesTop) {
            const proposedHeight = resizeStart.height - dy;
            if (proposedHeight >= MIN_HEIGHT) {
                newHeight = proposedHeight;
                newY = resizeStart.rectY + dy;
            } else {
                newHeight = MIN_HEIGHT;
                newY = resizeStart.rectY + (resizeStart.height - MIN_HEIGHT);
            }
        }
        return { x: newX, y: newY, w: newWidth, h: newHeight };
    }

    const onEdgeResizePointerDown = (edge: ResizeEdge, e: PointerEvent) => {
        e.preventDefault();
        e.stopPropagation();
        props.onBringToFront(props.layoutWindow.windowId);

        const r = rect();
        resizeStart = {
            x: e.clientX,
            y: e.clientY,
            rectX: r.x,
            rectY: r.y,
            width: r.width,
            height: r.height,
        };

        const resizesLeft = edge === "w" || edge === "nw" || edge === "sw";
        const resizesRight = edge === "e" || edge === "ne" || edge === "se";
        const resizesTop = edge === "n" || edge === "nw" || edge === "ne";
        const resizesBottom = edge === "s" || edge === "sw" || edge === "se";
        const isRealtime = props.layoutContext.isRealtimeResize();

        let outline: HTMLDivElement | undefined;
        if (!isRealtime) {
            const layoutRoot = props.layoutContext.getLayoutRootDiv();
            if (layoutRoot) {
                outline = createOutlineDiv(r);
                layoutRoot.appendChild(outline);
            }
        }

        const onMove = (ev: PointerEvent) => {
            const dx = ev.clientX - resizeStart.x;
            const dy = ev.clientY - resizeStart.y;
            const newRect = computeResizedRect(dx, dy, resizesLeft, resizesRight, resizesTop, resizesBottom);

            if (isRealtime) {
                props.layoutContext.doAction(
                    Action.moveWindow(props.layoutWindow.windowId, newRect.x, newRect.y, newRect.w, newRect.h),
                );
            } else if (outline) {
                outline.style.left = newRect.x + "px";
                outline.style.top = newRect.y + "px";
                outline.style.width = newRect.w + "px";
                outline.style.height = newRect.h + "px";
            }
        };

        const onUp = () => {
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);

            if (!isRealtime && outline) {
                props.layoutContext.doAction(
                    Action.moveWindow(
                        props.layoutWindow.windowId,
                        parseFloat(outline.style.left),
                        parseFloat(outline.style.top),
                        parseFloat(outline.style.width),
                        parseFloat(outline.style.height),
                    ),
                );
                const layoutRoot = props.layoutContext.getLayoutRootDiv();
                if (layoutRoot) layoutRoot.removeChild(outline);
            }
        };

        document.addEventListener("pointermove", onMove);
        document.addEventListener("pointerup", onUp);
    };

    const onPanelPointerDown = () => {
        props.onBringToFront(props.layoutWindow.windowId);
    };

    const cm = props.layoutContext.getClassName;

    return (
        <div
            ref={panelRef}
            class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL)}
            data-window-id={props.layoutWindow.windowId}
            style={{
                left: `${rect().x}px`,
                top: `${rect().y}px`,
                width: `${rect().width}px`,
                height: `${rect().height}px`,
                "z-index": props.zIndex ?? 1000,
            }}
            onPointerDown={onPanelPointerDown}
        >
            <div
                ref={(el: HTMLDivElement) => {
                    if (props.onContentRef) props.onContentRef(el);
                }}
                class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_CONTENT)}
            >
                <Show when={props.layoutWindow.root}>
                    <Row
                        layout={props.layoutContext}
                        node={props.layoutWindow.root as RowNode}
                    />
                </Show>

                <For each={tabNodes()}>
                    {(tabEntry) => {
                        const tabStyle = (): Record<string, any> => {
                            const parent = tabEntry.parent;
                            const contentRect = parent.getContentRect();
                            const s: Record<string, any> = {};
                            if (contentRect.width > 0 && contentRect.height > 0) {
                                contentRect.styleWithPosition(s);
                            } else {
                                s.display = "none";
                            }
                            if (!tabEntry.node.isSelected()) {
                                s.display = "none";
                            }
                            return s;
                        };
                        return (
                            <div
                                class={cm(CLASSES.FLEXLAYOUT__TAB)}
                                data-layout-path={tabEntry.node.getPath()}
                                style={tabStyle()}
                                onPointerDown={() => {
                                    const p = tabEntry.node.getParent();
                                    if (p instanceof TabSetNode) {
                                        if (!p.isActive()) {
                                            props.layoutContext.doAction(
                                                Action.setActiveTabset(
                                                    p.getId(),
                                                    props.layoutWindow.windowId,
                                                ),
                                            );
                                        }
                                    }
                                }}
                            >
                                {props.layoutContext.factory(tabEntry.node)}
                            </div>
                        );
                    }}
                </For>
            </div>

            <div
                class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_N)}
                onPointerDown={(e) => onEdgeResizePointerDown("n", e)}
            />
            <div
                class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_S)}
                onPointerDown={(e) => onEdgeResizePointerDown("s", e)}
            />
            <div
                class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_E)}
                onPointerDown={(e) => onEdgeResizePointerDown("e", e)}
            />
            <div
                class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_W)}
                onPointerDown={(e) => onEdgeResizePointerDown("w", e)}
            />
            <div
                class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_NW)}
                onPointerDown={(e) => onEdgeResizePointerDown("nw", e)}
            />
            <div
                class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_NE)}
                onPointerDown={(e) => onEdgeResizePointerDown("ne", e)}
            />
            <div
                class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_SW)}
                onPointerDown={(e) => onEdgeResizePointerDown("sw", e)}
            />
            <div
                class={cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_SE) + " " + cm(CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_HANDLE)}
                onPointerDown={(e) => onEdgeResizePointerDown("se", e)}
            />
        </div>
    );
};
