import { Component } from "solid-js";
import { RowNode } from "../flexlayout/model/RowNode";
import { BorderNode } from "../flexlayout/model/BorderNode";
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
        const isRealtime = props.layout.isRealtimeResize();

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

        if (!isRealtime) {
            const rootdiv = props.layout.getRootDiv();
            outlineDiv = document.createElement("div");
            outlineDiv.style.flexDirection = props.horizontal ? "row" : "column";
            outlineDiv.className = props.layout.getClassName(CLASSES.FLEXLAYOUT__SPLITTER_DRAG);
            outlineDiv.style.cursor = props.node.getOrientation() === Orientation.VERT ? "ns-resize" : "ew-resize";
            rect.positionElement(outlineDiv);
            if (rootdiv) {
                rootdiv.appendChild(outlineDiv);
            }
        }

        const applyAtPosition = (position: number) => {
            if (props.node instanceof BorderNode) {
                const pos = props.node.calculateSplit(props.node, position);
                props.layout.doAction(
                    Action.adjustBorderSplit(props.node.getId(), pos),
                );
            } else {
                const weights = props.node.calculateSplit(
                    props.index,
                    position,
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
            }
        };

        const onMove = (e: PointerEvent) => {
            const clientRect = props.layout.getDomRect();
            const position = props.node.getOrientation() === Orientation.VERT
                ? getBoundPosition(e.clientY - clientRect.y - dragStartY)
                : getBoundPosition(e.clientX - clientRect.x - dragStartX);

            if (isRealtime) {
                applyAtPosition(position);
            } else if (outlineDiv) {
                if (props.node.getOrientation() === Orientation.VERT) {
                    outlineDiv.style.top = position + "px";
                } else {
                    outlineDiv.style.left = position + "px";
                }
            }
        };

        const onUp = (_e: PointerEvent) => {
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);

            if (!isRealtime && outlineDiv) {
                let value = 0;
                if (props.node.getOrientation() === Orientation.VERT) {
                    value = outlineDiv.offsetTop;
                } else {
                    value = outlineDiv.offsetLeft;
                }
                applyAtPosition(value);

                const rootdiv = props.layout.getRootDiv();
                if (rootdiv && outlineDiv) {
                    rootdiv.removeChild(outlineDiv);
                }
                outlineDiv = undefined;
            }
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
        void props.layout.getRevision();
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
            data-layout-path={(() => { void props.layout.getRevision(); return props.node.getPath() + "/s" + (props.index - 1); })()}
            onPointerDown={onPointerDown}
        />
    );
};
