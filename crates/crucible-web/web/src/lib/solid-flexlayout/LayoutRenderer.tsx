import { Component, For, JSX, Show } from "solid-js";
import { CLASSES } from "../flexlayout/core/Types";
import { DockLocation } from "../flexlayout/core/DockLocation";
import { Action } from "../flexlayout/model/Action";
import { Model } from "../flexlayout/model/Model";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { Node } from "../flexlayout/model/Node";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { Rect } from "../flexlayout/core/Rect";
import type { ILayoutContext, ILayoutProps } from "./LayoutTypes";
import { BorderLayoutEngine } from "./BorderLayoutEngine";

interface LayoutRendererProps {
    model: ILayoutProps["model"];
    factory: ILayoutProps["factory"];
    getClassName: (defaultClassName: string) => string;
    layoutContext: () => ILayoutContext;
    rect: () => Rect;
    revision: () => number;
    layoutVersion: () => number;
    showEdges: () => boolean;
    showOverlay: () => boolean;
    showHiddenBorder: () => DockLocation;
    doAction: (action: any) => void;
    renderFloatingWindows: () => JSX.Element;
    renderPopoutWindows: () => JSX.Element;
    renderPopupMenu: () => JSX.Element | undefined;
    renderContextMenu: () => JSX.Element | undefined;
    onDragEnterRaw: (event: DragEvent) => void;
    onDragLeaveRaw: (event: DragEvent) => void;
    onDragOver: (event: DragEvent) => void;
    onDrop: (event: DragEvent) => void;
    setSelfRef: (el: HTMLDivElement | undefined) => void;
    setMainRef: (el: HTMLDivElement | undefined) => void;
}

export const LayoutRenderer: Component<LayoutRendererProps> = (props) => {
    const mainTabNodes = () => {
        void props.revision();
        void props.layoutVersion();
        const tabs: Array<{ node: TabNode; parent: TabSetNode | BorderNode }> = [];

        const visitNode = (n: Node) => {
            if (n instanceof TabNode) {
                tabs.push({ node: n, parent: n.getParent() as TabSetNode | BorderNode });
            }
            for (const child of n.getChildren()) {
                visitNode(child as Node);
            }
        };

        const root = props.model.getRoot();
        if (root) visitNode(root as Node);

        for (const border of props.model.getBorderSet().getBorders()) {
            for (const child of border.getChildren()) {
                if (child instanceof TabNode) {
                    tabs.push({ node: child, parent: border });
                }
            }
        }

        return tabs;
    };

    return (
        <div
            ref={(el) => props.setSelfRef(el)}
            class={props.getClassName(CLASSES.FLEXLAYOUT__LAYOUT)}
            data-layout-path="/"
            onDragEnter={props.onDragEnterRaw}
            onDragLeave={props.onDragLeaveRaw}
            onDragOver={props.onDragOver}
            onDrop={props.onDrop}
            style={{ position: "relative", overflow: "hidden" }}
        >
            {props.showOverlay() && (
                <div
                    class={props.getClassName(CLASSES.FLEXLAYOUT__LAYOUT_OVERLAY)}
                    style={{ position: "absolute", inset: 0, "z-index": 998 }}
                />
            )}

            <Show when={props.showEdges()}>
                {(() => {
                    const edgeLength = 100;
                    const edgeWidth = 10;
                    const offset = edgeLength / 2;
                    const r = props.rect();
                    const cls = props.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT);
                    const radius = 50;
                    return (
                        <>
                            <div
                                class={cls + " " + props.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_TOP)}
                                style={{
                                    position: "absolute",
                                    top: "0px",
                                    left: r.width / 2 - offset + "px",
                                    width: edgeLength + "px",
                                    height: edgeWidth + "px",
                                    "border-bottom-left-radius": radius + "%",
                                    "border-bottom-right-radius": radius + "%",
                                    "z-index": 999,
                                }}
                            />
                            <div
                                class={cls + " " + props.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_LEFT)}
                                style={{
                                    position: "absolute",
                                    top: r.height / 2 - offset + "px",
                                    left: "0px",
                                    width: edgeWidth + "px",
                                    height: edgeLength + "px",
                                    "border-top-right-radius": radius + "%",
                                    "border-bottom-right-radius": radius + "%",
                                    "z-index": 999,
                                }}
                            />
                            <div
                                class={cls + " " + props.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_BOTTOM)}
                                style={{
                                    position: "absolute",
                                    top: r.height - edgeWidth + "px",
                                    left: r.width / 2 - offset + "px",
                                    width: edgeLength + "px",
                                    height: edgeWidth + "px",
                                    "border-top-left-radius": radius + "%",
                                    "border-top-right-radius": radius + "%",
                                    "z-index": 999,
                                }}
                            />
                            <div
                                class={cls + " " + props.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_RIGHT)}
                                style={{
                                    position: "absolute",
                                    top: r.height / 2 - offset + "px",
                                    left: r.width - edgeWidth + "px",
                                    width: edgeWidth + "px",
                                    height: edgeLength + "px",
                                    "border-top-left-radius": radius + "%",
                                    "border-bottom-left-radius": radius + "%",
                                    "z-index": 999,
                                }}
                            />
                        </>
                    );
                })()}
            </Show>

            <BorderLayoutEngine
                model={props.model}
                layoutContext={props.layoutContext}
                getClassName={props.getClassName}
                rect={props.rect}
                revision={props.revision}
                layoutVersion={props.layoutVersion}
                showHiddenBorder={props.showHiddenBorder}
                doAction={props.doAction}
                setMainRef={(el) => props.setMainRef(el)}
            />

            <Show when={props.rect().width > 0}>
                <For each={mainTabNodes()}>
                    {(tabEntry) => {
                        const tabStyle = (): Record<string, any> => {
                            void props.revision();
                            void props.layoutVersion();
                            const parent = tabEntry.parent;
                            const contentRect = parent.getContentRect();
                            const s: Record<string, any> = {};
                            if (contentRect.width > 0 && contentRect.height > 0) {
                                contentRect.styleWithPosition(s);
                            } else {
                                s.display = "none";
                            }
                            if (!tabEntry.node.isSelected()) {
                                s.display = "none";
                            }
                            return s;
                        };

                        const tabPath = () => {
                            void props.revision();
                            return tabEntry.node.getPath();
                        };

                        return (
                            <div
                                class={props.getClassName(CLASSES.FLEXLAYOUT__TAB)}
                                data-layout-path={tabPath()}
                                style={tabStyle()}
                                onPointerDown={() => {
                                    const p = tabEntry.node.getParent();
                                    if (p instanceof TabSetNode && !p.isActive()) {
                                        props.doAction(Action.setActiveTabset(p.getId(), Model.MAIN_WINDOW_ID));
                                    }
                                }}
                            >
                                {props.factory(tabEntry.node)}
                            </div>
                        );
                    }}
                </For>
            </Show>

            {props.renderFloatingWindows()}
            {props.renderPopoutWindows()}
            {props.renderPopupMenu()}
            {props.renderContextMenu()}
        </div>
    );
};
