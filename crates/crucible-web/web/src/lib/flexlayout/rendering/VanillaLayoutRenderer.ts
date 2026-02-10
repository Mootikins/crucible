import { Rect } from "../core/Rect";
import { DockLocation } from "../core/DockLocation";
import { CLASSES } from "../core/Types";
import { Action, type LayoutAction } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import { Model } from "../model/Model";
import { Node } from "../model/Node";
import { RowNode } from "../model/RowNode";
import { TabNode } from "../model/TabNode";
import { TabSetNode } from "../model/TabSetNode";
import { LayoutEngine } from "../layout/LayoutEngine";
import type { IDisposable } from "../model/Event";
import type { IContentRenderer } from "./IContentRenderer";
import { VanillaDndManager } from "./VanillaDndManager";
import { VanillaPopupManager } from "./VanillaPopupManager";
import { VanillaFloatingWindowManager } from "./VanillaFloatingWindowManager";
import { BORDER_BAR_SIZE, handleCollapsedBorderTabClick } from "./VanillaBorderLayoutEngine";

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
    createContentRenderer(node: TabNode): IContentRenderer;
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
    private readonly disposables: IDisposable[] = [];
    private resizeObserver: ResizeObserver | undefined;
    private flyoutState: IFlyoutState | undefined;
    private flyoutPanel: HTMLDivElement | undefined;
    private flyoutBackdrop: HTMLDivElement | undefined;

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

        this.flyoutPanel = undefined;
        this.flyoutBackdrop = undefined;
        this.flyoutState = undefined;

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

        this.renderEdges();
        this.renderCollapsedBorderStrips();
        this.updateFlyoutState();
        this.renderFlyout();
        this.renderPaneviews();
        this.renderTabContents();
        this.floatingWindowManager?.render();
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
            if (!border.isShowing()) {
                continue;
            }

            const dockState = border.getDockState();
            if (dockState !== "collapsed") {
                continue;
            }

            const location = border.getLocation();
            const locationName = location.getName();
            const isVertical = location === DockLocation.LEFT || location === DockLocation.RIGHT;
            const hasTabs = border.getChildren().length > 0;

            if (!hasTabs && border.isAutoHide()) {
                this.renderEmptyBorderFab(container, border);
                continue;
            }

            const strip = document.createElement("div");
            strip.dataset.layoutPath = border.getPath();
            strip.dataset.collapsedStrip = "true";
            strip.dataset.state = "collapsed";
            strip.className = [
                this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER),
                this.options.getClassName(`${CLASSES.FLEXLAYOUT__BORDER_}${locationName}`),
                this.options.getClassName(CLASSES.FLEXLAYOUT__BORDER__COLLAPSED),
                "flexlayout__collapsed_strip",
            ].join(" ");

            strip.style.position = "absolute";
            strip.style.display = "flex";
            strip.style.alignItems = "center";
            strip.style.pointerEvents = "auto";
            strip.style.zIndex = "910";

            if (isVertical) {
                strip.style.flexDirection = "column";
                strip.style.justifyContent = "flex-start";
            } else {
                strip.style.justifyContent = "flex-start";
            }

            if (location === DockLocation.TOP) {
                strip.style.top = "0";
                strip.style.left = "0";
                strip.style.right = "0";
                strip.style.height = `${BORDER_BAR_SIZE}px`;
            } else if (location === DockLocation.BOTTOM) {
                strip.style.bottom = "0";
                strip.style.left = "0";
                strip.style.right = "0";
                strip.style.height = `${BORDER_BAR_SIZE}px`;
            } else if (location === DockLocation.LEFT) {
                strip.style.top = "0";
                strip.style.bottom = "0";
                strip.style.left = "0";
                strip.style.width = `${BORDER_BAR_SIZE}px`;
            } else {
                strip.style.top = "0";
                strip.style.bottom = "0";
                strip.style.right = "0";
                strip.style.width = `${BORDER_BAR_SIZE}px`;
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
            if (border.isEnableDock() && fabPosition === "start") {
                strip.appendChild(this.createCollapsedStripFab(border));
            }

            const tabs = border.getChildren();
            for (let i = 0; i < tabs.length; i++) {
                const tab = tabs[i];
                if (!(tab instanceof TabNode)) {
                    continue;
                }

                const button = document.createElement("button");
                button.type = "button";
                button.dataset.layoutPath = `${border.getPath()}/tb${i}`;
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

                strip.appendChild(button);
            }

            if (border.isEnableDock() && fabPosition === "end") {
                strip.appendChild(this.createCollapsedStripFab(border));
            }

            container.appendChild(strip);
            border.setTabHeaderRect(this.getBoundingClientRect(strip));
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

    private renderFlyout(): void {
        if (!this.rootDiv) {
            return;
        }

        if (!this.flyoutState) {
            this.flyoutBackdrop?.remove();
            this.flyoutPanel?.remove();
            this.flyoutBackdrop = undefined;
            this.flyoutPanel = undefined;
            return;
        }

        if (!this.flyoutBackdrop) {
            this.flyoutBackdrop = document.createElement("div");
            this.flyoutBackdrop.dataset.layoutPath = "/flyout/backdrop";
            this.flyoutBackdrop.className = "flexlayout__flyout_backdrop";
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
            this.flyoutPanel.style.position = "absolute";
            this.flyoutPanel.style.zIndex = "900";
            this.flyoutPanel.addEventListener("pointerdown", (event) => {
                event.stopPropagation();
            });
            this.rootDiv.appendChild(this.flyoutPanel);
        }

        const style: Record<string, string> = {};
        this.flyoutState.rect.styleWithPosition(style);
        Object.assign(this.flyoutPanel.style, style);
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
                    const flyoutState = this.flyoutState;
                    if (flyoutState && flyoutState.border.getId() === parent.getId() && flyoutState.tab.getId() === node.getId()) {
                        flyoutState.rect.styleWithPosition(style);
                        shouldShow = true;
                        container.style.zIndex = "901";
                    }
                } else {
                    const contentRect = parent.getContentRect();
                    if (contentRect.width > 0 && contentRect.height > 0 && node.isSelected()) {
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
