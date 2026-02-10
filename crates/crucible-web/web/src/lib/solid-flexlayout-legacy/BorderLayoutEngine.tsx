import { Component, JSX, Show, createMemo } from "solid-js";
import { DockLocation } from "../flexlayout/core/DockLocation";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { Model } from "../flexlayout/model/Model";
import { RowNode } from "../flexlayout/model/RowNode";
import { BorderTab } from "./BorderTab";
import { BorderTabSet } from "./BorderTabSet";
import { Row } from "./Row";
import type { ILayoutContext } from "./Layout";

const LOCATION_TIE_ORDER: Record<string, number> = { top: 0, right: 1, bottom: 2, left: 3 };
const BORDER_BAR_SIZE = 38;

export function computeNestingOrder(borders: BorderNode[]): BorderNode[] {
    return [...borders].sort((a, b) => {
        const priorityDiff = b.getPriority() - a.getPriority();
        if (priorityDiff !== 0) return priorityDiff;
        return (LOCATION_TIE_ORDER[a.getLocation().getName()] ?? 4)
            - (LOCATION_TIE_ORDER[b.getLocation().getName()] ?? 4);
    });
}

interface BorderLayoutEngineProps {
    model: Model;
    layoutContext: () => ILayoutContext;
    getClassName: (defaultClassName: string) => string;
    rect: () => { width: number; height: number };
    revision: () => number;
    layoutVersion: () => number;
    showHiddenBorder: () => DockLocation;
    doAction: (action: any) => void;
    setMainRef: (el: HTMLDivElement | undefined) => void;
}

export const BorderLayoutEngine: Component<BorderLayoutEngineProps> = (props) => {
    const hasBorders = createMemo(() => {
        void props.layoutVersion();
        return props.model.getBorderSet().getBorderMap().size > 0;
    });

    const borderData = createMemo(() => {
        void props.layoutVersion();
        const hiddenBorderLoc = props.showHiddenBorder();
        if (!hasBorders()) return null;
        const borders = props.model.getBorderSet().getBorderMap();
        const strips = new Map<string, { border: BorderNode; show: boolean }>();
        for (const [_, location] of DockLocation.values) {
            const border = borders.get(location);
            if (
                border &&
                border.isShowing() &&
                (!border.isAutoHide() ||
                    (border.isAutoHide() &&
                        (border.getChildren().length > 0 || hiddenBorderLoc === location)))
            ) {
                strips.set(location.getName(), { border, show: border.getSelected() !== -1 });
            }
        }
        return { strips };
    });

    const borderStrip = (loc: DockLocation) => {
        const data = borderData();
        const entry = data?.strips.get(loc.getName());
        if (!entry) return undefined;
        const isEmpty = entry.border.getChildren().length === 0;
        const isExpanded = entry.border.getDockState() === "expanded";
        const stripSize = (isExpanded && entry.show) || (isEmpty && !isExpanded) ? 0 : BORDER_BAR_SIZE;
        return <BorderTabSet layout={props.layoutContext()} border={entry.border} size={stripSize} />;
    };

    const borderContent = (loc: DockLocation) => {
        const data = borderData();
        const entry = data?.strips.get(loc.getName());
        return entry ? <BorderTab layout={props.layoutContext()} border={entry.border} show={entry.show} /> : undefined;
    };

    const fabArrow = (loc: DockLocation): string => {
        if (loc === DockLocation.LEFT) return "▶";
        if (loc === DockLocation.RIGHT) return "◀";
        if (loc === DockLocation.TOP) return "▼";
        return "▲";
    };

    const fabStyle = (loc: DockLocation): Record<string, any> => {
        const r = props.rect();
        const size = 20;
        const base: Record<string, any> = {
            position: "absolute",
            width: size + "px",
            height: size + "px",
            "z-index": 50,
        };
        if (loc === DockLocation.LEFT) {
            base.left = "0px";
            base.top = r.height / 2 - size / 2 + "px";
        } else if (loc === DockLocation.RIGHT) {
            base.right = "0px";
            base.top = r.height / 2 - size / 2 + "px";
        } else if (loc === DockLocation.TOP) {
            base.top = "0px";
            base.left = r.width / 2 - size / 2 + "px";
        } else {
            base.bottom = "0px";
            base.left = r.width / 2 - size / 2 + "px";
        }
        return base;
    };

    const emptyCollapsedBorderFabs = () => {
        void props.layoutVersion();
        void props.revision();
        if (!hasBorders()) return null;
        const borders = props.model.getBorderSet().getBorderMap();
        const fabs: JSX.Element[] = [];
        for (const [_, location] of DockLocation.values) {
            const border = borders.get(location);
            if (
                border &&
                border.isShowing() &&
                border.getDockState() === "collapsed" &&
                border.getChildren().length === 0
            ) {
                const loc = border.getLocation();
                fabs.push(
                    <button
                        class={props.getClassName(CLASSES.FLEXLAYOUT__BORDER_FAB)}
                        data-layout-path={border.getPath() + "/fab"}
                        style={fabStyle(loc)}
                        onClick={() => props.doAction(Action.setDockState(border.getId(), "expanded"))}
                    >
                        {fabArrow(loc)}
                    </button>,
                );
            }
        }
        return fabs;
    };

    const mainContent = (
        <div
            ref={(el) => props.setMainRef(el)}
            class={props.getClassName(CLASSES.FLEXLAYOUT__LAYOUT_MAIN)}
            data-layout-path="/main"
            style={{ position: "absolute", top: "0", left: "0", bottom: "0", right: "0", display: "flex" }}
        >
            <Show when={props.rect().width > 0 && props.model.getRoot()}>
                <Row layout={props.layoutContext()} node={props.model.getRoot() as RowNode} />
            </Show>
        </div>
    );

    const renderNestedBorders = (): JSX.Element => {
        const data = borderData();
        if (!data) return mainContent;

        const visibleBorders = props.model.getBorderSet().getBordersByPriority();
        const sorted = computeNestingOrder(visibleBorders.filter((b) => data.strips.has(b.getLocation().getName())));
        const classBorderOuter = props.getClassName(CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER);
        const classBorderInner = props.getClassName(CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER_INNER);

        let current: JSX.Element = mainContent;
        for (let i = sorted.length - 1; i >= 0; i--) {
            const border = sorted[i];
            const loc = border.getLocation();
            const isHorz = loc === DockLocation.TOP || loc === DockLocation.BOTTOM;
            const flexDir = isHorz ? "column" : "row";
            const isStart = loc === DockLocation.LEFT || loc === DockLocation.TOP;

            const content = borderContent(loc);
            current = (
                <div class={classBorderInner} style={{ "flex-direction": flexDir }}>
                    {isStart ? content : null}
                    {current}
                    {!isStart ? content : null}
                </div>
            );

            const strip = borderStrip(loc);
            const wrapperClass = i === 0 ? classBorderOuter : classBorderInner;
            current = (
                <div class={wrapperClass} style={{ "flex-direction": flexDir }}>
                    {isStart ? strip : null}
                    {current}
                    {!isStart ? strip : null}
                </div>
            );
        }

        return current;
    };

    return (
        <>
            {renderNestedBorders()}
            {emptyCollapsedBorderFabs()}
        </>
    );
};
