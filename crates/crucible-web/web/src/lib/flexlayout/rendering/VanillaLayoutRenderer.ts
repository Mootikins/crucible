import { Rect } from "../core/Rect";
import { DockLocation } from "../core/DockLocation";
import { CLASSES } from "../core/Types";
import { Orientation } from "../core/Orientation";
import { Action, type LayoutAction } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import { Model } from "../model/Model";
import { Node } from "../model/Node";
import { RowNode } from "../model/RowNode";
import { TabNode } from "../model/TabNode";
import { TabSetNode } from "../model/TabSetNode";
import { ICloseType } from "../model/ICloseType";
import { LayoutEngine } from "../layout/LayoutEngine";
import type { IDisposable } from "../model/Event";
import type { IContentRenderer } from "./IContentRenderer";
import { VanillaDndManager } from "./VanillaDndManager";
import { VanillaPopupManager } from "./VanillaPopupManager";
import { VanillaFloatingWindowManager } from "./VanillaFloatingWindowManager";
import {
    BORDER_BAR_SIZE,
    collectVisibleBorderStrips,
    computeNestingOrder,
    handleCollapsedBorderTabClick,
} from "./VanillaBorderLayoutEngine";

interface IFlyoutState {
    border: BorderNode;
    tab: TabNode;
    rect: Rect;
}

export interface IVanillaLayoutRendererOptions {
    model: Model;
    getClassName(defaultClassName: string): string;
    doAction(action: LayoutAction): void;
    onModelChange?(model: Model, action: LayoutAction): void;
    onAction?(action: LayoutAction): LayoutAction | undefined;
    onContextMenu?(node: TabNode, event: MouseEvent): Array<{ label: string; action: () => void }>;
    onAllowDrop?(dragNode: Node, dropInfo: any): boolean;
    createContentRenderer(node: TabNode): IContentRenderer;
}

interface ITabSetDomRefs {
    container: HTMLDivElement;
    tabset: HTMLDivElement;
    tabStrip: HTMLDivElement;
    tabStripInner: HTMLDivElement;
    tabContainer: HTMLDivElement;
    toolbar: HTMLDivElement;
    content: HTMLDivElement;
}

interface IBorderNestingRefs {
    outer: HTMLDivElement;
    inner: HTMLDivElement;
}

export class VanillaLayoutRenderer {
    private rootDiv: HTMLDivElement | undefined;
    private mainDiv: HTMLDivElement | undefined;
    private rect = Rect.empty();
    private revision = 0;
    private layoutVersion = 0;
    private showEdges = false;
    private showOverlay = false;
    private showHiddenBorder = DockLocation.CENTER;
    private editingTab: TabNode | undefined;

    private readonly contentRenderers = new Map<string, IContentRenderer>();
    private readonly contentContainers = new Map<string, HTMLElement>();
    private readonly rowElements = new Map<string, HTMLDivElement>();
    private readonly splitterElements = new Map<string, HTMLDivElement>();
    private readonly tabSetElements = new Map<string, ITabSetDomRefs>();
    private readonly tabButtonElements = new Map<string, HTMLDivElement>();
    private readonly borderNestingElements = new Map<string, IBorderNestingRefs>();
    private readonly borderStripElements = new Map<string, HTMLDivElement>();
    private readonly borderContentElements = new Map<string, HTMLDivElement>();
    private readonly borderButtonElements = new Map<string, HTMLDivElement>();
    private readonly borderTabHosts = new Map<string, HTMLDivElement>();
    private readonly borderTileWeights = new Map<string, number[]>();
    private readonly hiddenTabIndices = new Map<string, number[]>();
    private readonly disposables: IDisposable[] = [];
    private resizeObserver: ResizeObserver | undefined;
    private flyoutState: IFlyoutState | undefined;
    private flyoutPanel: HTMLDivElement | undefined;
    private flyoutBackdrop: HTMLDivElement | undefined;

    private flyoutExitTimer: ReturnType<typeof setTimeout> | undefined;
    private previousBorderDockStates = new Map<string, string>();
    private borderTransitionStates = new Map<string, string>();

    private readonly paneviewExpanded = new Map<string, boolean>();
    private readonly paneviewContainers = new Map<string, HTMLDivElement>();
    private paneviewDragState: { tabsetId: string; dragTabId: string; placeholder: HTMLDivElement } | undefined;

    private readonly popupManager: VanillaPopupManager;
    private readonly dndManager: VanillaDndManager;
    private floatingWindowManager: VanillaFloatingWindowManager | undefined;

    constructor(private readonly options: IVanillaLayoutRendererOptions) {
        this.popupManager = new VanillaPopupManager({
            getSelfRef: () => this.rootDiv,
            getClassName: (className) => this.options.getClassName(className),
        });

        this.dndManager = new VanillaDndManager({
            model: this.options.model,
            getClassName: (className) => this.options.getClassName(className),
            getSelfRef: () => this.rootDiv,
            getMainRef: () => this.mainDiv,
            getBoundingClientRect: (div) => this.getBoundingClientRect(div),
            getShowHiddenBorder: () => this.showHiddenBorder,
            setShowHiddenBorder: (location) => {
                this.showHiddenBorder = location;
                this.render();
            },
            setShowOverlay: (show) => {
                this.showOverlay = show;
                this.render();
            },
            setShowEdges: (show) => {
                this.showEdges = show;
                this.render();
            },
            doAction: (action) => this.doAction(action),
            redraw: () => this.redraw(),
        });

        // Wire callbacks to model
        if (this.options.onAllowDrop) {
            this.options.model.setOnAllowDrop(this.options.onAllowDrop);
        }
    }

    mount(container: HTMLElement): void {
        this.rootDiv = document.createElement("div");
        this.rootDiv.className = this.options.getClassName(CLASSES.FLEXLAYOUT__LAYOUT);
        this.rootDiv.dataset.layoutPath = "/";
        this.rootDiv.style.position = "relative";
        this.rootDiv.style.overflow = "hidden";
        this.rootDiv.addEventListener("dragenter", (e) => this.dndManager.onDragEnterRaw(e));
        this.rootDiv.addEventListener("dragleave", (e) => this.dndManager.onDragLeaveRaw(e));
        this.rootDiv.addEventListener("dragover", (e) => this.dndManager.onDragOver(e));
        this.rootDiv.addEventListener("drop", (e) => this.dndManager.onDrop(e));

        container.appendChild(this.rootDiv);

        this.mainDiv = document.createElement("div");
        this.mainDiv.className = this.options.getClassName(CLASSES.FLEXLAYOUT__LAYOUT_MAIN);
        this.mainDiv.dataset.layoutPath = "/main";
        this.mainDiv.style.position = "absolute";
        this.mainDiv.style.top = "0";
        this.mainDiv.style.left = "0";
        this.mainDiv.style.bottom = "0";
        this.mainDiv.style.right = "0";
        this.mainDiv.style.display = "flex";
        this.rootDiv.appendChild(this.mainDiv);

        this.floatingWindowManager = new VanillaFloatingWindowManager({
            model: this.options.model,
            root: this.rootDiv,
            getClassName: (className) => this.options.getClassName(className),
            doAction: (action) => this.doAction(action),
            createContentRenderer: (node) => this.options.createContentRenderer(node),
            isRealtimeResize: () => this.options.model.isRealtimeResize(),
            setDragNode: (event, node) => this.dndManager.setDragNode(event, node),
            clearDragMain: () => this.dndManager.clearDragMain(),
        });

        this.resizeObserver = new ResizeObserver(() => {
            requestAnimationFrame(() => this.updateRect());
        });
        this.resizeObserver.observe(this.rootDiv);

        this.disposables.push(this.options.model.onDidChange(() => this.redraw()));
        this.disposables.push(this.options.model.onDidAction((action) => {
            if (this.options.onModelChange) {
                this.options.onModelChange(this.options.model, action);
            }

            if (action.type === "OPEN_FLYOUT" || action.type === "CLOSE_FLYOUT") {
                this.updateFlyoutState();
                this.render();
            }
        }));

        this.updateRect();
        this.updateFlyoutState();
        this.render();
    }

    resize(): void {
        this.updateRect();
        this.render();
    }

    unmount(): void {
        this.popupManager.cleanup();
        this.dndManager.clearDragMain();
        this.floatingWindowManager?.dispose();
        this.floatingWindowManager = undefined;

        if (this.resizeObserver && this.rootDiv) {
            this.resizeObserver.unobserve(this.rootDiv);
            this.resizeObserver.disconnect();
        }
        this.resizeObserver = undefined;

        for (const disposable of this.disposables.splice(0)) {
            disposable.dispose();
        }

        for (const [, renderer] of this.contentRenderers) {
            renderer.dispose();
        }
        this.contentRenderers.clear();
        this.contentContainers.clear();
        this.rowElements.clear();
        this.splitterElements.clear();
        this.tabSetElements.clear();
        this.tabButtonElements.clear();
        this.borderNestingElements.clear();
        this.borderStripElements.clear();
        this.borderContentElements.clear();
        this.borderButtonElements.clear();
        this.borderTabHosts.clear();
        this.borderTileWeights.clear();
        this.hiddenTabIndices.clear();

        this.flyoutPanel = undefined;
        this.flyoutBackdrop = undefined;
        this.flyoutState = undefined;
        if (this.flyoutExitTimer) {
            clearTimeout(this.flyoutExitTimer);
            this.flyoutExitTimer = undefined;
        }
        this.previousBorderDockStates.clear();
        this.borderTransitionStates.clear();

        for (const [, container] of this.paneviewContainers) {
            container.remove();
        }
        this.paneviewContainers.clear();
        this.paneviewExpanded.clear();
        this.paneviewDragState = undefined;

        this.rootDiv?.remove();
        this.rootDiv = undefined;
        this.mainDiv = undefined;
    }

    setEditingTab(tab: TabNode | undefined): void {
        this.editingTab = tab;
        this.render();
    }

    getEditingTab(): TabNode | undefined {
        return this.editingTab;
    }

    showPopup(
        triggerElement: HTMLElement,
        parentNode: TabSetNode | BorderNode,
        items: Array<{ index: number; node: TabNode }>,
        onSelect: (item: { index: number; node: TabNode }) => void,
    ): void {
        this.popupManager.showPopup(triggerElement, parentNode, items, onSelect);
    }

    showContextMenu(event: MouseEvent, items: Array<{ label: string; action: () => void }>): void {
        this.popupManager.showContextMenu(event, items);
    }

    private redraw(): void {
        this.revision += 1;
        this.updateLayout();
        this.render();
    }

    private doAction(action: LayoutAction): void {
        const handled = this.options.onAction ? this.options.onAction(action) : action;
        if (!handled) {
            return;
        }
        this.options.doAction(handled);
    }

    private updateRect(): void {
        if (!this.rootDiv) {
            return;
        }
        const domRect = this.rootDiv.getBoundingClientRect();
        const newRect = new Rect(0, 0, domRect.width, domRect.height);
        if (!newRect.equals(this.rect) && newRect.width > 0 && newRect.height > 0) {
            this.rect = newRect;
            this.updateLayout();
        }
    }

    private updateLayout(): void {
        const model = this.options.model;
        const root = model.getRoot();
        if (!root || this.rect.width <= 0 || this.rect.height <= 0) {
            return;
        }

        (root as RowNode).calcMinMaxSize();
        root.setPaths("");
        model.getBorderSet().setPaths();
        LayoutEngine.calculateLayout(root, this.rect);

        for (const [windowId, win] of model.getwindowsMap()) {
            if (windowId !== Model.MAIN_WINDOW_ID && win.root) {
                const winRect = new Rect(0, 0, win.rect.width, win.rect.height);
                (win.root as RowNode).calcMinMaxSize();
                win.root.setPaths(`/window/${windowId}`);
                LayoutEngine.calculateLayout(win.root, winRect);
            }
        }

        this.layoutVersion += 1;
    }

    private render(): void {
        if (!this.rootDiv || !this.mainDiv) {
            return;
        }

        const overlay = this.rootDiv.querySelector('[data-layout-path="/overlay"]') as HTMLElement | null;
        if (this.showOverlay) {
            if (!overlay) {
                const nextOverlay = document.createElement("div");
                nextOverlay.className = this.options.getClassName(CLASSES.FLEXLAYOUT__LAYOUT_OVERLAY);
                nextOverlay.dataset.layoutPath = "/overlay";
                nextOverlay.style.position = "absolute";
                nextOverlay.style.inset = "0";
                nextOverlay.style.zIndex = "998";
                this.rootDiv.appendChild(nextOverlay);
            }
        } else if (overlay) {
            overlay.remove();
        }

        this.renderMainLayout();
        this.renderBorderLayout();
        this.renderEdges();
        this.updateBorderDockStates();
        this.renderCollapsedBorderStrips();
        this.updateFlyoutState();
        this.renderFlyout();
        this.renderPaneviews();
        this.renderTabContents();
        this.floatingWindowManager?.render();
    }

    private renderMainLayout(): void {
        if (!this.mainDiv) {
            return;
        }

        const root = this.options.model.getRoot();
        if (!(root instanceof RowNode)) {
            this.mainDiv.replaceChildren();
            this.rowElements.clear();
            this.splitterElements.clear();
            this.tabSetElements.clear();
            this.tabButtonElements.clear();
            this.hiddenTabIndices.clear();
            return;
        }

        const seenRows = new Set<string>();
        const seenSplitters = new Set<string>();
        const seenTabsets = new Set<string>();
        const seenTabs = new Set<string>();

        const rootEl = this.renderRowNode(root, seenRows, seenSplitters, seenTabsets, seenTabs);
        this.mainDiv.replaceChildren(rootEl);

        for (const [id, el] of this.rowElements) {
            if (!seenRows.has(id)) {
                el.remove();
                this.rowElements.delete(id);
            }
        }

        for (const [key, el] of this.splitterElements) {
            if (!seenSplitters.has(key)) {
                el.remove();
                this.splitterElements.delete(key);
            }
        }

        for (const [id, refs] of this.tabSetElements) {
            if (!seenTabsets.has(id)) {
                refs.container.remove();
                this.tabSetElements.delete(id);
                this.hiddenTabIndices.delete(id);
            }
        }

        for (const [id, el] of this.tabButtonElements) {
            if (!seenTabs.has(id)) {
                el.remove();
                this.tabButtonElements.delete(id);
            }
        }
    }

    private renderRowNode(
        node: RowNode,
        seenRows: Set<string>,
        seenSplitters: Set<string>,
        seenTabsets: Set<string>,
        seenTabs: Set<string>,
    ): HTMLDivElement {
        const rowId = node.getId();
        seenRows.add(rowId);

        let rowEl = this.rowElements.get(rowId);
        if (!rowEl) {
            rowEl = document.createElement("div");
            this.rowElements.set(rowId, rowEl);
        }

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
                children.push(this.renderSplitterNode(node, i, horizontal, seenSplitters));
            }

            const child = modelChildren[i];
            if (child instanceof RowNode) {
                children.push(this.renderRowNode(child, seenRows, seenSplitters, seenTabsets, seenTabs));
            } else if (child instanceof TabSetNode) {
                children.push(this.renderTabSetNode(child, seenTabsets, seenTabs));
            }
        }

        rowEl.replaceChildren(...children);
        return rowEl;
    }

    private renderSplitterNode(row: RowNode, index: number, horizontal: boolean, seenSplitters: Set<string>): HTMLDivElement {
        const key = `${row.getId()}::${index}`;
        seenSplitters.add(key);

        let splitter = this.splitterElements.get(key);
        if (!splitter) {
            splitter = document.createElement("div");
            this.splitterElements.set(key, splitter);
        }

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

        splitter.onpointerdown = (event) => this.onSplitterPointerDown(event, row, index, horizontal, splitter!);

        return splitter;
    }

    private onSplitterPointerDown(
        event: PointerEvent,
        row: RowNode,
        index: number,
        horizontal: boolean,
        splitter: HTMLDivElement,
    ): void {
        event.stopPropagation();
        event.preventDefault();

        const initialSizes = row.getSplitterInitials(index);
        const bounds = row.getSplitterBounds(index);
        const isRealtime = this.options.model.isRealtimeResize();

        const domRect = splitter.getBoundingClientRect();
        const layoutRect = this.getDomRect();
        const splitterRect = new Rect(
            domRect.x - layoutRect.x,
            domRect.y - layoutRect.y,
            domRect.width,
            domRect.height,
        );

        const dragStartX = event.clientX - domRect.x;
        const dragStartY = event.clientY - domRect.y;

        let outlineDiv: HTMLDivElement | undefined;
        if (!isRealtime && this.rootDiv) {
            outlineDiv = document.createElement("div");
            outlineDiv.style.flexDirection = horizontal ? "row" : "column";
            outlineDiv.className = this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER_DRAG);
            outlineDiv.style.cursor = row.getOrientation() === Orientation.VERT ? "ns-resize" : "ew-resize";
            splitterRect.positionElement(outlineDiv);
            this.rootDiv.appendChild(outlineDiv);
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
            this.doAction(
                Action.adjustWeights(
                    row.getId(),
                    weights,
                    row.getOrientation().getName(),
                ),
            );
        };

        const onMove = (moveEvent: PointerEvent): void => {
            const clientRect = this.getDomRect();
            const position = row.getOrientation() === Orientation.VERT
                ? clampPosition(moveEvent.clientY - clientRect.y - dragStartY)
                : clampPosition(moveEvent.clientX - clientRect.x - dragStartX);

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

    private renderTabSetNode(node: TabSetNode, seenTabsets: Set<string>, seenTabs: Set<string>): HTMLDivElement {
        const tabsetId = node.getId();
        seenTabsets.add(tabsetId);

        let refs = this.tabSetElements.get(tabsetId);
        if (!refs) {
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

            refs = { container, tabset, tabStrip, tabStripInner, tabContainer, toolbar, content };
            this.tabSetElements.set(tabsetId, refs);
        }

        const parent = node.getParent();
        const nodeRect = node.getRect();
        const isHorizontal = parent instanceof RowNode && parent.getOrientation() === Orientation.HORZ;
        const flexSize = isHorizontal ? nodeRect.width : nodeRect.height;
        refs.container.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_CONTAINER);
        refs.container.style.flex = flexSize > 0 ? `0 0 ${flexSize}px` : "1 1 0%";
        refs.container.style.minWidth = `${node.getMinWidth()}px`;
        refs.container.style.minHeight = `${node.getMinHeight()}px`;
        refs.container.style.maxWidth = `${node.getMaxWidth()}px`;
        refs.container.style.maxHeight = `${node.getMaxHeight()}px`;

        const maximized = node.getModel().getMaximizedTabset(Model.MAIN_WINDOW_ID);
        refs.container.style.display = maximized && !node.isMaximized() ? "none" : "";

        refs.tabset.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET);
        refs.tabset.dataset.layoutPath = node.getPath();
        refs.tabset.dataset.state = node.isMaximized() ? "maximized" : (node.isActive() ? "active" : "inactive");

        const tabLocation = node.getTabLocation() || "top";
        refs.tabStrip.className = this.getTabStripClasses(node, tabLocation);
        refs.tabStrip.dataset.layoutPath = `${node.getPath()}/tabstrip`;
        refs.tabStrip.style.cursor = "";

        refs.tabStrip.onpointerdown = (event) => {
            if (!node.isActive()) {
                this.doAction(Action.setActiveTabset(node.getId(), Model.MAIN_WINDOW_ID));
            }
            event.stopPropagation();
        };

        refs.tabStrip.ondblclick = () => {
            if (node.canMaximize()) {
                this.doAction(Action.maximizeToggle(node.getId()));
            }
        };

        refs.tabStrip.draggable = true;
        refs.tabStrip.ondragstart = (event) => {
            if (this.editingTab) {
                event.preventDefault();
                return;
            }
            if (node.isEnableDrag()) {
                event.stopPropagation();
                this.dndManager.setDragNode(event, node);
            } else {
                event.preventDefault();
            }
        };

        refs.tabStrip.ondragend = () => {
            this.dndManager.clearDragMain();
        };

        refs.tabStripInner.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_ + tabLocation)}`;
        refs.tabStripInner.style.overflowX = "auto";
        refs.tabStripInner.style.overflowY = "hidden";

        refs.tabContainer.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER_ + tabLocation)}`;

        refs.toolbar.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR);

        refs.content.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_CONTENT);
        refs.content.dataset.layoutPath = `${node.getPath()}/content`;

        this.renderTabButtons(node, refs, seenTabs);
        this.syncTabSetToolbar(node, refs);
        refs.tabStripInner.onscroll = () => this.syncTabSetToolbar(node, refs!);

        if (tabLocation === "top") {
            refs.tabset.replaceChildren(refs.tabStrip, refs.content);
        } else {
            refs.tabset.replaceChildren(refs.content, refs.tabStrip);
        }

        return refs.container;
    }

    private getTabStripClasses(node: TabSetNode, tabLocation: string): string {
        let classes = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER);
        classes += ` ${CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER_ + tabLocation}`;

        if (node.isActive()) {
            classes += ` ${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_SELECTED)}`;
        }

        if (node.isMaximized()) {
            classes += ` ${this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_MAXIMIZED)}`;
        }

        return classes;
    }

    private renderTabButtons(node: TabSetNode, refs: ITabSetDomRefs, seenTabs: Set<string>): void {
        const children = node.getChildren() as TabNode[];
        const items: HTMLElement[] = [];

        for (let i = 0; i < children.length; i++) {
            const tab = children[i];
            const tabEl = this.renderTabButtonNode(tab, node, i, seenTabs);
            items.push(tabEl);

            if (i < children.length - 1) {
                const divider = document.createElement("div");
                divider.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TABSET_TAB_DIVIDER);
                items.push(divider);
            }
        }

        refs.tabContainer.replaceChildren(...items);

        for (const tab of children) {
            const tabElement = this.tabButtonElements.get(tab.getId());
            if (tabElement) {
                tab.setTabRect(this.getBoundingClientRect(tabElement));
            }
        }
    }

    private renderTabButtonNode(tab: TabNode, parent: TabSetNode, index: number, seenTabs: Set<string>): HTMLDivElement {
        const tabId = tab.getId();
        seenTabs.add(tabId);

        let tabEl = this.tabButtonElements.get(tabId);
        if (!tabEl) {
            tabEl = document.createElement("div");
            this.tabButtonElements.set(tabId, tabEl);
        }

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
        tabEl.dataset.pinned = tab.isPinned() ? "true" : "false";
        tabEl.title = tab.getHelpText() ?? "";
        tabEl.draggable = true;

        tabEl.onclick = () => {
            if (!selected) {
                this.doAction(Action.selectTab(tab.getId()));
            }
        };

        tabEl.ondblclick = (event) => {
            if (tab.isEnableRename()) {
                event.stopPropagation();
                this.setEditingTab(tab);
            }
        };

        tabEl.ondragstart = (event) => {
            if (tab.isEnableDrag()) {
                event.stopPropagation();
                this.dndManager.setDragNode(event, tab);
            } else {
                event.preventDefault();
            }
        };

        tabEl.ondragend = () => {
            this.dndManager.clearDragMain();
        };

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

        if (this.editingTab === tab) {
            const input = document.createElement("input");
            input.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_TEXTBOX);
            input.dataset.layoutPath = `${path}/textbox`;
            input.type = "text";
            input.value = tab.getName();
            input.onkeydown = (event) => {
                if (event.code === "Escape") {
                    this.setEditingTab(undefined);
                } else if (event.code === "Enter" || event.code === "NumpadEnter") {
                    this.doAction(Action.renameTab(tab.getId(), input.value));
                    this.setEditingTab(undefined);
                }
            };
            input.onpointerdown = (event) => event.stopPropagation();
            requestAnimationFrame(() => {
                input.focus();
                input.select();
            });
            children.push(input);
        } else {
            const content = document.createElement("div");
            content.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_CONTENT);
            content.textContent = tab.getName();
            children.push(content);
        }

        if (tab.isPinned()) {
            const pin = document.createElement("div");
            pin.dataset.layoutPath = `${path}/indicator/pin`;
            pin.title = "Pinned";
            pin.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_TRAILING);
            pin.style.pointerEvents = "none";
            pin.textContent = "📌";
            children.push(pin);
        }

        if (tab.isEnableClose() && !isStretch) {
            const close = document.createElement("div");
            close.dataset.layoutPath = `${path}/button/close`;
            close.title = "Close";
            close.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_TRAILING);
            close.textContent = "✕";
            close.onpointerdown = (event) => event.stopPropagation();
            close.onclick = (event) => {
                this.doAction(Action.deleteTab(tab.getId()));
                event.stopPropagation();
            };
            children.push(close);
        }

        tabEl.replaceChildren(...children);
        tab.setTabRect(this.getBoundingClientRect(tabEl));

        return tabEl;
    }

    private syncTabSetToolbar(node: TabSetNode, refs: ITabSetDomRefs): void {
        const hidden = this.findHiddenTabIndices(refs.tabStripInner);
        const existing = this.hiddenTabIndices.get(node.getId()) ?? [];
        const changed = hidden.length !== existing.length || hidden.some((value, idx) => value !== existing[idx]);
        if (changed) {
            this.hiddenTabIndices.set(node.getId(), hidden);
        }
        this.renderTabSetToolbar(node, refs, hidden);
    }

    private renderTabSetToolbar(node: TabSetNode, refs: ITabSetDomRefs, hiddenTabs: number[]): void {
        const buttons: HTMLButtonElement[] = [];

        if (hiddenTabs.length > 0) {
            const overflowButton = document.createElement("button");
            overflowButton.type = "button";
            overflowButton.dataset.layoutPath = `${node.getPath()}/button/overflow`;
            overflowButton.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_OVERFLOW)}`;
            overflowButton.title = "Overflow";
            overflowButton.textContent = "...";
            overflowButton.onpointerdown = (event) => event.stopPropagation();
            overflowButton.onclick = (event) => {
                const items = hiddenTabs.map((hiddenIndex) => ({
                    index: hiddenIndex,
                    node: node.getChildren()[hiddenIndex] as TabNode,
                }));
                this.showPopup(overflowButton, node, items, (item) => {
                    this.doAction(Action.selectTab(item.node.getId()));
                });
                event.stopPropagation();
            };

            const overflowCount = document.createElement("div");
            overflowCount.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_OVERFLOW_COUNT);
            overflowCount.textContent = String(hiddenTabs.length);
            overflowButton.appendChild(overflowCount);

            buttons.push(overflowButton);
        }

        if (node.canMaximize()) {
            const maximizeButton = document.createElement("button");
            maximizeButton.type = "button";
            maximizeButton.dataset.layoutPath = `${node.getPath()}/button/max`;
            maximizeButton.title = node.isMaximized() ? "Restore" : "Maximize";
            maximizeButton.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_ + (node.isMaximized() ? "max" : "min"))}`;
            maximizeButton.textContent = node.isMaximized() ? "⊡" : "⊞";
            maximizeButton.onpointerdown = (event) => event.stopPropagation();
            maximizeButton.onclick = (event) => {
                if (node.canMaximize()) {
                    this.doAction(Action.maximizeToggle(node.getId()));
                }
                event.stopPropagation();
            };
            buttons.push(maximizeButton);
        }

        if (!node.isMaximized()) {
            const floatButton = document.createElement("button");
            floatButton.type = "button";
            floatButton.dataset.layoutPath = `${node.getPath()}/button/float`;
            floatButton.title = "Float";
            floatButton.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_FLOAT)}`;
            floatButton.textContent = "⊡";
            floatButton.onpointerdown = (event) => event.stopPropagation();
            floatButton.onclick = (event) => {
                const rect = node.getRect();
                this.doAction(Action.floatTabset(node.getId(), rect.x, rect.y, rect.width, rect.height));
                event.stopPropagation();
            };
            buttons.push(floatButton);
        }

        if (!node.isMaximized() && node.isEnableClose()) {
            const closeButton = document.createElement("button");
            closeButton.type = "button";
            closeButton.dataset.layoutPath = `${node.getPath()}/button/close`;
            closeButton.title = "Close";
            closeButton.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_CLOSE)}`;
            closeButton.textContent = "✕";
            closeButton.onpointerdown = (event) => event.stopPropagation();
            closeButton.onclick = (event) => {
                this.doAction(Action.deleteTabset(node.getId()));
                event.stopPropagation();
            };
            buttons.push(closeButton);
        }

        refs.toolbar.replaceChildren(...buttons);
    }

    private findHiddenTabIndices(tabStripInner: HTMLDivElement): number[] {
        const stripRect = tabStripInner.getBoundingClientRect();
        if (stripRect.width <= 0) {
            return [];
        }

        const visibleLeft = stripRect.left - 1;
        const visibleRight = stripRect.right + 1;
        const tabContainer = tabStripInner.firstElementChild;
        if (!tabContainer) {
            return [];
        }

        const hidden: number[] = [];
        const tabButtonClass = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON);
        let tabIndex = 0;
        for (const child of Array.from(tabContainer.children)) {
            if ((child as HTMLElement).classList.contains(tabButtonClass)) {
                const childRect = (child as HTMLElement).getBoundingClientRect();
                if (childRect.left < visibleLeft || childRect.right > visibleRight) {
                    hidden.push(tabIndex);
                }
                tabIndex += 1;
            }
        }

        return hidden;
    }

    private renderBorderLayout(): void {
        if (!this.rootDiv || !this.mainDiv) {
            return;
        }

        this.borderTabHosts.clear();

        const borders = this.options.model.getBorderSet().getBorderMap();
        const strips = collectVisibleBorderStrips(borders, this.showHiddenBorder);
        const allVisibleBorders = this.options.model
            .getBorderSet()
            .getBordersByPriority()
            .filter((border) => strips.has(border.getLocation().getName()));
        const sorted = computeNestingOrder(allVisibleBorders);

        const seenBorders = new Set<string>();
        const seenButtons = new Set<string>();
        let currentContent: HTMLElement = this.mainDiv;

        for (let i = sorted.length - 1; i >= 0; i--) {
            const border = sorted[i];
            const borderId = border.getId();
            seenBorders.add(borderId);

            const isCollapsed = border.getDockState() === "collapsed";
            const refs = this.getOrCreateBorderNestingRefs(borderId);
            refs.outer.className = this.options.getClassName(
                i === 0
                    ? CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER
                    : CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER_INNER,
            );
            refs.inner.className = this.options.getClassName(CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER_INNER);

            const location = border.getLocation();
            const isHorizontalBorder = location === DockLocation.TOP || location === DockLocation.BOTTOM;
            const isStart = location === DockLocation.LEFT || location === DockLocation.TOP;
            const flexDirection = isHorizontalBorder ? "column" : "row";
            refs.outer.style.flexDirection = flexDirection;
            refs.inner.style.flexDirection = flexDirection;

            if (isCollapsed) {
                const strip = this.renderCollapsedBorderStripInline(border, seenButtons);
                refs.inner.replaceChildren(currentContent);
                if (isStart) {
                    refs.outer.replaceChildren(strip, refs.inner);
                } else {
                    refs.outer.replaceChildren(refs.inner, strip);
                }
            } else {
                const strip = this.renderExpandedBorderStrip(border, seenButtons);
                const contentItems = this.renderExpandedBorderContentItems(border, seenButtons);

                const innerChildren: HTMLElement[] = [];
                if (isStart) {
                    innerChildren.push(...contentItems, currentContent);
                } else {
                    innerChildren.push(currentContent, ...contentItems);
                }
                refs.inner.replaceChildren(...innerChildren);

                const outerChildren: HTMLElement[] = [];
                if (isStart) {
                    outerChildren.push(strip, refs.inner);
                } else {
                    outerChildren.push(refs.inner, strip);
                }
                refs.outer.replaceChildren(...outerChildren);
            }
            currentContent = refs.outer;
        }

        if (currentContent === this.mainDiv) {
            if (this.mainDiv.parentElement !== this.rootDiv) {
                this.rootDiv.insertBefore(this.mainDiv, this.findFirstOverlayElement());
            }
        } else {
            if (currentContent.parentElement !== this.rootDiv) {
                this.rootDiv.insertBefore(currentContent, this.findFirstOverlayElement());
            }
        }

        for (const [borderId, refs] of this.borderNestingElements) {
            if (!seenBorders.has(borderId)) {
                refs.outer.remove();
                this.borderNestingElements.delete(borderId);
            }
        }

        for (const [borderId, strip] of this.borderStripElements) {
            if (!seenBorders.has(borderId)) {
                strip.remove();
                this.borderStripElements.delete(borderId);
            }
        }

        for (const [borderId, content] of this.borderContentElements) {
            if (!seenBorders.has(borderId)) {
                content.remove();
                this.borderContentElements.delete(borderId);
                this.borderTileWeights.delete(borderId);
            }
        }

        for (const [buttonId, buttonEl] of this.borderButtonElements) {
            if (!seenButtons.has(buttonId)) {
                buttonEl.remove();
                this.borderButtonElements.delete(buttonId);
            }
        }
    }

    private getOrCreateBorderNestingRefs(borderId: string): IBorderNestingRefs {
        let refs = this.borderNestingElements.get(borderId);
        if (!refs) {
            const outer = document.createElement("div");
            const inner = document.createElement("div");
            refs = { outer, inner };
            this.borderNestingElements.set(borderId, refs);
        }
        return refs;
    }

    private findFirstOverlayElement(): ChildNode | null {
        if (!this.rootDiv) {
            return null;
        }
        const overlayPaths = [
            '/overlay',
            '/edges',
            '/collapsed-borders',
            '/flyout/backdrop',
            '/flyout/panel',
        ];
        for (const path of overlayPaths) {
            const found = this.rootDiv.querySelector(`[data-layout-path="${path}"]`);
            if (found) {
                return found;
            }
        }
        return null;
    }

    private renderCollapsedBorderStripInline(border: BorderNode, seenButtons: Set<string>): HTMLDivElement {
        let strip = this.borderStripElements.get(border.getId());
        if (!strip) {
            strip = document.createElement("div");
            this.borderStripElements.set(border.getId(), strip);
        }

        const location = border.getLocation();
        const locationName = location.getName();
        const isVertical = location === DockLocation.LEFT || location === DockLocation.RIGHT;

        strip.className = [
            this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER),
            this.options.getClassName(`${CLASSES.FLEXLAYOUT__BORDER_}${locationName}`),
            this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER__COLLAPSED),
        ].join(" ");

        strip.dataset.layoutPath = border.getPath();
        strip.dataset.collapsedStrip = "true";
        strip.dataset.state = this.borderTransitionStates.get(border.getId()) || "collapsed";
        strip.dataset.animate = border.isAnimateTransition() ? "true" : "false";
        strip.dataset.edge = locationName;
        strip.style.display = "flex";
        strip.style.alignItems = "center";
        strip.style.position = "relative";
        strip.style.zIndex = "1";

        if (isVertical) {
            strip.style.flexDirection = "column";
            strip.style.justifyContent = "flex-start";
            strip.style.width = `${BORDER_BAR_SIZE}px`;
            strip.style.minWidth = `${BORDER_BAR_SIZE}px`;
            strip.style.height = "";
            strip.style.minHeight = "";
            strip.style.alignSelf = "stretch";
            strip.style.overflow = "visible";
        } else {
            strip.style.justifyContent = "flex-start";
            strip.style.flexDirection = "row";
            strip.style.height = `${BORDER_BAR_SIZE}px`;
            strip.style.minHeight = `${BORDER_BAR_SIZE}px`;
            strip.style.width = "";
            strip.style.minWidth = "";
        }

        strip.addEventListener("dragover", (e) => {
            e.preventDefault();
            if (e.dataTransfer) {
                e.dataTransfer.dropEffect = "move";
            }
        });
        strip.addEventListener("drop", (e) => {
            e.preventDefault();
            this.doAction(Action.setDockState(border.getId(), "expanded"));
        });

        const fabPosition = border.getFabPosition();
        const children: HTMLElement[] = [];

        if (border.isEnableDock() && fabPosition === "start") {
            children.push(this.createCollapsedStripFab(border));
        }

        const tabs = border.getChildren();
        for (let i = 0; i < tabs.length; i++) {
            const tab = tabs[i];
            if (!(tab instanceof TabNode)) {
                continue;
            }

            const buttonPath = `${border.getPath()}/tb${i}`;
            seenButtons.add(buttonPath);

            const button = document.createElement("button");
            button.type = "button";
            button.dataset.layoutPath = buttonPath;
            button.dataset.flyoutTabButton = "true";
            button.dataset.collapsedTabItem = "true";
            button.className = [
                this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_BUTTON),
                this.options.getClassName(
                    border.getFlyoutTabId() === tab.getId()
                        ? CLASSES.FLEXLAYOUT__BORDER_BUTTON__SELECTED
                        : CLASSES.FLEXLAYOUT__BORDER_BUTTON__UNSELECTED,
                ),
            ].join(" ");

            if (isVertical) {
                button.style.writingMode = "vertical-rl";
                if (location === DockLocation.LEFT) {
                    button.style.transform = "rotate(180deg)";
                }
            }

            const content = document.createElement("span");
            content.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_BUTTON_CONTENT);
            content.textContent = tab.getName();
            button.appendChild(content);

            button.addEventListener("click", (event) => {
                event.stopPropagation();
                handleCollapsedBorderTabClick(border, tab, (action) => this.doAction(action));
            });

            children.push(button);
        }

        if (border.isEnableDock() && fabPosition === "end") {
            children.push(this.createCollapsedStripFab(border));
        }

        strip.replaceChildren(...children);
        border.setTabHeaderRect(this.getBoundingClientRect(strip));
        return strip;
    }

    private renderExpandedBorderStrip(border: BorderNode, seenButtons: Set<string>): HTMLDivElement {
        let strip = this.borderStripElements.get(border.getId());
        if (!strip) {
            strip = document.createElement("div");
            this.borderStripElements.set(border.getId(), strip);
        }

        delete strip.dataset.collapsedStrip;
        delete strip.dataset.flyoutTabButton;
        delete strip.dataset.collapsedTabItem;
        delete strip.dataset.collapsedFab;
        delete strip.dataset.animate;
        delete strip.dataset.edge;

        const location = border.getLocation();
        const locationName = location.getName();
        const dockState = border.getDockState();
        const isCollapsed = dockState === "collapsed";
        const isExpanded = dockState === "expanded";
        const selected = border.getSelected();
        const isVerticalBorder = location === DockLocation.LEFT || location === DockLocation.RIGHT;
        const stripSize = isExpanded && selected !== -1 ? 0 : BORDER_BAR_SIZE;

        let classes = `${this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_ + locationName)}`;
        if (border.getClassName()) {
            classes += ` ${border.getClassName()}`;
        }
        if (isCollapsed) {
            classes += ` ${this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER__COLLAPSED)}`;
        }

        strip.className = classes;
        strip.dataset.layoutPath = border.getPath();
        strip.dataset.state = dockState;
        strip.style.display = "flex";
        strip.style.flexDirection = border.getOrientation() === Orientation.VERT ? "row" : "column";
        strip.style.position = isCollapsed ? "relative" : "";
        strip.style.zIndex = isCollapsed ? "1" : "";

        if (isVerticalBorder) {
            strip.style.width = `${stripSize - 1}px`;
            strip.style.minWidth = `${stripSize - 1}px`;
            strip.style.height = "";
            strip.style.minHeight = "";
        } else {
            strip.style.height = `${stripSize - 1}px`;
            strip.style.minHeight = `${stripSize - 1}px`;
            strip.style.width = "";
            strip.style.minWidth = "";
        }

        const showTabButtons = !isExpanded || selected === -1;
        if (!showTabButtons || stripSize === 0) {
            strip.replaceChildren();
            return strip;
        }

        const miniScrollbar = document.createElement("div");
        miniScrollbar.className = this.options.getClassName(CLASSES.FLEXLAYOUT__MINI_SCROLLBAR_CONTAINER);
        if (isCollapsed && isVerticalBorder) {
            miniScrollbar.style.flex = "1";
            miniScrollbar.style.overflow = "visible";
        }

        const inner = document.createElement("div");
        inner.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_INNER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_INNER_ + locationName)}`;
        if (isVerticalBorder) {
            inner.style.width = `${stripSize - 1}px`;
            inner.style.minWidth = `${stripSize - 1}px`;
            if (isCollapsed) {
                inner.style.position = "relative";
                inner.style.overflow = "visible";
                inner.style.flex = "1";
            } else {
                inner.style.overflowY = "auto";
                inner.style.position = "";
                inner.style.flex = "";
            }
        } else {
            inner.style.height = `${stripSize - 1}px`;
            inner.style.minHeight = `${stripSize - 1}px`;
            inner.style.overflowX = "auto";
            inner.style.position = "";
            inner.style.flex = "";
        }

        const tabContainer = document.createElement("div");
        tabContainer.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_INNER_TAB_CONTAINER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_INNER_TAB_CONTAINER_ + locationName)}`;

        const tabs = border.getChildren() as TabNode[];
        const tabButtons: HTMLElement[] = [];
        for (let i = 0; i < tabs.length; i++) {
            const tab = tabs[i];
            const selectedTab = border.getSelected() === i;
            tabButtons.push(this.renderBorderButton(tab, border, i, selectedTab, seenButtons));
            if (i < tabs.length - 1) {
                const divider = document.createElement("div");
                divider.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_TAB_DIVIDER);
                tabButtons.push(divider);
            }
        }
        tabContainer.replaceChildren(...tabButtons);
        inner.replaceChildren(tabContainer);
        miniScrollbar.replaceChildren(inner);
        strip.replaceChildren(miniScrollbar);

        border.setTabHeaderRect(this.getBoundingClientRect(strip));
        return strip;
    }

    private renderExpandedBorderContentItems(border: BorderNode, seenButtons: Set<string>): HTMLElement[] {
        let content = this.borderContentElements.get(border.getId());
        if (!content) {
            content = document.createElement("div");
            this.borderContentElements.set(border.getId(), content);
        }

        const dockState = border.getDockState();
        const selected = border.getSelected();
        const isContentVisible = dockState === "expanded" && selected !== -1;
        const state = this.borderTransitionStates.get(border.getId()) ?? dockState;

        content.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_TAB_CONTENTS);
        content.dataset.borderContent = "true";
        content.dataset.state = state;
        content.dataset.animate = border.isAnimateTransition() ? "true" : "false";
        content.style.display = isContentVisible ? "flex" : "none";
        content.style.flexDirection = "column";

        if (border.getOrientation() === Orientation.HORZ) {
            content.style.width = `${border.getSize()}px`;
            content.style.minWidth = `${border.getMinSize()}px`;
            content.style.maxWidth = `${border.getMaxSize()}px`;
            content.style.height = "";
            content.style.minHeight = "";
            content.style.maxHeight = "";
        } else {
            content.style.height = `${border.getSize()}px`;
            content.style.minHeight = `${border.getMinSize()}px`;
            content.style.maxHeight = `${border.getMaxSize()}px`;
            content.style.width = "";
            content.style.minWidth = "";
            content.style.maxWidth = "";
        }

        if (!isContentVisible) {
            content.replaceChildren();
            border.setContentRect(Rect.empty());
            return [];
        }

        const visibleIndices = this.resolveBorderVisibleTabs(border);
        const visibleNodes = visibleIndices
            .map((index) => border.getChildren()[index])
            .filter((node): node is TabNode => node instanceof TabNode);
        const isTiled = visibleNodes.length > 1;
        const tileHorizontal = this.borderTilingIsHorizontal(border);
        const tabBar = document.createElement("div");
        tabBar.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_TABBAR);
        tabBar.dataset.borderTabbar = "true";

        if (!isTiled) {
            const tabButtonsContainer = document.createElement("div");
            tabButtonsContainer.style.display = "flex";
            tabButtonsContainer.style.flex = "1";
            tabButtonsContainer.style.overflowX = "auto";
            tabButtonsContainer.style.alignItems = "center";
            tabButtonsContainer.style.paddingLeft = "4px";

            const tabButtons: HTMLElement[] = [];
            for (let i = 0; i < border.getChildren().length; i++) {
                const child = border.getChildren()[i];
                if (!(child instanceof TabNode)) {
                    continue;
                }
                tabButtons.push(this.renderBorderButton(child, border, i, border.getSelected() === i, seenButtons));
                if (i < border.getChildren().length - 1) {
                    const divider = document.createElement("div");
                    divider.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_TAB_DIVIDER);
                    tabButtons.push(divider);
                }
            }
            tabButtonsContainer.replaceChildren(...tabButtons);

            const toolbar = document.createElement("div");
            toolbar.style.display = "flex";
            toolbar.style.alignItems = "center";
            toolbar.style.padding = "0 4px";
            toolbar.replaceChildren(...this.createExpandedBorderToolbarButtons(border));

            tabBar.replaceChildren(tabButtonsContainer, toolbar);

            const tileHost = document.createElement("div");
            tileHost.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BORDER)}`;
            tileHost.style.flex = "1";
            tileHost.style.position = "relative";
            tileHost.style.overflow = "hidden";
            tileHost.dataset.borderTile = "0";

            const selectedNode = visibleNodes[0];
            if (selectedNode) {
                this.borderTabHosts.set(selectedNode.getId(), tileHost);
            }

            const contentArea = document.createElement("div");
            contentArea.style.display = "flex";
            contentArea.style.flex = "1";
            contentArea.style.minWidth = "0";
            contentArea.style.minHeight = "0";
            contentArea.replaceChildren(tileHost);

            content.replaceChildren(tabBar, contentArea);
            border.setTabHeaderRect(this.getBoundingClientRect(tabBar));
            border.setContentRect(this.getBoundingClientRect(tileHost));
        } else {
            const tilesContainer = document.createElement("div");
            tilesContainer.style.display = "flex";
            tilesContainer.style.flex = "1";
            tilesContainer.style.minWidth = "0";
            tilesContainer.style.minHeight = "0";
            tilesContainer.style.flexDirection = tileHorizontal ? "row" : "column";

            const weights = this.getBorderTileWeights(border.getId(), visibleNodes.length);
            const splitters: HTMLElement[] = [];
            const splitterSize = border.getModel().getSplitterSize();
            const totalWeight = weights.reduce((sum, weight) => sum + weight, 0) || 1;
            const splitterCount = Math.max(0, visibleNodes.length - 1);

            for (let i = 0; i < visibleNodes.length; i++) {
                if (i > 0) {
                    const splitter = document.createElement("div");
                    splitter.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER_ + (tileHorizontal ? "horz" : "vert"))}`;
                    splitter.dataset.borderTileSplitter = String(i - 1);
                    splitter.style.cursor = tileHorizontal ? "ew-resize" : "ns-resize";
                    splitter.style.flexShrink = "0";
                    if (tileHorizontal) {
                        splitter.style.width = `${splitterSize}px`;
                        splitter.style.minWidth = `${splitterSize}px`;
                    } else {
                        splitter.style.height = `${splitterSize}px`;
                        splitter.style.minHeight = `${splitterSize}px`;
                    }
                    splitter.onpointerdown = (event) => this.onBorderTileSplitterPointerDown(border, i - 1, event, tilesContainer);
                    splitters.push(splitter);
                }

                const tile = document.createElement("div");
                tile.style.display = "flex";
                tile.style.flexDirection = "column";
                tile.style.overflow = "hidden";
                tile.style.position = "relative";
                const weight = weights[i] ?? 1;
                const pct = (weight / totalWeight) * 100;
                const splitterDeduction = splitterCount > 0
                    ? splitterSize * (splitterCount / visibleNodes.length)
                    : 0;
                tile.style.flex = `0 0 calc(${pct}% - ${splitterDeduction}px)`;
                tile.dataset.borderTile = String(i);

                const node = visibleNodes[i];
                const childIndex = border.getChildren().indexOf(node);
                const tileBar = document.createElement("div");
                tileBar.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_TABBAR);
                tileBar.dataset.borderTabbar = "true";

                const buttonContainer = document.createElement("div");
                buttonContainer.style.display = "flex";
                buttonContainer.style.flex = "1";
                buttonContainer.style.overflowX = "auto";
                buttonContainer.style.alignItems = "center";
                buttonContainer.style.paddingLeft = "4px";
                buttonContainer.replaceChildren(
                    this.renderBorderButton(node, border, childIndex, border.getSelected() === childIndex, seenButtons),
                );

                const tileChildren: HTMLElement[] = [buttonContainer];
                if (i === 0) {
                    const toolbar = document.createElement("div");
                    toolbar.style.display = "flex";
                    toolbar.style.alignItems = "center";
                    toolbar.style.padding = "0 4px";
                    toolbar.replaceChildren(...this.createExpandedBorderToolbarButtons(border));
                    tileChildren.push(toolbar);
                }
                tileBar.replaceChildren(...tileChildren);

                const tileHost = document.createElement("div");
                tileHost.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BORDER)}`;
                tileHost.style.flex = "1";
                tileHost.style.position = "relative";
                tileHost.style.overflow = "hidden";
                this.borderTabHosts.set(node.getId(), tileHost);

                tile.replaceChildren(tileBar, tileHost);
                tilesContainer.appendChild(tile);
                if (splitters[i - 1]) {
                    tilesContainer.insertBefore(splitters[i - 1], tile);
                }
            }

            content.replaceChildren(tilesContainer);
            border.setTabHeaderRect(this.getBoundingClientRect(tilesContainer.querySelector("[data-border-tabbar]") as HTMLElement));
            border.setContentRect(Rect.empty());
        }

        const edgeSplitter = this.createBorderEdgeSplitter(border);
        const location = border.getLocation();
        if (location === DockLocation.LEFT || location === DockLocation.TOP) {
            return [content, edgeSplitter];
        }
        return [edgeSplitter, content];
    }

    private resolveBorderVisibleTabs(border: BorderNode): number[] {
        const explicit = border.getVisibleTabs();
        if (explicit.length > 0) {
            return explicit;
        }
        const selected = border.getSelected();
        return selected >= 0 ? [selected] : [];
    }

    private borderTilingIsHorizontal(border: BorderNode): boolean {
        return border.getOrientation() === Orientation.VERT;
    }

    private getBorderTileWeights(borderId: string, count: number): number[] {
        const existing = this.borderTileWeights.get(borderId) ?? [];
        if (existing.length === count) {
            return existing;
        }
        const next = Array(count).fill(1);
        this.borderTileWeights.set(borderId, next);
        return next;
    }

    private onBorderTileSplitterPointerDown(
        border: BorderNode,
        splitterIndex: number,
        event: PointerEvent,
        tileContainer: HTMLElement,
    ): void {
        event.stopPropagation();
        event.preventDefault();

        const tileHorizontal = this.borderTilingIsHorizontal(border);
        const startPos = tileHorizontal ? event.clientX : event.clientY;
        const initialWeights = [...(this.borderTileWeights.get(border.getId()) ?? [])];
        const beforeIdx = splitterIndex;
        const afterIdx = splitterIndex + 1;
        const tiles = tileContainer.querySelectorAll<HTMLElement>("[data-border-tile]");
        const beforeEl = tiles[beforeIdx];
        const afterEl = tiles[afterIdx];
        if (!beforeEl || !afterEl) {
            return;
        }

        const beforeSize = tileHorizontal ? beforeEl.offsetWidth : beforeEl.offsetHeight;
        const afterSize = tileHorizontal ? afterEl.offsetWidth : afterEl.offsetHeight;
        const totalSize = beforeSize + afterSize;
        const totalWeight = (initialWeights[beforeIdx] ?? 1) + (initialWeights[afterIdx] ?? 1);
        const minPx = 30;

        const onMove = (moveEvent: PointerEvent): void => {
            const currentPos = tileHorizontal ? moveEvent.clientX : moveEvent.clientY;
            const delta = currentPos - startPos;

            const newBeforeSize = Math.max(minPx, Math.min(totalSize - minPx, beforeSize + delta));
            const newAfterSize = totalSize - newBeforeSize;

            const nextWeights = [...initialWeights];
            nextWeights[beforeIdx] = (newBeforeSize / totalSize) * totalWeight;
            nextWeights[afterIdx] = (newAfterSize / totalSize) * totalWeight;
            this.borderTileWeights.set(border.getId(), nextWeights);
            this.render();
        };

        const onUp = (): void => {
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);
        };

        document.addEventListener("pointermove", onMove);
        document.addEventListener("pointerup", onUp);
    }

    private createExpandedBorderToolbarButtons(border: BorderNode): HTMLButtonElement[] {
        const buttons: HTMLButtonElement[] = [];
        if (!border.isEnableDock()) {
            return buttons;
        }

        const button = document.createElement("button");
        button.type = "button";
        button.dataset.layoutPath = `${border.getPath()}/button/dock`;
        button.title = border.getDockState() === "expanded" ? "Collapse" : "Expand";
        button.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_DOCK_BUTTON);
        button.textContent = this.getBorderDockIcon(border);
        button.onpointerdown = (event) => event.stopPropagation();
        button.onclick = (event) => {
            event.stopPropagation();
            this.doAction(Action.setDockState(border.getId(), border.getDockState() === "expanded" ? "collapsed" : "expanded"));
        };

        buttons.push(button);
        return buttons;
    }

    private getBorderDockIcon(border: BorderNode): string {
        const state = border.getDockState();
        const loc = border.getLocation();
        if (state === "collapsed") {
            if (loc === DockLocation.LEFT) return "▶";
            if (loc === DockLocation.RIGHT) return "◀";
            if (loc === DockLocation.TOP) return "▼";
            return "▲";
        }
        if (loc === DockLocation.LEFT) return "◀";
        if (loc === DockLocation.RIGHT) return "▶";
        if (loc === DockLocation.TOP) return "▲";
        return "▼";
    }

    private createBorderEdgeSplitter(border: BorderNode): HTMLDivElement {
        const splitter = document.createElement("div");
        splitter.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER_ + border.getOrientation().getName())} ${this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER_BORDER)}`;
        splitter.dataset.layoutPath = `${border.getPath()}/s-1`;

        const horizontal = border.getOrientation() === Orientation.HORZ;
        const size = border.getModel().getSplitterSize();
        splitter.style.cursor = horizontal ? "ew-resize" : "ns-resize";
        splitter.style.flexDirection = horizontal ? "column" : "row";
        if (horizontal) {
            splitter.style.width = `${size}px`;
            splitter.style.minWidth = `${size}px`;
        } else {
            splitter.style.height = `${size}px`;
            splitter.style.minHeight = `${size}px`;
        }

        splitter.onpointerdown = (event) => this.onBorderEdgeSplitterPointerDown(event, border, horizontal, splitter);
        return splitter;
    }

    private onBorderEdgeSplitterPointerDown(
        event: PointerEvent,
        border: BorderNode,
        horizontal: boolean,
        splitter: HTMLDivElement,
    ): void {
        event.stopPropagation();
        event.preventDefault();

        const bounds = border.getSplitterBounds(0);
        const isRealtime = this.options.model.isRealtimeResize();
        const domRect = splitter.getBoundingClientRect();
        const layoutRect = this.getDomRect();
        const splitterRect = new Rect(
            domRect.x - layoutRect.x,
            domRect.y - layoutRect.y,
            domRect.width,
            domRect.height,
        );

        const dragStartX = event.clientX - domRect.x;
        const dragStartY = event.clientY - domRect.y;
        let outlineDiv: HTMLDivElement | undefined;

        if (!isRealtime && this.rootDiv) {
            outlineDiv = document.createElement("div");
            outlineDiv.style.flexDirection = horizontal ? "row" : "column";
            outlineDiv.className = this.options.getClassName(CLASSES.FLEXLAYOUT__SPLITTER_DRAG);
            outlineDiv.style.cursor = border.getOrientation() === Orientation.VERT ? "ns-resize" : "ew-resize";
            splitterRect.positionElement(outlineDiv);
            this.rootDiv.appendChild(outlineDiv);
        }

        const clampPosition = (position: number): number => {
            return Math.max(bounds[0], Math.min(bounds[1], position));
        };

        const applyAtPosition = (position: number): void => {
            const size = border.calculateSplit(border, position);
            this.doAction(Action.adjustBorderSplit(border.getId(), size));
        };

        const onMove = (moveEvent: PointerEvent): void => {
            const clientRect = this.getDomRect();
            const position = border.getOrientation() === Orientation.VERT
                ? clampPosition(moveEvent.clientY - clientRect.y - dragStartY)
                : clampPosition(moveEvent.clientX - clientRect.x - dragStartX);

            if (isRealtime) {
                applyAtPosition(position);
            } else if (outlineDiv) {
                if (border.getOrientation() === Orientation.VERT) {
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
                const value = border.getOrientation() === Orientation.VERT
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

    private renderBorderButton(
        node: TabNode,
        border: BorderNode,
        index: number,
        selected: boolean,
        seenButtons: Set<string>,
    ): HTMLDivElement {
        const buttonId = node.getId();
        seenButtons.add(buttonId);

        let button = this.borderButtonElements.get(buttonId);
        if (!button) {
            button = document.createElement("div");
            this.borderButtonElements.set(buttonId, button);
        }

        const borderName = border.getLocation().getName();
        const path = `${border.getPath()}/tb${index}`;
        let className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_BUTTON)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_BUTTON_ + borderName)}`;
        className += ` ${this.options.getClassName(selected ? CLASSES.FLEXLAYOUT__BORDER_BUTTON__SELECTED : CLASSES.FLEXLAYOUT__BORDER_BUTTON__UNSELECTED)}`;
        if (node.getClassName()) {
            className += ` ${node.getClassName()}`;
        }

        button.className = className;
        button.dataset.layoutPath = path;
        button.dataset.state = selected ? "selected" : "unselected";
        button.title = node.getHelpText() ?? "";
        button.draggable = true;

        button.onclick = () => {
            this.doAction(Action.selectTab(node.getId()));
        };

        button.ondragstart = (event) => {
            if (node.isEnableDrag()) {
                event.stopPropagation();
                if (!selected) {
                    this.doAction(Action.selectTab(node.getId()));
                }
                this.dndManager.setDragNode(event, node);
            } else {
                event.preventDefault();
            }
        };

        button.ondragend = () => {
            this.dndManager.clearDragMain();
        };

        button.oncontextmenu = (event) => {
            event.preventDefault();
            event.stopPropagation();
            const parent = node.getParent();
            if (!(parent instanceof BorderNode)) {
                return;
            }
            if (parent.getDockState() !== "expanded") {
                return;
            }

            const children = parent.getChildren() as TabNode[];
            const visibleTabs = parent.getVisibleTabs();
            const myIndex = children.indexOf(node);
            const isTiled = visibleTabs.length > 1;

            const items: Array<{ label: string; action: () => void }> = [];
            if (isTiled) {
                items.push({
                    label: "Untile",
                    action: () => this.doAction(Action.setVisibleTabs(parent.getId(), [])),
                });
            }

            for (let i = 0; i < children.length; i++) {
                if (i === myIndex || visibleTabs.includes(i)) {
                    continue;
                }
                const targetTab = children[i];
                items.push({
                    label: `Split with ${targetTab.getName()}`,
                    action: () => {
                        const nextVisible = isTiled ? [...visibleTabs, i] : [myIndex, i];
                        this.doAction(Action.setVisibleTabs(parent.getId(), nextVisible));
                    },
                });
            }

            if (items.length > 0) {
                this.showContextMenu(event, items);
            }
        };

        const children: HTMLElement[] = [];
        const icon = node.getIcon();
        if (icon) {
            let iconAngle = 0;
            if (node.getModel().getAttribute("enableRotateBorderIcons") === false) {
                if (borderName === "left") {
                    iconAngle = 90;
                } else if (borderName === "right") {
                    iconAngle = -90;
                }
            }

            const leading = document.createElement("div");
            leading.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_BUTTON_LEADING);
            const img = document.createElement("img");
            img.src = icon;
            img.alt = "";
            if (iconAngle !== 0) {
                img.style.transform = `rotate(${iconAngle}deg)`;
            }
            leading.appendChild(img);
            children.push(leading);
        }

        if (this.editingTab === node) {
            const input = document.createElement("input");
            input.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON_TEXTBOX);
            input.dataset.layoutPath = `${path}/textbox`;
            input.type = "text";
            input.value = node.getName();
            input.onkeydown = (event) => {
                if (event.code === "Escape") {
                    this.setEditingTab(undefined);
                } else if (event.code === "Enter" || event.code === "NumpadEnter") {
                    this.setEditingTab(undefined);
                    this.doAction(Action.renameTab(node.getId(), input.value));
                }
            };
            input.onpointerdown = (event) => event.stopPropagation();
            requestAnimationFrame(() => {
                input.focus();
                input.select();
            });
            children.push(input);
        } else {
            const content = document.createElement("div");
            content.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_BUTTON_CONTENT);
            content.textContent = node.getName();
            children.push(content);
        }

        if (node.isEnableClose()) {
            const close = document.createElement("div");
            close.dataset.layoutPath = `${path}/button/close`;
            close.title = "Close";
            close.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_BUTTON_TRAILING);
            close.textContent = "✕";
            close.onpointerdown = (event) => event.stopPropagation();
            close.onclick = (event) => {
                if (this.isBorderButtonClosable(node, selected)) {
                    this.doAction(Action.deleteTab(node.getId()));
                    event.stopPropagation();
                }
            };
            children.push(close);
        }

        button.replaceChildren(...children);

        const rect = this.getBoundingClientRect(button);
        if (borderName === "left" || borderName === "right") {
            node.setTabRect(new Rect(rect.x, rect.y, rect.width, rect.height + 1));
        } else {
            node.setTabRect(new Rect(rect.x, rect.y, rect.width + 1, rect.height));
        }

        return button;
    }

    private isBorderButtonClosable(node: TabNode, selected: boolean): boolean {
        const closeType = node.getCloseType();
        if (selected || closeType === ICloseType.Always) {
            return true;
        }
        if (closeType === ICloseType.Visible && window.matchMedia) {
            return window.matchMedia("(hover: hover) and (pointer: fine)").matches;
        }
        return false;
    }

    private updateBorderDockStates(): void {
        for (const border of this.options.model.getBorderSet().getBorders()) {
            const id = border.getId();
            const currentState = border.getDockState();
            const previousState = this.previousBorderDockStates.get(id);
            this.previousBorderDockStates.set(id, currentState);

            if (previousState === undefined) {
                continue;
            }

            if (previousState === "expanded" && currentState === "collapsed") {
                this.borderTransitionStates.set(id, "collapsing");
                if (border.isAnimateTransition()) {
                    setTimeout(() => {
                        this.borderTransitionStates.set(id, "collapsed");
                        this.render();
                    }, 250);
                } else {
                    this.borderTransitionStates.set(id, "collapsed");
                }
            } else if (previousState === "collapsed" && currentState === "expanded") {
                this.borderTransitionStates.set(id, "expanding");
                if (border.isAnimateTransition()) {
                    setTimeout(() => {
                        this.borderTransitionStates.set(id, "expanded");
                        this.render();
                    }, 250);
                } else {
                    this.borderTransitionStates.set(id, "expanded");
                }
            }
        }
    }

    private renderEdges(): void {
        if (!this.rootDiv) {
            return;
        }
        const existing = this.rootDiv.querySelector('[data-layout-path="/edges"]') as HTMLElement | null;
        if (!this.showEdges) {
            existing?.remove();
            return;
        }

        const edgeLength = 100;
        const edgeWidth = 10;
        const offset = edgeLength / 2;
        const radius = 50;

        const container = existing ?? document.createElement("div");
        container.dataset.layoutPath = "/edges";
        container.replaceChildren();

        const baseClass = this.options.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT);
        const edgeDefs: Array<{ className: string; style: Partial<CSSStyleDeclaration> }> = [
            {
                className: `${baseClass} ${this.options.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_TOP)}`,
                style: {
                    position: "absolute",
                    top: "0px",
                    left: `${this.rect.width / 2 - offset}px`,
                    width: `${edgeLength}px`,
                    height: `${edgeWidth}px`,
                    borderBottomLeftRadius: `${radius}%`,
                    borderBottomRightRadius: `${radius}%`,
                    zIndex: "999",
                },
            },
            {
                className: `${baseClass} ${this.options.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_LEFT)}`,
                style: {
                    position: "absolute",
                    top: `${this.rect.height / 2 - offset}px`,
                    left: "0px",
                    width: `${edgeWidth}px`,
                    height: `${edgeLength}px`,
                    borderTopRightRadius: `${radius}%`,
                    borderBottomRightRadius: `${radius}%`,
                    zIndex: "999",
                },
            },
            {
                className: `${baseClass} ${this.options.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_BOTTOM)}`,
                style: {
                    position: "absolute",
                    top: `${this.rect.height - edgeWidth}px`,
                    left: `${this.rect.width / 2 - offset}px`,
                    width: `${edgeLength}px`,
                    height: `${edgeWidth}px`,
                    borderTopLeftRadius: `${radius}%`,
                    borderTopRightRadius: `${radius}%`,
                    zIndex: "999",
                },
            },
            {
                className: `${baseClass} ${this.options.getClassName(CLASSES.FLEXLAYOUT__EDGE_RECT_RIGHT)}`,
                style: {
                    position: "absolute",
                    top: `${this.rect.height / 2 - offset}px`,
                    left: `${this.rect.width - edgeWidth}px`,
                    width: `${edgeWidth}px`,
                    height: `${edgeLength}px`,
                    borderTopLeftRadius: `${radius}%`,
                    borderBottomLeftRadius: `${radius}%`,
                    zIndex: "999",
                },
            },
        ];

        for (const edge of edgeDefs) {
            const el = document.createElement("div");
            el.className = edge.className;
            Object.assign(el.style, edge.style);
            container.appendChild(el);
        }

        if (!existing) {
            this.rootDiv.appendChild(container);
        }
    }

    private renderCollapsedBorderStrips(): void {
        if (!this.rootDiv) {
            return;
        }

        const existing = this.rootDiv.querySelector('[data-layout-path="/collapsed-borders"]') as HTMLDivElement | null;
        const container = existing ?? document.createElement("div");
        container.dataset.layoutPath = "/collapsed-borders";
        container.style.position = "absolute";
        container.style.inset = "0";
        container.style.pointerEvents = "none";
        container.replaceChildren();

        for (const border of this.options.model.getBorderSet().getBorders()) {
            if (!border.isShowing() || border.getDockState() !== "collapsed") {
                continue;
            }
            if (border.getChildren().length === 0 && border.isAutoHide()) {
                this.renderEmptyBorderFab(container, border);
            }
        }

        if (!existing) {
            this.rootDiv.appendChild(container);
        }
    }

    private createCollapsedStripFab(border: BorderNode): HTMLButtonElement {
        const loc = border.getLocation();
        const fab = document.createElement("button");
        fab.type = "button";
        fab.dataset.layoutPath = `${border.getPath()}/button/dock`;
        fab.dataset.collapsedFab = "true";
        fab.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_DOCK_BUTTON);
        fab.title = "Expand";

        let arrow: string;
        if (loc === DockLocation.LEFT) arrow = "▶";
        else if (loc === DockLocation.RIGHT) arrow = "◀";
        else if (loc === DockLocation.TOP) arrow = "▼";
        else arrow = "▲";
        fab.textContent = arrow;

        fab.addEventListener("pointerdown", (e) => e.stopPropagation());
        fab.addEventListener("click", (event) => {
            event.stopPropagation();
            this.doAction(Action.setDockState(border.getId(), "expanded"));
        });

        return fab;
    }

    private renderEmptyBorderFab(container: HTMLElement, border: BorderNode): void {
        const loc = border.getLocation();
        const fab = document.createElement("button");
        fab.type = "button";
        fab.dataset.layoutPath = `${border.getPath()}/fab`;
        fab.dataset.emptyBorderFab = "true";
        fab.className = this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER_FAB);
        fab.style.position = "absolute";
        fab.style.pointerEvents = "auto";
        fab.style.zIndex = "910";

        if (loc === DockLocation.LEFT) {
            fab.style.left = "4px";
            fab.style.top = "50%";
            fab.style.transform = "translateY(-50%)";
        } else if (loc === DockLocation.RIGHT) {
            fab.style.right = "4px";
            fab.style.top = "50%";
            fab.style.transform = "translateY(-50%)";
        } else if (loc === DockLocation.TOP) {
            fab.style.top = "4px";
            fab.style.left = "50%";
            fab.style.transform = "translateX(-50%)";
        } else {
            fab.style.bottom = "4px";
            fab.style.left = "50%";
            fab.style.transform = "translateX(-50%)";
        }

        let arrow: string;
        if (loc === DockLocation.LEFT) arrow = "▶";
        else if (loc === DockLocation.RIGHT) arrow = "◀";
        else if (loc === DockLocation.TOP) arrow = "▼";
        else arrow = "▲";
        fab.textContent = arrow;

        fab.addEventListener("click", (event) => {
            event.stopPropagation();
            this.doAction(Action.setDockState(border.getId(), "expanded"));
        });

        fab.addEventListener("dragover", (e) => {
            e.preventDefault();
            if (e.dataTransfer) {
                e.dataTransfer.dropEffect = "move";
            }
        });
        fab.addEventListener("drop", (e) => {
            e.preventDefault();
            this.doAction(Action.setDockState(border.getId(), "expanded"));
        });

        container.appendChild(fab);
    }

    private updateFlyoutState(): void {
        this.flyoutState = undefined;
        for (const border of this.options.model.getBorderSet().getBorders()) {
            const flyoutTabId = border.getFlyoutTabId();
            if (flyoutTabId === null) {
                continue;
            }

            const tab = border.getChildren().find((child) => child instanceof TabNode && child.getId() === flyoutTabId);
            if (!(tab instanceof TabNode)) {
                continue;
            }

            const size = border.getSize();
            const location = border.getLocation();
            let rect: Rect;
            if (location === DockLocation.LEFT) {
                rect = new Rect(BORDER_BAR_SIZE, 0, size, this.rect.height);
            } else if (location === DockLocation.RIGHT) {
                rect = new Rect(this.rect.width - BORDER_BAR_SIZE - size, 0, size, this.rect.height);
            } else if (location === DockLocation.TOP) {
                rect = new Rect(0, BORDER_BAR_SIZE, this.rect.width, size);
            } else {
                rect = new Rect(0, this.rect.height - BORDER_BAR_SIZE - size, this.rect.width, size);
            }

            this.flyoutState = { border, tab, rect };
            return;
        }
    }

    private getFlyoutEdgeName(): string {
        if (!this.flyoutState) {
            return "left";
        }
        const loc = this.flyoutState.border.getLocation();
        if (loc === DockLocation.LEFT) return "left";
        if (loc === DockLocation.RIGHT) return "right";
        if (loc === DockLocation.TOP) return "top";
        return "bottom";
    }

    private renderFlyout(): void {
        if (!this.rootDiv) {
            return;
        }

        if (!this.flyoutState) {
            if (this.flyoutPanel && this.flyoutBackdrop) {
                this.flyoutPanel.dataset.state = "exiting";
                this.flyoutBackdrop.dataset.state = "exiting";

                if (this.flyoutExitTimer) {
                    clearTimeout(this.flyoutExitTimer);
                }

                const panel = this.flyoutPanel;
                const backdrop = this.flyoutBackdrop;
                this.flyoutPanel = undefined;
                this.flyoutBackdrop = undefined;

                const cleanup = () => {
                    panel.remove();
                    backdrop.remove();
                };

                panel.addEventListener("transitionend", cleanup, { once: true });
                this.flyoutExitTimer = setTimeout(cleanup, 300);
            }
            return;
        }

        if (this.flyoutExitTimer) {
            clearTimeout(this.flyoutExitTimer);
            this.flyoutExitTimer = undefined;
        }

        // Immediately remove any stale exiting flyout panels to avoid duplicate
        // data-layout-path="/flyout/panel" elements in the DOM.
        if (this.rootDiv) {
            for (const stale of this.rootDiv.querySelectorAll('[data-layout-path="/flyout/panel"][data-state="exiting"]')) {
                stale.remove();
            }
            for (const stale of this.rootDiv.querySelectorAll('[data-layout-path="/flyout/backdrop"][data-state="exiting"]')) {
                stale.remove();
            }
        }

        const edgeName = this.getFlyoutEdgeName();

        if (!this.flyoutBackdrop) {
            this.flyoutBackdrop = document.createElement("div");
            this.flyoutBackdrop.dataset.layoutPath = "/flyout/backdrop";
            this.flyoutBackdrop.className = "flexlayout__flyout_backdrop";
            this.flyoutBackdrop.dataset.state = "exited";
            this.flyoutBackdrop.style.position = "absolute";
            this.flyoutBackdrop.style.inset = "0";
            this.flyoutBackdrop.style.zIndex = "850";
            this.flyoutBackdrop.addEventListener("pointerdown", (event) => {
                event.stopPropagation();
                this.doAction(Action.closeFlyout(this.flyoutState!.border.getId()));
            });
            this.rootDiv.appendChild(this.flyoutBackdrop);
        }

        if (!this.flyoutPanel) {
            this.flyoutPanel = document.createElement("div");
            this.flyoutPanel.dataset.layoutPath = "/flyout/panel";
            this.flyoutPanel.className = "flexlayout__flyout_panel";
            this.flyoutPanel.dataset.state = "exited";
            this.flyoutPanel.dataset.edge = edgeName;
            this.flyoutPanel.style.position = "absolute";
            this.flyoutPanel.style.zIndex = "900";
            this.flyoutPanel.addEventListener("pointerdown", (event) => {
                event.stopPropagation();
            });
            this.rootDiv.appendChild(this.flyoutPanel);
        }

        this.flyoutPanel.dataset.edge = edgeName;

        const style: Record<string, string> = {};
        this.flyoutState.rect.styleWithPosition(style);
        Object.assign(this.flyoutPanel.style, style);

        if (this.flyoutPanel.dataset.state === "exited") {
            requestAnimationFrame(() => {
                if (this.flyoutPanel) {
                    this.flyoutPanel.dataset.state = "entering";
                }
                if (this.flyoutBackdrop) {
                    this.flyoutBackdrop.dataset.state = "entering";
                }
                requestAnimationFrame(() => {
                    if (this.flyoutPanel) {
                        this.flyoutPanel.dataset.state = "entered";
                    }
                    if (this.flyoutBackdrop) {
                        this.flyoutBackdrop.dataset.state = "entered";
                    }
                });
            });
        } else {
            this.flyoutPanel.dataset.state = "entered";
            this.flyoutBackdrop.dataset.state = "entered";
        }
    }

    private collectPaneviewTabSets(): TabSetNode[] {
        const result: TabSetNode[] = [];
        const root = this.options.model.getRoot();
        if (!root) {
            return result;
        }
        const visit = (node: Node): void => {
            if (node instanceof TabSetNode && node.getMode() === "paneview") {
                result.push(node);
            }
            for (const child of node.getChildren()) {
                visit(child);
            }
        };
        visit(root);
        return result;
    }

    private isPaneviewTab(node: TabNode): boolean {
        const parent = node.getParent();
        return parent instanceof TabSetNode && parent.getMode() === "paneview";
    }

    private renderPaneviews(): void {
        if (!this.rootDiv) {
            return;
        }

        const paneviewTabSets = this.collectPaneviewTabSets();
        const seenIds = new Set<string>();

        for (const tabset of paneviewTabSets) {
            const tsId = tabset.getId();
            seenIds.add(tsId);
            const tsRect = tabset.getRect();

            let container = this.paneviewContainers.get(tsId);
            if (!container) {
                container = document.createElement("div");
                container.className = this.options.getClassName("flexlayout__paneview");
                container.dataset.layoutPath = tabset.getPath() + "/paneview";
                this.rootDiv.appendChild(container);
                this.paneviewContainers.set(tsId, container);
            }

            const posStyle: Record<string, string> = {};
            tsRect.styleWithPosition(posStyle);
            Object.assign(container.style, posStyle);
            container.style.display = "flex";

            const children = tabset.getChildren() as TabNode[];
            const existingSections = new Map<string, HTMLElement>();
            for (const sec of Array.from(container.children) as HTMLElement[]) {
                const tabId = sec.dataset.paneviewTabId;
                if (tabId) {
                    existingSections.set(tabId, sec);
                }
            }

            const sectionOrder: HTMLElement[] = [];

            for (let i = 0; i < children.length; i++) {
                const tab = children[i];
                const tabId = tab.getId();

                if (!this.paneviewExpanded.has(tabId)) {
                    this.paneviewExpanded.set(tabId, i === 0);
                }
                const isExpanded = this.paneviewExpanded.get(tabId) ?? false;

                let section = existingSections.get(tabId);
                if (!section) {
                    section = this.createPaneviewSection(tabset, tab, i);
                }

                existingSections.delete(tabId);

                const header = section.querySelector("[data-paneview-header]") as HTMLElement;
                if (header) {
                    header.dataset.state = isExpanded ? "expanded" : "collapsed";
                    const chevron = header.querySelector("[data-paneview-chevron]") as HTMLElement;
                    if (chevron) {
                        chevron.textContent = isExpanded ? "▾" : "▸";
                    }
                    const nameSpan = header.querySelector("[data-paneview-name]") as HTMLElement;
                    if (nameSpan) {
                        nameSpan.textContent = tab.getName();
                    }
                }

                const contentWrapper = section.querySelector("[data-paneview-content]") as HTMLElement;
                if (contentWrapper) {
                    contentWrapper.style.display = isExpanded ? "block" : "none";
                    contentWrapper.dataset.state = isExpanded ? "expanded" : "collapsed";
                    if (isExpanded) {
                        contentWrapper.style.flex = "1";
                        contentWrapper.style.minHeight = "0";
                    } else {
                        contentWrapper.style.flex = "";
                        contentWrapper.style.minHeight = "";
                    }
                }

                section.dataset.paneviewTabId = tabId;
                section.dataset.paneviewIndex = String(i);
                section.dataset.layoutPath = tabset.getPath() + "/pane" + i;
                sectionOrder.push(section);
            }

            for (const [, orphan] of existingSections) {
                orphan.remove();
            }

            for (const sec of sectionOrder) {
                container.appendChild(sec);
            }
        }

        for (const [tsId, container] of this.paneviewContainers) {
            if (!seenIds.has(tsId)) {
                container.remove();
                this.paneviewContainers.delete(tsId);
            }
        }
    }

    private createPaneviewSection(tabset: TabSetNode, tab: TabNode, index: number): HTMLElement {
        const section = document.createElement("div");
        section.className = this.options.getClassName("flexlayout__paneview_section");
        section.dataset.paneviewTabId = tab.getId();

        const header = document.createElement("div");
        header.className = this.options.getClassName("flexlayout__paneview_header");
        header.dataset.paneviewHeader = "true";
        header.dataset.layoutPath = tabset.getPath() + "/pane" + index + "/header";
        header.draggable = true;

        const chevron = document.createElement("span");
        chevron.dataset.paneviewChevron = "true";
        chevron.className = this.options.getClassName("flexlayout__paneview_chevron");
        chevron.textContent = "▾";

        const name = document.createElement("span");
        name.dataset.paneviewName = "true";
        name.className = this.options.getClassName("flexlayout__paneview_name");
        name.textContent = tab.getName();

        header.appendChild(chevron);
        header.appendChild(name);

        header.addEventListener("click", () => {
            const current = this.paneviewExpanded.get(tab.getId()) ?? false;
            this.paneviewExpanded.set(tab.getId(), !current);
            this.render();
        });

        header.addEventListener("dragstart", (e) => {
            e.dataTransfer?.setData("text/plain", tab.getId());
            section.style.opacity = "0.5";

            const placeholder = document.createElement("div");
            placeholder.className = this.options.getClassName("flexlayout__paneview_placeholder");
            this.paneviewDragState = {
                tabsetId: tabset.getId(),
                dragTabId: tab.getId(),
                placeholder,
            };
        });

        header.addEventListener("dragend", () => {
            section.style.opacity = "";
            if (this.paneviewDragState?.placeholder.parentNode) {
                this.paneviewDragState.placeholder.remove();
            }
            this.paneviewDragState = undefined;
        });

        section.addEventListener("dragover", (e) => {
            if (!this.paneviewDragState || this.paneviewDragState.tabsetId !== tabset.getId()) {
                return;
            }
            e.preventDefault();
            e.dataTransfer!.dropEffect = "move";
        });

        section.addEventListener("drop", (e) => {
            e.preventDefault();
            if (!this.paneviewDragState || this.paneviewDragState.tabsetId !== tabset.getId()) {
                return;
            }
            const dropIndex = Number.parseInt(section.dataset.paneviewIndex ?? "0", 10);
            this.doAction(Action.moveNode(
                this.paneviewDragState.dragTabId,
                tabset.getId(),
                "center",
                dropIndex,
            ));
            this.paneviewDragState = undefined;
        });

        const contentWrapper = document.createElement("div");
        contentWrapper.className = this.options.getClassName("flexlayout__paneview_content");
        contentWrapper.dataset.paneviewContent = "true";
        contentWrapper.dataset.layoutPath = tabset.getPath() + "/pane" + index + "/content";

        section.appendChild(header);
        section.appendChild(contentWrapper);

        return section;
    }

    private renderTabContents(): void {
        if (!this.rootDiv) {
            return;
        }

        const tabs = this.collectTabNodes();
        const seen = new Set<string>();

        for (const { node, parent } of tabs) {
            const tabId = node.getId();
            seen.add(tabId);

            const inPaneview = this.isPaneviewTab(node);

            let container = this.contentContainers.get(tabId);
            if (!container) {
                container = document.createElement("div");
                container.className = this.options.getClassName(CLASSES.FLEXLAYOUT__TAB);
                container.dataset.layoutPath = node.getPath();
                container.addEventListener("pointerdown", () => {
                    const p = node.getParent();
                    if (p instanceof TabSetNode && !p.isActive()) {
                        this.doAction(Action.setActiveTabset(p.getId(), Model.MAIN_WINDOW_ID));
                    }
                });

                if (inPaneview) {
                    const tsId = (parent as TabSetNode).getId();
                    const paneContainer = this.paneviewContainers.get(tsId);
                    const sections = paneContainer?.querySelectorAll(`[data-paneview-tab-id="${tabId}"] [data-paneview-content]`);
                    const contentWrapper = sections?.[0] as HTMLElement | undefined;
                    if (contentWrapper) {
                        contentWrapper.appendChild(container);
                    } else {
                        this.rootDiv.appendChild(container);
                    }
                } else {
                    this.rootDiv.appendChild(container);
                }

                this.contentContainers.set(tabId, container);

                const renderer = this.options.createContentRenderer(node);
                renderer.init(container, {
                    node,
                    selected: node.isSelected(),
                    windowId: Model.MAIN_WINDOW_ID,
                });
                this.contentRenderers.set(tabId, renderer);
            }

            if (inPaneview) {
                const isExpanded = this.paneviewExpanded.get(tabId) ?? false;
                if (isExpanded) {
                    container.style.position = "relative";
                    container.style.width = "100%";
                    container.style.height = "100%";
                    container.style.display = "block";
                    container.style.top = "";
                    container.style.left = "";
                    container.style.zIndex = "";
                } else {
                    container.style.display = "none";
                }

                const tsId = (parent as TabSetNode).getId();
                const paneContainer = this.paneviewContainers.get(tsId);
                if (paneContainer && !container.parentElement?.hasAttribute("data-paneview-content")) {
                    const contentWrapper = paneContainer.querySelector(`[data-paneview-tab-id="${tabId}"] [data-paneview-content]`) as HTMLElement | null;
                    if (contentWrapper && container.parentElement !== contentWrapper) {
                        contentWrapper.appendChild(container);
                    }
                }
            } else {
                let shouldShow = false;
                let style: Record<string, string> = {};

                if (parent instanceof BorderNode) {
                    const host = this.borderTabHosts.get(tabId);
                    if (host) {
                        if (container.parentElement !== host) {
                            host.appendChild(container);
                        }
                        container.style.position = "relative";
                        container.style.width = "100%";
                        container.style.height = "100%";
                        container.style.top = "";
                        container.style.left = "";
                        container.style.zIndex = "";
                        shouldShow = true;
                    } else {
                        const flyoutState = this.flyoutState;
                        if (flyoutState && flyoutState.border.getId() === parent.getId() && flyoutState.tab.getId() === node.getId()) {
                            if (container.parentElement !== this.rootDiv) {
                                this.rootDiv.appendChild(container);
                            }
                            flyoutState.rect.styleWithPosition(style);
                            shouldShow = true;
                            container.style.zIndex = "901";
                        }
                    }
                } else {
                    const contentRect = parent.getContentRect();
                    if (contentRect.width > 0 && contentRect.height > 0 && node.isSelected()) {
                        if (container.parentElement !== this.rootDiv) {
                            this.rootDiv.appendChild(container);
                        }
                        contentRect.styleWithPosition(style);
                        shouldShow = true;
                        container.style.zIndex = "";
                    }
                }

                if (shouldShow) {
                    Object.assign(container.style, style);
                    container.style.display = "block";
                } else {
                    container.style.display = "none";
                }
            }

            container.dataset.layoutPath = node.getPath();

            const renderer = this.contentRenderers.get(tabId);
            renderer?.update({
                selected: inPaneview ? (this.paneviewExpanded.get(tabId) ?? false) : node.isSelected(),
                windowId: Model.MAIN_WINDOW_ID,
            });
        }

        for (const [tabId, container] of this.contentContainers) {
            if (!seen.has(tabId)) {
                this.contentRenderers.get(tabId)?.dispose();
                this.contentRenderers.delete(tabId);
                container.remove();
                this.contentContainers.delete(tabId);
            }
        }
    }

    private collectTabNodes(): Array<{ node: TabNode; parent: TabSetNode | BorderNode }> {
        const tabs: Array<{ node: TabNode; parent: TabSetNode | BorderNode }> = [];
        const root = this.options.model.getRoot();

        const visit = (node: Node): void => {
            if (node instanceof TabNode) {
                tabs.push({
                    node,
                    parent: node.getParent() as TabSetNode | BorderNode,
                });
            }
            for (const child of node.getChildren()) {
                visit(child);
            }
        };

        if (root) {
            visit(root);
        }

        for (const border of this.options.model.getBorderSet().getBorders()) {
            for (const child of border.getChildren()) {
                if (child instanceof TabNode) {
                    tabs.push({ node: child, parent: border });
                }
            }
        }

        return tabs;
    }

    private getBoundingClientRect(div: HTMLElement): Rect {
        const layoutRect = this.getDomRect();
        const divRect = div.getBoundingClientRect();
        return new Rect(
            divRect.x - layoutRect.x,
            divRect.y - layoutRect.y,
            divRect.width,
            divRect.height,
        );
    }

    private getDomRect(): Rect {
        if (!this.rootDiv) {
            return Rect.empty();
        }
        const r = this.rootDiv.getBoundingClientRect();
        return new Rect(r.x, r.y, r.width, r.height);
    }
}
