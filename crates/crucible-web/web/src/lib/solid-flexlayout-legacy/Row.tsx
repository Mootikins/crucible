import { Component, createMemo, For } from "solid-js";
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

interface FlatChild {
    type: 'row' | 'tabset' | 'splitter';
    node?: RowNode | TabSetNode;
    key: string;
    splitterIndex?: number;
}

export const Row: Component<IRowProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;

    const horizontal = () => props.node.getOrientation() === Orientation.HORZ;

    const flatChildren = createMemo(() => {
        void props.layout.getRevision();
        const children = props.node.getChildren();
        const result: FlatChild[] = [];

        for (let i = 0; i < children.length; i++) {
            const child = children[i];

            if (i > 0) {
                result.push({ type: 'splitter', key: `splitter-${i}`, splitterIndex: i });
            }

            if (child instanceof RowNode) {
                result.push({ type: 'row', node: child, key: child.getId() });
            } else if (child instanceof TabSetNode) {
                result.push({ type: 'tabset', node: child, key: child.getId() });
            }
        }

        return result;
    });

    const style = (): Record<string, any> => {
        void props.layout.getRevision();
        const nodeRect = props.node.getRect();
        const parent = props.node.getParent();
        const isNested = parent instanceof RowNode;
        const parentHorizontal = isNested && parent.getOrientation() === Orientation.HORZ;
        const flexSize = parentHorizontal ? nodeRect.width : nodeRect.height;
        return {
            "flex": isNested && flexSize > 0 ? `0 0 ${flexSize}px` : `1 1 0%`,
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
            data-layout-path={(() => { void props.layout.getRevision(); return props.node.getPath(); })()}
            style={style()}
        >
            <For each={flatChildren()}>
                {(item) => {
                    if (item.type === 'splitter') {
                        return (
                            <Splitter
                                layout={props.layout}
                                node={props.node}
                                index={item.splitterIndex!}
                                horizontal={horizontal()}
                            />
                        );
                    } else if (item.type === 'row') {
                        return (
                            <Row
                                layout={props.layout}
                                node={item.node as RowNode}
                            />
                        );
                    } else {
                        return (
                            <TabSet
                                layout={props.layout}
                                node={item.node as TabSetNode}
                            />
                        );
                    }
                }}
            </For>
        </div>
    );
};
