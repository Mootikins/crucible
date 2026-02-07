import { describe, it, expect } from "vitest";
import { Rect } from "../core/Rect";
import { DockLocation } from "../core/DockLocation";
import { Orientation } from "../core/Orientation";
import { Attribute } from "../core/Attribute";
import { AttributeDefinitions } from "../core/AttributeDefinitions";
import { DropInfo } from "../core/DropInfo";
import { I18nLabel } from "../core/I18nLabel";
import { CLASSES } from "../core/Types";

describe("Rect", () => {
  it("should create a rect with correct dimensions", () => {
    const rect = new Rect(10, 20, 100, 50);
    expect(rect.x).toBe(10);
    expect(rect.y).toBe(20);
    expect(rect.width).toBe(100);
    expect(rect.height).toBe(50);
  });

  it("should create an empty rect", () => {
    const rect = Rect.empty();
    expect(rect.x).toBe(0);
    expect(rect.y).toBe(0);
    expect(rect.width).toBe(0);
    expect(rect.height).toBe(0);
  });

  it("should serialize to JSON", () => {
    const rect = new Rect(10, 20, 100, 50);
    const json = rect.toJson();
    expect(json).toEqual({ x: 10, y: 20, width: 100, height: 50 });
  });

  it("should deserialize from JSON", () => {
    const json = { x: 10, y: 20, width: 100, height: 50 };
    const rect = Rect.fromJson(json);
    expect(rect.x).toBe(10);
    expect(rect.y).toBe(20);
    expect(rect.width).toBe(100);
    expect(rect.height).toBe(50);
  });

  it("should check if point is contained", () => {
    const rect = new Rect(10, 20, 100, 50);
    expect(rect.contains(50, 45)).toBe(true);
    expect(rect.contains(10, 20)).toBe(true);
    expect(rect.contains(110, 70)).toBe(true); // right and bottom edges inclusive
    expect(rect.contains(9, 45)).toBe(false);
    expect(rect.contains(111, 45)).toBe(false);
    expect(rect.contains(50, 19)).toBe(false);
    expect(rect.contains(50, 71)).toBe(false);
  });

  it("should get center point", () => {
    const rect = new Rect(10, 20, 100, 50);
    const center = rect.getCenter();
    expect(center.x).toBe(60);
    expect(center.y).toBe(45);
  });

  it("should get right edge", () => {
    const rect = new Rect(10, 20, 100, 50);
    expect(rect.getRight()).toBe(110);
    expect(rect.right).toBe(110);
  });

  it("should get bottom edge", () => {
    const rect = new Rect(10, 20, 100, 50);
    expect(rect.getBottom()).toBe(70);
    expect(rect.bottom).toBe(70);
  });

  it("should clone a rect", () => {
    const rect = new Rect(10, 20, 100, 50);
    const cloned = rect.clone();
    expect(cloned).toEqual(rect);
    expect(cloned).not.toBe(rect);
  });

  it("should check equality", () => {
    const rect1 = new Rect(10, 20, 100, 50);
    const rect2 = new Rect(10, 20, 100, 50);
    const rect3 = new Rect(10, 20, 100, 51);
    expect(rect1.equals(rect2)).toBe(true);
    expect(rect1.equals(rect3)).toBe(false);
    expect(rect1.equals(null)).toBe(false);
    expect(rect1.equals(undefined)).toBe(false);
  });

  it("should check size equality", () => {
    const rect1 = new Rect(10, 20, 100, 50);
    const rect2 = new Rect(5, 10, 100, 50);
    const rect3 = new Rect(10, 20, 100, 51);
    expect(rect1.equalSize(rect2)).toBe(true);
    expect(rect1.equalSize(rect3)).toBe(false);
  });

  it("should make rect relative to another rect", () => {
    const rect1 = new Rect(50, 60, 100, 50);
    const rect2 = new Rect(10, 20, 200, 200);
    const relative = rect1.relativeTo(rect2);
    expect(relative.x).toBe(40);
    expect(relative.y).toBe(40);
    expect(relative.width).toBe(100);
    expect(relative.height).toBe(50);
  });

  it("should snap to grid", () => {
    const rect = new Rect(15, 25, 105, 55);
    rect.snap(10);
    expect(rect.x).toBe(20);
    expect(rect.y).toBe(30);
    expect(rect.width).toBe(110);
    expect(rect.height).toBe(60);
  });

  it("should remove insets", () => {
    const rect = new Rect(10, 20, 100, 50);
    const inset = rect.removeInsets({ top: 5, left: 10, bottom: 5, right: 10 });
    expect(inset.x).toBe(20);
    expect(inset.y).toBe(25);
    expect(inset.width).toBe(80);
    expect(inset.height).toBe(40);
  });

  it("should get size based on orientation", () => {
    const rect = new Rect(10, 20, 100, 50);
    expect(rect._getSize(Orientation.HORZ)).toBe(100);
    expect(rect._getSize(Orientation.VERT)).toBe(50);
  });

  it("should convert to string", () => {
    const rect = new Rect(10, 20, 100, 50);
    expect(rect.toString()).toContain("x=10");
    expect(rect.toString()).toContain("y=20");
    expect(rect.toString()).toContain("width=100");
    expect(rect.toString()).toContain("height=50");
  });
});

describe("DockLocation", () => {
  it("should have predefined locations", () => {
    expect(DockLocation.TOP).toBeDefined();
    expect(DockLocation.BOTTOM).toBeDefined();
    expect(DockLocation.LEFT).toBeDefined();
    expect(DockLocation.RIGHT).toBeDefined();
    expect(DockLocation.CENTER).toBeDefined();
  });

  it("should get location by name", () => {
    expect(DockLocation.getByName("top")).toBe(DockLocation.TOP);
    expect(DockLocation.getByName("bottom")).toBe(DockLocation.BOTTOM);
    expect(DockLocation.getByName("left")).toBe(DockLocation.LEFT);
    expect(DockLocation.getByName("right")).toBe(DockLocation.RIGHT);
    expect(DockLocation.getByName("center")).toBe(DockLocation.CENTER);
  });

  it("should detect center zone (25%-75%)", () => {
    const rect = new Rect(0, 0, 100, 100);
    // Center point
    expect(DockLocation.getLocation(rect, 50, 50)).toBe(DockLocation.CENTER);
    // 25% boundary (inclusive)
    expect(DockLocation.getLocation(rect, 25, 50)).toBe(DockLocation.CENTER);
    expect(DockLocation.getLocation(rect, 50, 25)).toBe(DockLocation.CENTER);
    // 75% boundary (exclusive, so 74 is center, 75 is not)
    expect(DockLocation.getLocation(rect, 74, 50)).toBe(DockLocation.CENTER);
    expect(DockLocation.getLocation(rect, 50, 74)).toBe(DockLocation.CENTER);
  });

  it("should detect top zone", () => {
    const rect = new Rect(0, 0, 100, 100);
    expect(DockLocation.getLocation(rect, 50, 10)).toBe(DockLocation.TOP);
    expect(DockLocation.getLocation(rect, 50, 0)).toBe(DockLocation.TOP);
  });

  it("should detect bottom zone", () => {
    const rect = new Rect(0, 0, 100, 100);
    expect(DockLocation.getLocation(rect, 50, 90)).toBe(DockLocation.BOTTOM);
    expect(DockLocation.getLocation(rect, 50, 100)).toBe(DockLocation.BOTTOM);
  });

  it("should detect left zone", () => {
    const rect = new Rect(0, 0, 100, 100);
    expect(DockLocation.getLocation(rect, 10, 50)).toBe(DockLocation.LEFT);
    expect(DockLocation.getLocation(rect, 0, 50)).toBe(DockLocation.LEFT);
  });

  it("should detect right zone", () => {
    const rect = new Rect(0, 0, 100, 100);
    expect(DockLocation.getLocation(rect, 90, 50)).toBe(DockLocation.RIGHT);
    expect(DockLocation.getLocation(rect, 100, 50)).toBe(DockLocation.RIGHT);
  });

  it("should detect diagonal zones correctly", () => {
    const rect = new Rect(0, 0, 100, 100);
    // Top-left corner (y < x, so above diagonal)
    expect(DockLocation.getLocation(rect, 10, 5)).toBe(DockLocation.TOP);
    // Bottom-left corner (y >= x, so below diagonal)
    expect(DockLocation.getLocation(rect, 10, 90)).toBe(DockLocation.BOTTOM);
    // Top-right corner (y < 1-x, so above anti-diagonal)
    expect(DockLocation.getLocation(rect, 90, 5)).toBe(DockLocation.TOP);
    // Bottom-right corner (y >= 1-x, so below anti-diagonal)
    expect(DockLocation.getLocation(rect, 90, 90)).toBe(DockLocation.BOTTOM);
  });

  it("should get name", () => {
    expect(DockLocation.TOP.getName()).toBe("top");
    expect(DockLocation.BOTTOM.getName()).toBe("bottom");
    expect(DockLocation.LEFT.getName()).toBe("left");
    expect(DockLocation.RIGHT.getName()).toBe("right");
    expect(DockLocation.CENTER.getName()).toBe("center");
  });

  it("should get orientation", () => {
    expect(DockLocation.TOP.getOrientation()).toBe(Orientation.VERT);
    expect(DockLocation.BOTTOM.getOrientation()).toBe(Orientation.VERT);
    expect(DockLocation.LEFT.getOrientation()).toBe(Orientation.HORZ);
    expect(DockLocation.RIGHT.getOrientation()).toBe(Orientation.HORZ);
    expect(DockLocation.CENTER.getOrientation()).toBe(Orientation.VERT);
  });

  it("should get dock rect", () => {
    const rect = new Rect(0, 0, 100, 100);
    const topDock = DockLocation.TOP.getDockRect(rect);
    expect(topDock.x).toBe(0);
    expect(topDock.y).toBe(0);
    expect(topDock.width).toBe(100);
    expect(topDock.height).toBe(50);

    const bottomDock = DockLocation.BOTTOM.getDockRect(rect);
    expect(bottomDock.y).toBe(50);
    expect(bottomDock.height).toBe(50);

    const leftDock = DockLocation.LEFT.getDockRect(rect);
    expect(leftDock.width).toBe(50);

    const rightDock = DockLocation.RIGHT.getDockRect(rect);
    expect(rightDock.x).toBe(50);
    expect(rightDock.width).toBe(50);

    const centerDock = DockLocation.CENTER.getDockRect(rect);
    expect(centerDock).toEqual(rect);
  });

  it("should split rect", () => {
    const rect = new Rect(0, 0, 100, 100);
    const split = DockLocation.TOP.split(rect, 30);
    expect(split.start.height).toBe(30);
    expect(split.end.height).toBe(70);
    expect(split.end.y).toBe(30);
  });

  it("should reflect location", () => {
    expect(DockLocation.TOP.reflect()).toBe(DockLocation.BOTTOM);
    expect(DockLocation.BOTTOM.reflect()).toBe(DockLocation.TOP);
    expect(DockLocation.LEFT.reflect()).toBe(DockLocation.RIGHT);
    expect(DockLocation.RIGHT.reflect()).toBe(DockLocation.LEFT);
  });

  it("should convert to string", () => {
    expect(DockLocation.TOP.toString()).toContain("top");
  });
});

describe("Orientation", () => {
  it("should have predefined orientations", () => {
    expect(Orientation.HORZ).toBeDefined();
    expect(Orientation.VERT).toBeDefined();
  });

  it("should flip orientation", () => {
    expect(Orientation.flip(Orientation.HORZ)).toBe(Orientation.VERT);
    expect(Orientation.flip(Orientation.VERT)).toBe(Orientation.HORZ);
  });

  it("should get name", () => {
    expect(Orientation.HORZ.getName()).toBe("horz");
    expect(Orientation.VERT.getName()).toBe("vert");
  });

  it("should convert to string", () => {
    expect(Orientation.HORZ.toString()).toBe("horz");
    expect(Orientation.VERT.toString()).toBe("vert");
  });
});

describe("Attribute", () => {
  it("should create an attribute", () => {
    const attr = new Attribute("testAttr", "modelAttr", "defaultValue");
    expect(attr.name).toBe("testAttr");
    expect(attr.modelName).toBe("modelAttr");
    expect(attr.defaultValue).toBe("defaultValue");
    expect(attr.required).toBe(false);
    expect(attr.fixed).toBe(false);
  });

  it("should set type", () => {
    const attr = new Attribute("testAttr", undefined, "value");
    attr.setType("string");
    expect(attr.type).toBe("string");
  });

  it("should set alias", () => {
    const attr = new Attribute("testAttr", undefined, "value");
    attr.setAlias("aliasName");
    expect(attr.alias).toBe("aliasName");
  });

  it("should set description", () => {
    const attr = new Attribute("testAttr", undefined, "value");
    attr.setDescription("Test description");
    expect(attr.description).toBe("Test description");
  });

  it("should set required", () => {
    const attr = new Attribute("testAttr", undefined, "value");
    attr.setRequired();
    expect(attr.required).toBe(true);
  });

  it("should set fixed", () => {
    const attr = new Attribute("testAttr", undefined, "value");
    attr.setFixed();
    expect(attr.fixed).toBe(true);
  });

  it("should set paired attribute", () => {
    const attr1 = new Attribute("attr1", undefined, "value1");
    const attr2 = new Attribute("attr2", undefined, "value2");
    attr1.setpairedAttr(attr2);
    expect(attr1.pairedAttr).toBe(attr2);
  });

  it("should set paired type", () => {
    const attr = new Attribute("testAttr", undefined, "value");
    attr.setPairedType("TabNode");
    expect(attr.pairedType).toBe("TabNode");
  });

  it("should have static type constants", () => {
    expect(Attribute.NUMBER).toBe("number");
    expect(Attribute.STRING).toBe("string");
    expect(Attribute.BOOLEAN).toBe("boolean");
  });
});

describe("AttributeDefinitions", () => {
  it("should create empty definitions", () => {
    const defs = new AttributeDefinitions();
    expect(defs.getAttributes()).toEqual([]);
  });

  it("should add attribute with all parameters", () => {
    const defs = new AttributeDefinitions();
    const attr = defs.addWithAll("testAttr", "modelAttr", "defaultValue", true);
    expect(attr.name).toBe("testAttr");
    expect(defs.getAttributes().length).toBe(1);
  });

  it("should add inherited attribute", () => {
    const defs = new AttributeDefinitions();
    const attr = defs.addInherited("testAttr", "modelAttr");
    expect(attr.modelName).toBe("modelAttr");
    expect(attr.defaultValue).toBeUndefined();
  });

  it("should add simple attribute", () => {
    const defs = new AttributeDefinitions();
    const attr = defs.add("testAttr", "defaultValue");
    expect(attr.name).toBe("testAttr");
    expect(attr.defaultValue).toBe("defaultValue");
    expect(attr.modelName).toBeUndefined();
  });

  it("should get model name for attribute", () => {
    const defs = new AttributeDefinitions();
    defs.addWithAll("testAttr", "modelAttr", "value");
    expect(defs.getModelName("testAttr")).toBe("modelAttr");
    expect(defs.getModelName("nonexistent")).toBeUndefined();
  });

  it("should serialize to JSON", () => {
    const defs = new AttributeDefinitions();
    defs.add("attr1", "default1");
    defs.add("attr2", "default2");
    const obj = { attr1: "value1", attr2: "default2" };
    const json: any = {};
    defs.toJson(json, obj);
    expect(json.attr1).toBe("value1");
    expect(json.attr2).toBeUndefined(); // not written because it equals default
  });

  it("should deserialize from JSON", () => {
    const defs = new AttributeDefinitions();
    defs.add("attr1", "default1");
    defs.add("attr2", "default2");
    const json = { attr1: "value1" };
    const obj: any = {};
    defs.fromJson(json, obj);
    expect(obj.attr1).toBe("value1");
    expect(obj.attr2).toBe("default2");
  });

  it("should handle alias in fromJson", () => {
    const defs = new AttributeDefinitions();
    const attr = defs.add("attr1", "default1");
    attr.setAlias("oldName");
    const json = { oldName: "value1" };
    const obj: any = {};
    defs.fromJson(json, obj);
    expect(obj.attr1).toBe("value1");
  });

  it("should update attributes", () => {
    const defs = new AttributeDefinitions();
    defs.add("attr1", "default1");
    defs.add("attr2", "default2");
    const obj = { attr1: "value1", attr2: "value2" };
    const json = { attr1: "newValue1" };
    defs.update(json, obj);
    expect(obj.attr1).toBe("newValue1");
    expect(obj.attr2).toBe("value2");
  });

  it("should set defaults", () => {
    const defs = new AttributeDefinitions();
    defs.add("attr1", "default1");
    defs.add("attr2", "default2");
    const obj: any = {};
    defs.setDefaults(obj);
    expect(obj.attr1).toBe("default1");
    expect(obj.attr2).toBe("default2");
  });

  it("should pair attributes", () => {
    const globalDefs = new AttributeDefinitions();
    globalDefs.add("globalAttr", "globalDefault");
    const nodeDefs = new AttributeDefinitions();
    nodeDefs.addInherited("nodeAttr", "globalAttr");
    globalDefs.pairAttributes("TestNode", nodeDefs);
    const globalAttr = globalDefs.getAttributes()[0];
    const nodeAttr = nodeDefs.getAttributes()[0];
    expect(globalAttr.pairedAttr).toBe(nodeAttr);
    expect(nodeAttr.pairedAttr).toBe(globalAttr);
  });

  it("should generate TypeScript interface", () => {
    const defs = new AttributeDefinitions();
    defs.add("attr1", "default1").setType("string").setDescription("Test attribute");
    const iface = defs.toTypescriptInterface("TestNode", undefined);
    expect(iface).toContain("ITestNodeAttributes");
    expect(iface).toContain("attr1");
    expect(iface).toContain("string");
  });
});

describe("DropInfo", () => {
  it("should create drop info", () => {
    const rect = new Rect(10, 20, 100, 50);
    const mockNode = {} as any;
    const dropInfo = new DropInfo(mockNode, rect, DockLocation.CENTER, 0, "testClass");
    expect(dropInfo.node).toBe(mockNode);
    expect(dropInfo.rect).toBe(rect);
    expect(dropInfo.location).toBe(DockLocation.CENTER);
    expect(dropInfo.index).toBe(0);
    expect(dropInfo.className).toBe("testClass");
  });
});

describe("I18nLabel", () => {
  it("should have all labels defined", () => {
    expect(I18nLabel.Close_Tab).toBe("Close");
    expect(I18nLabel.Close_Tabset).toBe("Close tab set");
    expect(I18nLabel.Active_Tabset).toBe("Active tab set");
    expect(I18nLabel.Move_Tabset).toBe("Move tab set");
    expect(I18nLabel.Move_Tabs).toBe("Move tabs(?)");
    expect(I18nLabel.Maximize).toBe("Maximize tab set");
    expect(I18nLabel.Restore).toBe("Restore tab set");
    expect(I18nLabel.Popout_Tab).toBe("Popout selected tab");
    expect(I18nLabel.Overflow_Menu_Tooltip).toBe("Hidden tabs");
    expect(I18nLabel.Error_rendering_component).toBe("Error rendering component");
    expect(I18nLabel.Error_rendering_component_retry).toBe("Retry");
  });
});

describe("CLASSES", () => {
  it("should have all CSS class constants defined", () => {
    expect(CLASSES.FLEXLAYOUT__BORDER).toBe("flexlayout__border");
    expect(CLASSES.FLEXLAYOUT__TABSET).toBe("flexlayout__tabset");
    expect(CLASSES.FLEXLAYOUT__TAB).toBe("flexlayout__tab");
    expect(CLASSES.FLEXLAYOUT__ROW).toBe("flexlayout__row");
    expect(CLASSES.FLEXLAYOUT__SPLITTER).toBe("flexlayout__splitter");
    expect(CLASSES.FLEXLAYOUT__LAYOUT).toBe("flexlayout__layout");
    expect(CLASSES.FLEXLAYOUT__DRAG_RECT).toBe("flexlayout__drag_rect");
    expect(CLASSES.FLEXLAYOUT__EDGE_RECT).toBe("flexlayout__edge_rect");
  });

  it("should have border-related classes", () => {
    expect(CLASSES.FLEXLAYOUT__BORDER_TAB_CONTENTS).toBe("flexlayout__border_tab_contents");
    expect(CLASSES.FLEXLAYOUT__BORDER_BUTTON).toBe("flexlayout__border_button");
    expect(CLASSES.FLEXLAYOUT__BORDER_TOOLBAR).toBe("flexlayout__border_toolbar");
  });

  it("should have tabset-related classes", () => {
    expect(CLASSES.FLEXLAYOUT__TABSET_CONTAINER).toBe("flexlayout__tabset_container");
    expect(CLASSES.FLEXLAYOUT__TABSET_HEADER).toBe("flexlayout__tabset_header");
    expect(CLASSES.FLEXLAYOUT__TABSET_CONTENT).toBe("flexlayout__tabset_content");
  });

  it("should have tab-related classes", () => {
    expect(CLASSES.FLEXLAYOUT__TAB_BUTTON).toBe("flexlayout__tab_button");
    expect(CLASSES.FLEXLAYOUT__TAB_TOOLBAR).toBe("flexlayout__tab_toolbar");
  });
});
