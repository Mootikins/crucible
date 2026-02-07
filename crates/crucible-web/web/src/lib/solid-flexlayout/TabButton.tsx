import { Component, JSX } from "solid-js";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import type { ILayoutContext, ITabRenderValues } from "./Layout";

export interface ITabButtonProps {
    layout: ILayoutContext;
    node: TabNode;
    selected: boolean;
    path: string;
}

export const TabButton: Component<ITabButtonProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;

    const cm = props.layout.getClassName;
    const node = props.node;

    const onClick = () => {
        props.layout.doAction(Action.selectTab(node.getId()));
    };

    const onDoubleClick = (event: MouseEvent) => {
        if (node.isEnableRename()) {
            event.stopPropagation();
        }
    };

    const onDragStart = (event: DragEvent) => {
        if (node.isEnableDrag()) {
            event.stopPropagation();
            props.layout.setDragNode(event, node);
        } else {
            event.preventDefault();
        }
    };

    const onDragEnd = () => {
        props.layout.clearDragMain();
    };

    const onClose = (event: MouseEvent) => {
        props.layout.doAction(Action.deleteTab(node.getId()));
        event.stopPropagation();
    };

    const onClosePointerDown = (event: PointerEvent) => {
        event.stopPropagation();
    };

    const parentNode = () => node.getParent() as TabSetNode;

    const classNames = (): string => {
        const isStretch =
            parentNode().isEnableSingleTabStretch() &&
            parentNode().getChildren().length === 1;
        const baseClassName = isStretch
            ? CLASSES.FLEXLAYOUT__TAB_BUTTON_STRETCH
            : CLASSES.FLEXLAYOUT__TAB_BUTTON;
        let classes = cm(baseClassName);
        classes += " " + cm(baseClassName + "_" + (parentNode().getTabLocation() || "top"));

        if (!isStretch) {
            if (props.selected) {
                classes += " " + cm(baseClassName + "--selected");
            } else {
                classes += " " + cm(baseClassName + "--unselected");
            }
        }

        if (node.getClassName() !== undefined) {
            classes += " " + node.getClassName();
        }

        return classes;
    };

    const renderContent = (): { leading: JSX.Element | undefined; content: JSX.Element | undefined; buttons: JSX.Element[] } => {
        const renderState: ITabRenderValues = {
            leading: undefined,
            content: <span>{node.getName()}</span>,
            buttons: [],
        };

        if (node.getIcon()) {
            renderState.leading = (
                <img src={node.getIcon()} alt="" />
            );
        }

        props.layout.customizeTab(node, renderState);

        if (node.isEnableClose()) {
            const isStretch =
                parentNode().isEnableSingleTabStretch() &&
                parentNode().getChildren().length === 1;
            if (!isStretch) {
                renderState.buttons.push(
                    <div
                        data-layout-path={props.path + "/button/close"}
                        title="Close"
                        class={cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_TRAILING)}
                        onPointerDown={onClosePointerDown}
                        onClick={onClose}
                    >
                        ✕
                    </div>,
                );
            }
        }

        return renderState;
    };

    return (
        <div
            ref={selfRef}
            data-layout-path={props.path}
            class={classNames()}
            onClick={onClick}
            onDblClick={onDoubleClick}
            title={node.getHelpText()}
            draggable={true}
            onDragStart={onDragStart}
            onDragEnd={onDragEnd}
        >
            {(() => {
                const state = renderContent();
                return (
                    <>
                        {state.leading && (
                            <div class={cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_LEADING)}>
                                {state.leading}
                            </div>
                        )}
                        {state.content && (
                            <div class={cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_CONTENT)}>
                                {state.content}
                            </div>
                        )}
                        {state.buttons}
                    </>
                );
            })()}
        </div>
    );
};
