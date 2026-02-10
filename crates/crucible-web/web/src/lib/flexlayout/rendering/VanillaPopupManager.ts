import { CLASSES } from "../core/Types";
import { BorderNode } from "../model/BorderNode";
import { TabNode } from "../model/TabNode";
import { TabSetNode } from "../model/TabSetNode";

type PopupPosition = { left?: string; right?: string; top?: string; bottom?: string };

interface PopupMenuState {
    items: Array<{ index: number; node: TabNode }>;
    onSelect: (item: { index: number; node: TabNode }) => void;
    position: PopupPosition;
    parentNode: TabSetNode | BorderNode;
}

interface ContextMenuState {
    items: Array<{ label: string; action: () => void }>;
    position: { x: number; y: number };
}

export interface IVanillaPopupManagerOptions {
    getSelfRef(): HTMLDivElement | undefined;
    getClassName(defaultClassName: string): string;
}

export class VanillaPopupManager {
    private popupMenu: PopupMenuState | undefined;
    private contextMenu: ContextMenuState | undefined;
    private popupCleanup: (() => void) | undefined;
    private contextCleanup: (() => void) | undefined;

    constructor(private readonly options: IVanillaPopupManagerOptions) {}

    showPopup(
        triggerElement: HTMLElement,
        parentNode: TabSetNode | BorderNode,
        items: Array<{ index: number; node: TabNode }>,
        onSelect: (item: { index: number; node: TabNode }) => void,
    ): void {
        const selfRef = this.options.getSelfRef();
        const layoutRect = selfRef?.getBoundingClientRect();
        const triggerRect = triggerElement.getBoundingClientRect();
        if (!layoutRect || !selfRef) {
            return;
        }

        const position: PopupPosition = {};
        if (triggerRect.left < layoutRect.left + layoutRect.width / 2) {
            position.left = `${triggerRect.left - layoutRect.left}px`;
        } else {
            position.right = `${layoutRect.right - triggerRect.right}px`;
        }
        if (triggerRect.top < layoutRect.top + layoutRect.height / 2) {
            position.top = `${triggerRect.top - layoutRect.top}px`;
        } else {
            position.bottom = `${layoutRect.bottom - triggerRect.bottom}px`;
        }

        if (this.popupCleanup) {
            this.popupCleanup();
        }

        this.popupMenu = { items, onSelect, position, parentNode };
        this.renderPopupMenu();

        const onDocPointerDown = (e: PointerEvent): void => {
            const popupEl = selfRef.querySelector('[data-layout-path="/popup-menu"]');
            if (popupEl && popupEl.contains(e.target as globalThis.Node)) {
                return;
            }
            cleanup();
        };

        const onDocKeyDown = (e: KeyboardEvent): void => {
            if (e.key === "Escape") {
                cleanup();
            }
        };

        const cleanup = (): void => {
            this.popupMenu = undefined;
            this.removePopupElement();
            document.removeEventListener("pointerdown", onDocPointerDown);
            document.removeEventListener("keydown", onDocKeyDown);
            this.popupCleanup = undefined;
        };

        this.popupCleanup = cleanup;
        document.addEventListener("keydown", onDocKeyDown);
        requestAnimationFrame(() => {
            document.addEventListener("pointerdown", onDocPointerDown);
        });
    }

    showContextMenu(
        event: MouseEvent,
        items: Array<{ label: string; action: () => void }>,
    ): void {
        event.preventDefault();
        event.stopPropagation();

        if (this.contextCleanup) {
            this.contextCleanup();
        }

        const selfRef = this.options.getSelfRef();
        const layoutRect = selfRef?.getBoundingClientRect();
        if (!layoutRect) {
            return;
        }

        this.contextMenu = {
            items,
            position: {
                x: event.clientX - layoutRect.left,
                y: event.clientY - layoutRect.top,
            },
        };
        this.renderContextMenu();

        const onDocPointerDown = (e: PointerEvent): void => {
            const menuEl = this.options.getSelfRef()?.querySelector('[data-layout-path="/context-menu"]');
            if (menuEl && menuEl.contains(e.target as globalThis.Node)) {
                return;
            }
            cleanup();
        };

        const onDocKeyDown = (e: KeyboardEvent): void => {
            if (e.key === "Escape") {
                cleanup();
            }
        };

        const cleanup = (): void => {
            this.contextMenu = undefined;
            this.removeContextMenuElement();
            document.removeEventListener("pointerdown", onDocPointerDown);
            document.removeEventListener("keydown", onDocKeyDown);
            this.contextCleanup = undefined;
        };

        this.contextCleanup = cleanup;
        document.addEventListener("keydown", onDocKeyDown);
        requestAnimationFrame(() => {
            document.addEventListener("pointerdown", onDocPointerDown);
        });
    }

    hidePopup(): void {
        if (this.popupCleanup) {
            this.popupCleanup();
        }
    }

    hideContextMenu(): void {
        if (this.contextCleanup) {
            this.contextCleanup();
        }
    }

    cleanup(): void {
        this.hidePopup();
        this.hideContextMenu();
    }

    private renderPopupMenu(): void {
        this.removePopupElement();
        const menu = this.popupMenu;
        const root = this.options.getSelfRef();
        if (!menu || !root) {
            return;
        }

        const container = document.createElement("div");
        container.className = this.options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER);
        container.dataset.layoutPath = "/popup-menu-container";
        container.style.position = "absolute";
        container.style.zIndex = "1002";
        if (menu.position.left) container.style.left = menu.position.left;
        if (menu.position.right) container.style.right = menu.position.right;
        if (menu.position.top) container.style.top = menu.position.top;
        if (menu.position.bottom) container.style.bottom = menu.position.bottom;
        container.addEventListener("pointerdown", (e) => e.stopPropagation());

        const popup = document.createElement("div");
        popup.className = this.options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU);
        popup.dataset.layoutPath = "/popup-menu";
        popup.tabIndex = 0;

        menu.items.forEach((item, index) => {
            let classes = this.options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
            if (menu.parentNode.getSelected() === item.index) {
                classes += ` ${this.options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED)}`;
            }

            const itemEl = document.createElement("div");
            itemEl.className = classes;
            itemEl.dataset.layoutPath = `/popup-menu/tb${index}`;
            itemEl.textContent = item.node.getName();
            itemEl.addEventListener("click", (event) => {
                menu.onSelect(item);
                this.hidePopup();
                event.stopPropagation();
            });
            popup.appendChild(itemEl);
        });

        container.appendChild(popup);
        root.appendChild(container);
        requestAnimationFrame(() => popup.focus());
    }

    private renderContextMenu(): void {
        this.removeContextMenuElement();
        const menu = this.contextMenu;
        const root = this.options.getSelfRef();
        if (!menu || !root) {
            return;
        }

        const container = document.createElement("div");
        container.className = this.options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER);
        container.dataset.layoutPath = "/context-menu-container";
        container.style.position = "absolute";
        container.style.inset = "0";
        container.style.zIndex = "1002";
        container.addEventListener("pointerdown", () => this.hideContextMenu());

        const popup = document.createElement("div");
        popup.className = this.options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU);
        popup.dataset.layoutPath = "/context-menu";
        popup.tabIndex = 0;
        popup.style.position = "absolute";
        popup.style.left = `${menu.position.x}px`;
        popup.style.top = `${menu.position.y}px`;
        popup.addEventListener("pointerdown", (e) => e.stopPropagation());

        if (menu.items.length === 0) {
            const itemEl = document.createElement("div");
            itemEl.className = this.options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
            itemEl.style.opacity = "0.5";
            itemEl.style.cursor = "default";
            itemEl.textContent = "No actions available";
            popup.appendChild(itemEl);
        } else {
            menu.items.forEach((item) => {
                const itemEl = document.createElement("div");
                itemEl.className = this.options.getClassName(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
                itemEl.setAttribute("data-context-menu-item", "");
                itemEl.textContent = item.label;
                itemEl.addEventListener("click", (event) => {
                    item.action();
                    this.hideContextMenu();
                    event.stopPropagation();
                });
                popup.appendChild(itemEl);
            });
        }

        container.appendChild(popup);
        root.appendChild(container);
        requestAnimationFrame(() => popup.focus());
    }

    private removePopupElement(): void {
        const el = this.options.getSelfRef()?.querySelector('[data-layout-path="/popup-menu-container"]');
        if (el && el.parentNode) {
            el.parentNode.removeChild(el);
        }
    }

    private removeContextMenuElement(): void {
        const el = this.options.getSelfRef()?.querySelector('[data-layout-path="/context-menu-container"]');
        if (el && el.parentNode) {
            el.parentNode.removeChild(el);
        }
    }
}
