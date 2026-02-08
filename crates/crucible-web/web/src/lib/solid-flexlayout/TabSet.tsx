import { Component, JSX, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { RowNode } from "../flexlayout/model/RowNode";
import { Orientation } from "../flexlayout/core/Orientation";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import { Model } from "../flexlayout/model/Model";
import type { ILayoutContext, ITabSetRenderValues } from "./Layout";
import { TabButton } from "./TabButton";

export interface ITabSetProps {
    layout: ILayoutContext;
    node: TabSetNode;
}

export const TabSet: Component<ITabSetProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;
    let contentRef: HTMLDivElement | undefined;
    let tabStripRef: HTMLDivElement | undefined;
    let tabStripInnerRef: HTMLDivElement | undefined;
    let overflowButtonRef: HTMLButtonElement | undefined;

    const cm = props.layout.getClassName;
    const node = props.node;
    const path = () => node.getPath();

    const [hiddenTabs, setHiddenTabs] = createSignal<number[]>([]);

    let userControlledScroll = false;

    function scrollSelectedIntoView() {
        if (!tabStripInnerRef || userControlledScroll) return;
        const selectedTabNode = node.getSelectedNode() as TabNode | undefined;
        if (!selectedTabNode) return;
        const selectedRect = selectedTabNode.getTabRect();
        if (!selectedRect) return;

        const stripRect = props.layout.getBoundingClientRect(tabStripInnerRef);
        if (stripRect.width <= 0) return;

        const shift = stripRect.x - selectedRect.x;
        if (shift > 0 || selectedRect.width > stripRect.width) {
            tabStripInnerRef.scrollLeft -= shift;
        } else {
            const shiftRight = (selectedRect.x + selectedRect.width) - (stripRect.x + stripRect.width);
            if (shiftRight > 0) {
                tabStripInnerRef.scrollLeft += shiftRight;
            }
        }
    }

    function findHiddenTabIndices(): number[] {
        if (!tabStripInnerRef) return [];
        const stripRect = tabStripInnerRef.getBoundingClientRect();
        const visibleLeft = stripRect.left - 1;
        const visibleRight = stripRect.right + 1;
        const tabContainer = tabStripInnerRef.firstElementChild;
        if (!tabContainer) return [];

        const hidden: number[] = [];
        const tabButtonClass = cm(CLASSES.FLEXLAYOUT__TAB_BUTTON);
        let tabIndex = 0;
        Array.from(tabContainer.children).forEach((child) => {
            if (child.classList.contains(tabButtonClass)) {
                const childRect = child.getBoundingClientRect();
                if (childRect.left < visibleLeft || childRect.right > visibleRight) {
                    hidden.push(tabIndex);
                }
                tabIndex++;
            }
        });
        return hidden;
    }

    createEffect(() => {
        void props.layout.getRevision();
        if (selfRef) {
            node.setRect(props.layout.getBoundingClientRect(selfRef));
        }
        if (tabStripRef) {
            node.setTabStripRect(props.layout.getBoundingClientRect(tabStripRef));
        }
        if (contentRef) {
            const newContentRect = props.layout.getBoundingClientRect(contentRef);
            if (!node.getContentRect().equals(newContentRect) && !isNaN(newContentRect.x)) {
                node.setContentRect(newContentRect);
                props.layout.redraw();
            }
        }
        scrollSelectedIntoView();
        const newHidden = findHiddenTabIndices();
        const current = hiddenTabs();
        if (newHidden.length !== current.length || newHidden.some((v, i) => v !== current[i])) {
            setHiddenTabs(newHidden);
        }
    });

    const isFloating = () => props.layout.getWindowId() !== Model.MAIN_WINDOW_ID;

    const onPointerDown = (e: PointerEvent) => {
        if (!node.isActive()) {
            props.layout.doAction(
                Action.setActiveTabset(node.getId(), props.layout.getWindowId()),
            );
        }
        if (isFloating() && props.layout.onFloatDragStart) {
            props.layout.onFloatDragStart(e);
        }
    };

    const onMaximizeToggle = (event: MouseEvent) => {
        if (node.canMaximize()) {
            props.layout.doAction(Action.maximizeToggle(node.getId()));
        }
        event.stopPropagation();
    };

    const onDoubleClick = () => {
        if (node.canMaximize()) {
            props.layout.doAction(Action.maximizeToggle(node.getId()));
        }
    };

    const onFloat = (event: MouseEvent) => {
        const r = node.getRect();
        props.layout.doAction(
            Action.floatTabset(node.getId(), r.x, r.y, r.width, r.height),
        );
        event.stopPropagation();
    };

    const onClose = (event: MouseEvent) => {
        props.layout.doAction(Action.deleteTabset(node.getId()));
        event.stopPropagation();
    };

    const onTabStripScroll = () => {
        const newHidden = findHiddenTabIndices();
        const current = hiddenTabs();
        if (newHidden.length !== current.length || newHidden.some((v, i) => v !== current[i])) {
            setHiddenTabs(newHidden);
        }
    };

    const onOverflowClick = (event: MouseEvent) => {
        const items = hiddenTabs().map(h => ({
            index: h,
            node: node.getChildren()[h] as TabNode,
        }));
        props.layout.showPopup(overflowButtonRef!, node, items, (item) => {
            userControlledScroll = false;
            props.layout.doAction(Action.selectTab(item.node.getId()));
        });
        event.stopPropagation();
    };

    const onDragStart = (event: DragEvent) => {
        if (!props.layout.getEditingTab()) {
            if (node.isEnableDrag()) {
                event.stopPropagation();
                props.layout.setDragNode(event, node);
            } else {
                event.preventDefault();
            }
        } else {
            event.preventDefault();
        }
    };

    const tabChildren = createMemo(() => {
        void props.layout.getRevision();
        return [...node.getChildren()] as TabNode[];
    });

    const tabStripClasses = () => {
        void props.layout.getRevision();
        let classes = cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER);
        classes += " " + CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER_ + (node.getTabLocation() || "top");

        if (node.isActive()) {
            classes += " " + cm(CLASSES.FLEXLAYOUT__TABSET_SELECTED);
        }

        if (node.isMaximized()) {
            classes += " " + cm(CLASSES.FLEXLAYOUT__TABSET_MAXIMIZED);
        }

        return classes;
    };

    const getRenderState = () => {
        void props.layout.getRevision();
        const renderState: ITabSetRenderValues = {
            leading: undefined,
            stickyButtons: [],
            buttons: [],
            overflowPosition: undefined,
        };
        props.layout.customizeTabSet(node, renderState);

        const buttons: JSX.Element[] = [...renderState.buttons];

        const overflowPos = renderState.overflowPosition ?? renderState.stickyButtons.length;

        if (hiddenTabs().length > 0) {
            buttons.splice(Math.min(overflowPos, buttons.length), 0,
                <button
                    ref={overflowButtonRef}
                    data-layout-path={path() + "/button/overflow"}
                    class={
                        cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON) +
                        " " +
                        cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_OVERFLOW)
                    }
                    title="Overflow"
                    onClick={onOverflowClick}
                    onPointerDown={(e) => e.stopPropagation()}
                >
                    {"..."}
                    <Show when={hiddenTabs().length > 0}>
                        <div class={cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_OVERFLOW_COUNT)}>
                            {hiddenTabs().length}
                        </div>
                    </Show>
                </button>,
            );
        }

        if (node.canMaximize()) {
            const title = node.isMaximized() ? "Restore" : "Maximize";
            buttons.push(
                <button
                    data-layout-path={path() + "/button/max"}
                    title={title}
                    class={
                        cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON) +
                        " " +
                        cm(
                            CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_ +
                                (node.isMaximized() ? "max" : "min"),
                        )
                    }
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={onMaximizeToggle}
                >
                    {node.isMaximized() ? "⊡" : "⊞"}
                </button>,
            );
        }

        if (!node.isMaximized() && props.layout.getWindowId() === Model.MAIN_WINDOW_ID) {
            buttons.push(
                <button
                    data-layout-path={path() + "/button/float"}
                    title="Float"
                    class={
                        cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON) +
                        " " +
                        cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_FLOAT)
                    }
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={onFloat}
                >
                    ⊡
                </button>,
            );
        }

        if (!node.isMaximized() && node.isEnableClose() && !isFloating()) {
            buttons.push(
                <button
                    data-layout-path={path() + "/button/close"}
                    title="Close"
                    class={
                        cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON) +
                        " " +
                        cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_CLOSE)
                    }
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={onClose}
                >
                    ✕
                </button>,
            );
        }

        if (isFloating()) {
            if (props.layout.onFloatDock) {
                const dockFn = props.layout.onFloatDock;
                buttons.push(
                    <button
                        data-layout-path={path() + "/button/dock"}
                        title="Dock"
                        class={cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON)}
                        onPointerDown={(e) => e.stopPropagation()}
                        onClick={(e) => { e.stopPropagation(); dockFn(); }}
                    >
                        ⊟
                    </button>,
                );
            }
            if (props.layout.onFloatClose) {
                const closeFn = props.layout.onFloatClose;
                buttons.push(
                    <button
                        data-layout-path={path() + "/button/close-float"}
                        title="Close"
                        class={
                            cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON) +
                            " " +
                            cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_CLOSE)
                        }
                        onPointerDown={(e) => e.stopPropagation()}
                        onClick={(e) => { e.stopPropagation(); closeFn(); }}
                    >
                        ✕
                    </button>,
                );
            }
        }

        return { buttons, stickyButtons: renderState.stickyButtons };
    };

    const renderTabStrip = (): JSX.Element => {
        const state = getRenderState();
        const floating = isFloating();
        return (
            <div
                ref={tabStripRef}
                class={tabStripClasses()}
                data-layout-path={path() + "/tabstrip"}
                style={floating ? { cursor: "move" } : undefined}
                onPointerDown={onPointerDown}
                onDblClick={floating ? undefined : onDoubleClick}
                draggable={!floating}
                onDragStart={floating ? undefined : onDragStart}
            >
                <div
                    ref={tabStripInnerRef}
                    class={cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER) + " " + cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_ + (node.getTabLocation() || "top"))}
                    style={{ "overflow-x": "auto", "overflow-y": "hidden" }}
                    onScroll={onTabStripScroll}
                >
                    <div
                        class={cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER) + " " + cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER_ + (node.getTabLocation() || "top"))}
                    >
                        <For each={tabChildren()}>
                            {(child, index) => (
                                <>
                                    <TabButton
                                        layout={props.layout}
                                        node={child}
                                        path={path() + "/tb" + index()}
                                        selected={node.getSelected() === index()}
                                    />
                                    {index() < tabChildren().length - 1 && (
                                        <div class={cm(CLASSES.FLEXLAYOUT__TABSET_TAB_DIVIDER)} />
                                    )}
                                </>
                            )}
                        </For>
                        {state.stickyButtons.length > 0 && (
                            <div
                                class={cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_STICKY_BUTTONS_CONTAINER)}
                                onPointerDown={(e) => e.stopPropagation()}
                                onDragStart={(e) => e.preventDefault()}
                            >
                                {state.stickyButtons}
                            </div>
                        )}
                    </div>
                </div>
                <div class={cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR)}>
                    {state.buttons}
                </div>
            </div>
        );
    };

    const style = (): Record<string, any> => {
        void props.layout.getRevision();
        const nodeRect = node.getRect();
        const parent = node.getParent();
        const isHorizontal = (parent instanceof RowNode) && parent.getOrientation() === Orientation.HORZ;
        const flexSize = isHorizontal ? nodeRect.width : nodeRect.height;
        const s: Record<string, any> = {
            "flex": flexSize > 0 ? `0 0 ${flexSize}px` : `1 1 0%`,
            "min-width": node.getMinWidth() + "px",
            "min-height": node.getMinHeight() + "px",
            "max-width": node.getMaxWidth() + "px",
            "max-height": node.getMaxHeight() + "px",
        };

        if (
            node.getModel().getMaximizedTabset(props.layout.getWindowId()) !== undefined &&
            !node.isMaximized()
        ) {
            s.display = "none";
        }

        return s;
    };

    const tabLocation = () => node.getTabLocation() || "top";

    return (
        <div
            ref={selfRef}
            class={cm(CLASSES.FLEXLAYOUT__TABSET_CONTAINER)}
            style={style()}
        >
            <div
                class={cm(CLASSES.FLEXLAYOUT__TABSET)}
                data-layout-path={path()}
            >
                {tabLocation() === "top" && renderTabStrip()}
                <div
                    ref={contentRef}
                    class={cm(CLASSES.FLEXLAYOUT__TABSET_CONTENT)}
                    data-layout-path={path() + "/content"}
                />
                {tabLocation() !== "top" && renderTabStrip()}
            </div>
        </div>
    );
};
