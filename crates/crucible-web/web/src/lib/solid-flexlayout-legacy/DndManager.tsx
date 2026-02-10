import { DockLocation } from "../flexlayout/core/DockLocation";
import { DropInfo } from "../flexlayout/core/DropInfo";
import { Rect } from "../flexlayout/core/Rect";
import { CLASSES } from "../flexlayout/core/Types";
import { Action } from "../flexlayout/model/Action";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { Model } from "../flexlayout/model/Model";
import { Node } from "../flexlayout/model/Node";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";

interface DndManagerOptions {
    model: Model;
    getClassName: (defaultClassName: string) => string;
    getSelfRef: () => HTMLDivElement | undefined;
    getMainRef: () => HTMLDivElement | undefined;
    getBoundingClientRect: (div: HTMLElement) => Rect;
    getShowHiddenBorder: () => DockLocation;
    setShowHiddenBorder: (location: DockLocation) => void;
    setShowOverlay: (show: boolean) => void;
    setShowEdges: (show: boolean) => void;
    doAction: (action: any) => void;
    redraw: () => void;
}

export interface DndManager {
    setDragNode: (event: DragEvent, node: Node) => void;
    clearDragMain: () => void;
    onDragEnterRaw: (event: DragEvent) => void;
    onDragLeaveRaw: (event: DragEvent) => void;
    onDragOver: (event: DragEvent) => void;
    onDrop: (event: DragEvent) => void;
}

export function createDndManager(options: DndManagerOptions): DndManager {
    let dropInfo: DropInfo | undefined;
    let outlineDiv: HTMLDivElement | undefined;
    let dragEnterCount = 0;
    let dragging = false;

    const syncBorderRects = () => {
        const selfRef = options.getSelfRef();
        if (!selfRef) return;
        void selfRef.offsetHeight;
        const borders = options.model.getBorderSet().getBorderMap();
        for (const [_, location] of DockLocation.values) {
            const border = borders.get(location);
            if (border) {
                const borderPath = border.getPath();
                const el = selfRef.querySelector(`[data-layout-path="${borderPath}"]`) as HTMLElement | null;
                if (el) {
                    border.setTabHeaderRect(options.getBoundingClientRect(el));
                }
            }
        }
    };

    const checkForBorderToShow = (x: number, y: number) => {
        const mainRef = options.getMainRef();
        const selfRef = options.getSelfRef();
        if (!mainRef || !selfRef) return;

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
        if (options.model.isEnableEdgeDock() && options.getShowHiddenBorder() === DockLocation.CENTER) {
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
            const borders = options.model.getBorderSet().getBorderMap();
            const border = borders.get(location);
            if (!border || !border.isAutoHide() || border.getChildren().length > 0) {
                location = DockLocation.CENTER;
            }
        }

        if (location !== options.getShowHiddenBorder()) {
            options.setShowHiddenBorder(location);
        }
    };

    const onEscapeKey = (event: KeyboardEvent) => {
        if (event.key === "Escape") {
            clearDragMain();
        }
    };

    const beginDragUi = () => {
        const selfRef = options.getSelfRef();
        if (!selfRef) return;
        dropInfo = undefined;
        outlineDiv = document.createElement("div");
        outlineDiv.className = options.getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT);
        outlineDiv.style.visibility = "hidden";
        const speed = (options.model.getAttribute("tabDragSpeed") as number) || 0.3;
        outlineDiv.style.transition = `top ${speed}s, left ${speed}s, width ${speed}s, height ${speed}s`;
        selfRef.appendChild(outlineDiv);
        dragging = true;
        document.addEventListener("keydown", onEscapeKey);
        options.setShowOverlay(true);
        if (options.model.getMaximizedTabset(Model.MAIN_WINDOW_ID) === undefined) {
            options.setShowEdges(options.model.isEnableEdgeDock());
        }
    };

    const setDragNode = (event: DragEvent, node: Node) => {
        const selfRef = options.getSelfRef();
        if (!selfRef) return;
        event.dataTransfer!.setData("text/plain", "--flexlayout--");
        event.dataTransfer!.effectAllowed = "copyMove";
        event.dataTransfer!.dropEffect = "move";
        dragEnterCount = 0;
        (selfRef as any).__dragNode = node;

        if (node instanceof TabNode) {
            const dragImg = document.createElement("div");
            dragImg.className = `${options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON)} ${options.getClassName(CLASSES.FLEXLAYOUT__TAB_BUTTON + "--selected")}`;
            dragImg.style.position = "absolute";
            dragImg.style.left = "-10000px";
            dragImg.style.top = "-10000px";
            dragImg.style.padding = "4px 12px";
            dragImg.style.fontSize = "12px";
            dragImg.style.whiteSpace = "nowrap";
            dragImg.style.pointerEvents = "none";
            dragImg.textContent = node.getName();
            document.body.appendChild(dragImg);
            event.dataTransfer!.setDragImage(dragImg, 0, 0);
            setTimeout(() => document.body.removeChild(dragImg), 0);
        }

        beginDragUi();
    };

    const clearDragMain = () => {
        const selfRef = options.getSelfRef();
        if (selfRef) {
            (selfRef as any).__dragNode = undefined;
        }
        options.setShowEdges(false);
        options.setShowOverlay(false);
        options.setShowHiddenBorder(DockLocation.CENTER);
        dragEnterCount = 0;
        dragging = false;
        document.removeEventListener("keydown", onEscapeKey);
        if (outlineDiv && selfRef) {
            selfRef.removeChild(outlineDiv);
            outlineDiv = undefined;
        }
    };

    const onDragEnter = (event: DragEvent) => {
        const selfRef = options.getSelfRef();
        const dragNode = selfRef ? (selfRef as any).__dragNode : undefined;
        if (dragNode && !dragging) {
            event.preventDefault();
            beginDragUi();
        }
    };

    const onDragEnterRaw = (event: DragEvent) => {
        dragEnterCount++;
        if (dragEnterCount === 1) {
            onDragEnter(event);
        }
    };

    const onDragLeaveRaw = (event: DragEvent) => {
        const selfRef = options.getSelfRef();
        dragEnterCount--;
        if (dragEnterCount === 0) {
            if (!selfRef?.contains(event.relatedTarget as globalThis.Node | null)) {
                clearDragMain();
            } else {
                dragEnterCount = 1;
            }
        }
    };

    const onDragOver = (event: DragEvent) => {
        if (!dragging) return;
        const selfRef = options.getSelfRef();
        if (!selfRef) return;
        event.preventDefault();
        const clientRect = selfRef.getBoundingClientRect();
        const pos = {
            x: event.clientX - clientRect.left,
            y: event.clientY - clientRect.top,
        };

        checkForBorderToShow(pos.x, pos.y);

        const dragNode = (selfRef as any).__dragNode;
        if (!dragNode) return;
        const root = options.model.getRoot();
        if (!root) return;

        let di: DropInfo | undefined;
        const hiddenBorderActive = options.getShowHiddenBorder() !== DockLocation.CENTER;
        if (hiddenBorderActive) {
            syncBorderRects();
            di = options.model.getBorderSet().findDropTargetNode(dragNode, pos.x, pos.y);
        }

        if (di === undefined) {
            di = root.findDropTargetNode(Model.MAIN_WINDOW_ID, dragNode, pos.x, pos.y);
        }
        if (di === undefined) {
            di = options.model.getBorderSet().findDropTargetNode(dragNode, pos.x, pos.y);
        }

        if (di) {
            dropInfo = di;
            if (outlineDiv) {
                let cls = options.getClassName(di.className);
                if (di.rect.width <= 5) {
                    cls += " " + options.getClassName(CLASSES.FLEXLAYOUT__OUTLINE_RECT + "_tab_reorder");
                }
                outlineDiv.className = cls;
                di.rect.positionElement(outlineDiv);
                outlineDiv.style.visibility = "visible";
            }
        }
    };

    const onDrop = (event: DragEvent) => {
        if (dragging) {
            event.preventDefault();
            const selfRef = options.getSelfRef();
            const dragNode = selfRef ? (selfRef as any).__dragNode : undefined;

            if (dropInfo && dragNode) {
                if (dragNode instanceof TabNode && dragNode.getParent() === undefined) {
                    const targetNode = dropInfo.node;
                    const savedSelected = targetNode instanceof BorderNode ? targetNode.getSelected() : -1;
                    options.doAction(
                        Action.addNode(
                            dragNode.toJson(),
                            targetNode.getId(),
                            dropInfo.location.getName(),
                            dropInfo.index,
                        ),
                    );
                    if (targetNode instanceof BorderNode && !targetNode.isAutoSelectTab(savedSelected !== -1)) {
                        targetNode.setSelected(savedSelected);
                        options.redraw();
                    }
                } else if (dragNode instanceof TabNode || dragNode instanceof TabSetNode) {
                    options.doAction(
                        Action.moveNode(
                            dragNode.getId(),
                            dropInfo.node.getId(),
                            dropInfo.location.getName(),
                            dropInfo.index,
                        ),
                    );
                }
            }
            clearDragMain();
        }
        dragEnterCount = 0;
    };

    return {
        setDragNode,
        clearDragMain,
        onDragEnterRaw,
        onDragLeaveRaw,
        onDragOver,
        onDrop,
    };
}
