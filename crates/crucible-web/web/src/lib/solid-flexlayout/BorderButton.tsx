import { Component, JSX } from "solid-js";
import { TabNode } from "../flexlayout/model/TabNode";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import { ICloseType } from "../flexlayout/model/ICloseType";
import type { ILayoutContext, ITabRenderValues } from "./Layout";

export interface IBorderButtonProps {
    layout: ILayoutContext;
    node: TabNode;
    selected: boolean;
    border: string;
    path: string;
}

export const BorderButton: Component<IBorderButtonProps> = (props) => {
    let selfRef: HTMLDivElement | undefined;
    let contentRef: HTMLInputElement | undefined;

    const cm = props.layout.getClassName;
    const node = props.node;

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

    const onClick = () => {
        props.layout.doAction(Action.selectTab(node.getId()));
    };

    const onEndEdit = (event: Event) => {
        if (event.target !== contentRef!) {
            document.body.removeEventListener("pointerdown", onEndEdit);
            props.layout.setEditingTab(undefined);
        }
    };

    const isClosable = (): boolean => {
        const closeType = node.getCloseType();
        if (props.selected || closeType === ICloseType.Always) {
            return true;
        }
        if (closeType === ICloseType.Visible) {
            if (window.matchMedia && window.matchMedia("(hover: hover) and (pointer: fine)").matches) {
                return true;
            }
        }
        return false;
    };

    const onClose = (event: MouseEvent) => {
        if (isClosable()) {
            props.layout.doAction(Action.deleteTab(node.getId()));
            event.stopPropagation();
        }
    };

    const onClosePointerDown = (event: PointerEvent) => {
        event.stopPropagation();
    };

    const onTextBoxPointerDown = (event: PointerEvent) => {
        event.stopPropagation();
    };

    const onTextBoxKeyPress = (event: KeyboardEvent) => {
        if (event.code === "Escape") {
            props.layout.setEditingTab(undefined);
        } else if (event.code === "Enter" || event.code === "NumpadEnter") {
            props.layout.setEditingTab(undefined);
            props.layout.doAction(Action.renameTab(node.getId(), (event.target as HTMLInputElement).value));
        }
    };

    const classNames = (): string => {
        let classes = cm(CLASSES.FLEXLAYOUT__BORDER_BUTTON) + " " + cm(CLASSES.FLEXLAYOUT__BORDER_BUTTON_ + props.border);

        if (props.selected) {
            classes += " " + cm(CLASSES.FLEXLAYOUT__BORDER_BUTTON__SELECTED);
        } else {
            classes += " " + cm(CLASSES.FLEXLAYOUT__BORDER_BUTTON__UNSELECTED);
        }

        if (node.getClassName() !== undefined) {
            classes += " " + node.getClassName();
        }

        return classes;
    };

    const renderContent = (): { leading: JSX.Element | undefined; content: JSX.Element | undefined; buttons: JSX.Element[] } => {
        let iconAngle = 0;
        if (node.getModel().isEnableRotateBorderIcons() === false) {
            if (props.border === "left") {
                iconAngle = 90;
            } else if (props.border === "right") {
                iconAngle = -90;
            }
        }

        const renderState: ITabRenderValues = {
            leading: undefined,
            content: <span>{node.getName()}</span>,
            buttons: [],
        };

        if (node.getIcon()) {
            const iconStyle: Record<string, string> = {};
            if (iconAngle !== 0) {
                iconStyle.transform = `rotate(${iconAngle}deg)`;
            }
            renderState.leading = <img src={node.getIcon()} alt="" style={iconStyle} />;
        }

        props.layout.customizeTab(node, renderState);

        if (props.layout.getEditingTab() === node) {
            renderState.content = (
                <input
                    ref={contentRef}
                    class={cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_TEXTBOX)}
                    data-layout-path={props.path + "/textbox"}
                    type="text"
                    autofocus={true}
                    value={node.getName()}
                    onKeyDown={onTextBoxKeyPress}
                    onPointerDown={onTextBoxPointerDown}
                />
            );
        }

        if (node.isEnableClose()) {
            renderState.buttons.push(
                <div
                    data-layout-path={props.path + "/button/close"}
                    title="Close"
                    class={cm(CLASSES.FLEXLAYOUT__BORDER_BUTTON_TRAILING)}
                    onPointerDown={onClosePointerDown}
                    onClick={onClose}
                >
                    ✕
                </div>,
            );
        }

        return renderState;
    };

    return (
        <div
            ref={selfRef}
            data-layout-path={props.path}
            class={classNames()}
            onClick={onClick}
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
                            <div class={cm(CLASSES.FLEXLAYOUT__BORDER_BUTTON_LEADING)}>
                                {state.leading}
                            </div>
                        )}
                        {state.content && (
                            <div class={cm(CLASSES.FLEXLAYOUT__BORDER_BUTTON_CONTENT)}>
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
