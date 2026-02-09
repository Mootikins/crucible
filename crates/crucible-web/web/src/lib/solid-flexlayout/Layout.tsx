import { Component, JSX, createSignal, createEffect, onMount, onCleanup, createMemo, Show, For } from "solid-js";
import { Portal } from "solid-js/web";
import { Model } from "../flexlayout/model/Model";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { RowNode } from "../flexlayout/model/RowNode";
import { Node } from "../flexlayout/model/Node";
import { Rect } from "../flexlayout/core/Rect";
import { CLASSES } from "../flexlayout/core/Types";
import { DockLocation } from "../flexlayout/core/DockLocation";
import { DropInfo } from "../flexlayout/core/DropInfo";
import { LayoutEngine } from "../flexlayout/layout/LayoutEngine";
import { Action } from "../flexlayout/model/Action";
import { Row } from "./Row";
import { BorderTabSet } from "./BorderTabSet";
import { BorderTab } from "./BorderTab";
import { FloatingPanel } from "./FloatingPanel";

const LOCATION_TIE_ORDER: Record<string, number> = { top: 0, right: 1, bottom: 2, left: 3 };

export function computeNestingOrder(borders: BorderNode[]): BorderNode[] {
    return [...borders].sort((a, b) => {
        const priorityDiff = b.getPriority() - a.getPriority();
        if (priorityDiff !== 0) return priorityDiff;
        return (LOCATION_TIE_ORDER[a.getLocation().getName()] ?? 4)
             - (LOCATION_TIE_ORDER[b.getLocation().getName()] ?? 4);
    });
}

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

function createWindowContext(
    windowId: string,
    baseContext: ILayoutContext,
    containerRef: () => HTMLDivElement | undefined,
): ILayoutContext {
    return {
        ...baseContext,
        getWindowId: () => windowId,
        getRootDiv: containerRef,
        getMainElement: containerRef,
        getDomRect: () => {
            const el = containerRef();
            if (el) {
                const r = el.getBoundingClientRect();
                return new Rect(r.x, r.y, r.width, r.height);
            }
            return Rect.empty();
        },
        getBoundingClientRect: (div: HTMLElement) => {
            const el = containerRef();
            if (el) {
                const containerRect = el.getBoundingClientRect();
                const divRect = div.getBoundingClientRect();
                return new Rect(
                    divRect.x - containerRect.x,
                    divRect.y - containerRect.y,
                    divRect.width,
                    divRect.height,
                );
            }
            return Rect.empty();
        },
    };
}

export const Layout: Component<ILayoutProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;
    let mainRef: HTMLDivElement | undefined;

    const [rect, setRect] = createSignal<Rect>(Rect.empty());
    const [revision, setRevision] = createSignal(0);
    const [layoutVersion, setLayoutVersion] = createSignal(0);
    const [showEdges, setShowEdges] = createSignal(false);
    const [showOverlay, setShowOverlay] = createSignal(false);
    const [floatZOrder, setFloatZOrder] = createSignal<string[]>([]);

    const [editingTab, setEditingTab] = createSignal<TabNode | undefined>(undefined);
    const [showHiddenBorder, setShowHiddenBorder] = createSignal<DockLocation>(DockLocation.CENTER);
    const [popupMenu, setPopupMenu] = createSignal<{
        items: { index: number; node: TabNode }[];
        onSelect: (item: { index: number; node: TabNode }) => void;
        position: { left?: string; right?: string; top?: string; bottom?: string };
        parentNode: TabSetNode | BorderNode;
    } | undefined>(undefined);

    const [contextMenu, setContextMenu] = createSignal<{
        items: { label: string; action: () => void }[];
        position: { x: number; y: number };
    } | undefined>(undefined);

    let dropInfo: DropInfo | undefined;
    let outlineDiv: HTMLDivElement | undefined;
    let dragEnterCount = 0;
    let dragging = false;
    let popupCleanup: (() => void) | undefined;
    let contextMenuCleanup: (() => void) | undefined;

    // Stable object — NOT a createMemo. A createMemo recreates the object on any
    // signal change (including editingTab), which destroys all child components
    // that receive it as props. Closures capture the signals reactively.
    const layoutContextObj: ILayoutContext = {
        get model() { return props.model; },
        get factory() { return props.factory; },
        getClassName,
        doAction,
        customizeTab,
        customizeTabSet,
        getRootDiv: () => selfRef,
        getMainElement: () => mainRef,
        getDomRect,
        getBoundingClientRect: getBoundingClientRectFn,
        getWindowId: () => Model.MAIN_WINDOW_ID,
        setEditingTab: (tab?: TabNode) => setEditingTab(tab),
        getEditingTab: () => editingTab(),
        isRealtimeResize: () => props.model.isRealtimeResize(),
        getLayoutRootDiv: () => selfRef,
        redraw,
        setDragNode,
        clearDragMain,
        getRevision: () => layoutVersion(),
        showPopup,
        showContextMenu,
    };
    const layoutContext = () => layoutContextObj;

    onMount(() => {
        updateRect();

        // Initialize floatZOrder from model
        const savedZOrder = props.model.getFloatZOrder();
        if (savedZOrder.length > 0) {
            setFloatZOrder(savedZOrder);
        }

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

    onCleanup(() => {
        if (popupCleanup) popupCleanup();
        if (contextMenuCleanup) contextMenuCleanup();
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

                for (const [wid, lw] of model.getwindowsMap()) {
                    if (wid !== Model.MAIN_WINDOW_ID && lw.root) {
                        const innerRect = new Rect(0, 0, lw.rect.width, lw.rect.height);
                        (lw.root as RowNode).calcMinMaxSize();
                        lw.root.setPaths(`/window/${wid}`);
                        LayoutEngine.calculateLayout(lw.root, innerRect);
                    }
                }

                setLayoutVersion((v) => v + 1);
            }
        }
    });

    createEffect(() => {
        void layoutVersion();
        if (mainRef) {
            const mainDomRect = mainRef.getBoundingClientRect();
            if (mainDomRect.width > 0 && mainDomRect.height > 0) {
                const mainRect = new Rect(0, 0, mainDomRect.width, mainDomRect.height);
                const root = props.model.getRoot();
                if (root) {
                    const currentRect = root.getRect();
                    if (!currentRect.equalSize(mainRect)) {
                        LayoutEngine.calculateLayout(root, mainRect);
                        setLayoutVersion((v) => v + 1);
                    }
                }
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

    function showPopup(
        triggerElement: HTMLElement,
        parentNode: TabSetNode | BorderNode,
        items: { index: number; node: TabNode }[],
        onSelect: (item: { index: number; node: TabNode }) => void,
    ) {
        const layoutRect = selfRef?.getBoundingClientRect();
        const triggerRect = triggerElement.getBoundingClientRect();
        if (!layoutRect) return;

        const position: { left?: string; right?: string; top?: string; bottom?: string } = {};
        if (triggerRect.left < layoutRect.left + layoutRect.width / 2) {
            position.left = (triggerRect.left - layoutRect.left) + "px";
        } else {
            position.right = (layoutRect.right - triggerRect.right) + "px";
        }
        if (triggerRect.top < layoutRect.top + layoutRect.height / 2) {
            position.top = (triggerRect.top - layoutRect.top) + "px";
        } else {
            position.bottom = (layoutRect.bottom - triggerRect.bottom) + "px";
        }

        if (popupCleanup) popupCleanup();

        setPopupMenu({ items, onSelect, position, parentNode });

        const onDocPointerDown = (e: PointerEvent) => {
            const popupEl = selfRef?.querySelector('[data-layout-path="/popup-menu"]');
            if (popupEl && popupEl.contains(e.target as globalThis.Node)) {
                return;
            }
            cleanup();
        };
        const onDocKeyDown = (e: KeyboardEvent) => {
            if (e.key === "Escape") cleanup();
        };
        const cleanup = () => {
            hidePopup();
            document.removeEventListener("pointerdown", onDocPointerDown);
            document.removeEventListener("keydown", onDocKeyDown);
            popupCleanup = undefined;
        };
        popupCleanup = cleanup;
        // keydown can be registered immediately — only pointerdown needs
        // the rAF delay to prevent the opening click from closing the popup.
        document.addEventListener("keydown", onDocKeyDown);
        requestAnimationFrame(() => {
            document.addEventListener("pointerdown", onDocPointerDown);
        });
    }

    function hidePopup() {
        setPopupMenu(undefined);
    }

    function showContextMenu(
        event: MouseEvent,
        items: { label: string; action: () => void }[],
    ) {
        event.preventDefault();
        event.stopPropagation();

        if (contextMenuCleanup) contextMenuCleanup();

        const layoutRect = selfRef?.getBoundingClientRect();
        if (!layoutRect) return;

        setContextMenu({
            items,
            position: {
                x: event.clientX - layoutRect.left,
                y: event.clientY - layoutRect.top,
            },
        });

        const onDocPointerDown = (e: PointerEvent) => {
            const menuEl = selfRef?.querySelector('[data-layout-path="/context-menu"]');
            if (menuEl && menuEl.contains(e.target as globalThis.Node)) return;
            cleanup();
        };
        const onDocKeyDown = (e: KeyboardEvent) => {
            if (e.key === "Escape") cleanup();
        };
        const cleanup = () => {
            hideContextMenu();
            document.removeEventListener("pointerdown", onDocPointerDown);
            document.removeEventListener("keydown", onDocKeyDown);
            contextMenuCleanup = undefined;
        };
        contextMenuCleanup = cleanup;
        document.addEventListener("keydown", onDocKeyDown);
        requestAnimationFrame(() => {
            document.addEventListener("pointerdown", onDocPointerDown);
        });
    }

    function hideContextMenu() {
        setContextMenu(undefined);
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

    function syncBorderRects() {
        if (!selfRef) return;
        // Force layout reflow so newly-added border elements have valid rects
        void selfRef.offsetHeight;
        const borders = props.model.getBorderSet().getBorderMap();
        for (const [_, location] of DockLocation.values) {
            const border = borders.get(location);
            if (border) {
                const borderPath = border.getPath();
                const el = selfRef.querySelector(`[data-layout-path="${borderPath}"]`) as HTMLElement | null;
                if (el) {
                    border.setTabHeaderRect(getBoundingClientRectFn(el));
                }
            }
        }
    }

    function checkForBorderToShow(x: number, y: number) {
        if (!mainRef) return;
        const mainDomRect = mainRef.getBoundingClientRect();
        const layoutDomRect = selfRef?.getBoundingClientRect();
        if (!layoutDomRect) return;

        // mainRef rect relative to layout
        const r = new Rect(
            mainDomRect.x - layoutDomRect.x,
            mainDomRect.y - layoutDomRect.y,
            mainDomRect.width,
            mainDomRect.height,
        );

        const edgeRectWidth = 10;
        const edgeRectLength = 100;
        const offset = edgeRectLength / 2;
        const cx = r.x + r.width / 2;
        const cy = r.y + r.height / 2;

        let overEdge = false;
        if (props.model.isEnableEdgeDock() && showHiddenBorder() === DockLocation.CENTER) {
            if ((y > cy - offset && y < cy + offset) ||
                (x > cx - offset && x < cx + offset)) {
                overEdge = true;
            }
        }

        let location = DockLocation.CENTER;
        if (!overEdge) {
            if (x <= r.x + edgeRectWidth) {
                location = DockLocation.LEFT;
            } else if (x >= r.x + r.width - edgeRectWidth) {
                location = DockLocation.RIGHT;
            } else if (y <= r.y + edgeRectWidth) {
                location = DockLocation.TOP;
            } else if (y >= r.y + r.height - edgeRectWidth) {
                location = DockLocation.BOTTOM;
            }
        }

        if (location !== DockLocation.CENTER) {
            const borders = props.model.getBorderSet().getBorderMap();
            const border = borders.get(location);
            if (!border || !border.isAutoHide() || border.getChildren().length > 0) {
                location = DockLocation.CENTER;
            }
        }

        if (location !== showHiddenBorder()) {
            setShowHiddenBorder(location);
        }
    }

    function setDragNode(event: DragEvent, node: Node) {
            event.dataTransfer!.setData("text/plain", "--flexlayout--");
        event.dataTransfer!.effectAllowed = "copyMove";
        event.dataTransfer!.dropEffect = "move";
        dragEnterCount = 0;
            (selfRef as any).__dragNode = node;

        // Custom drag preview — styled div instead of browser default
        if (node instanceof TabNode) {
            const dragImg = document.createElement("div");
            dragImg.className = getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON) + " " + getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON + "--selected");
            dragImg.style.position = "absolute";
            dragImg.style.left = "-10000px";
            dragImg.style.top = "-10000px";
            dragImg.style.padding = "4px 12px";
            dragImg.style.fontSize = "12px";
            dragImg.style.whiteSpace = "nowrap";
            dragImg.style.pointerEvents = "none";
            dragImg.textContent = (node as TabNode).getName();
            document.body.appendChild(dragImg);
            event.dataTransfer!.setDragImage(dragImg, 0, 0);
            setTimeout(() => document.body.removeChild(dragImg), 0);
        }

        // Set up drag UI immediately (not in onDragEnter) so it works for float panel drags
        dropInfo = undefined;
        outlineDiv = document.createElement("div");
        outlineDiv.className = getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT);
        outlineDiv.style.visibility = "hidden";
        const speed = props.model.getAttribute("tabDragSpeed") as number || 0.3;
        outlineDiv.style.transition = `top ${speed}s, left ${speed}s, width ${speed}s, height ${speed}s`;
        selfRef!.appendChild(outlineDiv);
        dragging = true;
        document.addEventListener("keydown", onEscapeKey);
        setShowOverlay(true);
        if (props.model.getMaximizedTabset(Model.MAIN_WINDOW_ID) === undefined) {
            setShowEdges(props.model.isEnableEdgeDock());
        }
    }

    function onEscapeKey(event: KeyboardEvent) {
        if (event.key === "Escape") {
            clearDragMain();
        }
    }

    function clearDragMain() {
        (selfRef as any).__dragNode = undefined;
        setShowEdges(false);
        setShowOverlay(false);
        setShowHiddenBorder(DockLocation.CENTER);
        dragEnterCount = 0;
        dragging = false;
        document.removeEventListener("keydown", onEscapeKey);
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

    function onDragLeaveRaw(event: DragEvent) {
        dragEnterCount--;
        if (dragEnterCount === 0) {
            if (!selfRef?.contains(event.relatedTarget as globalThis.Node | null)) {
                clearDragMain();
            } else {
                dragEnterCount = 1;
            }
        }
    }

    function onDragEnter(event: DragEvent) {
        const dragNode = (selfRef as any)?.__dragNode;
        if (dragNode && !dragging) {
            event.preventDefault();
            dropInfo = undefined;
            outlineDiv = document.createElement("div");
            outlineDiv.className = getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT);
            outlineDiv.style.visibility = "hidden";
            const speed = props.model.getAttribute("tabDragSpeed") as number || 0.3;
            outlineDiv.style.transition = `top ${speed}s, left ${speed}s, width ${speed}s, height ${speed}s`;
            selfRef!.appendChild(outlineDiv);
            dragging = true;
            document.addEventListener("keydown", onEscapeKey);
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

            checkForBorderToShow(pos.x, pos.y);

            const dragNode = (selfRef as any)?.__dragNode;
            if (dragNode) {
                const root = props.model.getRoot();
                if (root) {
                    let di: DropInfo | undefined;

                    // Check borders first when a hidden border is being shown,
                    // since the root rect hasn't been recalculated yet
                    const hiddenBorderActive = showHiddenBorder() !== DockLocation.CENTER;
                    if (hiddenBorderActive) {
                        syncBorderRects();
                        di = props.model.getBorderSet().findDropTargetNode(dragNode, pos.x, pos.y);
                    }

                    if (di === undefined) {
                        di = root.findDropTargetNode(
                            Model.MAIN_WINDOW_ID,
                            dragNode,
                            pos.x,
                            pos.y,
                        );
                    }

                    if (di === undefined) {
                        di = props.model.getBorderSet().findDropTargetNode(dragNode, pos.x, pos.y);
                    }

                    if (di) {
                        dropInfo = di;
                        if (outlineDiv) {
                            let cls = getClassName(di.className);
                            if (di.rect.width <= 5) {
                                cls += " " + getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT + "_tab_reorder");
                            }
                            outlineDiv.className = cls;
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
                if (dragNode instanceof TabNode && dragNode.getParent() === undefined) {
                    const targetNode = dropInfo.node;
                    const savedSelected = targetNode instanceof BorderNode ? targetNode.getSelected() : -1;
                    doAction(
                        Action.addNode(
                            dragNode.toJson(),
                            targetNode.getId(),
                            dropInfo.location.getName(),
                            dropInfo.index,
                        ),
                    );
                    if (targetNode instanceof BorderNode && !targetNode.isAutoSelectTab(savedSelected !== -1)) {
                        targetNode.setSelected(savedSelected);
                        redraw();
                    }
                } else if (dragNode instanceof TabNode || dragNode instanceof TabSetNode) {
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

    const mainTabNodes = createMemo(() => {
        void revision();
        void layoutVersion();
        const tabs: Array<{node: TabNode, parent: TabSetNode | BorderNode}> = [];

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

        for (const border of props.model.getBorderSet().getBorders()) {
            for (const child of border.getChildren()) {
                if (child instanceof TabNode) {
                    tabs.push({node: child, parent: border});
                }
            }
        }

        return tabs;
    });

    const floatWindows = createMemo(() => {
        void revision();
        void layoutVersion();
        return [...props.model.getwindowsMap().values()].filter(
            (w) => w.windowType === "float",
        );
    });

    const popoutWindows = createMemo(() => {
        void revision();
        void layoutVersion();
        return [...props.model.getwindowsMap().values()].filter(
            (w) => w.windowType === "popout",
        );
    });

    const windowContexts = new Map<string, ILayoutContext>();

    const getWindowContext = (windowId: string, containerRef: () => HTMLDivElement | undefined): ILayoutContext => {
        let ctx = windowContexts.get(windowId);
        if (!ctx) {
            ctx = createWindowContext(windowId, layoutContextObj, containerRef);
            windowContexts.set(windowId, ctx);
        }
        return ctx;
    };

    const bringToFront = (windowId: string) => {
        setFloatZOrder((order) => {
            const filtered = order.filter((id) => id !== windowId);
            const newOrder = [...filtered, windowId];
            props.model.setFloatZOrder(newOrder);
            return newOrder;
        });
    };

    const getFloatZIndex = (windowId: string): number => {
        const order = floatZOrder();
        const idx = order.indexOf(windowId);
        return 1000 + (idx >= 0 ? idx : 0);
    };

    const BORDER_BAR_SIZE = 29;

    const hasBorders = createMemo(() => {
        void layoutVersion();
        return props.model.getBorderSet().getBorderMap().size > 0;
    });

    const borderData = createMemo(() => {
        void layoutVersion();
        const hiddenBorderLoc = showHiddenBorder();
        if (!hasBorders()) return null;
        const borders = props.model.getBorderSet().getBorderMap();
        const strips = new Map<string, {border: BorderNode, show: boolean}>();
        for (const [_, location] of DockLocation.values) {
            const border = borders.get(location);
            if (border && border.isShowing() && (
                !border.isAutoHide() ||
                (border.isAutoHide() && (border.getChildren().length > 0 || hiddenBorderLoc === location)))) {
                strips.set(location.getName(), { border, show: border.getSelected() !== -1 });
            }
        }
        return { strips };
    });

    const borderStrip = (loc: DockLocation) => {
        const data = borderData();
        const entry = data?.strips.get(loc.getName());
        if (!entry) return undefined;
        const stripSize = entry.border.getDockState() === "expanded" ? 0 : BORDER_BAR_SIZE;
        return <BorderTabSet layout={layoutContext()} border={entry.border} size={stripSize} />;
    };

    const borderContent = (loc: DockLocation) => {
        const data = borderData();
        const entry = data?.strips.get(loc.getName());
        return entry ? <BorderTab layout={layoutContext()} border={entry.border} show={entry.show} /> : undefined;
    };

    const fabArrow = (loc: DockLocation): string => {
        if (loc === DockLocation.LEFT) return "▶";
        if (loc === DockLocation.RIGHT) return "◀";
        if (loc === DockLocation.TOP) return "▼";
        return "▲";
    };

    const fabStyle = (loc: DockLocation): Record<string, any> => {
        const r = rect();
        const size = 20;
        const base: Record<string, any> = {
            position: "absolute",
            width: size + "px",
            height: size + "px",
            "z-index": 50,
        };
        if (loc === DockLocation.LEFT) {
            base.left = "0px";
            base.top = (r.height / 2 - size / 2) + "px";
        } else if (loc === DockLocation.RIGHT) {
            base.right = "0px";
            base.top = (r.height / 2 - size / 2) + "px";
        } else if (loc === DockLocation.TOP) {
            base.top = "0px";
            base.left = (r.width / 2 - size / 2) + "px";
        } else {
            base.bottom = "0px";
            base.left = (r.width / 2 - size / 2) + "px";
        }
        return base;
    };

    const hiddenBorderFabs = () => {
        void layoutVersion();
        void revision();
        if (!hasBorders()) return null;
        const borders = props.model.getBorderSet().getBorderMap();
        const fabs: JSX.Element[] = [];
        for (const [_, location] of DockLocation.values) {
            const border = borders.get(location);
            if (border && border.isShowing() && border.getDockState() === "hidden" && border.getChildren().length > 0) {
                const loc = border.getLocation();
                fabs.push(
                    <button
                        class={getClassName(CLASSES.FLEXLAYOUT__BORDER_FAB)}
                        data-layout-path={border.getPath() + "/fab"}
                        style={fabStyle(loc)}
                        onClick={() => doAction(Action.setDockState(border.getId(), "expanded"))}
                    >
                        {fabArrow(loc)}
                    </button>
                );
            }
        }
        return fabs;
    };

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
                    style={{ position: "absolute", inset: 0, "z-index": 998 }}
                />
            )}

            <Show when={showEdges()}>
                {(() => {
                    const edgeLength = 100;
                    const edgeWidth = 10;
                    const offset = edgeLength / 2;
                    const r = rect();
                    const cls = getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT);
                    const radius = 50;
                    return (
                        <>
                            <div
                                class={cls + " " + getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_TOP)}
                                style={{
                                    position: "absolute",
                                    top: "0px",
                                    left: (r.width / 2 - offset) + "px",
                                    width: edgeLength + "px",
                                    height: edgeWidth + "px",
                                    "border-bottom-left-radius": radius + "%",
                                    "border-bottom-right-radius": radius + "%",
                                    "z-index": 999,
                                }}
                            />
                            <div
                                class={cls + " " + getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_LEFT)}
                                style={{
                                    position: "absolute",
                                    top: (r.height / 2 - offset) + "px",
                                    left: "0px",
                                    width: edgeWidth + "px",
                                    height: edgeLength + "px",
                                    "border-top-right-radius": radius + "%",
                                    "border-bottom-right-radius": radius + "%",
                                    "z-index": 999,
                                }}
                            />
                            <div
                                class={cls + " " + getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_BOTTOM)}
                                style={{
                                    position: "absolute",
                                    top: (r.height - edgeWidth) + "px",
                                    left: (r.width / 2 - offset) + "px",
                                    width: edgeLength + "px",
                                    height: edgeWidth + "px",
                                    "border-top-left-radius": radius + "%",
                                    "border-top-right-radius": radius + "%",
                                    "z-index": 999,
                                }}
                            />
                            <div
                                class={cls + " " + getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_RIGHT)}
                                style={{
                                    position: "absolute",
                                    top: (r.height / 2 - offset) + "px",
                                    left: (r.width - edgeWidth) + "px",
                                    width: edgeWidth + "px",
                                    height: edgeLength + "px",
                                    "border-top-left-radius": radius + "%",
                                    "border-bottom-left-radius": radius + "%",
                                    "z-index": 999,
                                }}
                            />
                        </>
                    );
                })()}
            </Show>

            <Show when={hasBorders()} fallback={
                <div
                    ref={mainRef}
                    class={getClassName(CLASSES.FLEXLAYOUT__LAYOUT_MAIN)}
                    data-layout-path="/main"
                    style={{ position: "absolute", top: "0", left: "0", bottom: "0", right: "0", display: "flex" }}
                >
                    <Show when={rect().width > 0 && props.model.getRoot()}>
                        <Row layout={layoutContext()} node={props.model.getRoot() as RowNode} />
                    </Show>
                </div>
            }>
                {(() => {
                    const classBorderOuter = getClassName(CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER);
                    const classBorderInner = getClassName(CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER_INNER);

                    const mainContent = (
                        <div
                            ref={mainRef}
                            class={getClassName(CLASSES.FLEXLAYOUT__LAYOUT_MAIN)}
                            data-layout-path="/main"
                        >
                            <Show when={rect().width > 0 && props.model.getRoot()}>
                                <Row layout={layoutContext()} node={props.model.getRoot() as RowNode} />
                            </Show>
                        </div>
                    );

                    const buildNestedBorders = (): JSX.Element => {
                        const data = borderData();
                        if (!data) return mainContent;

                        const visibleBorders = props.model.getBorderSet().getBordersByPriority();
                        const sorted = computeNestingOrder(
                            visibleBorders.filter(b => data.strips.has(b.getLocation().getName()))
                        );

                        let current: JSX.Element = mainContent;

                        for (let i = sorted.length - 1; i >= 0; i--) {
                            const border = sorted[i];
                            const loc = border.getLocation();
                            const isHorz = loc === DockLocation.TOP || loc === DockLocation.BOTTOM;
                            const flexDir = isHorz ? "column" : "row";
                            const isStart = loc === DockLocation.LEFT || loc === DockLocation.TOP;

                            const content = borderContent(loc);
                            current = (
                                <div class={classBorderInner} style={{ "flex-direction": flexDir }}>
                                    {isStart ? content : null}
                                    {current}
                                    {!isStart ? content : null}
                                </div>
                            );

                            const strip = borderStrip(loc);
                            const wrapperClass = i === 0 ? classBorderOuter : classBorderInner;
                            current = (
                                <div class={wrapperClass} style={{ "flex-direction": flexDir }}>
                                    {isStart ? strip : null}
                                    {current}
                                    {!isStart ? strip : null}
                                </div>
                            );
                        }

                        return current;
                    };

                    return buildNestedBorders();
                })()}
            </Show>

            {hiddenBorderFabs()}

            <Show when={rect().width > 0}>
                <For each={mainTabNodes()}>
                    {(tabEntry) => {
                        const tabStyle = (): Record<string, any> => {
                            void revision();
                            void layoutVersion();
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
                        const tabPath = () => {
                            void revision();
                            return tabEntry.node.getPath();
                        };
                        return (
                            <div
                                class={getClassName(CLASSES.FLEXLAYOUT__TAB)}
                                data-layout-path={tabPath()}
                                style={tabStyle()}
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

            <For each={floatWindows()}>
                {(lw) => {
                    let panelContentRef: HTMLDivElement | undefined;
                    const ctx = getWindowContext(lw.windowId, () => panelContentRef);
                    return (
                        <FloatingPanel
                            layoutWindow={lw}
                            layoutContext={ctx}
                            onBringToFront={bringToFront}
                            onContentRef={(el: HTMLDivElement) => { panelContentRef = el; }}
                            zIndex={getFloatZIndex(lw.windowId)}
                        />
                    );
                }}
            </For>

            <For each={popoutWindows()}>
                {(lw) => {
                    const [mountEl, setMountEl] = createSignal<HTMLElement | null>(null);

                    createEffect(() => {
                        const r = lw.rect;
                        const popup = window.open(
                            "",
                            lw.windowId,
                            `width=${r.width},height=${r.height},left=${r.x},top=${r.y}`,
                        );
                        if (!popup) {
                            console.warn("Popup blocked for window", lw.windowId);
                            doAction(Action.closeWindow(lw.windowId));
                            return;
                        }

                        const setup = () => {
                            if (!popup) return;
                            const parentStyles = document.querySelectorAll('link[rel="stylesheet"], style');
                            parentStyles.forEach((style) => {
                                popup.document.head.appendChild(style.cloneNode(true));
                            });

                            const container = popup.document.createElement("div");
                            container.id = "flexlayout-popout-root";
                            container.style.cssText = "position:relative;width:100%;height:100%;overflow:hidden;";
                            popup.document.body.style.margin = "0";
                            popup.document.body.appendChild(container);

                            lw.window = popup;
                            setMountEl(container);
                        };

                        if (popup.document.readyState === "complete") {
                            setup();
                        } else {
                            popup.addEventListener("load", setup);
                        }

                        const handleParentUnload = () => {
                            if (!popup.closed) popup.close();
                        };
                        window.addEventListener("beforeunload", handleParentUnload);

                        popup.addEventListener("beforeunload", () => {
                            doAction(Action.closeWindow(lw.windowId));
                        });

                        onCleanup(() => {
                            window.removeEventListener("beforeunload", handleParentUnload);
                            setMountEl(null);
                            if (!popup.closed) popup.close();
                            lw.window = undefined;
                        });
                    });

                    const popoutCtx = getWindowContext(lw.windowId, () => mountEl() as HTMLDivElement | undefined);

                    const popoutTabNodes = createMemo(() => {
                        void revision();
                        void layoutVersion();
                        const tabs: Array<{ node: TabNode; parent: TabSetNode | BorderNode }> = [];
                        if (!lw.root) return tabs;
                        const visitNode = (n: Node) => {
                            if (n instanceof TabNode) {
                                tabs.push({ node: n, parent: n.getParent() as TabSetNode | BorderNode });
                            }
                            for (const child of n.getChildren()) {
                                visitNode(child);
                            }
                        };
                        visitNode(lw.root);
                        return tabs;
                    });

                    return (
                        <Show when={mountEl()}>
                            {(el) => (
                                <Portal mount={el()}>
                                    <Show when={lw.root}>
                                        <Row layout={popoutCtx} node={lw.root as RowNode} />
                                    </Show>
                                    <For each={popoutTabNodes()}>
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
                                                    class={getClassName(CLASSES.FLEXLAYOUT__TAB)}
                                                    data-layout-path={tabEntry.node.getPath()}
                                                    style={tabStyle()}
                                                    onPointerDown={() => {
                                                        const p = tabEntry.node.getParent();
                                                        if (p instanceof TabSetNode && !p.isActive()) {
                                                            doAction(Action.setActiveTabset(p.getId(), lw.windowId));
                                                        }
                                                    }}
                                                >
                                                    {props.factory(tabEntry.node)}
                                                </div>
                                            );
                                        }}
                                    </For>
                                </Portal>
                            )}
                        </Show>
                    );
                }}
            </For>

            <Show when={popupMenu()}>
                {(menu) => {
                    const pos = menu().position;
                    return (
                        <div
                            class={getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER)}
                            style={{
                                position: "absolute",
                                "z-index": 1002,
                                left: pos.left,
                                right: pos.right,
                                top: pos.top,
                                bottom: pos.bottom,
                            }}
                            onPointerDown={(e) => e.stopPropagation()}
                        >
                        <div
                            class={getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU)}
                            data-layout-path="/popup-menu"
                            tabIndex={0}
                            ref={(el: HTMLDivElement) => requestAnimationFrame(() => el.focus())}
                        >
                                <For each={menu().items}>
                                    {(item, i) => {
                                        let classes = getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
                                        if (menu().parentNode.getSelected() === item.index) {
                                            classes += " " + getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED);
                                        }
                                        return (
                                            <div
                                                class={classes}
                                                data-layout-path={"/popup-menu/tb" + i()}
                                                onClick={(event) => {
                                                    menu().onSelect(item);
                                                    hidePopup();
                                                    event.stopPropagation();
                                                }}
                                            >
                                                {item.node.getName()}
                                            </div>
                                        );
                                    }}
                                </For>
                            </div>
                        </div>
                    );
                }}
            </Show>

            <Show when={contextMenu()}>
                {(menu) => (
                    <div
                        class={getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER)}
                        style={{ position: "absolute", inset: 0, "z-index": 1002 }}
                        onPointerDown={() => {
                            if (contextMenuCleanup) contextMenuCleanup();
                            else hideContextMenu();
                        }}
                    >
                        <div
                            class={getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU)}
                            data-layout-path="/context-menu"
                            tabIndex={0}
                            ref={(el: HTMLDivElement) => requestAnimationFrame(() => el.focus())}
                            style={{
                                position: "absolute",
                                left: menu().position.x + "px",
                                top: menu().position.y + "px",
                            }}
                            onPointerDown={(e) => e.stopPropagation()}
                        >
                            <Show when={menu().items.length === 0}>
                                <div
                                    class={getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM)}
                                    style={{ opacity: 0.5, cursor: "default" }}
                                >
                                    No actions available
                                </div>
                            </Show>
                            <For each={menu().items}>
                                {(item) => (
                                    <div
                                        class={getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM)}
                                        data-context-menu-item
                                        onClick={(event) => {
                                            item.action();
                                            if (contextMenuCleanup) {
                                                contextMenuCleanup();
                                            } else {
                                                hideContextMenu();
                                            }
                                            event.stopPropagation();
                                        }}
                                    >
                                        {item.label}
                                    </div>
                                )}
                            </For>
                        </div>
                    </div>
                )}
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
    getLayoutRootDiv: () => HTMLDivElement | undefined;
    onFloatDragStart?: (e: PointerEvent) => void;
    onFloatDock?: () => void;
    onFloatClose?: () => void;
    redraw: () => void;
    setDragNode: (event: DragEvent, node: Node) => void;
    clearDragMain: () => void;
    getRevision: () => number;
    showPopup: (
        triggerElement: HTMLElement,
        parentNode: TabSetNode | BorderNode,
        items: { index: number; node: TabNode }[],
        onSelect: (item: { index: number; node: TabNode }) => void,
    ) => void;
    showContextMenu: (
        event: MouseEvent,
        items: { label: string; action: () => void }[],
    ) => void;
}
