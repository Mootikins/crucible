import { Component, createSignal, createEffect, onMount, onCleanup } from "solid-js";
import { Model } from "../flexlayout/model/Model";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { RowNode } from "../flexlayout/model/RowNode";
import { Node } from "../flexlayout/model/Node";
import { Rect } from "../flexlayout/core/Rect";
import { DockLocation } from "../flexlayout/core/DockLocation";
import { LayoutEngine } from "../flexlayout/layout/LayoutEngine";
import { createDndManager } from "./DndManager";
import { createPopupManager } from "./PopupManager";
import { createFloatingWindowManager } from "./FloatingWindowManager";
import { LayoutRenderer } from "./LayoutRenderer";
import type { ILayoutContext, ILayoutProps, ITabRenderValues, ITabSetRenderValues } from "./LayoutTypes";
export type { ILayoutContext, ILayoutProps, ITabRenderValues, ITabSetRenderValues } from "./LayoutTypes";
export const Layout: Component<ILayoutProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;
    let mainRef: HTMLDivElement | undefined;

    const [rect, setRect] = createSignal<Rect>(Rect.empty());
    const [revision, setRevision] = createSignal(0);
    const [layoutVersion, setLayoutVersion] = createSignal(0);
    const [showEdges, setShowEdges] = createSignal(false);
    const [showOverlay, setShowOverlay] = createSignal(false);
    const [editingTab, setEditingTab] = createSignal<TabNode | undefined>(undefined);
    const [showHiddenBorder, setShowHiddenBorder] = createSignal<DockLocation>(DockLocation.CENTER);
    let dndManager: ReturnType<typeof createDndManager>;
    let popupManager: ReturnType<typeof createPopupManager>;
    let floatingWindowManager: ReturnType<typeof createFloatingWindowManager>;

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
        popupManager.cleanup();
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

    function getDomRect(): Rect {
        if (selfRef) {
            const r = selfRef.getBoundingClientRect();
            return new Rect(r.x, r.y, r.width, r.height);
        }
        return Rect.empty();
    }

    popupManager = createPopupManager({
        getSelfRef: () => selfRef,
        getClassName,
    });

    function showPopup(
        triggerElement: HTMLElement,
        parentNode: TabSetNode | BorderNode,
        items: { index: number; node: TabNode }[],
        onSelect: (item: { index: number; node: TabNode }) => void,
    ) {
        popupManager.showPopup(triggerElement, parentNode, items, onSelect);
    }

    function showContextMenu(
        event: MouseEvent,
        items: { label: string; action: () => void }[],
    ) {
        popupManager.showContextMenu(event, items);
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

    dndManager = createDndManager({
        model: props.model,
        getClassName,
        getSelfRef: () => selfRef,
        getMainRef: () => mainRef,
        getBoundingClientRect: getBoundingClientRectFn,
        getShowHiddenBorder: showHiddenBorder,
        setShowHiddenBorder,
        setShowOverlay,
        setShowEdges,
        doAction,
        redraw,
    });

    floatingWindowManager = createFloatingWindowManager({
        model: props.model,
        getClassName,
        getRevision: revision,
        getLayoutVersion: layoutVersion,
        getLayoutContext: layoutContext,
        doAction,
        factory: props.factory,
    });

    function setDragNode(event: DragEvent, node: Node) {
        dndManager.setDragNode(event, node);
    }

    function clearDragMain() {
        dndManager.clearDragMain();
    }

    function onDragEnterRaw(event: DragEvent) {
        dndManager.onDragEnterRaw(event);
    }

    function onDragLeaveRaw(event: DragEvent) {
        dndManager.onDragLeaveRaw(event);
    }

    function onDragOver(event: DragEvent) {
        dndManager.onDragOver(event);
    }

    function onDrop(event: DragEvent) {
        dndManager.onDrop(event);
    }
    return (
        <LayoutRenderer
            model={props.model}
            factory={props.factory}
            getClassName={getClassName}
            layoutContext={layoutContext}
            rect={rect}
            revision={revision}
            layoutVersion={layoutVersion}
            showEdges={showEdges}
            showOverlay={showOverlay}
            showHiddenBorder={showHiddenBorder}
            doAction={doAction}
            renderFloatingWindows={() => floatingWindowManager.renderFloatingWindows()}
            renderPopoutWindows={() => floatingWindowManager.renderPopoutWindows()}
            renderPopupMenu={() => popupManager.renderPopupMenu()}
            renderContextMenu={() => popupManager.renderContextMenu()}
            onDragEnterRaw={onDragEnterRaw}
            onDragLeaveRaw={onDragLeaveRaw}
            onDragOver={onDragOver}
            onDrop={onDrop}
            setSelfRef={(el) => {
                selfRef = el;
            }}
            setMainRef={(el) => {
                mainRef = el;
            }}
        />
    );
};
