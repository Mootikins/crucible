import { describe, it, expect } from "vitest";
import { twoTabs, withBorders, threeTabs } from "./fixtures";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { TabNode } from "../model/TabNode";
import type { IJsonModel } from "../types";

function getFirstTab(model: Model): TabNode {
  const root = model.getRoot()!;
  let tab: TabNode | undefined;
  root.forEachNode((node) => {
    if (!tab && node instanceof TabNode) tab = node;
  }, 0);
  return tab!;
}

function getTabByName(model: Model, name: string): TabNode | undefined {
  let found: TabNode | undefined;
  const root = model.getRoot()!;
  root.forEachNode((node) => {
    if (node instanceof TabNode && (node as TabNode).getName() === name) {
      found = node as TabNode;
    }
  }, 0);
  if (!found) {
    for (const loc of ["top", "bottom", "left", "right"] as const) {
      const border = model.getBorderSet().getBorder(loc);
      if (border) {
        for (const child of border.getChildren()) {
          if (child instanceof TabNode && (child as TabNode).getName() === name) {
            found = child as TabNode;
          }
        }
      }
    }
  }
  return found;
}

describe("pin > tab pinning", () => {
  it("pin tab sets isPinned to true", () => {
    const model = Model.fromJson(twoTabs);
    const tab = getFirstTab(model);
    expect(tab.isPinned()).toBe(false);

    model.doAction(Action.pinTab(tab.getId()));

    expect(tab.isPinned()).toBe(true);
  });

  it("unpin tab sets isPinned to false", () => {
    const model = Model.fromJson(twoTabs);
    const tab = getFirstTab(model);
    model.doAction(Action.pinTab(tab.getId()));
    expect(tab.isPinned()).toBe(true);

    model.doAction(Action.unpinTab(tab.getId()));

    expect(tab.isPinned()).toBe(false);
  });

  it("pinning nonexistent tab is a no-op", () => {
    const model = Model.fromJson(twoTabs);
    model.doAction(Action.pinTab("nonexistent"));
  });
});

describe("pin > pinned tab rejects move", () => {
  it("pinned tab cannot be moved to another tabset", () => {
    const model = Model.fromJson(threeTabs);
    const tabOne = getTabByName(model, "One")!;
    const tabTwo = getTabByName(model, "Two")!;
    const ts1Parent = tabTwo.getParent()!;

    model.doAction(Action.pinTab(tabOne.getId()));

    model.doAction(
      Action.moveNode(tabOne.getId(), ts1Parent.getId(), "center", -1)
    );

    expect(tabOne.getParent()!.getId()).not.toBe(ts1Parent.getId());
    expect(tabOne.isPinned()).toBe(true);
  });

  it("unpinned tab can be moved normally", () => {
    const model = Model.fromJson(threeTabs);
    const tabOne = getTabByName(model, "One")!;
    const tabTwo = getTabByName(model, "Two")!;
    const ts1Parent = tabTwo.getParent()!;
    model.doAction(
      Action.moveNode(tabOne.getId(), ts1Parent.getId(), "center", -1)
    );

    expect(tabOne.getParent()!.getId()).toBe(ts1Parent.getId());
  });
});

describe("pin > pinned tab rejects close/delete", () => {
  it("pinned tab cannot be deleted via DELETE_TAB", () => {
    const model = Model.fromJson(twoTabs);
    const tab = getFirstTab(model);
    const tabId = tab.getId();
    const parentId = tab.getParent()!.getId();
    model.doAction(Action.pinTab(tabId));

    model.doAction(Action.deleteTab(tabId));

    const parent = model.getNodeById(parentId)!;
    const childIds = parent.getChildren().map(c => c.getId());
    expect(childIds).toContain(tabId);
    expect(tab.isPinned()).toBe(true);
  });

  it("unpinned tab can be deleted", () => {
    const model = Model.fromJson(threeTabs);
    const tab = getTabByName(model, "One")!;
    const tabId = tab.getId();
    const parentId = tab.getParent()!.getId();

    model.doAction(Action.deleteTab(tabId));

    const parent = model.getNodeById(parentId);
    const childIds = parent ? parent.getChildren().map(c => c.getId()) : [];
    expect(childIds).not.toContain(tabId);
  });
});

describe("pin > pinned tab survives DELETE_TABSET", () => {
  it("DELETE_TABSET preserves pinned tabs", () => {
    const model = Model.fromJson(threeTabs);
    const tabOne = getTabByName(model, "One")!;
    const tabOneId = tabOne.getId();
    const tsParent = tabOne.getParent()!;
    const tsId = tsParent.getId();

    model.doAction(Action.pinTab(tabOneId));
    model.doAction(Action.deleteTabset(tsId));

    expect(model.getNodeById(tabOneId)).toBeDefined();
    expect(tabOne.isPinned()).toBe(true);
  });
});

describe("pin > border pinning", () => {
  it("pin border sets isPinned to true", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;
    expect(topBorder.isPinned()).toBe(false);

    model.doAction(Action.pinBorder(topBorder.getId()));

    expect(topBorder.isPinned()).toBe(true);
  });

  it("unpin border sets isPinned to false", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;
    model.doAction(Action.pinBorder(topBorder.getId()));

    model.doAction(Action.unpinBorder(topBorder.getId()));

    expect(topBorder.isPinned()).toBe(false);
  });

  it("pinning nonexistent border is a no-op", () => {
    const model = Model.fromJson(withBorders);
    model.doAction(Action.pinBorder("nonexistent"));
  });
});

describe("pin > serialization", () => {
  it("pinned tab survives JSON round-trip", () => {
    const model = Model.fromJson(twoTabs);
    const tab = getFirstTab(model);
    model.doAction(Action.pinTab(tab.getId()));

    const json = model.toJson();
    const restored = Model.fromJson(json);
    const restoredTab = getFirstTab(restored);

    expect(restoredTab.isPinned()).toBe(true);
  });

  it("unpinned tab defaults to false in old JSON", () => {
    const model = Model.fromJson(twoTabs);
    const tab = getFirstTab(model);
    expect(tab.isPinned()).toBe(false);
  });

  it("pinned border survives JSON round-trip", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;
    model.doAction(Action.pinBorder(topBorder.getId()));

    const json = model.toJson();
    const restored = Model.fromJson(json);
    const restoredBorder = restored.getBorderSet().getBorder("top")!;

    expect(restoredBorder.isPinned()).toBe(true);
  });

  it("pinned=true only written when true (backward compat)", () => {
    const model = Model.fromJson(twoTabs);
    const json = model.toJson();
    const layoutJson = JSON.stringify(json);
    expect(layoutJson).not.toContain('"pinned"');
  });

  it("tab loaded from JSON with pinned=true has isPinned true", () => {
    const jsonWithPinned: IJsonModel = {
      global: {},
      borders: [],
      layout: {
        type: "row",
        weight: 100,
        children: [
          {
            type: "tabset",
            weight: 100,
            children: [
              {
                type: "tab",
                name: "Pinned",
                component: "text",
                pinned: true,
              } as any,
            ],
          },
        ],
      } as any,
    };

    const model = Model.fromJson(jsonWithPinned);
    const tab = getFirstTab(model);
    expect(tab.isPinned()).toBe(true);
  });
});
