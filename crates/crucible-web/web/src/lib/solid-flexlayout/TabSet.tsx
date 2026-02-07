import { Component, JSX, createEffect } from "solid-js";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
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

    const cm = props.layout.getClassName;
    const node = props.node;
    const path = () => node.getPath();

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
    });

    const onPointerDown = () => {
        props.layout.doAction(
            Action.setActiveTabset(node.getId(), props.layout.getWindowId()),
        );
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

    const onClose = (event: MouseEvent) => {
        props.layout.doAction(Action.deleteTabset(node.getId()));
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

    const renderTabStrip = (): JSX.Element => {
        void props.layout.getRevision();
        const tabs: JSX.Element[] = [];
        const children = node.getChildren();

        if (node.isEnableTabStrip()) {
            for (let i = 0; i < children.length; i++) {
                const child = children[i] as TabNode;
                const isSelected = node.getSelected() === i;
                tabs.push(
                    <TabButton
                        layout={props.layout}
                        node={child}
                        path={path() + "/tb" + i}
                        selected={isSelected}
                    />,
                );
                if (i < children.length - 1) {
                    tabs.push(
                        <div class={cm(CLASSES.FLEXLAYOUT__TABSET_TAB_DIVIDER)} />,
                    );
                }
            }
        }

        const renderState: ITabSetRenderValues = {
            leading: undefined,
            stickyButtons: [],
            buttons: [],
            overflowPosition: undefined,
        };
        props.layout.customizeTabSet(node, renderState);

        const buttons: JSX.Element[] = [...renderState.buttons];

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
                    onClick={onMaximizeToggle}
                >
                    {node.isMaximized() ? "⊡" : "⊞"}
                </button>,
            );
        }

        if (!node.isMaximized() && node.isEnableClose()) {
            buttons.push(
                <button
                    data-layout-path={path() + "/button/close"}
                    title="Close"
                    class={
                        cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON) +
                        " " +
                        cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_CLOSE)
                    }
                    onClick={onClose}
                >
                    ✕
                </button>,
            );
        }

        const buttonbar = (
            <div class={cm(CLASSES.FLEXLAYOUT__TAB_TOOLBAR)}>
                {buttons}
            </div>
        );

        let tabStripClasses = cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER);
        tabStripClasses += " " + CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER_ + (node.getTabLocation() || "top");

        if (node.isActive()) {
            tabStripClasses += " " + cm(CLASSES.FLEXLAYOUT__TABSET_SELECTED);
        }

        if (node.isMaximized()) {
            tabStripClasses += " " + cm(CLASSES.FLEXLAYOUT__TABSET_MAXIMIZED);
        }

        return (
            <div
                ref={tabStripRef}
                class={tabStripClasses}
                data-layout-path={path() + "/tabstrip"}
                onPointerDown={onPointerDown}
                onDblClick={onDoubleClick}
                draggable={true}
                onDragStart={onDragStart}
            >
                <div
                    class={cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER) + " " + cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_ + (node.getTabLocation() || "top"))}
                    style={{ "overflow-x": "auto", "overflow-y": "hidden" }}
                >
                    <div
                        class={cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER) + " " + cm(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER_ + (node.getTabLocation() || "top"))}
                    >
                        {tabs}
                    </div>
                </div>
                {buttonbar}
            </div>
        );
    };

    const style = (): Record<string, any> => {
        void props.layout.getRevision();
        const s: Record<string, any> = {
            "flex-grow": Math.max(1, node.getWeight() * 1000),
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
