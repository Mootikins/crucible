import { Component } from "solid-js";
import { Orientation } from "../flexlayout/core/Orientation";
import { DockLocation } from "../flexlayout/core/DockLocation";
import { CLASSES } from "../flexlayout/core/Types";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { Splitter } from "./Splitter";
import type { ILayoutContext } from "./Layout";

export interface IBorderTabProps {
    layout: ILayoutContext;
    border: BorderNode;
    show: boolean;
}

export const BorderTab: Component<IBorderTabProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;

    const style = (): Record<string, any> => {
        const s: Record<string, any> = {};

        if (props.border.getOrientation() === Orientation.HORZ) {
            s.width = props.border.getSize() + "px";
            s["min-width"] = props.border.getMinSize() + "px";
            s["max-width"] = props.border.getMaxSize() + "px";
        } else {
            s.height = props.border.getSize() + "px";
            s["min-height"] = props.border.getMinSize() + "px";
            s["max-height"] = props.border.getMaxSize() + "px";
        }

        s.display = props.show ? "flex" : "none";

        return s;
    };

    const horizontal = () => props.border.getOrientation() === Orientation.HORZ;

    const className = props.layout.getClassName(CLASSES.FLEXLAYOUT__BORDER_TAB_CONTENTS);

    const location = props.border.getLocation();
    const isBeforeSplitter = location === DockLocation.LEFT || location === DockLocation.TOP;

    if (isBeforeSplitter) {
        return (
            <>
                <div ref={selfRef} style={style()} class={className} />
                {props.show && (
                    <Splitter layout={props.layout} node={props.border as any} index={0} horizontal={horizontal()} />
                )}
            </>
        );
    } else {
        return (
            <>
                {props.show && (
                    <Splitter layout={props.layout} node={props.border as any} index={0} horizontal={horizontal()} />
                )}
                <div ref={selfRef} style={style()} class={className} />
            </>
        );
    }
};
