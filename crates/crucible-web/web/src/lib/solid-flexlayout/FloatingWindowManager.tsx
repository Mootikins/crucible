import { For, JSX, Show, createEffect, createMemo, createSignal } from "solid-js";
import { Portal } from "solid-js/web";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { Model } from "../flexlayout/model/Model";
import { Node } from "../flexlayout/model/Node";
import { RowNode } from "../flexlayout/model/RowNode";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { Rect } from "../flexlayout/core/Rect";
import { FloatingPanel } from "./FloatingPanel";
import { Row } from "./Row";
import type { ILayoutContext } from "./Layout";

interface FloatingWindowManagerOptions {
    model: Model;
    getClassName: (defaultClassName: string) => string;
    getRevision: () => number;
    getLayoutVersion: () => number;
    getLayoutContext: () => ILayoutContext;
    doAction: (action: any) => void;
    factory: (node: TabNode) => JSX.Element;
}

interface FloatingWindowManager {
    renderFloatingWindows: () => JSX.Element;
    renderPopoutWindows: () => JSX.Element;
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

function collectTabNodes(root: Node | undefined): Array<{ node: TabNode; parent: TabSetNode | BorderNode }> {
    const tabs: Array<{ node: TabNode; parent: TabSetNode | BorderNode }> = [];
    if (!root) return tabs;

    const visitNode = (n: Node) => {
        if (n instanceof TabNode) {
            tabs.push({ node: n, parent: n.getParent() as TabSetNode | BorderNode });
        }
        for (const child of n.getChildren()) {
            visitNode(child);
        }
    };

    visitNode(root);
    return tabs;
}

export function createFloatingWindowManager(options: FloatingWindowManagerOptions): FloatingWindowManager {
    const [floatZOrder, setFloatZOrder] = createSignal<string[]>([]);

    const windowContexts = new Map<string, ILayoutContext>();

    const getWindowContext = (
        windowId: string,
        containerRef: () => HTMLDivElement | undefined,
    ): ILayoutContext => {
        let ctx = windowContexts.get(windowId);
        if (!ctx) {
            ctx = createWindowContext(windowId, options.getLayoutContext(), containerRef);
            windowContexts.set(windowId, ctx);
        }
        return ctx;
    };

    const bringToFront = (windowId: string) => {
        setFloatZOrder((order) => {
            const filtered = order.filter((id) => id !== windowId);
            const newOrder = [...filtered, windowId];
            options.model.setFloatZOrder(newOrder);
            return newOrder;
        });
    };

    const getFloatZIndex = (windowId: string): number => {
        const order = floatZOrder();
        const idx = order.indexOf(windowId);
        return 1000 + (idx >= 0 ? idx : 0);
    };

    const floatWindows = createMemo(() => {
        void options.getRevision();
        void options.getLayoutVersion();
        return [...options.model.getwindowsMap().values()].filter((w) => w.windowType === "float");
    });

    const popoutWindows = createMemo(() => {
        void options.getRevision();
        void options.getLayoutVersion();
        return [...options.model.getwindowsMap().values()].filter((w) => w.windowType === "popout");
    });

    const savedZOrder = options.model.getFloatZOrder();
    if (savedZOrder.length > 0 && floatZOrder().length === 0) {
        setFloatZOrder(savedZOrder);
    }

    const renderFloatingWindows = () => (
        <For each={floatWindows()}>
            {(lw) => {
                let panelContentRef: HTMLDivElement | undefined;
                const ctx = getWindowContext(lw.windowId, () => panelContentRef);
                return (
                    <FloatingPanel
                        layoutWindow={lw}
                        layoutContext={ctx}
                        onBringToFront={bringToFront}
                        onContentRef={(el: HTMLDivElement) => {
                            panelContentRef = el;
                        }}
                        zIndex={getFloatZIndex(lw.windowId)}
                    />
                );
            }}
        </For>
    );

    const renderPopoutWindows = () => (
        <For each={popoutWindows()}>
            {(lw) => {
                const [mountEl, setMountEl] = createSignal<HTMLElement | null>(null);

                createEffect(() => {
                    const r = lw.rect;
                    const popup = window.open("", lw.windowId, `width=${r.width},height=${r.height},left=${r.x},top=${r.y}`);
                    if (!popup) {
                        console.warn("Popup blocked for window", lw.windowId);
                        options.doAction(Action.closeWindow(lw.windowId));
                        return;
                    }

                    const setup = () => {
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
                        options.doAction(Action.closeWindow(lw.windowId));
                    });

                    return () => {
                        window.removeEventListener("beforeunload", handleParentUnload);
                        setMountEl(null);
                        if (!popup.closed) popup.close();
                        lw.window = undefined;
                    };
                });

                const popoutCtx = getWindowContext(lw.windowId, () => mountEl() as HTMLDivElement | undefined);

                const popoutTabNodes = createMemo(() => {
                    void options.getRevision();
                    void options.getLayoutVersion();
                    return collectTabNodes(lw.root);
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
                                                class={options.getClassName(CLASSES.FLEXLAYOUT__TAB)}
                                                data-layout-path={tabEntry.node.getPath()}
                                                style={tabStyle()}
                                                onPointerDown={() => {
                                                    const p = tabEntry.node.getParent();
                                                    if (p instanceof TabSetNode && !p.isActive()) {
                                                        options.doAction(Action.setActiveTabset(p.getId(), lw.windowId));
                                                    }
                                                }}
                                            >
                                                {options.factory(tabEntry.node)}
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
    );

    return {
        renderFloatingWindows,
        renderPopoutWindows,
    };
}
