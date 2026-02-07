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
	private maximizedTabset?: TabSetNode;
	private activeTabset?: TabSetNode;

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
		const { json, toNodeId, index } = data;
		const toNode = this.getNodeById(toNodeId);
		if (!toNode) return;

		if (toNode.getType() === "tabset" || toNode.getType() === "border") {
			const tab = new TabNode(this, json);
			const insertIndex = index === -1 ? toNode.getChildren().length : index;
			(toNode as any).addChild(tab, insertIndex);
			this.registerNode(tab);
		} else if (toNode.getType() === "row") {
			const newTabset = new TabSetNode(this, { type: "tabset", weight: 50 });
			const tab = new TabNode(this, json);
			(newTabset as any).addChild(tab);
			this.registerNode(newTabset);
			this.registerNode(tab);

			const row = toNode as RowNode;
			const insertIndex = index === -1 ? row.getChildren().length : index;
			(row as any).addChild(newTabset, insertIndex);
		}
	}

	private actionMoveNode(data: any): void {
		const { fromNodeId, toNodeId, index } = data;
		const fromNode = this.getNodeById(fromNodeId);
		const toNode = this.getNodeById(toNodeId);

		if (!fromNode || !toNode) return;

		const parent = fromNode.getParent();
		if (parent) {
			(parent as any).removeChild(fromNode);
		}

		if (toNode.getType() === "tabset" || toNode.getType() === "border") {
			const insertIndex = index === -1 ? toNode.getChildren().length : index;
			(toNode as any).addChild(fromNode, insertIndex);
		}
	}

	private actionDeleteTab(data: any): void {
		const { tabId } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (!tab) return;

		const parent = tab.getParent();
		if (parent) {
			tab.fireEvent("close", {});
			(parent as any).removeChild(tab);
			this.nodeRegistry.delete(tabId);
		}
	}

	private actionDeleteTabset(data: any): void {
		const { tabsetId } = data;
		const tabset = this.getNodeById(tabsetId);
		if (!tabset) return;

		const parent = tabset.getParent();
		if (parent) {
			(parent as any).removeChild(tabset);
			this.nodeRegistry.delete(tabsetId);
		}
	}

	private actionRenameTab(data: any): void {
		const { tabId, newName } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (tab) {
			tab.setName(newName);
		}
	}

	private actionSelectTab(data: any): void {
		const { tabId } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (!tab) return;

		const parent = tab.getParent() as TabSetNode;
		if (parent && parent.getType() === "tabset") {
			const index = parent.getChildren().indexOf(tab);
			if (index !== -1) {
				(parent as any).setSelected(index);
			}
		}
	}

	private actionSetActiveTabset(data: any): void {
		const { tabsetId } = data;
		const tabset = this.getNodeById(tabsetId) as TabSetNode;
		if (tabset && tabset.getType() === "tabset") {
			this.activeTabset = tabset;
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
			if (this.maximizedTabset === tabset) {
				this.maximizedTabset = undefined;
			} else {
				this.maximizedTabset = tabset;
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

	private actionPopoutTab(_data: any): void {
		// Placeholder for popout functionality
	}

	private actionPopoutTabset(_data: any): void {
		// Placeholder for popout functionality
	}

	private actionCloseWindow(data: any): void {
		const { windowId } = data;
		this.windowsMap.delete(windowId);
	}

	private actionCreateWindow(_data: any): void {
		const windowId = this.nextUniqueId();
		this.windowsMap.set(windowId, new LayoutWindow(windowId, Rect.empty()));
	}

	private actionFloatTab(_data: any): void {
		// Placeholder for floating functionality
	}

	private actionFloatTabset(_data: any): void {
		// Placeholder for floating functionality
	}

	private actionDockTab(_data: any): void {
		// Placeholder for docking functionality
	}

	private actionDockTabset(_data: any): void {
		// Placeholder for docking functionality
	}

	private actionSetTabIcon(data: any): void {
		const { tabId, icon } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (tab) {
			(tab as any).setIcon(icon);
		}
	}

	private actionSetTabComponent(data: any): void {
		const { tabId, component } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (tab) {
			(tab as any).setComponent(component);
		}
	}

	private actionSetTabConfig(data: any): void {
		const { tabId, config } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (tab) {
			(tab as any).setConfig(config);
		}
	}

	private actionSetTabEnableClose(data: any): void {
		const { tabId, enableClose } = data;
		const tab = this.getNodeById(tabId) as TabNode;
		if (tab) {
			(tab as any).setEnableClose(enableClose);
		}
	}

	getwindowsMap(): Map<string, LayoutWindow> {
		return this.windowsMap;
	}

	getMaximizedTabset(): TabSetNode | undefined {
		return this.maximizedTabset;
	}

	getActiveTabset(): TabSetNode | undefined {
		return this.activeTabset;
	}

	setActiveTabset(tabset: TabSetNode): void {
		this.activeTabset = tabset;
	}

	getRoot(): Node | undefined {
		const mainWindow = this.windowsMap.get(Model.MAIN_WINDOW_ID);
		return mainWindow?.root;
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
		// Placeholder for tidying layout
	}

	getAttribute(name: string): any {
		return this.attributes[name];
	}

	getSplitterSize(): number {
		return this.attributes.splitterSize || 4;
	}

	isRootOrientationVertical(): boolean {
		return true;
	}

	toJson(): IJsonModel {
		const layout = this.getRoot()?.toJson();
		return {
			global: this.attributes,
			borders: this.borderSet.toJson(),
			layout: layout as any,
		};
	}
}
