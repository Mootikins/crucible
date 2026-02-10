import { For, JSX, Show, createSignal } from "solid-js";
import { CLASSES } from "../flexlayout/core/Types";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";

type PopupPosition = { left?: string; right?: string; top?: string; bottom?: string };

interface PopupMenuState {
    items: { index: number; node: TabNode }[];
    onSelect: (item: { index: number; node: TabNode }) => void;
    position: PopupPosition;
    parentNode: TabSetNode | BorderNode;
}

interface ContextMenuState {
    items: { label: string; action: () => void }[];
    position: { x: number; y: number };
}

interface PopupManagerOptions {
    getSelfRef: () => HTMLDivElement | undefined;
    getClassName: (defaultClassName: string) => string;
}

export interface PopupManager {
    showPopup: (
        triggerElement: HTMLElement,
        parentNode: TabSetNode | BorderNode,
        items: { index: number; node: TabNode }[],
        onSelect: (item: { index: number; node: TabNode }) => void,
    ) => void;
    hidePopup: () => void;
    showContextMenu: (event: MouseEvent, items: { label: string; action: () => void }[]) => void;
    hideContextMenu: () => void;
    cleanup: () => void;
    renderPopupMenu: () => JSX.Element | undefined;
    renderContextMenu: () => JSX.Element | undefined;
}

export function createPopupManager(options: PopupManagerOptions): PopupManager {
    const [popupMenu, setPopupMenu] = createSignal<PopupMenuState | undefined>(undefined);
    const [contextMenu, setContextMenu] = createSignal<ContextMenuState | undefined>(undefined);

    let popupCleanup: (() => void) | undefined;
    let contextMenuCleanup: (() => void) | undefined;

    const hidePopup = () => {
        setPopupMenu(undefined);
    };

    const hideContextMenu = () => {
        setContextMenu(undefined);
    };

    const showPopup = (
        triggerElement: HTMLElement,
        parentNode: TabSetNode | BorderNode,
        items: { index: number; node: TabNode }[],
        onSelect: (item: { index: number; node: TabNode }) => void,
    ) => {
        const selfRef = options.getSelfRef();
        const layoutRect = selfRef?.getBoundingClientRect();
        const triggerRect = triggerElement.getBoundingClientRect();
        if (!layoutRect) return;

        const position: PopupPosition = {};
        if (triggerRect.left < layoutRect.left + layoutRect.width / 2) {
            position.left = triggerRect.left - layoutRect.left + "px";
        } else {
            position.right = layoutRect.right - triggerRect.right + "px";
        }
        if (triggerRect.top < layoutRect.top + layoutRect.height / 2) {
            position.top = triggerRect.top - layoutRect.top + "px";
        } else {
            position.bottom = layoutRect.bottom - triggerRect.bottom + "px";
        }

        if (popupCleanup) popupCleanup();

        setPopupMenu({ items, onSelect, position, parentNode });

        const onDocPointerDown = (e: PointerEvent) => {
            const popupEl = options.getSelfRef()?.querySelector('[data-layout-path="/popup-menu"]');
            if (popupEl && popupEl.contains(e.target as globalThis.Node)) return;
            cleanup();
        };
        const onDocKeyDown = (e: KeyboardEvent) => {
            if (e.key === "Escape") cleanup();
        };
        const cleanup = () => {
            hidePopup();
            document.removeEventListener("pointerdown", onDocPointerDown);
            document.removeEventListener("keydown", onDocKeyDown);
            popupCleanup = undefined;
        };

        popupCleanup = cleanup;
        document.addEventListener("keydown", onDocKeyDown);
        requestAnimationFrame(() => {
            document.addEventListener("pointerdown", onDocPointerDown);
        });
    };

    const showContextMenu = (
        event: MouseEvent,
        items: { label: string; action: () => void }[],
    ) => {
        event.preventDefault();
        event.stopPropagation();

        if (contextMenuCleanup) contextMenuCleanup();

        const selfRef = options.getSelfRef();
        const layoutRect = selfRef?.getBoundingClientRect();
        if (!layoutRect) return;

        setContextMenu({
            items,
            position: {
                x: event.clientX - layoutRect.left,
                y: event.clientY - layoutRect.top,
            },
        });

        const onDocPointerDown = (e: PointerEvent) => {
            const menuEl = options.getSelfRef()?.querySelector('[data-layout-path="/context-menu"]');
            if (menuEl && menuEl.contains(e.target as globalThis.Node)) return;
            cleanup();
        };
        const onDocKeyDown = (e: KeyboardEvent) => {
            if (e.key === "Escape") cleanup();
        };
        const cleanup = () => {
            hideContextMenu();
            document.removeEventListener("pointerdown", onDocPointerDown);
            document.removeEventListener("keydown", onDocKeyDown);
            contextMenuCleanup = undefined;
        };

        contextMenuCleanup = cleanup;
        document.addEventListener("keydown", onDocKeyDown);
        requestAnimationFrame(() => {
            document.addEventListener("pointerdown", onDocPointerDown);
        });
    };

    const cleanup = () => {
        if (popupCleanup) popupCleanup();
        if (contextMenuCleanup) contextMenuCleanup();
    };

    const renderPopupMenu = () => (
        <Show when={popupMenu()}>
            {(menu) => {
                const pos = menu().position;
                return (
                    <div
                        class={options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER)}
                        style={{
                            position: "absolute",
                            "z-index": 1002,
                            left: pos.left,
                            right: pos.right,
                            top: pos.top,
                            bottom: pos.bottom,
                        }}
                        onPointerDown={(e) => e.stopPropagation()}
                    >
                        <div
                            class={options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU)}
                            data-layout-path="/popup-menu"
                            tabIndex={0}
                            ref={(el: HTMLDivElement) => requestAnimationFrame(() => el.focus())}
                        >
                            <For each={menu().items}>
                                {(item, i) => {
                                    let classes = options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
                                    if (menu().parentNode.getSelected() === item.index) {
                                        classes += " " + options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED);
                                    }
                                    return (
                                        <div
                                            class={classes}
                                            data-layout-path={"/popup-menu/tb" + i()}
                                            onClick={(event) => {
                                                menu().onSelect(item);
                                                hidePopup();
                                                event.stopPropagation();
                                            }}
                                        >
                                            {item.node.getName()}
                                        </div>
                                    );
                                }}
                            </For>
                        </div>
                    </div>
                );
            }}
        </Show>
    );

    const renderContextMenu = () => (
        <Show when={contextMenu()}>
            {(menu) => (
                <div
                    class={options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER)}
                    style={{ position: "absolute", inset: 0, "z-index": 1002 }}
                    onPointerDown={() => {
                        if (contextMenuCleanup) {
                            contextMenuCleanup();
                        } else {
                            hideContextMenu();
                        }
                    }}
                >
                    <div
                        class={options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU)}
                        data-layout-path="/context-menu"
                        tabIndex={0}
                        ref={(el: HTMLDivElement) => requestAnimationFrame(() => el.focus())}
                        style={{
                            position: "absolute",
                            left: menu().position.x + "px",
                            top: menu().position.y + "px",
                        }}
                        onPointerDown={(e) => e.stopPropagation()}
                    >
                        <Show when={menu().items.length === 0}>
                            <div
                                class={options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM)}
                                style={{ opacity: 0.5, cursor: "default" }}
                            >
                                No actions available
                            </div>
                        </Show>
                        <For each={menu().items}>
                            {(item) => (
                                <div
                                    class={options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM)}
                                    data-context-menu-item
                                    onClick={(event) => {
                                        item.action();
                                        if (contextMenuCleanup) {
                                            contextMenuCleanup();
                                        } else {
                                            hideContextMenu();
                                        }
                                        event.stopPropagation();
                                    }}
                                >
                                    {item.label}
                                </div>
                            )}
                        </For>
                    </div>
                </div>
            )}
        </Show>
    );

    return {
        showPopup,
        hidePopup,
        showContextMenu,
        hideContextMenu,
        cleanup,
        renderPopupMenu,
        renderContextMenu,
    };
}
