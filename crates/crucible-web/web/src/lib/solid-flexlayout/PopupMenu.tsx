import { Component, For, onMount } from "solid-js";
import { render } from "solid-js/web";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { CLASSES } from "../flexlayout/core/Types";
import { TabButtonStamp } from "./TabButtonStamp";
import type { ILayoutContext } from "./Layout";

export function showPopup(
    triggerElement: Element,
    parentNode: TabSetNode | BorderNode,
    items: { index: number; node: TabNode }[],
    onSelect: (item: { index: number; node: TabNode }) => void,
    layout: ILayoutContext,
) {
    const layoutDiv = layout.getRootDiv();
    const classNameMapper = layout.getClassName;
    const currentDocument = triggerElement.ownerDocument;
    const triggerRect = triggerElement.getBoundingClientRect();
    const layoutRect = layoutDiv?.getBoundingClientRect() ?? new DOMRect(0, 0, 100, 100);

    const elm = currentDocument.createElement("div");
    elm.className = classNameMapper(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER);
    if (triggerRect.left < layoutRect.left + layoutRect.width / 2) {
        elm.style.left = triggerRect.left - layoutRect.left + "px";
    } else {
        elm.style.right = layoutRect.right - triggerRect.right + "px";
    }

    if (triggerRect.top < layoutRect.top + layoutRect.height / 2) {
        elm.style.top = triggerRect.top - layoutRect.top + "px";
    } else {
        elm.style.bottom = layoutRect.bottom - triggerRect.bottom + "px";
    }

    if (layoutDiv) {
        layoutDiv.appendChild(elm);
    }

    const onHide = () => {
        dispose();
        if (layoutDiv) {
            layoutDiv.removeChild(elm);
        }
        elm.removeEventListener("pointerdown", onElementPointerDown);
        currentDocument.removeEventListener("pointerdown", onDocPointerDown);
    };

    const onElementPointerDown = (event: Event) => {
        event.stopPropagation();
    };

    const onDocPointerDown = () => {
        onHide();
    };

    elm.addEventListener("pointerdown", onElementPointerDown);
    currentDocument.addEventListener("pointerdown", onDocPointerDown);

    const dispose = render(
        () => (
            <PopupMenu
                parentNode={parentNode}
                onSelect={onSelect}
                onHide={onHide}
                items={items}
                classNameMapper={classNameMapper}
                layout={layout}
            />
        ),
        elm,
    );
}

interface IPopupMenuProps {
    parentNode: TabSetNode | BorderNode;
    items: { index: number; node: TabNode }[];
    onHide: () => void;
    onSelect: (item: { index: number; node: TabNode }) => void;
    classNameMapper: (defaultClassName: string) => string;
    layout: ILayoutContext;
}

const PopupMenu: Component<IPopupMenuProps> = (props) => {
    let divRef: HTMLDivElement | undefined;

    onMount(() => {
        if (divRef) {
            divRef.focus();
        }
    });

    const onItemClick = (item: { index: number; node: TabNode }, event: MouseEvent) => {
        props.onSelect(item);
        props.onHide();
        event.stopPropagation();
    };

    const onDragStart = (event: DragEvent, node: TabNode) => {
        event.stopPropagation();
        props.layout.setDragNode(event, node);
        setTimeout(() => {
            props.onHide();
        }, 0);
    };

    const onDragEnd = () => {
        props.layout.clearDragMain();
    };

    const handleKeyDown = (event: KeyboardEvent) => {
        if (event.key === "Escape") {
            props.onHide();
        }
    };

    return (
        <div
            class={props.classNameMapper(CLASSES.FLEXLAYOUT__POPUP_MENU)}
            ref={divRef}
            tabIndex={0}
            onKeyDown={handleKeyDown}
            data-layout-path="/popup-menu"
        >
            <For each={props.items}>
                {(item, i) => {
                    let classes = props.classNameMapper(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
                    if (props.parentNode.getSelected() === item.index) {
                        classes += " " + props.classNameMapper(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED);
                    }
                    return (
                        <div
                            class={classes}
                            data-layout-path={"/popup-menu/tb" + i()}
                            onClick={(event) => onItemClick(item, event)}
                            draggable={true}
                            onDragStart={(e) => onDragStart(e, item.node)}
                            onDragEnd={onDragEnd}
                            title={item.node.getHelpText()}
                        >
                            <TabButtonStamp
                                node={item.node}
                                layout={props.layout}
                            />
                        </div>
                    );
                }}
            </For>
        </div>
    );
};
