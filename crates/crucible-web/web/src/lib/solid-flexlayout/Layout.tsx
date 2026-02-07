import { Component, JSX, createSignal, createEffect, onMount, onCleanup, createMemo, Show, For } from "solid-js";
import { Model } from "../flexlayout/model/Model";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { RowNode } from "../flexlayout/model/RowNode";
import { Node } from "../flexlayout/model/Node";
import { Rect } from "../flexlayout/core/Rect";
import { CLASSES } from "../flexlayout/core/Types";
import { DropInfo } from "../flexlayout/core/DropInfo";
import { LayoutEngine } from "../flexlayout/layout/LayoutEngine";
import { Action } from "../flexlayout/model/Action";
import { Row } from "./Row";

export interface ILayoutProps {
    /** The model for this layout */
    model: Model;
    /** Factory function for creating tab content components */
    factory: (node: TabNode) => JSX.Element;
    /** Function called whenever the layout generates an action */
    onAction?: (action: any) => any | undefined;
    /** Function called when model has changed */
    onModelChange?: (model: Model, action: any) => void;
    /** Function called when rendering a tab, allows customization */
    onRenderTab?: (node: TabNode, renderValues: ITabRenderValues) => void;
    /** Function called when rendering a tabset, allows customization */
    onRenderTabSet?: (
        tabSetNode: TabSetNode | BorderNode,
        renderValues: ITabSetRenderValues,
    ) => void;
    /** Function for CSS class name mapping (e.g. CSS modules) */
    classNameMapper?: (defaultClassName: string) => string;
}

export interface ITabSetRenderValues {
    leading: JSX.Element | undefined;
    stickyButtons: JSX.Element[];
    buttons: JSX.Element[];
    overflowPosition: number | undefined;
}

export interface ITabRenderValues {
    leading: JSX.Element | undefined;
    content: JSX.Element | undefined;
    buttons: JSX.Element[];
}

export const Layout: Component<ILayoutProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;
    let mainRef: HTMLDivElement | undefined;

    const [rect, setRect] = createSignal<Rect>(Rect.empty());
    const [revision, setRevision] = createSignal(0);
    const [layoutVersion, setLayoutVersion] = createSignal(0);
    const [, setShowEdges] = createSignal(false);
    const [showOverlay, setShowOverlay] = createSignal(false);

    let dropInfo: DropInfo | undefined;
    let outlineDiv: HTMLDivElement | undefined;
    let dragEnterCount = 0;
    let dragging = false;

    const layoutContext = createMemo(() => ({
        model: props.model,
        factory: props.factory,
        getClassName,
        doAction,
        customizeTab,
        customizeTabSet,
        getRootDiv: () => selfRef,
        getMainElement: () => mainRef,
        getDomRect,
        getBoundingClientRect: getBoundingClientRectFn,
        getWindowId: () => Model.MAIN_WINDOW_ID,
        setEditingTab: (_tab?: TabNode) => {},
        getEditingTab: () => undefined as TabNode | undefined,
        isRealtimeResize: () => false,
        redraw,
        setDragNode,
        clearDragMain,
        getRevision: () => layoutVersion(),
    }));

    onMount(() => {
        updateRect();

        const observer = new ResizeObserver(() => {
            requestAnimationFrame(updateRect);
        });
        if (selfRef) {
            observer.observe(selfRef);
        }

        onCleanup(() => {
            if (selfRef) observer.unobserve(selfRef);
            observer.disconnect();
        });
    });

    createEffect(() => {
        void revision();
        const _rect = rect();

        if (_rect.width > 0 && _rect.height > 0) {
            const model = props.model;
            const root = model.getRoot();
            if (root) {
                (root as RowNode).calcMinMaxSize();
                root.setPaths("");
                model.getBorderSet().setPaths();
                LayoutEngine.calculateLayout(root, _rect);
                setLayoutVersion((v) => v + 1);
            }
        }
    });

    function updateRect() {
        if (selfRef) {
            const domRect = selfRef.getBoundingClientRect();
            const newRect = new Rect(0, 0, domRect.width, domRect.height);
            if (!newRect.equals(rect()) && newRect.width > 0 && newRect.height > 0) {
                setRect(newRect);
            }
        }
    }

    function getClassName(defaultClassName: string): string {
        if (props.classNameMapper) {
            return props.classNameMapper(defaultClassName);
        }
        return defaultClassName;
    }

    function doAction(action: any): Node | undefined {
        if (props.onAction) {
            const outcome = props.onAction(action);
            if (outcome !== undefined) {
                props.model.doAction(outcome);
                redraw();
                if (props.onModelChange) {
                    props.onModelChange(props.model, outcome);
                }
                return undefined;
            }
            return undefined;
        } else {
            props.model.doAction(action);
            redraw();
            if (props.onModelChange) {
                props.onModelChange(props.model, action);
            }
            return undefined;
        }
    }

    function redraw() {
        setRevision((r) => r + 1);
    }

    function customizeTab(tabNode: TabNode, renderValues: ITabRenderValues) {
        if (props.onRenderTab) {
            props.onRenderTab(tabNode, renderValues);
        }
    }

    function customizeTabSet(
        tabSetNode: TabSetNode | BorderNode,
        renderValues: ITabSetRenderValues,
    ) {
        if (props.onRenderTabSet) {
            props.onRenderTabSet(tabSetNode, renderValues);
        }
    }

    function getDomRect(): Rect {
        if (selfRef) {
            const r = selfRef.getBoundingClientRect();
            return new Rect(r.x, r.y, r.width, r.height);
        }
        return Rect.empty();
    }

    function getBoundingClientRectFn(div: HTMLElement): Rect {
        const layoutRect = getDomRect();
        if (layoutRect) {
            const divRect = div.getBoundingClientRect();
            return new Rect(
                divRect.x - layoutRect.x,
                divRect.y - layoutRect.y,
                divRect.width,
                divRect.height,
            );
        }
        return Rect.empty();
    }

    function setDragNode(event: DragEvent, node: Node) {
            event.dataTransfer!.setData("text/plain", "--flexlayout--");
        event.dataTransfer!.effectAllowed = "copyMove";
        event.dataTransfer!.dropEffect = "move";
        dragEnterCount = 0;
            (selfRef as any).__dragNode = node;
    }

    function clearDragMain() {
        (selfRef as any).__dragNode = undefined;
        setShowEdges(false);
        setShowOverlay(false);
        dragEnterCount = 0;
        dragging = false;
        if (outlineDiv && selfRef) {
            selfRef.removeChild(outlineDiv);
            outlineDiv = undefined;
        }
    }

    function onDragEnterRaw(event: DragEvent) {
        dragEnterCount++;
        if (dragEnterCount === 1) {
            onDragEnter(event);
        }
    }

    function onDragLeaveRaw(_event: DragEvent) {
        dragEnterCount--;
        if (dragEnterCount === 0) {
            clearDragMain();
        }
    }

    function onDragEnter(event: DragEvent) {
        const dragNode = (selfRef as any)?.__dragNode;
        if (dragNode) {
            event.preventDefault();
            dropInfo = undefined;
            outlineDiv = document.createElement("div");
            outlineDiv.className = getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT);
            outlineDiv.style.visibility = "hidden";
            const speed = props.model.getAttribute("tabDragSpeed") as number || 0.3;
            outlineDiv.style.transition = `top ${speed}s, left ${speed}s, width ${speed}s, height ${speed}s`;
            selfRef!.appendChild(outlineDiv);
            dragging = true;
            setShowOverlay(true);
            if (props.model.getMaximizedTabset(Model.MAIN_WINDOW_ID) === undefined) {
                setShowEdges(props.model.isEnableEdgeDock());
            }
        }
    }

    function onDragOver(event: DragEvent) {
        if (dragging) {
            event.preventDefault();
            const clientRect = selfRef?.getBoundingClientRect();
            const pos = {
                x: event.clientX - (clientRect?.left ?? 0),
                y: event.clientY - (clientRect?.top ?? 0),
            };

            const dragNode = (selfRef as any)?.__dragNode;
            if (dragNode) {
                const root = props.model.getRoot();
                if (root) {
                    const di = root.findDropTargetNode(
                        Model.MAIN_WINDOW_ID,
                        dragNode,
                        pos.x,
                        pos.y,
                    );
                    if (di) {
                        dropInfo = di;
                        if (outlineDiv) {
                            outlineDiv.className = getClassName(di.className);
                            di.rect.positionElement(outlineDiv);
                            outlineDiv.style.visibility = "visible";
                        }
                    }
                }
            }
        }
    }

    function onDrop(event: DragEvent) {
        if (dragging) {
            event.preventDefault();
            const dragNode = (selfRef as any)?.__dragNode;
            if (dropInfo && dragNode) {
                if (dragNode instanceof TabNode || dragNode instanceof TabSetNode) {
                    doAction(
                        Action.moveNode(
                            dragNode.getId(),
                            dropInfo.node.getId(),
                            dropInfo.location.getName(),
                            dropInfo.index,
                        ),
                    );
                }
            }
            clearDragMain();
        }
        dragEnterCount = 0;
    }

    const allTabNodes = createMemo(() => {
        void revision();  // Track model changes
        void layoutVersion();  // Track layout changes
        const tabs: Array<{node: TabNode, parent: TabSetNode | BorderNode}> = [];
        
        // Walk root tree
        const visitNode = (n: Node) => {
            if (n instanceof TabNode) {
                tabs.push({node: n, parent: n.getParent() as TabSetNode | BorderNode});
            }
            for (const child of n.getChildren()) {
                visitNode(child);
            }
        };
        
        const root = props.model.getRoot();
        if (root) visitNode(root);
        
        // Walk borders
        for (const border of props.model.getBorderSet().getBorders()) {
            for (const child of border.getChildren()) {
                if (child instanceof TabNode) {
                    tabs.push({node: child, parent: border});
                }
            }
        }
        
        return tabs;
    });

    return (
        <div
            ref={selfRef}
            class={getClassName(CLASSES.FLEXLAYOUT__LAYOUT)}
            data-layout-path="/"
            onDragEnter={onDragEnterRaw}
            onDragLeave={onDragLeaveRaw}
            onDragOver={onDragOver}
            onDrop={onDrop}
            style={{ position: "relative", overflow: "hidden" }}
        >
            {showOverlay() && (
                <div
                    class={getClassName(CLASSES.FLEXLAYOUT__LAYOUT_OVERLAY)}
                    style={{ position: "absolute", inset: 0, "z-index": 1000 }}
                />
            )}

            <div
                ref={mainRef}
                class={getClassName(CLASSES.FLEXLAYOUT__LAYOUT_MAIN)}
                data-layout-path="/main"
                style={{
                    position: "absolute",
                    top: "0",
                    left: "0",
                    bottom: "0",
                    right: "0",
                    display: "flex",
                }}
            >
                <Show when={rect().width > 0 && props.model.getRoot()}>
                    <Row layout={layoutContext()} node={props.model.getRoot() as RowNode} />
                </Show>
            </div>

            <Show when={rect().width > 0}>
                <For each={allTabNodes()}>
                    {(tabEntry) => {
                        const parent = tabEntry.parent;
                        const contentRect = parent instanceof TabSetNode
                            ? parent.getContentRect()
                            : parent.getRect();
                        const style: Record<string, any> = {};
                        contentRect.styleWithPosition(style);
                        if (!tabEntry.node.isSelected()) {
                            style.display = "none";
                        }
                        return (
                            <div
                                class={getClassName(CLASSES.FLEXLAYOUT__TAB)}
                                data-layout-path={tabEntry.node.getPath()}
                                style={style}
                                onPointerDown={() => {
                                    const p = tabEntry.node.getParent();
                                    if (p instanceof TabSetNode) {
                                        if (!p.isActive()) {
                                            doAction(
                                                Action.setActiveTabset(p.getId(), Model.MAIN_WINDOW_ID),
                                            );
                                        }
                                    }
                                }}
                            >
                                {props.factory(tabEntry.node)}
                            </div>
                        );
                    }}
                </For>
            </Show>
        </div>
    );


};

/** Layout context type passed to child components */
export interface ILayoutContext {
    model: Model;
    factory: (node: TabNode) => JSX.Element;
    getClassName: (defaultClassName: string) => string;
    doAction: (action: any) => Node | undefined;
    customizeTab: (tabNode: TabNode, renderValues: ITabRenderValues) => void;
    customizeTabSet: (
        tabSetNode: TabSetNode | BorderNode,
        renderValues: ITabSetRenderValues,
    ) => void;
    getRootDiv: () => HTMLDivElement | undefined;
    getMainElement: () => HTMLDivElement | undefined;
    getDomRect: () => Rect;
    getBoundingClientRect: (div: HTMLElement) => Rect;
    getWindowId: () => string;
    setEditingTab: (tab?: TabNode) => void;
    getEditingTab: () => TabNode | undefined;
    isRealtimeResize: () => boolean;
    redraw: () => void;
    setDragNode: (event: DragEvent, node: Node) => void;
    clearDragMain: () => void;
    getRevision: () => number;
}
