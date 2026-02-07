import { Component, JSX } from "solid-js";
import { RowNode } from "../flexlayout/model/RowNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { Orientation } from "../flexlayout/core/Orientation";
import { CLASSES } from "../flexlayout/core/Types";
import type { ILayoutContext } from "./Layout";
import { TabSet } from "./TabSet";
import { Splitter } from "./Splitter";

export interface IRowProps {
    layout: ILayoutContext;
    node: RowNode;
}

export const Row: Component<IRowProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;

    const horizontal = () => props.node.getOrientation() === Orientation.HORZ;

    const items = (): JSX.Element[] => {
        void props.layout.getRevision();
        const result: JSX.Element[] = [];
        const children = props.node.getChildren();

        for (let i = 0; i < children.length; i++) {
            const child = children[i];

            if (i > 0) {
                result.push(
                    <Splitter
                        layout={props.layout}
                        node={props.node}
                        index={i}
                        horizontal={horizontal()}
                    />,
                );
            }

            if (child instanceof RowNode) {
                result.push(
                    <Row layout={props.layout} node={child} />,
                );
            } else if (child instanceof TabSetNode) {
                result.push(
                    <TabSet layout={props.layout} node={child} />,
                );
            }
        }

        return result;
    };

    const style = (): Record<string, any> => {
        void props.layout.getRevision();
        return {
            "flex-grow": Math.max(1, props.node.getWeight() * 1000),
            "min-width": props.node.getMinWidth() + "px",
            "min-height": props.node.getMinHeight() + "px",
            "max-width": props.node.getMaxWidth() + "px",
            "max-height": props.node.getMaxHeight() + "px",
            "flex-direction": horizontal() ? "row" : "column",
        };
    };

    return (
        <div
            ref={selfRef}
            class={props.layout.getClassName(CLASSES.FLEXLAYOUT__ROW)}
            data-layout-path={props.node.getPath()}
            style={style()}
        >
            {items()}
        </div>
    );
};
