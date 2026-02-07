import { Component } from "solid-js";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import type { ILayoutContext } from "./Layout";

export interface ITabProps {
    layout: ILayoutContext;
    node: TabNode;
    selected: boolean;
    path: string;
}

export const Tab: Component<ITabProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;

    const cm = props.layout.getClassName;
    const node = props.node;
    const parentNode = () => node.getParent() as TabSetNode | BorderNode;

    const onPointerDown = () => {
        const parent = node.getParent()!;
        if (parent instanceof TabSetNode) {
            if (!parent.isActive()) {
                props.layout.doAction(
                    Action.setActiveTabset(parent.getId(), props.layout.getWindowId()),
                );
            }
        }
    };

    const style = (): Record<string, any> => {
        const parent = parentNode();
        const contentRect =
            parent instanceof TabSetNode
                ? parent.getContentRect()
                : parent.getRect();

        const s: Record<string, any> = {};
        contentRect.styleWithPosition(s);

        if (!props.selected) {
            s.display = "none";
        }

        if (parent instanceof TabSetNode) {
            if (
                node.getModel().getMaximizedTabset(props.layout.getWindowId()) !== undefined
            ) {
                if (parent.isMaximized()) {
                    s["z-index"] = 10;
                } else {
                    s.display = "none";
                }
            }
        }

        if (parent instanceof BorderNode) {
            if (!parent.isShowing()) {
                s.display = "none";
            }
        }

        return s;
    };

    const className = (): string => {
        let cls = cm(CLASSES.FLEXLAYOUT__TAB);
        const parent = parentNode();
        if (parent instanceof BorderNode) {
            cls += " " + cm(CLASSES.FLEXLAYOUT__TAB_BORDER);
            cls += " " + cm(CLASSES.FLEXLAYOUT__TAB_BORDER_ + parent.getLocation().getName());
        }

        if (node.getContentClassName() !== undefined) {
            cls += " " + node.getContentClassName();
        }

        return cls;
    };

    return (
        <div
            ref={selfRef}
            style={style()}
            class={className()}
            data-layout-path={props.path}
            onPointerDown={onPointerDown}
        >
            {props.layout.factory(node)}
        </div>
    );
};
