import { Component, JSX } from "solid-js";
import { TabNode } from "../flexlayout/model/TabNode";
import { CLASSES } from "../flexlayout/core/Types";
import type { ILayoutContext, ITabRenderValues } from "./Layout";

export interface ITabButtonStampProps {
    node: TabNode;
    layout: ILayoutContext;
}

export const TabButtonStamp: Component<ITabButtonStampProps> = (props) => {
    const cm = props.layout.getClassName;

    const classNames = cm(CLASSES.FLEXLAYOUT__TAB_BUTTON_STAMP);

    const renderState = (): { leading: JSX.Element | undefined; content: JSX.Element | undefined } => {
        const state: ITabRenderValues = {
            leading: undefined,
            content: <span>{props.node.getName()}</span>,
            buttons: [],
        };

        if (props.node.getIcon()) {
            state.leading = <img src={props.node.getIcon()} alt="" />;
        }

        props.layout.customizeTab(props.node, state);

        return {
            leading: state.leading,
            content: state.content,
        };
    };

    return (
        <div
            class={classNames}
            title={props.node.getHelpText()}
        >
            {(() => {
                const state = renderState();
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
                    </>
                );
            })()}
        </div>
    );
};
