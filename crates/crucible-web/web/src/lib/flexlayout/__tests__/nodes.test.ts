import { describe, it, expect, beforeEach } from "vitest";
import { TabNode } from "../model/TabNode";
import { TabSetNode } from "../model/TabSetNode";
import { RowNode } from "../model/RowNode";
import { BorderNode } from "../model/BorderNode";
import { BorderSet } from "../model/BorderSet";
import { LayoutWindow } from "../model/LayoutWindow";
import { Model } from "../model/Model";
import { Rect } from "../core/Rect";
import { DockLocation } from "../core/DockLocation";

describe("FlexLayout Node Types", () => {
	let model: Model;

	beforeEach(() => {
		model = new Model();
	});

	describe("TabNode", () => {
		it("should create a TabNode with default attributes", () => {
			const tab = new TabNode(model, { name: "Test Tab" });
			expect(tab).toBeDefined();
			expect(tab.getName()).toBe("Test Tab");
		});

		it("should get and set name", () => {
			const tab = new TabNode(model, { name: "Original" });
			tab.setName("Updated");
			expect(tab.getName()).toBe("Updated");
		});

		it("should get component", () => {
			const tab = new TabNode(model, { component: "MyComponent" });
			expect(tab.getComponent()).toBe("MyComponent");
		});

		it("should get help text", () => {
			const tab = new TabNode(model, { helpText: "Help me" });
			expect(tab.getHelpText()).toBe("Help me");
		});

		it("should get icon", () => {
			const tab = new TabNode(model, { icon: "icon-name" });
			expect(tab.getIcon()).toBe("icon-name");
		});

		it("should check enable close", () => {
			const tab = new TabNode(model, { enableClose: true });
			expect(tab.isEnableClose()).toBe(true);
		});

		it("should get close type", () => {
			const tab = new TabNode(model, { closeType: 1 });
			expect(tab.getCloseType()).toBe(1);
		});

		it("should check enable popout", () => {
			const tab = new TabNode(model, { enablePopout: true });
			expect(tab.isEnablePopout()).toBe(true);
		});

		it("should check enable drag", () => {
			const tab = new TabNode(model, { enableDrag: true });
			expect(tab.isEnableDrag()).toBe(true);
		});

		it("should check enable rename", () => {
			const tab = new TabNode(model, { enableRename: true });
			expect(tab.isEnableRename()).toBe(true);
		});

		it("should get class name", () => {
			const tab = new TabNode(model, { className: "my-class" });
			expect(tab.getClassName()).toBe("my-class");
		});

		it("should get content class name", () => {
			const tab = new TabNode(model, { contentClassName: "content-class" });
			expect(tab.getContentClassName()).toBe("content-class");
		});

		it("should get min/max dimensions", () => {
			const tab = new TabNode(model, {
				minWidth: 100,
				minHeight: 50,
				maxWidth: 500,
				maxHeight: 400,
			});
			expect(tab.getMinWidth()).toBe(100);
			expect(tab.getMinHeight()).toBe(50);
			expect(tab.getMaxWidth()).toBe(500);
			expect(tab.getMaxHeight()).toBe(400);
		});

		it("should get and set scroll position", () => {
			const tab = new TabNode(model, {});
			tab.setScrollTop(100);
			tab.setScrollLeft(50);
			expect(tab.getScrollTop()).toBe(100);
			expect(tab.getScrollLeft()).toBe(50);
		});

		it("should get and set rendered state", () => {
			const tab = new TabNode(model, {});
			expect(tab.isRendered()).toBe(false);
			tab.setRendered(true);
			expect(tab.isRendered()).toBe(true);
		});

		it("should get and set tab rect", () => {
			const tab = new TabNode(model, {});
			const rect = new Rect(10, 20, 100, 50);
			tab.setTabRect(rect);
			expect(tab.getTabRect()).toEqual(rect);
		});

		it("should get and set moveable element", () => {
			const tab = new TabNode(model, {});
			const element = document.createElement("div");
			tab.setMoveableElement(element);
			expect(tab.getMoveableElement()).toBe(element);
		});

		it("should get and set tab stamp", () => {
			const tab = new TabNode(model, {});
			const stamp = document.createElement("div");
			tab.setTabStamp(stamp);
			expect(tab.getTabStamp()).toBe(stamp);
		});

		it("should get config", () => {
			const config = { key: "value" };
			const tab = new TabNode(model, { config });
			expect(tab.getConfig()).toEqual(config);
		});

		it("should get extra data", () => {
			const tab = new TabNode(model, {});
			const extra = tab.getExtraData();
			expect(extra).toBeDefined();
			expect(typeof extra).toBe("object");
		});

		it("should convert to JSON", () => {
			const tab = new TabNode(model, { name: "Test", component: "Comp" });
			const json = tab.toJson();
			expect(json).toBeDefined();
			expect(typeof json).toBe("object");
		});

		it("should set border width and height", () => {
			const tab = new TabNode(model, {});
			tab.setBorderWidth(200);
			tab.setBorderHeight(150);
			expect(tab.getAttributeDefinitions()).toBeDefined();
		});

		it("should get attribute definitions", () => {
			const tab = new TabNode(model, {});
			const defs = tab.getAttributeDefinitions();
			expect(defs).toBeDefined();
		});

		it("should get static attribute definitions", () => {
			const defs = TabNode.getAttributeDefinitions();
			expect(defs).toBeDefined();
		});
	});

	describe("TabSetNode", () => {
		it("should create a TabSetNode", () => {
			const tabset = new TabSetNode(model, {});
			expect(tabset).toBeDefined();
		});

		it("should get name", () => {
			const tabset = new TabSetNode(model, { name: "MyTabSet" });
			expect(tabset.getName()).toBe("MyTabSet");
		});

		it("should get weight", () => {
			const tabset = new TabSetNode(model, { weight: 75 });
			expect(tabset.getWeight()).toBe(75);
		});

		it("should get selected index", () => {
			const tabset = new TabSetNode(model, { selected: 2 });
			expect(tabset.getSelected()).toBe(2);
		});

		it("should get selected node", () => {
			const tabset = new TabSetNode(model, {});
			const tab = new TabNode(model, { name: "Tab1" });
			tabset.addChild(tab);
			tabset.setSelected(0);
			expect(tabset.getSelectedNode()).toBe(tab);
		});

		it("should check enable drop", () => {
			const tabset = new TabSetNode(model, { enableDrop: true });
			expect(tabset.isEnableDrop()).toBe(true);
		});

		it("should check enable drag", () => {
			const tabset = new TabSetNode(model, { enableDrag: true });
			expect(tabset.isEnableDrag()).toBe(true);
		});

		it("should check enable divide", () => {
			const tabset = new TabSetNode(model, { enableDivide: true });
			expect(tabset.isEnableDivide()).toBe(true);
		});

		it("should check enable maximize", () => {
			const tabset = new TabSetNode(model, { enableMaximize: true });
			expect(tabset.isEnableMaximize()).toBe(true);
		});

		it("should check enable close", () => {
			const tabset = new TabSetNode(model, { enableClose: true });
			expect(tabset.isEnableClose()).toBe(true);
		});

		it("should check enable tab strip", () => {
			const tabset = new TabSetNode(model, { enableTabStrip: true });
			expect(tabset.isEnableTabStrip()).toBe(true);
		});

		it("should get tab location", () => {
			const tabset = new TabSetNode(model, { tabLocation: "top" });
			expect(tabset.getTabLocation()).toBe("top");
		});

		it("should get min/max dimensions", () => {
			const tabset = new TabSetNode(model, {
				minWidth: 200,
				minHeight: 100,
				maxWidth: 800,
				maxHeight: 600,
			});
			expect(tabset.getAttrMinWidth()).toBe(200);
			expect(tabset.getAttrMinHeight()).toBe(100);
			expect(tabset.getAttrMaxWidth()).toBe(800);
			expect(tabset.getAttrMaxHeight()).toBe(600);
		});

		it("should get config", () => {
			const config = { setting: "value" };
			const tabset = new TabSetNode(model, { config });
			expect(tabset.getConfig()).toEqual(config);
		});

		it("should set and get content rect", () => {
			const tabset = new TabSetNode(model, {});
			const rect = new Rect(0, 0, 500, 400);
			tabset.setContentRect(rect);
			expect(tabset.getContentRect()).toEqual(rect);
		});

		it("should set and get tab strip rect", () => {
			const tabset = new TabSetNode(model, {});
			const rect = new Rect(0, 0, 500, 30);
			tabset.setTabStripRect(rect);
			expect(tabset.getAttributeDefinitions()).toBeDefined();
		});

		it("should add and remove children", () => {
			const tabset = new TabSetNode(model, {});
			const tab1 = new TabNode(model, { name: "Tab1" });
			const tab2 = new TabNode(model, { name: "Tab2" });

			tabset.addChild(tab1);
			tabset.addChild(tab2);
			expect(tabset.getChildren().length).toBe(2);

			tabset.removeChild(tab1);
			expect(tabset.getChildren().length).toBe(1);
		});

		it("should convert to JSON", () => {
			const row = new RowNode(model, "main", {});
			const tabset = new TabSetNode(model, { name: "TestSet" });
			row.addChild(tabset);
			const tab = new TabNode(model, { name: "Tab1" });
			tabset.addChild(tab);
			const json = tabset.toJson();
			expect(json).toBeDefined();
			expect(json.type).toBe("tabset");
		});

		it("should get static attribute definitions", () => {
			const defs = TabSetNode.getAttributeDefinitions();
			expect(defs).toBeDefined();
		});
	});

	describe("RowNode", () => {
		it("should create a RowNode", () => {
			const row = new RowNode(model, "main", {});
			expect(row).toBeDefined();
		});

		it("should get weight", () => {
			const row = new RowNode(model, "main", { weight: 50 });
			expect(row.getWeight()).toBe(50);
		});

		it("should get orientation", () => {
			const row = new RowNode(model, "main", {});
			expect(row.getOrientation()).toBeDefined();
		});

		it("should add and remove children", () => {
			const row = new RowNode(model, "main", {});
			const tabset = new TabSetNode(model, {});
			row.addChild(tabset);
			expect(row.getChildren().length).toBe(1);
			row.removeChild(tabset);
			expect(row.getChildren().length).toBe(0);
		});

		it("should convert to JSON", () => {
			const row = new RowNode(model, "main", {});
			const tabset = new TabSetNode(model, {});
			row.addChild(tabset);
			const json = row.toJson();
			expect(json).toBeDefined();
			expect(json.type).toBe("row");
		});

		it("should get static attribute definitions", () => {
			const defs = RowNode.getAttributeDefinitions();
			expect(defs).toBeDefined();
		});
	});

	describe("BorderNode", () => {
		it("should create a BorderNode", () => {
			const border = new BorderNode(DockLocation.TOP, {}, model);
			expect(border).toBeDefined();
		});

		it("should get location", () => {
			const border = new BorderNode(DockLocation.TOP, {}, model);
			expect(border.getLocation()).toBe(DockLocation.TOP);
		});

		it("should add and remove children", () => {
			const border = new BorderNode(DockLocation.TOP, {}, model);
			const tab = new TabNode(model, { name: "BorderTab" });
			border.addChild(tab);
			expect(border.getChildren().length).toBe(1);
			border.removeChild(tab);
			expect(border.getChildren().length).toBe(0);
		});

		it("should convert to JSON", () => {
			const border = new BorderNode(DockLocation.TOP, {}, model);
			const tab = new TabNode(model, { name: "BorderTab" });
			border.addChild(tab);
			const json = border.toJson();
			expect(json).toBeDefined();
			expect(json.type).toBe("border");
		});

		it("should get static attribute definitions", () => {
			const defs = BorderNode.getAttributeDefinitions();
			expect(defs).toBeDefined();
		});
	});

	describe("BorderSet", () => {
		it("should create a BorderSet", () => {
			const borderSet = new BorderSet(model);
			expect(borderSet).toBeDefined();
		});

		it("should get borders", () => {
			const borderSet = new BorderSet(model);
			const borders = borderSet.getBorders();
			expect(borders).toBeDefined();
			expect(Array.isArray(borders)).toBe(true);
		});
	});

	describe("LayoutWindow", () => {
		it("should create a LayoutWindow", () => {
			const rect = new Rect(0, 0, 800, 600);
			const window = new LayoutWindow("main", rect);
			expect(window).toBeDefined();
		});

		it("should get window id", () => {
			const rect = new Rect(0, 0, 800, 600);
			const window = new LayoutWindow("test-window", rect);
			expect(window.windowId).toBe("test-window");
		});

		it("should get rect", () => {
			const rect = new Rect(0, 0, 800, 600);
			const window = new LayoutWindow("main", rect);
			expect(window.rect).toEqual(rect);
		});
	});

	describe("Node base class", () => {
		it("should get and set id", () => {
			const tab = new TabNode(model, {});
			const id = tab.getId();
			expect(id).toBeDefined();
			expect(typeof id).toBe("string");
		});

		it("should get model", () => {
			const tab = new TabNode(model, {});
			expect(tab.getModel()).toBe(model);
		});

		it("should get type", () => {
			const tab = new TabNode(model, {});
			expect(tab.getType()).toBe("tab");
		});

		it("should get parent", () => {
			const tabset = new TabSetNode(model, {});
			const tab = new TabNode(model, {});
			tabset.addChild(tab);
			expect(tab.getParent()).toBe(tabset);
		});

		it("should get children", () => {
			const tabset = new TabSetNode(model, {});
			const tab = new TabNode(model, {});
			tabset.addChild(tab);
			expect(tabset.getChildren()).toContain(tab);
		});

		it("should get rect", () => {
			const tab = new TabNode(model, {});
			const rect = tab.getRect();
			expect(rect).toBeDefined();
		});

		it("should get path", () => {
			const tab = new TabNode(model, {});
			expect(tab.getPath()).toBeDefined();
		});

		it("should set and get event listener", () => {
			const tab = new TabNode(model, {});
			let eventFired = false;
			tab.setEventListener("test", () => {
				eventFired = true;
			});
			tab.fireEvent("test", {});
			expect(eventFired).toBe(true);
		});

		it("should remove event listener", () => {
			const tab = new TabNode(model, {});
			let eventFired = false;
			tab.setEventListener("test", () => {
				eventFired = true;
			});
			tab.removeEventListener("test");
			tab.fireEvent("test", {});
			expect(eventFired).toBe(false);
		});

		it("should style with position", () => {
			const tab = new TabNode(model, {});
			const rect = new Rect(10, 20, 100, 50);
			(tab as any).rect = rect;
			const style = tab.styleWithPosition();
			expect(style.position).toBe("absolute");
			expect(style.left).toBe("10px");
			expect(style.top).toBe("20px");
		});

		it("should convert to attribute string", () => {
			const tab = new TabNode(model, { name: "Test" });
			const str = tab.toAttributeString();
			expect(typeof str).toBe("string");
			expect(str.length).toBeGreaterThan(0);
		});
	});
});
