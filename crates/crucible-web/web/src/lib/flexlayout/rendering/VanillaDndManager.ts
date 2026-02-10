import { DockLocation } from "../core/DockLocation";
import { DropInfo } from "../core/DropInfo";
import { Rect } from "../core/Rect";
import { CLASSES } from "../core/Types";
import { Action, type LayoutAction } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import { Model } from "../model/Model";
import { Node } from "../model/Node";
import { IDraggable } from "../model/IDraggable";
import { TabNode } from "../model/TabNode";
import { TabSetNode } from "../model/TabSetNode";

export interface IVanillaDndOptions {
    model: Model;
    getClassName(defaultClassName: string): string;
    getSelfRef(): HTMLDivElement | undefined;
    getMainRef(): HTMLDivElement | undefined;
    getBoundingClientRect(div: HTMLElement): Rect;
    getShowHiddenBorder(): DockLocation;
    setShowHiddenBorder(location: DockLocation): void;
    setShowOverlay(show: boolean): void;
    setShowEdges(show: boolean): void;
    doAction(action: LayoutAction): void;
    redraw(): void;
}

export class VanillaDndManager {
    private dropInfo: DropInfo | undefined;
    private outlineDiv: HTMLDivElement | undefined;
    private dragEnterCount = 0;
    private dragging = false;

    constructor(private readonly options: IVanillaDndOptions) {}

    setDragNode(event: DragEvent, node: Node): void {
        const selfRef = this.options.getSelfRef();
        if (!selfRef || !event.dataTransfer) {
            return;
        }

        event.dataTransfer.setData("text/plain", "--flexlayout--");
        event.dataTransfer.effectAllowed = "copyMove";
        event.dataTransfer.dropEffect = "move";
        this.dragEnterCount = 0;
        (selfRef as { __dragNode?: Node }).__dragNode = node;

        if (node instanceof TabNode) {
            const dragImg = document.createElement("div");
            dragImg.className = `${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON)} ${this.options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON + "--selected")}`;
            dragImg.style.position = "absolute";
            dragImg.style.left = "-10000px";
            dragImg.style.top = "-10000px";
            dragImg.style.padding = "4px 12px";
            dragImg.style.fontSize = "12px";
            dragImg.style.whiteSpace = "nowrap";
            dragImg.style.pointerEvents = "none";
            dragImg.textContent = node.getName();
            document.body.appendChild(dragImg);
            event.dataTransfer.setDragImage(dragImg, 0, 0);
            setTimeout(() => {
                if (dragImg.parentNode) {
                    dragImg.parentNode.removeChild(dragImg);
                }
            }, 0);
        }

        this.beginDragUi();
    }

    clearDragMain(): void {
        const selfRef = this.options.getSelfRef();
        if (selfRef) {
            (selfRef as { __dragNode?: Node }).__dragNode = undefined;
        }
        this.options.setShowEdges(false);
        this.options.setShowOverlay(false);
        this.options.setShowHiddenBorder(DockLocation.CENTER);
        this.dragEnterCount = 0;
        this.dragging = false;
        document.removeEventListener("keydown", this.onEscapeKey);
        if (this.outlineDiv && selfRef) {
            selfRef.removeChild(this.outlineDiv);
            this.outlineDiv = undefined;
        }
    }

    onDragEnterRaw(event: DragEvent): void {
        this.dragEnterCount++;
        if (this.dragEnterCount === 1) {
            this.onDragEnter(event);
        }
    }

    onDragLeaveRaw(event: DragEvent): void {
        const selfRef = this.options.getSelfRef();
        this.dragEnterCount--;
        if (this.dragEnterCount === 0) {
            if (!selfRef?.contains(event.relatedTarget as globalThis.Node | null)) {
                this.clearDragMain();
            } else {
                this.dragEnterCount = 1;
            }
        }
    }

    onDragOver(event: DragEvent): void {
        if (!this.dragging) {
            return;
        }
        const selfRef = this.options.getSelfRef();
        if (!selfRef) {
            return;
        }

        event.preventDefault();
        const clientRect = selfRef.getBoundingClientRect();
        const pos = {
            x: event.clientX - clientRect.left,
            y: event.clientY - clientRect.top,
        };

        this.checkForBorderToShow(pos.x, pos.y);

        const dragNode = (selfRef as { __dragNode?: Node }).__dragNode as (Node & IDraggable) | undefined;
        if (!dragNode) {
            return;
        }
        const root = this.options.model.getRoot();
        if (!root) {
            return;
        }

        let di: DropInfo | undefined;
        if (this.options.getShowHiddenBorder() !== DockLocation.CENTER) {
            this.syncBorderRects();
            di = this.options.model.getBorderSet().findDropTargetNode(dragNode, pos.x, pos.y);
        }

        if (!di) {
            di = root.findDropTargetNode(Model.MAIN_WINDOW_ID, dragNode, pos.x, pos.y);
        }
        if (!di) {
            di = this.options.model.getBorderSet().findDropTargetNode(dragNode, pos.x, pos.y);
        }

        if (di) {
            this.dropInfo = di;
            if (this.outlineDiv) {
                let cls = this.options.getClassName(di.className);
                if (di.rect.width <= 5) {
                    cls += " " + this.options.getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT + "_tab_reorder");
                }
                this.outlineDiv.className = cls;
                di.rect.positionElement(this.outlineDiv);
                this.outlineDiv.style.visibility = "visible";
            }
        }
    }

    onDrop(event: DragEvent): void {
        if (this.dragging) {
            event.preventDefault();
            const selfRef = this.options.getSelfRef();
            const dragNode = selfRef ? (selfRef as { __dragNode?: Node }).__dragNode : undefined;

            if (this.dropInfo && dragNode) {
                if (dragNode instanceof TabNode && dragNode.getParent() === undefined) {
                    const targetNode = this.dropInfo.node;
                    const savedSelected = targetNode instanceof BorderNode ? targetNode.getSelected() : -1;
                    this.options.doAction(
                        Action.addNode(
                            dragNode.toJson(),
                            targetNode.getId(),
                            this.dropInfo.location.getName(),
                            this.dropInfo.index,
                        ),
                    );
                    if (targetNode instanceof BorderNode && !targetNode.isAutoSelectTab(savedSelected !== -1)) {
                        targetNode.setSelected(savedSelected);
                        this.options.redraw();
                    }
                } else if (dragNode instanceof TabNode || dragNode instanceof TabSetNode) {
                    this.options.doAction(
                        Action.moveNode(
                            dragNode.getId(),
                            this.dropInfo.node.getId(),
                            this.dropInfo.location.getName(),
                            this.dropInfo.index,
                        ),
                    );
                }
            }
            this.clearDragMain();
        }
        this.dragEnterCount = 0;
    }

    private onDragEnter(event: DragEvent): void {
        const selfRef = this.options.getSelfRef();
        const dragNode = selfRef ? (selfRef as { __dragNode?: Node }).__dragNode : undefined;
        if (dragNode && !this.dragging) {
            event.preventDefault();
            this.beginDragUi();
        }
    }

    private beginDragUi(): void {
        const selfRef = this.options.getSelfRef();
        if (!selfRef) {
            return;
        }
        this.dropInfo = undefined;
        this.outlineDiv = document.createElement("div");
        this.outlineDiv.className = this.options.getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT);
        this.outlineDiv.style.visibility = "hidden";
        const speed = (this.options.model.getAttribute("tabDragSpeed") as number) || 0.3;
        this.outlineDiv.style.transition = `top ${speed}s, left ${speed}s, width ${speed}s, height ${speed}s`;
        selfRef.appendChild(this.outlineDiv);
        this.dragging = true;
        document.addEventListener("keydown", this.onEscapeKey);
        this.options.setShowOverlay(true);
        if (this.options.model.getMaximizedTabset(Model.MAIN_WINDOW_ID) === undefined) {
            this.options.setShowEdges(this.options.model.isEnableEdgeDock());
        }
    }

    private readonly onEscapeKey = (event: KeyboardEvent): void => {
        if (event.key === "Escape") {
            this.clearDragMain();
        }
    };

    private syncBorderRects(): void {
        const selfRef = this.options.getSelfRef();
        if (!selfRef) {
            return;
        }
        void selfRef.offsetHeight;
        const borders = this.options.model.getBorderSet().getBorderMap();
        for (const [, location] of DockLocation.values) {
            const border = borders.get(location);
            if (!border) {
                continue;
            }
            const borderPath = border.getPath();
            const el = selfRef.querySelector(`[data-layout-path="${borderPath}"]`) as HTMLElement | null;
            if (el) {
                border.setTabHeaderRect(this.options.getBoundingClientRect(el));
            }
        }
    }

    private checkForBorderToShow(x: number, y: number): void {
        const mainRef = this.options.getMainRef();
        const selfRef = this.options.getSelfRef();
        if (!mainRef || !selfRef) {
            return;
        }

        const mainDomRect = mainRef.getBoundingClientRect();
        const layoutDomRect = selfRef.getBoundingClientRect();
        const r = new Rect(
            mainDomRect.x - layoutDomRect.x,
            mainDomRect.y - layoutDomRect.y,
            mainDomRect.width,
            mainDomRect.height,
        );

        const edgeRectWidth = 10;
        const edgeRectLength = 100;
        const offset = edgeRectLength / 2;
        const cx = r.x + r.width / 2;
        const cy = r.y + r.height / 2;

        let overEdge = false;
        if (this.options.model.isEnableEdgeDock() && this.options.getShowHiddenBorder() === DockLocation.CENTER) {
            if ((y > cy - offset && y < cy + offset) || (x > cx - offset && x < cx + offset)) {
                overEdge = true;
            }
        }

        let location = DockLocation.CENTER;
        if (!overEdge) {
            if (x <= r.x + edgeRectWidth) {
                location = DockLocation.LEFT;
            } else if (x >= r.x + r.width - edgeRectWidth) {
                location = DockLocation.RIGHT;
            } else if (y <= r.y + edgeRectWidth) {
                location = DockLocation.TOP;
            } else if (y >= r.y + r.height - edgeRectWidth) {
                location = DockLocation.BOTTOM;
            }
        }

        if (location !== DockLocation.CENTER) {
            const borders = this.options.model.getBorderSet().getBorderMap();
            const border = borders.get(location);
            if (!border || !border.isAutoHide() || border.getChildren().length > 0) {
                location = DockLocation.CENTER;
            }
        }

        if (location !== this.options.getShowHiddenBorder()) {
            this.options.setShowHiddenBorder(location);
        }
    }
}
