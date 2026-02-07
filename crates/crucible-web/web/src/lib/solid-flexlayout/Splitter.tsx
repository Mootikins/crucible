import { Component, createSignal } from "solid-js";
import { RowNode } from "../flexlayout/model/RowNode";
import { Orientation } from "../flexlayout/core/Orientation";
import { Rect } from "../flexlayout/core/Rect";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import type { ILayoutContext } from "./Layout";

export interface ISplitterProps {
    layout: ILayoutContext;
    node: RowNode;
    index: number;
    horizontal: boolean;
}

export const Splitter: Component<ISplitterProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;

    const [, setIsDragging] = createSignal(false);

    const size = () => props.node.getModel().getSplitterSize();

    let pBounds: number[] = [];
    let outlineDiv: HTMLDivElement | undefined;
    let dragStartX = 0;
    let dragStartY = 0;
    let initialSizes: { initialSizes: number[]; sum: number; startPosition: number } = {
        initialSizes: [],
        sum: 0,
        startPosition: 0,
    };

    const onPointerDown = (event: PointerEvent) => {
        event.stopPropagation();
        event.preventDefault();

        if (props.node instanceof RowNode) {
            initialSizes = props.node.getSplitterInitials(props.index);
        }

        pBounds = props.node.getSplitterBounds(props.index);
        const rootdiv = props.layout.getRootDiv();
        outlineDiv = document.createElement("div");
        outlineDiv.style.flexDirection = props.horizontal ? "row" : "column";
        outlineDiv.className = props.layout.getClassName(CLASSES.FLEXLAYOUT__SPLITTER_DRAG);
        outlineDiv.style.cursor = props.node.getOrientation() === Orientation.VERT ? "ns-resize" : "ew-resize";

        const r = selfRef?.getBoundingClientRect()!;
        const layoutRect = props.layout.getDomRect();
        const rect = new Rect(
            r.x - layoutRect.x,
            r.y - layoutRect.y,
            r.width,
            r.height,
        );

        dragStartX = event.clientX - r.x;
        dragStartY = event.clientY - r.y;

        rect.positionElement(outlineDiv);
        if (rootdiv) {
            rootdiv.appendChild(outlineDiv);
        }

        setIsDragging(true);

        const onMove = (e: PointerEvent) => {
            if (outlineDiv) {
                const clientRect = props.layout.getDomRect();
                if (props.node.getOrientation() === Orientation.VERT) {
                    outlineDiv.style.top = getBoundPosition(e.clientY - clientRect.y - dragStartY) + "px";
                } else {
                    outlineDiv.style.left = getBoundPosition(e.clientX - clientRect.x - dragStartX) + "px";
                }
            }
        };

        const onUp = () => {
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);

            if (outlineDiv) {
                let value = 0;
                if (props.node.getOrientation() === Orientation.VERT) {
                    value = outlineDiv.offsetTop;
                } else {
                    value = outlineDiv.offsetLeft;
                }

                const weights = props.node.calculateSplit(
                    props.index,
                    value,
                    initialSizes.initialSizes,
                    initialSizes.sum,
                    initialSizes.startPosition,
                );
                props.layout.doAction(
                    Action.adjustWeights(
                        props.node.getId(),
                        weights,
                        props.node.getOrientation().getName(),
                    ),
                );

                const rootdiv = props.layout.getRootDiv();
                if (rootdiv && outlineDiv) {
                    rootdiv.removeChild(outlineDiv);
                }
                outlineDiv = undefined;
            }

            setIsDragging(false);
        };

        document.addEventListener("pointermove", onMove);
        document.addEventListener("pointerup", onUp);
    };

    const getBoundPosition = (p: number): number => {
        let rtn = p;
        if (p < pBounds[0]) rtn = pBounds[0];
        if (p > pBounds[1]) rtn = pBounds[1];
        return rtn;
    };

    const cm = props.layout.getClassName;

    const style = (): Record<string, any> => {
        const s: Record<string, any> = {
            cursor: props.horizontal ? "ew-resize" : "ns-resize",
            "flex-direction": props.horizontal ? "column" : "row",
        };

        if (props.horizontal) {
            s.width = size() + "px";
            s["min-width"] = size() + "px";
        } else {
            s.height = size() + "px";
            s["min-height"] = size() + "px";
        }

        if (!(props.node instanceof RowNode)) {
            return s;
        }

        if (
            props.node.getModel().getMaximizedTabset(props.layout.getWindowId()) !== undefined
        ) {
            s.display = "none";
        }

        return s;
    };

    const className = (): string => {
        let cls =
            cm(CLASSES.FLEXLAYOUT__SPLITTER) +
            " " +
            cm(CLASSES.FLEXLAYOUT__SPLITTER_ + props.node.getOrientation().getName());
        return cls;
    };

    return (
        <div
            ref={selfRef}
            class={className()}
            style={style()}
            data-layout-path={props.node.getPath() + "/s" + (props.index - 1)}
            onPointerDown={onPointerDown}
        />
    );
};
