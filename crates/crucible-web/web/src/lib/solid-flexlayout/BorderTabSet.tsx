import { Component, JSX, createEffect, Show } from "solid-js";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { TabNode } from "../flexlayout/model/TabNode";
import { DockLocation } from "../flexlayout/core/DockLocation";
import { Orientation } from "../flexlayout/core/Orientation";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import { BorderButton } from "./BorderButton";
import type { ILayoutContext, ITabSetRenderValues } from "./Layout";

export interface IBorderTabSetProps {
    border: BorderNode;
    layout: ILayoutContext;
    size: number;
}

export const BorderTabSet: Component<IBorderTabSetProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;
    const cm = props.layout.getClassName;
    const border = props.border;

    createEffect(() => {
        void props.layout.getRevision();
        if (selfRef) {
            border.setTabHeaderRect(props.layout.getBoundingClientRect(selfRef));
        }
    });

    const dockState = (): string => {
        void props.layout.getRevision();
        return border.getDockState();
    };

    const isCollapsed = () => dockState() === "collapsed";
    const isHidden = () => dockState() === "hidden";

    const borderClasses = (): string => {
        let classes = cm(CLASSES.FLEXLAYOUT__BORDER) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_ + border.getLocation().getName());
        if (border.getClassName() !== undefined) {
            classes += " " + border.getClassName();
        }
        const state = dockState();
        if (state === "collapsed") {
            classes += " " + cm(CLASSES.FLEXLAYOUT__BORDER__COLLAPSED);
        } else if (state === "hidden") {
            classes += " " + cm(CLASSES.FLEXLAYOUT__BORDER__HIDDEN);
        }
        return classes;
    };

    const tabButtons = (): JSX.Element[] => {
        const buttons: JSX.Element[] = [];
        const children = border.getChildren();

        if (children.length === 0 && border.isEnableDrop()) {
            buttons.push(
                <div class={cm(CLASSES.FLEXLAYOUT__BORDER_EMPTY_PLACEHOLDER)}>
                    Drop tabs here
                </div>,
            );
            return buttons;
        }

        for (let i = 0; i < children.length; i++) {
            const isSelected = border.getSelected() === i;
            const child = children[i] as TabNode;

            buttons.push(
                <BorderButton
                    layout={props.layout}
                    border={border.getLocation().getName()}
                    node={child}
                    path={border.getPath() + "/tb" + i}
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

    const collapsedLabels = (): JSX.Element[] => {
        const labels: JSX.Element[] = [];
        const children = border.getChildren();

        if (children.length === 0 && border.isEnableDrop()) {
            labels.push(
                <div class={cm(CLASSES.FLEXLAYOUT__BORDER_EMPTY_PLACEHOLDER)}>
                    Drop tabs here
                </div>,
            );
            return labels;
        }

        for (let i = 0; i < children.length; i++) {
            const child = children[i] as TabNode;
            labels.push(
                <div class={cm(CLASSES.FLEXLAYOUT__BORDER_COLLAPSED_LABEL)}>
                    {child.getName()}
                </div>,
            );
        }
        return labels;
    };

    const onDockToggle = (event: MouseEvent) => {
        event.stopPropagation();
        const current = dockState();
        let next: "expanded" | "collapsed" | "hidden";
        if (current === "expanded") {
            next = "collapsed";
        } else if (current === "collapsed") {
            next = "hidden";
        } else {
            next = "expanded";
        }
        props.layout.doAction(Action.setDockState(border.getId(), next));
    };

    // Arrow direction: hidden = away from edge, expanded/collapsed = toward edge
    const dockIcon = (): string => {
        const state = dockState();
        const loc = border.getLocation();

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
        const state = dockState();
        if (state === "expanded") return "Collapse";
        if (state === "collapsed") return "Hide";
        return "Expand";
    };

    const toolbarButtons = (): JSX.Element[] => {
        let buttons: JSX.Element[] = [];
        let stickyButtons: JSX.Element[] = [];
        const renderState: ITabSetRenderValues = {
            leading: undefined,
            buttons,
            stickyButtons,
            overflowPosition: undefined,
        };
        props.layout.customizeTabSet(border, renderState);
        stickyButtons = renderState.stickyButtons;
        buttons = renderState.buttons;

        if (border.isEnableDock()) {
            buttons.push(
                <button
                    data-layout-path={border.getPath() + "/button/dock"}
                    title={dockTitle()}
                    class={cm(CLASSES.FLEXLAYOUT__BORDER_DOCK_BUTTON)}
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={onDockToggle}
                >
                    {dockIcon()}
                </button>,
            );
        }

        return buttons;
    };

    const innerStyle = (): Record<string, any> => {
        const loc = border.getLocation();
        const collapsed = isCollapsed();

        if (loc === DockLocation.LEFT) {
            if (collapsed) {
                return {
                    position: "absolute",
                    top: "0",
                    left: "0",
                    "white-space": "nowrap",
                    transform: "rotate(-90deg) translateX(-100%)",
                    "transform-origin": "top left",
                    "flex-direction": "row-reverse",
                };
            }
            return { right: "100%", top: "0" };
        } else if (loc === DockLocation.RIGHT) {
            if (collapsed) {
                return {
                    position: "absolute",
                    top: "0",
                    right: "0",
                    "white-space": "nowrap",
                    transform: "rotate(90deg) translateX(-100%)",
                    "transform-origin": "top right",
                    "flex-direction": "row",
                };
            }
            return { left: "100%", top: "0" };
        } else {
            return { left: "0" };
        }
    };

    const outerStyle = (): Record<string, any> => {
        const borderHeight = props.size - 1;
        const loc = border.getLocation();
        const isVertical = loc === DockLocation.LEFT || loc === DockLocation.RIGHT;

        if (isVertical) {
            const style: Record<string, any> = { width: borderHeight + "px" };
            if (isCollapsed()) {
                style.position = "relative";
                style.overflow = "visible";
                style.flex = "1";
            } else {
                style["overflow-y"] = "auto";
            }
            return style;
        } else {
            return { height: borderHeight + "px", "overflow-x": "auto" };
        }
    };

    const rootStyle = (): Record<string, any> => {
        const isVert = border.getOrientation() === Orientation.VERT;
        const loc = border.getLocation();
        const isVerticalBorder = loc === DockLocation.LEFT || loc === DockLocation.RIGHT;
        const style: Record<string, any> = {
            display: "flex",
            "flex-direction": isVert ? "row" : "column",
        };
        if (isCollapsed() && isVerticalBorder) {
            style["align-self"] = "stretch";
            style.overflow = "visible";
            style.position = "relative";
            style["z-index"] = "1";
        }
        return style;
    };

    const isExpanded = () => dockState() === "expanded";

    return (
        <div
            ref={selfRef}
            style={rootStyle()}
            class={borderClasses()}
            data-layout-path={border.getPath()}
        >
            <Show when={!isHidden() && (!isExpanded() || border.getChildren().length === 0)}>
                <div class={cm(CLASSES.FLEXLAYOUT__MINI_SCROLLBAR_CONTAINER)} style={isCollapsed() && (border.getLocation() === DockLocation.LEFT || border.getLocation() === DockLocation.RIGHT) ? { flex: "1", overflow: "visible" } : {}}>
                    <div
                        class={cm(CLASSES.FLEXLAYOUT__BORDER_INNER) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_INNER_ + border.getLocation().getName())}
                        style={outerStyle()}
                    >
                        <div
                            style={innerStyle()}
                            class={cm(CLASSES.FLEXLAYOUT__BORDER_INNER_TAB_CONTAINER) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_INNER_TAB_CONTAINER_ + border.getLocation().getName())}
                        >
                            <Show when={!isCollapsed()} fallback={collapsedLabels()}>
                                {tabButtons()}
                            </Show>
                        </div>
                    </div>
                </div>
            </Show>
            <Show when={!isExpanded()}>
                <div class={cm(CLASSES.FLEXLAYOUT__BORDER_TOOLBAR) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_TOOLBAR_ + border.getLocation().getName())}>
                    {toolbarButtons()}
                </div>
            </Show>
        </div>
    );
};
