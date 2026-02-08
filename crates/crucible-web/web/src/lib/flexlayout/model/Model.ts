import type { IJsonModel } from "../types";
import { Node } from "./Node";
import { TabNode } from "./TabNode";
import { TabSetNode } from "./TabSetNode";
import { RowNode } from "./RowNode";
import { BorderNode } from "./BorderNode";
import { BorderSet } from "./BorderSet";
import { LayoutWindow } from "./LayoutWindow";
import type { IAction } from "./Action";
import { Rect } from "../core/Rect";
import { DockLocation } from "../core/DockLocation";

export const DefaultMax = 100000;
export const DefaultMin = 0;

export class Model {
	static readonly MAIN_WINDOW_ID = "main";

	private windowsMap = new Map<string, LayoutWindow>();
	private attributes: Record<string, any> = {};
	private borderSet: BorderSet;
	private nodeRegistry = new Map<string, Node>();
	private nextIdNum = 0;

	constructor(json?: IJsonModel) {
		this.borderSet = new BorderSet(this);
		if (json) {
			this.loadFromJson(json);
		} else {
			const mainWindow = new LayoutWindow(Model.MAIN_WINDOW_ID, Rect.empty());
			this.windowsMap.set(Model.MAIN_WINDOW_ID, mainWindow);
		}
	}

	static fromJson(json: IJsonModel): Model {
		const model = new Model();
		model.loadFromJson(json);
		return model;
	}

	private loadFromJson(json: IJsonModel): void {
		this.attributes = json.global || {};
		this.nodeRegistry.clear();
		this.windowsMap.clear();

		const mainWindow = new LayoutWindow(Model.MAIN_WINDOW_ID, Rect.empty());
		this.windowsMap.set(Model.MAIN_WINDOW_ID, mainWindow);

		if (json.layout) {
			const root = this.createRootFromJson(json.layout);
			if (root) {
				mainWindow.root = root;
				this.registerNode(root);
			}
		}

		if (json.borders) {
			for (const borderJson of json.borders) {
				const locationName = borderJson.location || "top";
				const location = DockLocation.getByName(locationName);
				const border = new BorderNode(location, borderJson, this);
				if (borderJson.children) {
					for (const tabJson of borderJson.children) {
						const tab = new TabNode(this, tabJson);
						(border as any).addChild(tab);
						this.registerNode(tab);
					}
				}
				(this.borderSet as any).borderMap.set(location, border);
				(this.borderSet as any).borders.push(border);
			}
		}

		if (json.windows) {
			for (const [windowId, windowJson] of Object.entries(json.windows)) {
				const layoutWindow = LayoutWindow.fromJson(windowJson, this, windowId);
				this.windowsMap.set(windowId, layoutWindow);
				if (layoutWindow.root) {
					this.registerNode(layoutWindow.root);
				}
			}
		}
	}

	private createRootFromJson(json: any): RowNode | undefined {
		if (!json) return undefined;

		if (json.type === "row") {
			const row = new RowNode(this, Model.MAIN_WINDOW_ID, json);
			if (json.children) {
				for (const childJson of json.children) {
					const child = this.createNodeFromJson(childJson);
					if (child) {
						(row as any).addChild(child);
					}
				}
			}
			return row;
		} else if (json.type === "tabset") {
			const row = new RowNode(this, Model.MAIN_WINDOW_ID, { type: "row", weight: 100 });
			const tabset = new TabSetNode(this, json);
			if (json.children) {
				for (const tabJson of json.children) {
					const tab = new TabNode(this, tabJson);
					(tabset as any).addChild(tab);
					this.registerNode(tab);
				}
			}
			(row as any).addChild(tabset);
			return row;
		}

		return undefined;
	}

	private createNodeFromJson(json: any): RowNode | TabSetNode | undefined {
		if (!json) return undefined;

		if (json.type === "row") {
			const row = new RowNode(this, Model.MAIN_WINDOW_ID, json);
			if (json.children) {
				for (const childJson of json.children) {
					const child = this.createNodeFromJson(childJson);
					if (child) {
						(row as any).addChild(child);
					}
				}
			}
			return row;
		} else if (json.type === "tabset") {
			const tabset = new TabSetNode(this, json);
			if (json.children) {
				for (const tabJson of json.children) {
					const tab = new TabNode(this, tabJson);
					(tabset as any).addChild(tab);
					this.registerNode(tab);
				}
			}
			return tabset;
		}

		return undefined;
	}

	private registerNode(node: Node): void {
		const id = node.getId();
		this.nodeRegistry.set(id, node);

		for (const child of node.getChildren()) {
			this.registerNode(child);
		}
	}

	nextUniqueId(): string {
		return `${this.nextIdNum++}`;
	}

	addNode(node: Node): void {
		const id = node.getId();
		this.nodeRegistry.set(id, node);
	}

	getNodeById(id: string): Node | undefined {
		return this.nodeRegistry.get(id);
	}

	doAction(action: IAction): void {
		const type = action.type;
		const data = action.data || {};

		switch (type) {
			case "ADD_NODE":
				this.actionAddNode(data);
				break;
			case "MOVE_NODE":
				this.actionMoveNode(data);
				break;
			case "DELETE_TAB":
				this.actionDeleteTab(data);
				break;
			case "DELETE_TABSET":
				this.actionDeleteTabset(data);
				break;
			case "RENAME_TAB":
				this.actionRenameTab(data);
				break;
			case "SELECT_TAB":
				this.actionSelectTab(data);
				break;
			case "SET_ACTIVE_TABSET":
				this.actionSetActiveTabset(data);
				break;
			case "ADJUST_WEIGHTS":
				this.actionAdjustWeights(data);
				break;
			case "ADJUST_BORDER_SPLIT":
				this.actionAdjustBorderSplit(data);
				break;
			case "MAXIMIZE_TOGGLE":
				this.actionMaximizeToggle(data);
				break;
			case "UPDATE_MODEL_ATTRIBUTES":
				this.actionUpdateModelAttributes(data);
				break;
			case "UPDATE_NODE_ATTRIBUTES":
				this.actionUpdateNodeAttributes(data);
				break;
			case "POPOUT_TAB":
				this.actionPopoutTab(data);
				break;
			case "POPOUT_TABSET":
				this.actionPopoutTabset(data);
				break;
			case "CLOSE_WINDOW":
				this.actionCloseWindow(data);
				break;
			case "CREATE_WINDOW":
				this.actionCreateWindow(data);
				break;
			case "FLOAT_TAB":
				this.actionFloatTab(data);
				break;
			case "FLOAT_TABSET":
				this.actionFloatTabset(data);
				break;
			case "DOCK_TAB":
				this.actionDockTab(data);
				break;
			case "DOCK_TABSET":
				this.actionDockTabset(data);
				break;
		case "MOVE_WINDOW":
			this.actionMoveWindow(data);
			break;
		case "SET_TAB_ICON":
			this.actionSetTabIcon(data);
			break;
			case "SET_TAB_COMPONENT":
				this.actionSetTabComponent(data);
				break;
			case "SET_TAB_CONFIG":
				this.actionSetTabConfig(data);
				break;
			case "SET_TAB_ENABLE_CLOSE":
				this.actionSetTabEnableClose(data);
				break;
		}
	}

	private actionAddNode(data: any): void {
		const { json, toNodeId, location, index } = data;
		const toNode = this.getNodeById(toNodeId);
		if (!toNode) return;

		const newTab = new TabNode(this, json);
		this.registerNode(newTab);

		const dockLocation = DockLocation.getByName(location);
		if (toNode.getType() === "tabset" || toNode.getType() === "border" || toNode.getType() === "row") {
			(toNode as any).drop(newTab, dockLocation, index, true);
		}
	}

	private actionMoveNode(data: any): void {
		const { fromNode: fromNodeId, toNode: toNodeId, location, index, select } = data;
		const fromNode = this.getNodeById(fromNodeId);
		const toNode = this.getNodeById(toNodeId);

		if (!fromNode || !toNode) return;

		if (fromNode instanceof TabNode || fromNode instanceof TabSetNode || fromNode instanceof RowNode) {
			if (fromNode === this.getMaximizedTabset(fromNode.getWindowId())) {
				const fromWindow = this.windowsMap.get(fromNode.getWindowId());
				if (fromWindow) {
					fromWindow.maximizedTabSet = undefined;
				}
			}
			if (toNode instanceof TabSetNode || toNode instanceof BorderNode || toNode instanceof RowNode) {
				(toNode as any).drop(fromNode, DockLocation.getByName(location), index, select);
			}
		}
		this.removeEmptyWindows();
	}

	private actionDeleteTab(data: any): void {
		const { node } = data;
		const tab = this.getNodeById(node);
		if (tab instanceof TabNode) {
			tab.delete();
		}
		this.removeEmptyWindows();
	}

	private actionDeleteTabset(data: any): void {
		const { node } = data;
		const tabset = this.getNodeById(node);

		if (tabset instanceof TabSetNode) {
			// first delete all child tabs that are closeable
			const children = [...tabset.getChildren()];
			for (let i = 0; i < children.length; i++) {
				const child = children[i];
				if ((child as TabNode).isEnableClose()) {
					(child as TabNode).delete();
				}
			}

			if (tabset.getChildren().length === 0) {
				tabset.delete();
			}
			this.tidy();
		}
		this.removeEmptyWindows();
	}

	private actionRenameTab(data: any): void {
		const { node, text } = data;
		const tab = this.getNodeById(node) as TabNode;
		if (tab) {
			tab.setName(text);
		}
	}

	private actionSelectTab(data: any): void {
		const { tabNode, windowId } = data;
		const tab = this.getNodeById(tabNode) as TabNode;
		const wId = windowId || Model.MAIN_WINDOW_ID;
		const window = this.windowsMap.get(wId);

		if (tab && window) {
			const parent = tab.getParent() as Node;
			const pos = parent.getChildren().indexOf(tab);

			if (parent instanceof BorderNode) {
				if (parent.getSelected() === pos) {
					parent.setSelected(-1);
				} else {
					parent.setSelected(pos);
				}
			} else if (parent instanceof TabSetNode) {
				if (parent.getSelected() !== pos) {
					parent.setSelected(pos);
				}
				window.activeTabSet = parent;
			}
		}
	}

	private actionSetActiveTabset(data: any): void {
		const { tabsetNode, windowId } = data;
		const tabset = this.getNodeById(tabsetNode) as TabSetNode;
		if (tabset && tabset.getType() === "tabset") {
			this.setActiveTabset(tabset, windowId || Model.MAIN_WINDOW_ID);
		}
	}

	private actionAdjustWeights(data: any): void {
		const { nodeId, weights } = data;
		const node = this.getNodeById(nodeId) as RowNode;
		if (node && node.getType() === "row") {
			const children = node.getChildren();
			for (let i = 0; i < children.length && i < weights.length; i++) {
				(children[i] as any).setWeight(weights[i]);
			}
		}
	}

	private actionAdjustBorderSplit(data: any): void {
		const { borderId, size } = data;
		const border = this.getNodeById(borderId) as BorderNode;
		if (border) {
			(border as any).setSize(size);
		}
	}

	private actionMaximizeToggle(data: any): void {
		const { tabsetId } = data;
		const tabset = this.getNodeById(tabsetId) as TabSetNode;
		if (tabset && tabset.getType() === "tabset") {
			const windowId = tabset.getWindowId();
			const window = this.windowsMap.get(windowId);
			if (window) {
				if (window.maximizedTabSet === tabset) {
					window.maximizedTabSet = undefined;
				} else {
					window.maximizedTabSet = tabset;
				}
			}
		}
	}

	private actionUpdateModelAttributes(data: any): void {
		const { attributes } = data;
		Object.assign(this.attributes, attributes);
	}

	private actionUpdateNodeAttributes(data: any): void {
		const { nodeId, attributes } = data;
		const node = this.getNodeById(nodeId);
		if (node) {
			Object.assign((node as any).attributes, attributes);
			node.fireEvent("save", {});
		}
	}

	private actionPopoutTab(data: any): void {
		const { tabId } = data;
		const node = this.getNodeById(tabId);
		if (node instanceof TabNode) {
			let r = Rect.empty();
			if (node.getParent() instanceof TabSetNode) {
				r = node.getParent()!.getRect();
			} else if (node.getParent() instanceof BorderNode) {
				r = (node.getParent() as BorderNode).getContentRect();
			}
			const oldLayoutWindow = this.windowsMap.get(node.getWindowId())!;
			const windowId = this.nextUniqueId();
			const layoutWindow = new LayoutWindow(windowId, oldLayoutWindow.toScreenRectFunction(r), "popout");
			const tabsetId = this.nextUniqueId();
			const row = RowNode.fromJson({
				type: "row",
				children: [{ type: "tabset", id: tabsetId }]
			}, this, layoutWindow);
			layoutWindow.root = row;
			this.windowsMap.set(windowId, layoutWindow);

			const tabset = this.getNodeById(tabsetId) as TabSetNode;
			(tabset as any).drop(node, DockLocation.CENTER, 0, true);
		}
		this.removeEmptyWindows();
	}

	private actionPopoutTabset(data: any): void {
		const { tabsetId } = data;
		const node = this.getNodeById(tabsetId);
		if (node instanceof TabSetNode) {
			const isMaximized = node === this.getMaximizedTabset(node.getWindowId());
			const oldLayoutWindow = this.windowsMap.get(node.getWindowId())!;
			const windowId = this.nextUniqueId();
			const layoutWindow = new LayoutWindow(windowId, oldLayoutWindow.toScreenRectFunction(node.getRect()), "popout");
			const row = RowNode.fromJson({ type: "row", children: [] }, this, layoutWindow);
			layoutWindow.root = row;
			this.windowsMap.set(windowId, layoutWindow);
			row.drop(node, DockLocation.CENTER, 0);

			if (isMaximized) {
				const fromWindow = this.windowsMap.get(Model.MAIN_WINDOW_ID)!;
				fromWindow.maximizedTabSet = undefined;
			}
		}
		this.removeEmptyWindows();
	}

	private actionCloseWindow(data: any): void {
		const { windowId } = data;
		this.windowsMap.delete(windowId);
	}

	private actionCreateWindow(_data: any): void {
		const windowId = this.nextUniqueId();
		this.windowsMap.set(windowId, new LayoutWindow(windowId, Rect.empty(), "popout"));
	}

	private actionFloatTab(data: any): void {
		const { tabId, x, y, width, height } = data;
		const node = this.getNodeById(tabId);
		if (node instanceof TabNode) {
			const windowId = this.nextUniqueId();
			const rect = new Rect(x, y, width, height);
			const layoutWindow = new LayoutWindow(windowId, rect, "float");
			const tabsetId = this.nextUniqueId();
			const row = RowNode.fromJson({
				type: "row",
				children: [{ type: "tabset", id: tabsetId }]
			}, this, layoutWindow);
			layoutWindow.root = row;
			this.windowsMap.set(windowId, layoutWindow);

			const tabset = this.getNodeById(tabsetId) as TabSetNode;
			(tabset as any).drop(node, DockLocation.CENTER, 0, true);
		}
		this.removeEmptyWindows();
	}

	private actionFloatTabset(data: any): void {
		const { tabsetId, x, y, width, height } = data;
		const node = this.getNodeById(tabsetId);
		if (node instanceof TabSetNode) {
			const isMaximized = node === this.getMaximizedTabset(node.getWindowId());
			const windowId = this.nextUniqueId();
			const rect = new Rect(x, y, width, height);
			const layoutWindow = new LayoutWindow(windowId, rect, "float");
			const row = RowNode.fromJson({ type: "row", children: [] }, this, layoutWindow);
			layoutWindow.root = row;
			this.windowsMap.set(windowId, layoutWindow);
			row.drop(node, DockLocation.CENTER, 0);

			if (isMaximized) {
				const fromWindow = this.windowsMap.get(Model.MAIN_WINDOW_ID)!;
				fromWindow.maximizedTabSet = undefined;
			}
		}
		this.removeEmptyWindows();
	}

	private actionDockTab(data: any): void {
		const { tabId, location } = data;
		const node = this.getNodeById(tabId);
		if (node instanceof TabNode) {
			const mainWindow = this.windowsMap.get(Model.MAIN_WINDOW_ID)!;
			const dockLocation = DockLocation.getByName(location);
			const root = mainWindow.root!;
			(root as any).drop(node, dockLocation, -1);
		}
		this.removeEmptyWindows();
	}

	private actionDockTabset(data: any): void {
		const { tabsetId, location } = data;
		const node = this.getNodeById(tabsetId);
		if (node instanceof TabSetNode) {
			const mainWindow = this.windowsMap.get(Model.MAIN_WINDOW_ID)!;
			const dockLocation = DockLocation.getByName(location);
			const root = mainWindow.root!;
			(root as any).drop(node, dockLocation, -1);
		}
		this.removeEmptyWindows();
	}

	private actionMoveWindow(data: any): void {
		const { windowId, x, y, width, height } = data;
		const layoutWindow = this.windowsMap.get(windowId);
		if (layoutWindow) {
			layoutWindow.rect = new Rect(x, y, width, height);
		}
	}

	private actionSetTabIcon(data: any): void {
		const { tabId, icon } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (tab) {
			tab.setIcon(icon);
		}
	}

	private actionSetTabComponent(data: any): void {
		const { tabId, component } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (tab) {
			tab.setComponent(component);
		}
	}

	private actionSetTabConfig(data: any): void {
		const { tabId, config } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (tab) {
			tab.setConfig(config);
		}
	}

	private actionSetTabEnableClose(data: any): void {
		const { tabId, enableClose } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (tab) {
			tab.setEnableClose(enableClose);
		}
	}

	getwindowsMap(): Map<string, LayoutWindow> {
		return this.windowsMap;
	}

	getMaximizedTabset(windowId: string = Model.MAIN_WINDOW_ID): TabSetNode | undefined {
		return this.windowsMap.get(windowId)?.maximizedTabSet;
	}

	setMaximizedTabset(tabsetNode: TabSetNode | undefined, windowId: string = Model.MAIN_WINDOW_ID): void {
		const window = this.windowsMap.get(windowId);
		if (window) {
			window.maximizedTabSet = tabsetNode;
		}
	}

	getActiveTabset(windowId: string = Model.MAIN_WINDOW_ID): TabSetNode | undefined {
		return this.windowsMap.get(windowId)?.activeTabSet;
	}

	setActiveTabset(tabset: TabSetNode | undefined, windowId: string = Model.MAIN_WINDOW_ID): void {
		const window = this.windowsMap.get(windowId);
		if (window) {
			if (tabset) {
				window.activeTabSet = tabset;
			} else {
				window.activeTabSet = undefined;
			}
		}
	}

	getRoot(windowId: string = Model.MAIN_WINDOW_ID): Node | undefined {
		const window = this.windowsMap.get(windowId);
		return window?.root;
	}

	getBorderSet(): BorderSet {
		return this.borderSet;
	}

	getOnAllowDrop(): any {
		return undefined;
	}

	getOnCreateTabSet(): any {
		return undefined;
	}

	tidy(): void {
		for (const [_, window] of this.windowsMap) {
			if (window.root) {
				window.root.tidy();
			}
		}
	}

	private visitWindowNodes(windowId: string, fn: (node: Node, level: number) => void): void {
		if (this.windowsMap.has(windowId)) {
			if (windowId === Model.MAIN_WINDOW_ID) {
				this.borderSet.forEachNode(fn);
			}
			this.windowsMap.get(windowId)?.visitNodes(fn);
		}
	}

	private removeEmptyWindows(): void {
		const emptyWindows = new Set<string>();
		for (const [windowId] of this.windowsMap) {
			if (windowId !== Model.MAIN_WINDOW_ID) {
				let count = 0;
				this.visitWindowNodes(windowId, (node: Node) => {
					if (node instanceof TabNode) {
						count++;
					}
				});
				if (count === 0) {
					emptyWindows.add(windowId);
				}
			}
		}

		for (const windowId of emptyWindows) {
			this.windowsMap.delete(windowId);
		}
	}

	getAttribute(name: string): any {
		const value = this.attributes[name];
		if (value !== undefined) {
			return value;
		}

		// Return default values for global attributes
		switch (name) {
			case "tabSetEnableDeleteWhenEmpty":
				return true;
			case "tabSetEnableDrop":
				return true;
			case "tabSetEnableDrag":
				return true;
			case "tabSetEnableDivide":
				return true;
			case "tabSetEnableMaximize":
				return true;
			case "tabSetEnableClose":
				return false;
			case "tabSetEnableSingleTabStretch":
				return false;
			case "tabSetEnableTabStrip":
				return true;
			case "tabSetAutoSelectTab":
				return true;
			case "tabSetEnableActiveIcon":
				return false;
			case "tabSetMinWidth":
				return DefaultMin;
			case "tabSetMinHeight":
				return DefaultMin;
			case "tabSetMaxWidth":
				return DefaultMax;
			case "tabSetMaxHeight":
				return DefaultMax;
			case "tabEnableClose":
				return true;
			case "tabEnableDrag":
				return true;
			case "tabEnableRename":
				return true;
			case "tabEnableRenderOnDemand":
				return true;
			case "tabDragSpeed":
				return 0.3;
			case "tabBorderWidth":
				return -1;
			case "tabBorderHeight":
				return -1;
			case "enableEdgeDock":
				return true;
			case "rootOrientationVertical":
				return false;
			case "enableRotateBorderIcons":
				return true;
			case "splitterSize":
				return 8;
			case "splitterExtra":
				return 0;
			case "splitterEnableHandle":
				return false;
			case "realtimeResize":
				return false;
			default:
				return undefined;
		}
	}

	getSplitterSize(): number {
		return this.getAttribute("splitterSize") as number;
	}

	isRootOrientationVertical(): boolean {
		return this.attributes.rootOrientationVertical ?? false;
	}

	isEnableEdgeDock(): boolean {
		return this.getAttribute("enableEdgeDock") ?? true;
	}

	isRealtimeResize(): boolean {
		return this.getAttribute("realtimeResize") ?? false;
	}

	toJson(): IJsonModel {
		const layout = this.getRoot()?.toJson();
		const windows: Record<string, any> = {};
		for (const [windowId, lw] of this.windowsMap) {
			if (windowId !== Model.MAIN_WINDOW_ID) {
				windows[windowId] = lw.toJson();
			}
		}
		const result: IJsonModel = {
			global: this.attributes,
			borders: this.borderSet.toJson(),
			layout: layout as any,
		};
		if (Object.keys(windows).length > 0) {
			result.windows = windows;
		}
		return result;
	}
}
