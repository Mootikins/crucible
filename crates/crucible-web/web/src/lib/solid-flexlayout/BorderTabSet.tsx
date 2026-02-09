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
        if (border.getLocation() === DockLocation.LEFT) {
            return { right: "100%", top: "0" };
        } else if (border.getLocation() === DockLocation.RIGHT) {
            return { left: "100%", top: "0" };
        } else {
            return { left: "0" };
        }
    };

    const outerStyle = (): Record<string, any> => {
        const borderHeight = props.size - 1;
        if (border.getLocation() === DockLocation.LEFT || border.getLocation() === DockLocation.RIGHT) {
            return { width: borderHeight + "px", "overflow-y": "auto" };
        } else {
            return { height: borderHeight + "px", "overflow-x": "auto" };
        }
    };

    return (
        <div
            ref={selfRef}
            style={{
                display: "flex",
                "flex-direction": border.getOrientation() === Orientation.VERT ? "row" : "column",
            }}
            class={borderClasses()}
            data-layout-path={border.getPath()}
        >
            <Show when={!isHidden()}>
                <div class={cm(CLASSES.FLEXLAYOUT__MINI_SCROLLBAR_CONTAINER)}>
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
            <div class={cm(CLASSES.FLEXLAYOUT__BORDER_TOOLBAR) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_TOOLBAR_ + border.getLocation().getName())}>
                {toolbarButtons()}
            </div>
        </div>
    );
};
