import { Component, Show, createMemo } from "solid-js";
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

    const isEditing = createMemo(() => {
        void props.layout.getRevision();
        return props.layout.getEditingTab() === node;
    });

    const tabName = () => {
        void props.layout.getRevision();
        return node.getName();
    };

    const onClick = () => {
        if (!props.selected) {
            props.layout.doAction(Action.selectTab(node.getId()));
        }
    };

    const onDoubleClick = (event: MouseEvent) => {
        if (node.isEnableRename()) {
            event.stopPropagation();
            props.layout.setEditingTab(node);
        }
    };

    const onTextBoxKeyPress = (event: KeyboardEvent) => {
        if (event.code === "Escape") {
            props.layout.setEditingTab(undefined);
        } else if (event.code === "Enter" || event.code === "NumpadEnter") {
            props.layout.doAction(Action.renameTab(node.getId(), (event.target as HTMLInputElement).value));
            props.layout.setEditingTab(undefined);
        }
    };

    const onTextBoxPointerDown = (event: PointerEvent) => {
        event.stopPropagation();
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

    const renderContent = (): ITabRenderValues => {
        const renderState: ITabRenderValues = {
            leading: undefined,
            content: undefined,
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

    const initialState = renderContent();

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
            {initialState.leading && (
                <div class={cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_LEADING)}>
                    {initialState.leading}
                </div>
            )}
            <Show
                when={isEditing()}
                fallback={
                    <div class={cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_CONTENT)}>
                        {tabName()}
                    </div>
                }
            >
                <input
                    ref={(el) => requestAnimationFrame(() => { el.focus(); el.select(); })}
                    class={cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_TEXTBOX)}
                    data-layout-path={props.path + "/textbox"}
                    type="text"
                    value={tabName()}
                    onKeyDown={onTextBoxKeyPress}
                    onPointerDown={onTextBoxPointerDown}
                />
            </Show>
            {initialState.buttons}
        </div>
    );
};
