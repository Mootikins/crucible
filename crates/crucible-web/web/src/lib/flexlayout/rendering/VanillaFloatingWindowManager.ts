import { Orientation } from "../core/Orientation";
import { Rect } from "../core/Rect";
import { DropInfo } from "../core/DropInfo";
import { CLASSES } from "../core/Types";
import { Action, type LayoutAction } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import { LayoutWindow } from "../model/LayoutWindow";
import { Model } from "../model/Model";
import { Node } from "../model/Node";
import { IDraggable } from "../model/IDraggable";
import { RowNode } from "../model/RowNode";
import { TabNode } from "../model/TabNode";
import { TabSetNode } from "../model/TabSetNode";
import type { IContentRenderer } from "./IContentRenderer";

export interface IFloatingWindowManagerOptions {
    model: Model;
    root: HTMLElement;
    getClassName(defaultClassName: string): string;
    doAction(action: LayoutAction): void;
    createContentRenderer(node: TabNode): IContentRenderer;
    isRealtimeResize(): boolean;
    setDragNode(event: DragEvent, node: Node): void;
    clearDragMain(): void;
}

interface IPanelRefs {
    panel: HTMLDivElement;
    titlebar: HTMLDivElement;
    title: HTMLDivElement;
    buttons: HTMLDivElement;
    content: HTMLDivElement;
    treeHost: HTMLDivElement;
    resizeHandles: HTMLDivElement[];
    contentContainers: Map<string, HTMLDivElement>;
    contentRenderers: Map<string, IContentRenderer>;
    dragEnterCount: number;
    dragging: boolean;
    dropInfo: DropInfo | undefined;
    outlineDiv: HTMLDivElement | undefined;
}

type ResizeEdge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

export class VanillaFloatingWindowManager {
    private static readonly MIN_WIDTH = 150;
    private static readonly MIN_HEIGHT = 80;

    private panels = new Map<string, IPanelRefs>();
    private floatZOrder: string[] = [];

    constructor(private readonly options: IFloatingWindowManagerOptions) {}

    render(): void {
        const windows = [...this.options.model.getwindowsMap().values()].filter((w) => w.windowType === "float");
        this.syncFloatZOrder(windows);
        const seen = new Set<string>();

        for (const win of windows) {
            seen.add(win.windowId);
            let refs = this.panels.get(win.windowId);
            if (!refs) {
                refs = this.createPanel(win.windowId);
                this.panels.set(win.windowId, refs);
                this.options.root.appendChild(refs.panel);
            }
            this.updatePanel(refs, win);
        }

        for (const [windowId, refs] of this.panels) {
            if (!seen.has(windowId)) {
                this.disposePanel(refs);
                this.panels.delete(windowId);
            }
        }
    }

    closeWindow(windowId: string): void {
        this.options.doAction(Action.closeWindow(windowId));
    }

    dispose(): void {
        for (const [, refs] of this.panels) {
            this.disposePanel(refs);
        }
        this.panels.clear();
        this.floatZOrder = [];
    }

    private syncFloatZOrder(windows: LayoutWindow[]): void {
        const windowIds = windows.map((w) => w.windowId);
        const existing = new Set(windowIds);
        let order = this.floatZOrder.length > 0 ? [...this.floatZOrder] : [...this.options.model.getFloatZOrder()];
        order = order.filter((id) => existing.has(id));

        for (const id of windowIds) {
            if (!order.includes(id)) {
                order.push(id);
            }
        }

        const changed = order.length !== this.floatZOrder.length || order.some((id, index) => this.floatZOrder[index] !== id);
        if (changed) {
            this.floatZOrder = order;
            this.options.model.setFloatZOrder([...order]);
            this.applyZIndices();
        }
    }

    private bringToFront(windowId: string): void {
        const filtered = this.floatZOrder.filter((id) => id !== windowId);
        filtered.push(windowId);
        const changed = filtered.length !== this.floatZOrder.length || filtered.some((id, index) => this.floatZOrder[index] !== id);
        if (!changed) {
            return;
        }
        this.floatZOrder = filtered;
        this.options.model.setFloatZOrder([...filtered]);
        this.applyZIndices();
    }

    private getFloatZIndex(windowId: string): number {
        const index = this.floatZOrder.indexOf(windowId);
        return 1000 + (index >= 0 ? index : 0);
    }

    private applyZIndices(): void {
        for (const [windowId, refs] of this.panels) {
            refs.panel.style.zIndex = String(this.getFloatZIndex(windowId));
        }
    }

    private createPanel(windowId: string): IPanelRefs {
        const panel = document.createElement("div");
        panel.className = this.options.getClassName(CLASSES.FLEXLAYOUT__FLOATING_PANEL);
        panel.dataset.windowId = windowId;
        panel.addEventListener("pointerdown", () => this.bringToFront(windowId));

        const titlebar = document.createElement("div");
        titlebar.className = this.options.getClassName(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR);
        titlebar.addEventListener("pointerdown", (event) => this.onMovePointerDown(event, windowId));

        const title = document.createElement("div");
        title.className = this.options.getClassName(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR_TITLE);

        const buttons = document.createElement("div");
        buttons.className = this.options.getClassName(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR_BUTTONS);

        const dockButton = document.createElement("button");
        dockButton.type = "button";
        dockButton.title = "Dock";
        dockButton.textContent = "⇱";
        dockButton.addEventListener("pointerdown", (event) => event.stopPropagation());
        dockButton.addEventListener("click", (event) => {
            event.stopPropagation();
            this.onDock(windowId);
        });

        const closeButton = document.createElement("button");
        closeButton.type = "button";
        closeButton.title = "Close";
        closeButton.textContent = "✕";
        closeButton.addEventListener("pointerdown", (event) => event.stopPropagation());
        closeButton.addEventListener("click", (event) => {
            event.stopPropagation();
            this.options.doAction(Action.closeWindow(windowId));
        });

        buttons.replaceChildren(dockButton, closeButton);
        titlebar.replaceChildren(title, buttons);

        const content = document.createElement("div");
        content.className = this.options.getClassName(CLASSES.FLEXLAYOUT__FLOATING_PANEL_CONTENT);

        const treeHost = document.createElement("div");
        treeHost.style.position = "absolute";
        treeHost.style.inset = "0";
        treeHost.style.display = "flex";
        content.appendChild(treeHost);

        const refs: IPanelRefs = {
            panel,
            titlebar,
            title,
            buttons,
            content,
            treeHost,
            resizeHandles: [],
            contentContainers: new Map<string, HTMLDivElement>(),
            contentRenderers: new Map<string, IContentRenderer>(),
            dragEnterCount: 0,
            dragging: false,
            dropInfo: undefined,
            outlineDiv: undefined,
        };

        content.addEventListener("dragenter", (event) => this.onFloatDragEnterRaw(event, windowId, refs));
        content.addEventListener("dragleave", (event) => this.onFloatDragLeaveRaw(event, refs));
        content.addEventListener("dragover", (event) => this.onFloatDragOver(event, windowId, refs));
        content.addEventListener("drop", (event) => this.onFloatDrop(event, refs));

        panel.replaceChildren(titlebar, content);
        refs.resizeHandles = this.createResizeHandles(panel, windowId);
        return refs;
    }

    private createResizeHandles(panel: HTMLDivElement, windowId: string): HTMLDivElement[] {
        const defs: Array<{ edge: ResizeEdge; className: CLASSES; extraClass?: CLASSES }> = [
            { edge: "n", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_N },
            { edge: "s", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_S },
            { edge: "e", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_E },
            { edge: "w", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_W },
            { edge: "nw", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_NW },
            { edge: "ne", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_NE },
            { edge: "sw", className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_SW },
            {
                edge: "se",
                className: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_SE,
                extraClass: CLASSES.FLEXLAYOUT__FLOATING_PANEL_RESIZE_HANDLE,
            },
        ];

        const handles: HTMLDivElement[] = [];
        for (const def of defs) {
            const handle = document.createElement("div");
            const baseClass = this.options.getClassName(def.className);
            handle.className = def.extraClass
                ? `${baseClass} ${this.options.getClassName(def.extraClass)}`
                : baseClass;
            handle.addEventListener("pointerdown", (event) => this.onResizePointerDown(event, windowId, def.edge));
            panel.appendChild(handle);
            handles.push(handle);
        }
        return handles;
    }

    private updatePanel(refs: IPanelRefs, win: LayoutWindow): void {
        refs.panel.style.left = `${win.rect.x}px`;
        refs.panel.style.top = `${win.rect.y}px`;
        refs.panel.style.width = `${win.rect.width}px`;
        refs.panel.style.height = `${win.rect.height}px`;
        refs.panel.style.zIndex = String(this.getFloatZIndex(win.windowId));

        refs.title.textContent = this.getTitle(win.root) || "Floating Panel";

        if (win.root) {
            const rowElement = this.renderRowNode(win.root, refs, win.windowId);
            refs.treeHost.replaceChildren(rowElement);
        } else {
            refs.treeHost.replaceChildren();
        }

        this.renderTabContents(refs, win);
    }

    private renderRowNode(node: RowNode, refs: IPanelRefs, windowId: string): HTMLDivElement {
        const rowEl = document.createElement("div");
        rowEl.className = this.options.getClassName(CLASSES.FLEXLAYOUT__ROW);
        rowEl.dataset.layoutPath = node.getPath();

        const parent = node.getParent();
        const isNested = parent instanceof RowNode;
        const parentHorizontal = isNested && parent.getOrientation() === Orientation.HORZ;
        const nodeRect = node.getRect();
        const flexSize = parentHorizontal ? nodeRect.width : nodeRect.height;

        rowEl.style.flex = isNested && flexSize > 0 ? `0 0 ${flexSize}px` : "1 1 0%";
        rowEl.style.minWidth = `${node.getMinWidth()}px`;
        rowEl.style.minHeight = `${node.getMinHeight()}px`;
        rowEl.style.maxWidth = `${node.getMaxWidth()}px`;
        rowEl.style.maxHeight = `${node.getMaxHeight()}px`;
        rowEl.style.flexDirection = node.getOrientation() === Orientation.HORZ ? "row" : "column";

        const children: HTMLElement[] = [];
        const modelChildren = node.getChildren();
        const horizontal = node.getOrientation() === Orientation.HORZ;

        for (let i = 0; i < modelChildren.length; i++) {
            if (i > 0) {
                children.push(this.renderSplitterNode(node, i, horizontal, refs));
            }
            const child = modelChildren[i];
            if (child instanceof RowNode) {
                children.push(this.renderRowNode(child, refs, windowId));
            } else if (child instanceof TabSetNode) {
                children.push(this.renderTabSetNode(child, refs, windowId));
            }
        }

        rowEl.replaceChildren(...children);
        return rowEl;
    }

    private renderSplitterNode(row: RowNode, index: number, horizontal: boolean, refs: IPanelRefs): HTMLDivElement {
        const splitter = document.createElement("div");
        splitter.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER_ + row.getOrientation().getName())}`;
        splitter.dataset.layoutPath = `${row.getPath()}/s${index - 1}`;
        splitter.style.cursor = horizontal ? "ew-resize" : "ns-resize";
        splitter.style.flexDirection = horizontal ? "column" : "row";

        const size = row.getModel().getSplitterSize();
        if (horizontal) {
            splitter.style.width = `${size}px`;
            splitter.style.minWidth = `${size}px`;
            splitter.style.height = "";
            splitter.style.minHeight = "";
        } else {
            splitter.style.height = `${size}px`;
            splitter.style.minHeight = `${size}px`;
            splitter.style.width = "";
            splitter.style.minWidth = "";
        }

        const maximized = row.getModel().getMaximizedTabset(row.getWindowId());
        splitter.style.display = maximized ? "none" : "";
        splitter.addEventListener("pointerdown", (event) => {
            this.onSplitterPointerDown(event, row, index, horizontal, splitter, refs.content);
        });
        return splitter;
    }

    private onSplitterPointerDown(
        event: PointerEvent,
        row: RowNode,
        index: number,
        horizontal: boolean,
        splitter: HTMLDivElement,
        panelContent: HTMLDivElement,
    ): void {
        event.stopPropagation();
        event.preventDefault();

        const initialSizes = row.getSplitterInitials(index);
        const bounds = row.getSplitterBounds(index);
        const isRealtime = this.options.isRealtimeResize();

        const contentRect = panelContent.getBoundingClientRect();
        const domRect = splitter.getBoundingClientRect();
        const splitterRect = new Rect(
            domRect.x - contentRect.x,
            domRect.y - contentRect.y,
            domRect.width,
            domRect.height,
        );

        const dragStartX = event.clientX - domRect.x;
        const dragStartY = event.clientY - domRect.y;

        let outlineDiv: HTMLDivElement | undefined;
        if (!isRealtime) {
            outlineDiv = document.createElement("div");
            outlineDiv.style.flexDirection = horizontal ? "row" : "column";
            outlineDiv.className = this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER_DRAG);
            outlineDiv.style.cursor = row.getOrientation() === Orientation.VERT ? "ns-resize" : "ew-resize";
            splitterRect.positionElement(outlineDiv);
            panelContent.appendChild(outlineDiv);
        }

        const clampPosition = (position: number): number => {
            return Math.max(bounds[0], Math.min(bounds[1], position));
        };

        const applyAtPosition = (position: number): void => {
            const weights = row.calculateSplit(
                index,
                position,
                initialSizes.initialSizes,
                initialSizes.sum,
                initialSizes.startPosition,
            );
            this.options.doAction(
                Action.adjustWeights(
                    row.getId(),
                    weights,
                    row.getOrientation().getName(),
                ),
            );
        };

        const onMove = (moveEvent: PointerEvent): void => {
            const position = row.getOrientation() === Orientation.VERT
                ? clampPosition(moveEvent.clientY - contentRect.y - dragStartY)
                : clampPosition(moveEvent.clientX - contentRect.x - dragStartX);

            if (isRealtime) {
                applyAtPosition(position);
            } else if (outlineDiv) {
                if (row.getOrientation() === Orientation.VERT) {
                    outlineDiv.style.top = `${position}px`;
                } else {
                    outlineDiv.style.left = `${position}px`;
                }
            }
        };

        const onUp = (): void => {
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);

            if (!isRealtime && outlineDiv) {
                const value = row.getOrientation() === Orientation.VERT
                    ? outlineDiv.offsetTop
                    : outlineDiv.offsetLeft;
                applyAtPosition(value);
                outlineDiv.remove();
                outlineDiv = undefined;
            }
        };

        document.addEventListener("pointermove", onMove);
        document.addEventListener("pointerup", onUp);
    }

    private renderTabSetNode(node: TabSetNode, refs: IPanelRefs, windowId: string): HTMLDivElement {
        const container = document.createElement("div");
        const tabset = document.createElement("div");
        const tabStrip = document.createElement("div");
        const tabStripInner = document.createElement("div");
        const tabContainer = document.createElement("div");
        const toolbar = document.createElement("div");
        const content = document.createElement("div");

        tabStripInner.appendChild(tabContainer);
        tabStrip.appendChild(tabStripInner);
        tabStrip.appendChild(toolbar);
        tabset.appendChild(tabStrip);
        tabset.appendChild(content);
        container.appendChild(tabset);

        const parent = node.getParent();
        const nodeRect = node.getRect();
        const isHorizontal = parent instanceof RowNode && parent.getOrientation() === Orientation.HORZ;
        const flexSize = isHorizontal ? nodeRect.width : nodeRect.height;
        container.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_CONTAINER);
        container.style.flex = flexSize > 0 ? `0 0 ${flexSize}px` : "1 1 0%";
        container.style.minWidth = `${node.getMinWidth()}px`;
        container.style.minHeight = `${node.getMinHeight()}px`;
        container.style.maxWidth = `${node.getMaxWidth()}px`;
        container.style.maxHeight = `${node.getMaxHeight()}px`;

        const maximized = node.getModel().getMaximizedTabset(windowId);
        container.style.display = maximized && !node.isMaximized() ? "none" : "";

        tabset.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET);
        tabset.dataset.layoutPath = node.getPath();
        tabset.dataset.state = node.isMaximized() ? "maximized" : (node.isActive() ? "active" : "inactive");

        const tabLocation = node.getTabLocation() || "top";
        tabStrip.className = this.getTabStripClasses(node, tabLocation);
        tabStrip.dataset.layoutPath = `${node.getPath()}/tabstrip`;

        tabStrip.addEventListener("pointerdown", (event) => {
            if (!node.isActive()) {
                this.options.doAction(Action.setActiveTabset(node.getId(), windowId));
            }
            event.stopPropagation();
        });

        tabStrip.addEventListener("dblclick", () => {
            if (node.canMaximize()) {
                this.options.doAction(Action.maximizeToggle(node.getId()));
            }
        });

        tabStrip.draggable = true;
        tabStrip.addEventListener("dragstart", (event) => {
            if (!node.isEnableDrag()) {
                event.preventDefault();
                return;
            }
            event.stopPropagation();
            this.options.setDragNode(event, node);
        });
        tabStrip.addEventListener("dragend", () => {
            this.options.clearDragMain();
        });

        tabStripInner.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_ + tabLocation)}`;
        tabStripInner.style.overflowX = "auto";
        tabStripInner.style.overflowY = "hidden";

        tabContainer.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER_ + tabLocation)}`;

        toolbar.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR);
        this.renderFloatingToolbar(node, toolbar);

        content.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_CONTENT);
        content.dataset.layoutPath = `${node.getPath()}/content`;

        const tabs = node.getChildren() as TabNode[];
        const tabChildren: HTMLElement[] = [];
        for (let i = 0; i < tabs.length; i++) {
            tabChildren.push(this.renderTabButtonNode(tabs[i], node, i));
            if (i < tabs.length - 1) {
                const divider = document.createElement("div");
                divider.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TAB_DIVIDER);
                tabChildren.push(divider);
            }
        }
        tabContainer.replaceChildren(...tabChildren);

        if (tabLocation === "top") {
            tabset.replaceChildren(tabStrip, content);
        } else {
            tabset.replaceChildren(content, tabStrip);
        }

        node.setRect(this.getBoundingClientRect(container, refs.content));
        node.setTabStripRect(this.getBoundingClientRect(tabStrip, refs.content));
        node.setContentRect(this.getBoundingClientRect(content, refs.content));
        return container;
    }

    private renderFloatingToolbar(node: TabSetNode, toolbar: HTMLDivElement): void {
        const buttons: HTMLButtonElement[] = [];

        if (node.canMaximize()) {
            const maximizeButton = document.createElement("button");
            maximizeButton.type = "button";
            maximizeButton.title = node.isMaximized() ? "Restore" : "Maximize";
            maximizeButton.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_ + (node.isMaximized() ? "max" : "min"))}`;
            maximizeButton.textContent = node.isMaximized() ? "⊡" : "⊞";
            maximizeButton.addEventListener("pointerdown", (event) => event.stopPropagation());
            maximizeButton.addEventListener("click", (event) => {
                event.stopPropagation();
                if (node.canMaximize()) {
                    this.options.doAction(Action.maximizeToggle(node.getId()));
                }
            });
            buttons.push(maximizeButton);
        }

        if (!node.isMaximized() && node.isEnableClose()) {
            const closeButton = document.createElement("button");
            closeButton.type = "button";
            closeButton.title = "Close";
            closeButton.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_CLOSE)}`;
            closeButton.textContent = "✕";
            closeButton.addEventListener("pointerdown", (event) => event.stopPropagation());
            closeButton.addEventListener("click", (event) => {
                event.stopPropagation();
                this.options.doAction(Action.deleteTabset(node.getId()));
            });
            buttons.push(closeButton);
        }

        toolbar.replaceChildren(...buttons);
    }

    private renderTabButtonNode(tab: TabNode, parent: TabSetNode, index: number): HTMLDivElement {
        const tabEl = document.createElement("div");
        const selected = parent.getSelected() === index;
        const path = `${parent.getPath()}/tb${index}`;
        const isStretch = parent.isEnableSingleTabStretch() && parent.getChildren().length === 1;
        const baseClass = isStretch ? CLASSES.FLEXLAYOUT__TAB_BUTTON_STRETCH : CLASSES.FLEXLAYOUT__TAB_BUTTON;

        let className = this.options.getClassName(baseClass);
        className += ` ${this.options.getClassName(baseClass + "_" + (parent.getTabLocation() || "top"))}`;
        if (!isStretch) {
            className += ` ${this.options.getClassName(baseClass + (selected ? "--selected" : "--unselected"))}`;
        }
        if (tab.getClassName()) {
            className += ` ${tab.getClassName()}`;
        }

        tabEl.className = className;
        tabEl.dataset.layoutPath = path;
        tabEl.dataset.state = selected ? "selected" : "unselected";
        tabEl.title = tab.getHelpText() ?? "";
        tabEl.draggable = true;

        tabEl.addEventListener("click", () => {
            if (!selected) {
                this.options.doAction(Action.selectTab(tab.getId()));
            }
        });

        tabEl.addEventListener("dragstart", (event) => {
            if (!tab.isEnableDrag()) {
                event.preventDefault();
                return;
            }
            event.stopPropagation();
            this.options.setDragNode(event, tab);
        });

        tabEl.addEventListener("dragend", () => {
            this.options.clearDragMain();
        });

        const children: HTMLElement[] = [];
        const icon = tab.getIcon();
        if (icon) {
            const leading = document.createElement("div");
            leading.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_LEADING);
            const img = document.createElement("img");
            img.src = icon;
            img.alt = "";
            leading.appendChild(img);
            children.push(leading);
        }

        const content = document.createElement("div");
        content.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_CONTENT);
        content.textContent = tab.getName();
        children.push(content);

        if (tab.isEnableClose() && !isStretch) {
            const close = document.createElement("div");
            close.title = "Close";
            close.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_TRAILING);
            close.textContent = "✕";
            close.addEventListener("pointerdown", (event) => event.stopPropagation());
            close.addEventListener("click", (event) => {
                event.stopPropagation();
                this.options.doAction(Action.deleteTab(tab.getId()));
            });
            children.push(close);
        }

        tabEl.replaceChildren(...children);
        return tabEl;
    }

    private renderTabContents(refs: IPanelRefs, win: LayoutWindow): void {
        const tabs = this.collectTabNodes(win.root);
        const seen = new Set<string>();

        for (const { node, parent } of tabs) {
            const tabId = node.getId();
            seen.add(tabId);

            let container = refs.contentContainers.get(tabId);
            if (!container) {
                container = document.createElement("div");
                container.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB);
                container.dataset.layoutPath = node.getPath();
                container.addEventListener("pointerdown", () => {
                    const p = node.getParent();
                    if (p instanceof TabSetNode && !p.isActive()) {
                        this.options.doAction(Action.setActiveTabset(p.getId(), win.windowId));
                    }
                });
                refs.content.appendChild(container);
                refs.contentContainers.set(tabId, container);

                const renderer = this.options.createContentRenderer(node);
                renderer.init(container, {
                    node,
                    selected: node.isSelected(),
                    windowId: win.windowId,
                });
                refs.contentRenderers.set(tabId, renderer);
            }

            const contentRect = parent.getContentRect();
            if (contentRect.width > 0 && contentRect.height > 0 && node.isSelected()) {
                const style = contentRect.styleWithPosition({});
                Object.assign(container.style, style);
                container.style.display = "block";
            } else {
                container.style.display = "none";
            }

            container.dataset.layoutPath = node.getPath();

            const renderer = refs.contentRenderers.get(tabId);
            renderer?.update({ selected: node.isSelected(), windowId: win.windowId });
        }

        for (const [tabId, container] of refs.contentContainers) {
            if (!seen.has(tabId)) {
                refs.contentRenderers.get(tabId)?.dispose();
                refs.contentRenderers.delete(tabId);
                refs.contentContainers.delete(tabId);
                container.remove();
            }
        }
    }

    private collectTabNodes(root: RowNode | undefined): Array<{ node: TabNode; parent: TabSetNode | BorderNode }> {
        const tabs: Array<{ node: TabNode; parent: TabSetNode | BorderNode }> = [];
        if (!root) {
            return tabs;
        }

        const visit = (node: Node): void => {
            if (node instanceof TabNode) {
                tabs.push({ node, parent: node.getParent() as TabSetNode | BorderNode });
            }
            for (const child of node.getChildren()) {
                visit(child);
            }
        };

        visit(root);
        return tabs;
    }

    private getTabStripClasses(node: TabSetNode, tabLocation: string): string {
        let classes = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER);
        classes += ` ${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER_ + tabLocation)}`;
        if (node.isActive()) {
            classes += ` ${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_SELECTED)}`;
        }
        if (node.isMaximized()) {
            classes += ` ${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_MAXIMIZED)}`;
        }
        return classes;
    }

    private getBoundingClientRect(div: HTMLElement, container: HTMLElement): Rect {
        const containerRect = container.getBoundingClientRect();
        const divRect = div.getBoundingClientRect();
        return new Rect(
            divRect.x - containerRect.x,
            divRect.y - containerRect.y,
            divRect.width,
            divRect.height,
        );
    }

    private onDock(windowId: string): void {
        const win = this.options.model.getwindowsMap().get(windowId);
        if (!win?.root) {
            return;
        }
        const first = this.findFirstTabset(win.root);
        if (first) {
            this.options.doAction(Action.dockTabset(first.getId(), "center"));
        }
    }

    private findFirstTabset(node: RowNode): TabSetNode | undefined {
        const children = node.getChildren();
        if (children.length > 0 && children[0] instanceof TabSetNode) {
            return children[0];
        }
        for (const child of children) {
            if (child instanceof RowNode) {
                const found = this.findFirstTabset(child);
                if (found) {
                    return found;
                }
            }
        }
        return undefined;
    }

    private getTitle(root: RowNode | undefined): string {
        if (!root) {
            return "";
        }

        const tabs = this.collectTabNodes(root);
        const selected = tabs.find((entry) => entry.node.isSelected());
        if (selected) {
            return selected.node.getName();
        }
        return tabs[0]?.node.getName() ?? "";
    }

    private onMovePointerDown(event: PointerEvent, windowId: string): void {
        const target = event.target as HTMLElement | null;
        if (target?.closest(`.${this.options.getClassName(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR_BUTTONS)}`)) {
            return;
        }

        const win = this.options.model.getwindowsMap().get(windowId);
        if (!win) {
            return;
        }

        event.preventDefault();
        this.bringToFront(windowId);

        const start = { x: event.clientX, y: event.clientY, rectX: win.rect.x, rectY: win.rect.y, width: win.rect.width, height: win.rect.height };
        const isRealtime = this.options.isRealtimeResize();
        let outline = this.createPanelOutline(win.rect);

        if (!isRealtime) {
            this.options.root.appendChild(outline);
        }

        const onMove = (moveEvent: PointerEvent): void => {
            const dx = moveEvent.clientX - start.x;
            const dy = moveEvent.clientY - start.y;
            const nextX = start.rectX + dx;
            const nextY = start.rectY + dy;

            if (isRealtime) {
                this.options.doAction(Action.moveWindow(windowId, nextX, nextY, start.width, start.height));
            } else {
                outline.style.left = `${nextX}px`;
                outline.style.top = `${nextY}px`;
            }
        };

        const onUp = (): void => {
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);

            if (!isRealtime) {
                this.options.doAction(
                    Action.moveWindow(
                        windowId,
                        Number.parseFloat(outline.style.left),
                        Number.parseFloat(outline.style.top),
                        start.width,
                        start.height,
                    ),
                );
                outline.remove();
            }
        };

        document.addEventListener("pointermove", onMove);
        document.addEventListener("pointerup", onUp);
    }

    private onResizePointerDown(event: PointerEvent, windowId: string, edge: ResizeEdge): void {
        const win = this.options.model.getwindowsMap().get(windowId);
        if (!win) {
            return;
        }

        event.preventDefault();
        event.stopPropagation();
        this.bringToFront(windowId);

        const start = {
            x: event.clientX,
            y: event.clientY,
            rectX: win.rect.x,
            rectY: win.rect.y,
            width: win.rect.width,
            height: win.rect.height,
        };

        const resizesLeft = edge === "w" || edge === "nw" || edge === "sw";
        const resizesRight = edge === "e" || edge === "ne" || edge === "se";
        const resizesTop = edge === "n" || edge === "nw" || edge === "ne";
        const resizesBottom = edge === "s" || edge === "sw" || edge === "se";
        const isRealtime = this.options.isRealtimeResize();

        let outline = this.createPanelOutline(win.rect);
        if (!isRealtime) {
            this.options.root.appendChild(outline);
        }

        const onMove = (moveEvent: PointerEvent): void => {
            const dx = moveEvent.clientX - start.x;
            const dy = moveEvent.clientY - start.y;
            const next = this.computeResizedRect(dx, dy, start, resizesLeft, resizesRight, resizesTop, resizesBottom);

            if (isRealtime) {
                this.options.doAction(Action.moveWindow(windowId, next.x, next.y, next.width, next.height));
            } else {
                outline.style.left = `${next.x}px`;
                outline.style.top = `${next.y}px`;
                outline.style.width = `${next.width}px`;
                outline.style.height = `${next.height}px`;
            }
        };

        const onUp = (): void => {
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);

            if (!isRealtime) {
                this.options.doAction(
                    Action.moveWindow(
                        windowId,
                        Number.parseFloat(outline.style.left),
                        Number.parseFloat(outline.style.top),
                        Number.parseFloat(outline.style.width),
                        Number.parseFloat(outline.style.height),
                    ),
                );
                outline.remove();
            }
        };

        document.addEventListener("pointermove", onMove);
        document.addEventListener("pointerup", onUp);
    }

    private computeResizedRect(
        dx: number,
        dy: number,
        start: { rectX: number; rectY: number; width: number; height: number },
        resizesLeft: boolean,
        resizesRight: boolean,
        resizesTop: boolean,
        resizesBottom: boolean,
    ): { x: number; y: number; width: number; height: number } {
        let x = start.rectX;
        let y = start.rectY;
        let width = start.width;
        let height = start.height;

        if (resizesRight) {
            width = Math.max(VanillaFloatingWindowManager.MIN_WIDTH, start.width + dx);
        }
        if (resizesBottom) {
            height = Math.max(VanillaFloatingWindowManager.MIN_HEIGHT, start.height + dy);
        }
        if (resizesLeft) {
            const proposedWidth = start.width - dx;
            if (proposedWidth >= VanillaFloatingWindowManager.MIN_WIDTH) {
                width = proposedWidth;
                x = start.rectX + dx;
            } else {
                width = VanillaFloatingWindowManager.MIN_WIDTH;
                x = start.rectX + (start.width - VanillaFloatingWindowManager.MIN_WIDTH);
            }
        }
        if (resizesTop) {
            const proposedHeight = start.height - dy;
            if (proposedHeight >= VanillaFloatingWindowManager.MIN_HEIGHT) {
                height = proposedHeight;
                y = start.rectY + dy;
            } else {
                height = VanillaFloatingWindowManager.MIN_HEIGHT;
                y = start.rectY + (start.height - VanillaFloatingWindowManager.MIN_HEIGHT);
            }
        }

        return { x, y, width, height };
    }

    private createPanelOutline(rect: Rect): HTMLDivElement {
        const outline = document.createElement("div");
        outline.className = this.options.getClassName(CLASSES.FLEXLAYOUT__FLOATING_PANEL_OUTLINE);
        outline.style.left = `${rect.x}px`;
        outline.style.top = `${rect.y}px`;
        outline.style.width = `${rect.width}px`;
        outline.style.height = `${rect.height}px`;
        return outline;
    }

    private onFloatDragEnterRaw(event: DragEvent, windowId: string, refs: IPanelRefs): void {
        refs.dragEnterCount += 1;
        if (refs.dragEnterCount === 1) {
            this.onFloatDragEnter(event, windowId, refs);
        }
    }

    private onFloatDragLeaveRaw(event: DragEvent, refs: IPanelRefs): void {
        refs.dragEnterCount -= 1;
        if (refs.dragEnterCount <= 0) {
            const related = event.relatedTarget as globalThis.Node | null;
            if (!related || !refs.content.contains(related)) {
                this.clearFloatDrag(refs);
            } else {
                refs.dragEnterCount = 1;
            }
        }
    }

    private onFloatDragEnter(event: DragEvent, _windowId: string, refs: IPanelRefs): void {
        const dragNode = (this.options.root as { __dragNode?: Node }).__dragNode;
        if (!dragNode) {
            return;
        }

        event.preventDefault();
        event.stopPropagation();
        refs.dropInfo = undefined;
        refs.outlineDiv = document.createElement("div");
        refs.outlineDiv.className = this.options.getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT);
        refs.outlineDiv.style.visibility = "hidden";
        const speed = (this.options.model.getAttribute("tabDragSpeed") as number) || 0.3;
        refs.outlineDiv.style.transition = `top ${speed}s, left ${speed}s, width ${speed}s, height ${speed}s`;
        refs.content.appendChild(refs.outlineDiv);
        refs.dragging = true;
    }

    private onFloatDragOver(event: DragEvent, windowId: string, refs: IPanelRefs): void {
        if (!refs.dragging) {
            return;
        }

        event.preventDefault();
        event.stopPropagation();

        const dragNode = (this.options.root as { __dragNode?: Node }).__dragNode as (Node & IDraggable) | undefined;
        if (!dragNode) {
            return;
        }

        const win = this.options.model.getwindowsMap().get(windowId);
        const root = win?.root;
        if (!root) {
            return;
        }

        const contentRect = refs.content.getBoundingClientRect();
        const localX = event.clientX - contentRect.left;
        const localY = event.clientY - contentRect.top;
        const dropInfo = root.findDropTargetNode(windowId, dragNode, localX, localY);

        if (dropInfo) {
            refs.dropInfo = dropInfo;
            if (refs.outlineDiv) {
                let cls = this.options.getClassName(dropInfo.className);
                if (dropInfo.rect.width <= 5) {
                    cls += ` ${this.options.getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT + "_tab_reorder")}`;
                }
                refs.outlineDiv.className = cls;
                dropInfo.rect.positionElement(refs.outlineDiv);
                refs.outlineDiv.style.visibility = "visible";
            }
        } else if (refs.outlineDiv) {
            refs.outlineDiv.style.visibility = "hidden";
        }
    }

    private onFloatDrop(event: DragEvent, refs: IPanelRefs): void {
        if (!refs.dragging) {
            return;
        }

        event.preventDefault();
        event.stopPropagation();

        const dragNode = (this.options.root as { __dragNode?: Node }).__dragNode;
        if (refs.dropInfo && dragNode && (dragNode instanceof TabNode || dragNode instanceof TabSetNode)) {
            this.options.doAction(
                Action.moveNode(
                    dragNode.getId(),
                    refs.dropInfo.node.getId(),
                    refs.dropInfo.location.getName(),
                    refs.dropInfo.index,
                ),
            );
        }

        this.clearFloatDrag(refs);
        this.options.clearDragMain();
    }

    private clearFloatDrag(refs: IPanelRefs): void {
        refs.dragEnterCount = 0;
        refs.dragging = false;
        refs.dropInfo = undefined;
        if (refs.outlineDiv) {
            refs.outlineDiv.remove();
            refs.outlineDiv = undefined;
        }
    }

    private disposePanel(refs: IPanelRefs): void {
        this.clearFloatDrag(refs);
        for (const [, renderer] of refs.contentRenderers) {
            renderer.dispose();
        }
        refs.contentRenderers.clear();
        refs.contentContainers.clear();
        refs.panel.remove();
    }
}
