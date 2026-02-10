import { Attribute } from "../core/Attribute";
import { AttributeDefinitions } from "../core/AttributeDefinitions";
import { DockLocation } from "../core/DockLocation";
import { DropInfo } from "../core/DropInfo";
import { Orientation } from "../core/Orientation";
import { Rect } from "../core/Rect";
import { CLASSES } from "../core/Types";
import { IDraggable } from "./IDraggable";
import { IDropTarget } from "./IDropTarget";
import { IJsonBorderNode } from "../types";
import { Model } from "./Model";
import { Node } from "./Node";
import { TabNode } from "./TabNode";
import { TabSetNode } from "./TabSetNode";
import { adjustSelectedIndex } from "./Utils";

export class BorderNode extends Node implements IDropTarget {
    static readonly TYPE = "border";

    /** @internal */
    static fromJson(json: any, model: Model) {
        const location = DockLocation.getByName(json.location);
        const border = new BorderNode(location, json, model);
        if (json.children) {
            border.children = json.children.map((jsonChild: any) => {
                const child = TabNode.fromJson(jsonChild, model);
                child.setParent(border);
                return child;
            });
        }

        return border;
    }
    /** @internal */
    private static attributeDefinitions: AttributeDefinitions = BorderNode.createAttributeDefinitions();

    /** @internal */
    private contentRect: Rect = Rect.empty();
    /** @internal */
    private tabHeaderRect: Rect = Rect.empty();
    /** @internal */
    private location: DockLocation;
    private _pinned: boolean = false;
    private _flyoutTabId: string | null = null;

    /** @internal */
    constructor(location: DockLocation, json: any, model: Model) {
        super(model);

        this.location = location;
        this.attributes.id = `border_${location.getName()}`;
        BorderNode.attributeDefinitions.fromJson(json, this.attributes);
        // Backward compat: convert legacy states to 2-state system
        if (this.attributes.dockState === "minimized" || this.attributes.dockState === "hidden") {
            this.attributes.dockState = "collapsed";
        }
        this._pinned = json.pinned === true;
        this._flyoutTabId = json.flyoutTabId ?? null;
        model.addNode(this);
    }

    getLocation() {
        return this.location;
    }

    getClassName() {
        return this.getAttr("className") as string | undefined;
    }

    isHorizontal() {
        return this.location.orientation === Orientation.HORZ;
    }

    getSize() {
        const defaultSize = this.getAttr("size") as number;
        const selected = this.getSelected();
        if (selected === -1) {
            return defaultSize;
        } else {
            const tabNode = this.children[selected] as TabNode;
            const tabBorderSize = this.isHorizontal() ? tabNode.getAttr("borderWidth") : tabNode.getAttr("borderHeight");
            if (tabBorderSize === -1) {
                return defaultSize;
            } else {
                return tabBorderSize;
            }
        }
    }

    getMinSize() {
        const selectedNode = this.getSelectedNode();
        let min = this.getAttr("minSize") as number;
        if (selectedNode) {
            const nodeMin = this.isHorizontal() ? selectedNode.getMinWidth() : selectedNode.getMinHeight();
            min = Math.max(min, nodeMin);
        }
        return min;
    }

    getMaxSize() {
        const selectedNode = this.getSelectedNode();
        let max = this.getAttr("maxSize") as number;
        if (selectedNode) {
            const nodeMax = this.isHorizontal() ? selectedNode.getMaxWidth() : selectedNode.getMaxHeight();
            max = Math.min(max, nodeMax);
        }
        return max;
    }

    getSelected(): number {
        return this.attributes.selected as number;
    }

    isAutoHide() {
        return this.getAttr("enableAutoHide") as boolean;
    }

    getSelectedNode(): TabNode | undefined {
        if (this.getSelected() !== -1) {
            return this.children[this.getSelected()] as TabNode;
        }
        return undefined;
    }

    getOrientation() {
        return this.location.getOrientation();
    }

    /**
     * Returns the config attribute that can be used to store node specific data that
     * WILL be saved to the json. The config attribute should be changed via the action Actions.updateNodeAttributes rather
     * than directly, for example:
     * this.state.model.doAction(
     *   FlexLayout.Actions.updateNodeAttributes(node.getId(), {config:myConfigObject}));
     */
    getConfig() {
        return this.attributes.config;
    }

    isMaximized() {
        return false;
    }

    isShowing() {
        return this.attributes.show as boolean;
    }

    getDockState(): string {
        return this.getAttr("dockState") as string;
    }

    /** @internal */
    setDockState(state: string) {
        this.attributes.dockState = state;
    }

    getVisibleTabs(): number[] {
        const val = this.attributes.visibleTabs;
        if (Array.isArray(val) && val.length > 0) {
            return val;
        }
        return [];
    }

    getPriority(): number {
        return this.getAttr("priority") as number;
    }

    isEnableDock(): boolean {
        return this.getAttr("enableDock") as boolean;
    }

    getCollapsedSize(): "fit" | "full" {
        return (this.getAttr("collapsedSize") as string || "full") as "fit" | "full";
    }

    getFabPosition(): "start" | "end" {
        return (this.getAttr("fabPosition") as string || "start") as "start" | "end";
    }

    isAnimateTransition(): boolean {
        return this.getAttr("animateTransition") as boolean ?? false;
    }

    isPinned(): boolean {
        return this._pinned;
    }

    setPinned(pinned: boolean): void {
        this._pinned = pinned;
    }

    getFlyoutTabId(): string | null {
        return this._flyoutTabId;
    }

    setFlyoutTabId(tabId: string | null): void {
        this._flyoutTabId = tabId;
    }

    toJson(): IJsonBorderNode {
        const json: any = {};
        BorderNode.attributeDefinitions.toJson(json, this.attributes);
        if (json.id && /^\d+$/.test(json.id)) {
            delete json.id;
        }
        json.location = this.location.getName();
        json.children = this.children.map((child) => (child as TabNode).toJson());
        if (this._pinned) {
            json.pinned = true;
        }
        if (this._flyoutTabId !== null) {
            json.flyoutTabId = this._flyoutTabId;
        }
        return json;
    }

    /** @internal */
    isAutoSelectTab(whenOpen?: boolean) {
        if (whenOpen == null) {
            whenOpen = this.getSelected() !== -1;
        }
        if (whenOpen) {
            return this.getAttr("autoSelectTabWhenOpen") as boolean;
        } else {
            return this.getAttr("autoSelectTabWhenClosed") as boolean;
        }
    }

    isEnableTabScrollbar() {
        return this.getAttr("enableTabScrollbar") as boolean;
    }

    /** @internal */
    setSelected(index: number) {
        this.attributes.selected = index;
    }

    /** @internal */
    getTabHeaderRect() {
        return this.tabHeaderRect;
    }

    /** @internal */
    setTabHeaderRect(r: Rect) {
        this.tabHeaderRect = r;
    }

    /** @internal */
    getRect() {
        return this.tabHeaderRect!;
    }

    /** @internal */
    getContentRect() {
        return this.contentRect;
    }

    /** @internal */
    setContentRect(r: Rect) {
        this.contentRect = r;
    }

    /** @internal */
    isEnableDrop() {
        return this.getAttr("enableDrop") as boolean;
    }

    /** @internal */
    setSize(pos: number) {
        const selected = this.getSelected();
        if (selected === -1) {
            this.attributes.size = pos;
        } else {
            const tabNode = this.children[selected] as TabNode;
            const tabBorderSize = this.isHorizontal() ? tabNode.getAttr("borderWidth") : tabNode.getAttr("borderHeight");
            if (tabBorderSize === -1) {
                this.attributes.size = pos;
            } else {
                if (this.isHorizontal()) {
                    tabNode.setBorderWidth(pos);
                } else {
                    tabNode.setBorderHeight(pos);
                }
            }
        }
    }

    /** @internal */
    updateAttrs(json: any) {
        BorderNode.attributeDefinitions.update(json, this.attributes);
    }

    /** @internal */
    remove(node: TabNode) {
        const removedIndex = this.removeChild(node);
        if (this.getSelected() !== -1) {
            adjustSelectedIndex(this, removedIndex);
        }
        this.adjustVisibleTabs(removedIndex);
    }

    /** @internal */
    adjustVisibleTabs(removedIndex: number) {
        const visibleTabs = this.getVisibleTabs();
        if (visibleTabs.length === 0) {
            return;
        }

        const adjusted: number[] = [];
        for (const idx of visibleTabs) {
            if (idx === removedIndex) {
                continue;
            } else if (idx > removedIndex) {
                adjusted.push(idx - 1);
            } else {
                adjusted.push(idx);
            }
        }

        if (adjusted.length === 0) {
            this.attributes.visibleTabs = [];
        } else {
            this.attributes.visibleTabs = adjusted;
        }
    }

    private shiftVisibleTabs(insertedIndex: number) {
        const visibleTabs = this.getVisibleTabs();
        if (visibleTabs.length === 0) {
            return;
        }

        const shifted: number[] = [];
        for (const idx of visibleTabs) {
            if (idx >= insertedIndex) {
                shifted.push(idx + 1);
            } else {
                shifted.push(idx);
            }
        }

        this.attributes.visibleTabs = shifted;
    }

    /** @internal */
    canDrop(dragNode: Node & IDraggable, x: number, y: number): DropInfo | undefined {
        if (!(dragNode instanceof TabNode)) {
            return undefined;
        }

        let dropInfo;
        const dockLocation = DockLocation.CENTER;

        if (this.tabHeaderRect!.contains(x, y)) {
            if (this.location.orientation === Orientation.VERT) {
                if (this.children.length > 0) {
                    let child = this.children[0];
                    let childRect = (child as TabNode).getTabRect()!;
                    const childY = childRect.y;

                    const childHeight = childRect.height;

                    let pos = this.tabHeaderRect!.x;
                    let childCenter = 0;
                    for (let i = 0; i < this.children.length; i++) {
                        child = this.children[i];
                        childRect = (child as TabNode).getTabRect()!;
                        childCenter = childRect.x + childRect.width / 2;
                        if (x >= pos && x < childCenter) {
                            const outlineRect = new Rect(childRect.x - 2, childY, 3, childHeight);
                            dropInfo = new DropInfo(this, outlineRect, dockLocation, i, CLASSES.FLEXLAYOUT__OUTLINE_RECT);
                            break;
                        }
                        pos = childCenter;
                    }
                    if (dropInfo == null) {
                        const outlineRect = new Rect(childRect.getRight() - 2, childY, 3, childHeight);
                        dropInfo = new DropInfo(this, outlineRect, dockLocation, this.children.length, CLASSES.FLEXLAYOUT__OUTLINE_RECT);
                    }
                } else {
                    const outlineRect = new Rect(this.tabHeaderRect!.x + 1, this.tabHeaderRect!.y + 2, 3, 18);
                    dropInfo = new DropInfo(this, outlineRect, dockLocation, 0, CLASSES.FLEXLAYOUT__OUTLINE_RECT);
                }
            } else {
                if (this.children.length > 0) {
                    let child = this.children[0];
                    let childRect = (child as TabNode).getTabRect()!;
                    const childX = childRect.x;
                    const childWidth = childRect.width;

                    let pos = this.tabHeaderRect!.y;
                    let childCenter = 0;
                    for (let i = 0; i < this.children.length; i++) {
                        child = this.children[i];
                        childRect = (child as TabNode).getTabRect()!;
                        childCenter = childRect.y + childRect.height / 2;
                        if (y >= pos && y < childCenter) {
                            const outlineRect = new Rect(childX, childRect.y - 2, childWidth, 3);
                            dropInfo = new DropInfo(this, outlineRect, dockLocation, i, CLASSES.FLEXLAYOUT__OUTLINE_RECT);
                            break;
                        }
                        pos = childCenter;
                    }
                    if (dropInfo == null) {
                        const outlineRect = new Rect(childX, childRect.getBottom() - 2, childWidth, 3);
                        dropInfo = new DropInfo(this, outlineRect, dockLocation, this.children.length, CLASSES.FLEXLAYOUT__OUTLINE_RECT);
                    }
                } else {
                    const outlineRect = new Rect(this.tabHeaderRect!.x + 2, this.tabHeaderRect!.y + 1, 18, 3);
                    dropInfo = new DropInfo(this, outlineRect, dockLocation, 0, CLASSES.FLEXLAYOUT__OUTLINE_RECT);
                }
            }
            if (!dragNode.canDockInto(dragNode, dropInfo)) {
                return undefined;
            }
        } else if (this.getSelected() !== -1 && this.contentRect!.width > 0 && this.contentRect!.height > 0 && this.contentRect!.contains(x, y)) {
            const splitResult = this.contentAreaSplitDrop(dragNode, x, y);
            if (splitResult) {
                return splitResult;
            }
            const outlineRect = this.contentRect;
            dropInfo = new DropInfo(this, outlineRect!, dockLocation, -1, CLASSES.FLEXLAYOUT__OUTLINE_RECT);
            if (!dragNode.canDockInto(dragNode, dropInfo)) {
                return undefined;
            }
        }

        return dropInfo;
    }

    /**
     * Split indicator for content-area drops. Direction is perpendicular to border orientation:
     * VERT borders → LEFT/RIGHT halves, HORZ borders → TOP/BOTTOM halves.
     * Non-CENTER DockLocation signals tiling intent to the drop handler.
     * @internal
     */
    private contentAreaSplitDrop(dragNode: Node & IDraggable, x: number, y: number): DropInfo | undefined {
        const cr = this.contentRect;
        if (!cr || cr.width === 0 || cr.height === 0) return undefined;
        if (this.getSelected() === -1) return undefined;

        const splitHorizontally = this.location.orientation === Orientation.VERT;
        let splitLocation: DockLocation;
        let outlineRect: Rect;

        if (splitHorizontally) {
            const midX = cr.x + cr.width / 2;
            if (x < midX) {
                splitLocation = DockLocation.LEFT;
                outlineRect = new Rect(cr.x, cr.y, cr.width / 2, cr.height);
            } else {
                splitLocation = DockLocation.RIGHT;
                outlineRect = new Rect(cr.x + cr.width / 2, cr.y, cr.width / 2, cr.height);
            }
        } else {
            const midY = cr.y + cr.height / 2;
            if (y < midY) {
                splitLocation = DockLocation.TOP;
                outlineRect = new Rect(cr.x, cr.y, cr.width, cr.height / 2);
            } else {
                splitLocation = DockLocation.BOTTOM;
                outlineRect = new Rect(cr.x, cr.y + cr.height / 2, cr.width, cr.height / 2);
            }
        }

        const dropInfo = new DropInfo(this, outlineRect, splitLocation, -1, CLASSES.FLEXLAYOUT__OUTLINE_RECT);
        if (!dragNode.canDockInto(dragNode, dropInfo)) {
            return undefined;
        }
        return dropInfo;
    }

    /** @internal */
    drop(dragNode: Node & IDraggable, location: DockLocation, index: number, select?: boolean): void {
        const isTilingDrop = location !== DockLocation.CENTER && index === -1;

        const currentVisibleIndices = isTilingDrop ? this.resolveCurrentVisibleIndices() : [];

        let fromIndex = 0;
        const dragParent = dragNode.getParent() as BorderNode | TabSetNode;
        if (dragParent !== undefined) {
            fromIndex = dragParent.removeChild(dragNode);
            adjustSelectedIndex(dragParent, fromIndex);
            if (dragParent instanceof BorderNode) {
                dragParent.adjustVisibleTabs(fromIndex);
                if (dragParent.getChildren().length === 0) {
                    dragParent.setDockState("collapsed");
                }
            }
        }

        if (dragNode instanceof TabNode && dragParent === this && fromIndex < index && index > 0) {
            index--;
        }

        let insertPos = index;
        if (insertPos === -1) {
            insertPos = this.children.length;
        }

        if (dragNode instanceof TabNode) {
            this.addChild(dragNode, insertPos);

            if (isTilingDrop) {
                const adjustedExisting = currentVisibleIndices.map(
                    (i) => (dragParent === this && fromIndex <= i ? i - 1 : i)
                ).map(
                    (i) => (i >= insertPos ? i + 1 : i)
                );

                const draggedFirst = location === DockLocation.LEFT || location === DockLocation.TOP;
                this.attributes.visibleTabs = draggedFirst
                    ? [insertPos, ...adjustedExisting]
                    : [...adjustedExisting, insertPos];
                this.setSelected(adjustedExisting[0] ?? insertPos);
            } else {
                this.shiftVisibleTabs(insertPos);
            }
        }

        if (!isTilingDrop && (select || (select !== false && this.isAutoSelectTab()))) {
            this.setSelected(insertPos);
        }

        if (this.getDockState() !== "expanded") {
            this.setDockState("expanded");
        }

        this.model.tidy();
    }

    /** @internal */
    private resolveCurrentVisibleIndices(): number[] {
        const explicit = this.getVisibleTabs();
        if (explicit.length > 0) return [...explicit];
        const sel = this.getSelected();
        return sel >= 0 ? [sel] : [];
    }

    /** @internal */
    getSplitterBounds(_index: number, useMinSize: boolean = false) {
        const pBounds = [0, 0];
        const minSize = useMinSize ? this.getMinSize() : 0;
        const maxSize = useMinSize ? this.getMaxSize() : 99999;
        const rootRow = this.model.getRoot(Model.MAIN_WINDOW_ID);
        const innerRect = rootRow.getRect();
        const splitterSize = this.model.getSplitterSize()
        if (this.location === DockLocation.TOP) {
            pBounds[0] = this.tabHeaderRect!.getBottom() + minSize;
            const maxPos = this.tabHeaderRect!.getBottom() + maxSize;
            pBounds[1] = Math.max(pBounds[0], innerRect.getBottom() - rootRow.getMinHeight() - splitterSize);
            pBounds[1] = Math.min(pBounds[1], maxPos);
        } else if (this.location === DockLocation.LEFT) {
            pBounds[0] = this.tabHeaderRect!.getRight() + minSize;
            const maxPos = this.tabHeaderRect!.getRight() + maxSize;
            pBounds[1] = Math.max(pBounds[0], innerRect.getRight() - rootRow.getMinWidth() - splitterSize);
            pBounds[1] = Math.min(pBounds[1], maxPos);
        } else if (this.location === DockLocation.BOTTOM) {
            pBounds[1] = this.tabHeaderRect!.y - minSize - splitterSize;
            const maxPos = this.tabHeaderRect!.y - maxSize - splitterSize;
            pBounds[0] = Math.min(pBounds[1], innerRect.y + rootRow.getMinHeight());
            pBounds[0] = Math.max(pBounds[0], maxPos);
        } else if (this.location === DockLocation.RIGHT) {
            pBounds[1] = this.tabHeaderRect!.x - minSize - splitterSize;
            const maxPos = this.tabHeaderRect!.x - maxSize - splitterSize;
            pBounds[0] = Math.min(pBounds[1], innerRect.x + rootRow.getMinWidth());
            pBounds[0] = Math.max(pBounds[0], maxPos);
        }
        return pBounds;
    }

    /** @internal */
    calculateSplit(_splitter: BorderNode, splitterPos: number) {
        const pBounds = this.getSplitterBounds(splitterPos);
        if (this.location === DockLocation.BOTTOM || this.location === DockLocation.RIGHT) {
            return Math.max(0, pBounds[1] - splitterPos);
        } else {
            return Math.max(0, splitterPos - pBounds[0]);
        }
    }

    /** @internal */
    getAttributeDefinitions() {
        return BorderNode.attributeDefinitions;
    }

    /** @internal */
    static getAttributeDefinitions() {
        return BorderNode.attributeDefinitions;
    }

    /** @internal */
    private static createAttributeDefinitions(): AttributeDefinitions {
        const attributeDefinitions = new AttributeDefinitions();
        attributeDefinitions.add("type", BorderNode.TYPE, true).setType(Attribute.STRING).setFixed();

        attributeDefinitions.add("selected", -1).setType(Attribute.NUMBER).setDescription(
            `index of selected/visible tab in border; -1 means no tab selected`
        );
        attributeDefinitions.add("show", true).setType(Attribute.BOOLEAN).setDescription(
            `show/hide this border`
        );
        attributeDefinitions.add("config", undefined).setType("any").setDescription(
            `a place to hold json config used in your own code`
        );

        attributeDefinitions.addInherited("enableDrop", "borderEnableDrop").setType(Attribute.BOOLEAN).setDescription(
            `whether tabs can be dropped into this border`
        );
        attributeDefinitions.addInherited("className", "borderClassName").setType(Attribute.STRING).setDescription(
            `class applied to tab button`
        );
        attributeDefinitions.addInherited("autoSelectTabWhenOpen", "borderAutoSelectTabWhenOpen").setType(Attribute.BOOLEAN).setDescription(
            `whether to select new/moved tabs in border when the border is already open`
        );
        attributeDefinitions.addInherited("autoSelectTabWhenClosed", "borderAutoSelectTabWhenClosed").setType(Attribute.BOOLEAN).setDescription(
            `whether to select new/moved tabs in border when the border is currently closed`
        );
        attributeDefinitions.addInherited("size", "borderSize").setType(Attribute.NUMBER).setDescription(
            `size of the tab area when selected`
        );
        attributeDefinitions.addInherited("minSize", "borderMinSize").setType(Attribute.NUMBER).setDescription(
            `the minimum size of the tab area`
        );
        attributeDefinitions.addInherited("maxSize", "borderMaxSize").setType(Attribute.NUMBER).setDescription(
            `the maximum size of the tab area`
        );
        attributeDefinitions.addInherited("enableAutoHide", "borderEnableAutoHide").setType(Attribute.BOOLEAN).setDescription(
            `hide border if it has zero tabs`
        );
        attributeDefinitions.addInherited("enableTabScrollbar", "borderEnableTabScrollbar").setType(Attribute.BOOLEAN).setDescription(
            `whether to show a mini scrollbar for the tabs`
        );
        attributeDefinitions.addInherited("dockState", "borderDockState").setType(Attribute.STRING).setDescription(
            `dock state of the border: "expanded" | "collapsed"`
        );
        attributeDefinitions.addInherited("collapsedSize", "borderCollapsedSize").setType(Attribute.STRING).setDescription(
            `collapsed strip sizing: "fit" (shrink-to-fit) or "full" (full edge)`
        );
        attributeDefinitions.addInherited("fabPosition", "borderFabPosition").setType(Attribute.STRING).setDescription(
            `FAB button position in collapsed strip: "start" or "end"`
        );
        attributeDefinitions.add("visibleTabs", []).setType("any").setDescription(
            `array of tab indices visible/tiled simultaneously; empty means fallback to [selected]`
        );
        attributeDefinitions.addInherited("priority", "borderPriority").setType(Attribute.NUMBER).setDescription(
            `priority for border spanning; higher priority borders span full edge`
        );
        attributeDefinitions.addInherited("enableDock", "borderEnableDock").setType(Attribute.BOOLEAN).setDescription(
            `whether the collapse/expand/minimize dock button appears`
        );
        attributeDefinitions.addInherited("animateTransition", "borderAnimateTransition").setType(Attribute.BOOLEAN).setDescription(
            `whether collapse/expand transitions are animated`
        );
        return attributeDefinitions;
    }
}
