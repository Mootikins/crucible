import { Component, JSX } from "solid-js";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { TabNode } from "../flexlayout/model/TabNode";
import { DockLocation } from "../flexlayout/core/DockLocation";
import { Orientation } from "../flexlayout/core/Orientation";
import { CLASSES } from "../flexlayout/core/Types";
import { BorderButton } from "./BorderButton";
import type { ILayoutContext, ITabSetRenderValues } from "./Layout";

export interface IBorderTabSetProps {
    border: BorderNode;
    layout: ILayoutContext;
    size: number;
}

export const BorderTabSet: Component<IBorderTabSetProps> = (props) => {
    const cm = props.layout.getClassName;
    const border = props.border;

    const borderClasses = (): string => {
        let classes = cm(CLASSES.FLEXLAYOUT__BORDER) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_ + border.getLocation().getName());
        if (border.getClassName() !== undefined) {
            classes += " " + border.getClassName();
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
            style={{
                display: "flex",
                "flex-direction": border.getOrientation() === Orientation.VERT ? "row" : "column",
            }}
            class={borderClasses()}
            data-layout-path={border.getPath()}
        >
            <div class={cm(CLASSES.FLEXLAYOUT__MINI_SCROLLBAR_CONTAINER)}>
                <div
                    class={cm(CLASSES.FLEXLAYOUT__BORDER_INNER) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_INNER_ + border.getLocation().getName())}
                    style={outerStyle()}
                >
                    <div
                        style={innerStyle()}
                        class={cm(CLASSES.FLEXLAYOUT__BORDER_INNER_TAB_CONTAINER) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_INNER_TAB_CONTAINER_ + border.getLocation().getName())}
                    >
                        {tabButtons()}
                    </div>
                </div>
            </div>
            <div class={cm(CLASSES.FLEXLAYOUT__BORDER_TOOLBAR) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_TOOLBAR_ + border.getLocation().getName())}>
                {toolbarButtons()}
            </div>
        </div>
    );
};
