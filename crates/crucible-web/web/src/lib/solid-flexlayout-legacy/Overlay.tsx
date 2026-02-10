import { Component } from "solid-js";
import { CLASSES } from "../flexlayout/core/Types";
import type { ILayoutContext } from "./Layout";

export interface IOverlayProps {
    layout: ILayoutContext;
    show: boolean;
}

export const Overlay: Component<IOverlayProps> = (props) => {
    return (
        <div
            class={props.layout.getClassName(CLASSES.FLEXLAYOUT__LAYOUT_OVERLAY)}
            style={{ display: props.show ? "flex" : "none" }}
        />
    );
};
