import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import { TabSetNode } from "../model/TabSetNode";
import { TabNode } from "../model/TabNode";
import type { IJsonModel } from "../types";

const dragDockFixture: IJsonModel = {
  global: {
    borderEnableDock: true,
    tabEnableDrag: true,
    borderEnableDrop: true,
  },
  borders: [
    {
      type: "border",
      location: "left",
      selected: 0,
      size: 200,
      dockState: "expanded",
      visibleTabs: [0, 1],
      children: [
        { type: "tab", name: "Explorer", component: "text" },
        { type: "tab", name: "Search", component: "text" },
      ],
    },
    {
      type: "border",
      location: "bottom",
      selected: 0,
      size: 200,
      dockState: "expanded",
      children: [
        { type: "tab", name: "Terminal", component: "text" },
        { type: "tab", name: "Output", component: "text" },
      ],
    },
    {
      type: "border",
      location: "right",
      selected: -1,
      size: 200,
      dockState: "expanded",
      children: [],
    },
    {
      type: "border",
      location: "top",
      selected: -1,
      size: 150,
      dockState: "expanded",
      children: [],
    },
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        weight: 100,
        children: [
          { type: "tab", name: "Main", component: "text" },
          { type: "tab", name: "Document", component: "text" },
        ],
      },
    ],
  },
};

function getBorder(model: Model, location: string): BorderNode {
  const border = model.getBorderSet().getBorder(location as any);
  if (!border) throw new Error(`No border at ${location}`);
  return border;
}

function getFirstTabSet(model: Model): TabSetNode {
  const root = model.getRoot()!;
  return root.getChildren()[0] as TabSetNode;
}

describe("Auto-select after drag-out", () => {
  it("drag active tab from border to tabset auto-selects next", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    expect(left.getSelected()).toBe(0);
    expect(left.getChildren().length).toBe(2);

    const explorerTab = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(explorerTab.getId(), tabset.getId(), "center", -1));

    expect(left.getChildren().length).toBe(1);
    expect(left.getSelected()).toBe(0);
    expect((left.getChildren()[0] as TabNode).getName()).toBe("Search");
  });

  it("drag active tab from border to other border auto-selects next", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const right = getBorder(model, "right");

    expect(left.getSelected()).toBe(0);

    const explorerTab = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(explorerTab.getId(), right.getId(), "center", -1));

    expect(left.getChildren().length).toBe(1);
    expect(left.getSelected()).toBe(0);
    expect((left.getChildren()[0] as TabNode).getName()).toBe("Search");
  });

  it("drag active tab from border to row auto-selects next", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");

    expect(left.getSelected()).toBe(0);

    const explorerTab = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(explorerTab.getId(), model.getRoot()!.getId(), "left", -1));

    expect(left.getChildren().length).toBe(1);
    expect(left.getSelected()).toBe(0);
    expect((left.getChildren()[0] as TabNode).getName()).toBe("Search");
  });

  it("drag inactive tab from border preserves selection", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    expect(left.getSelected()).toBe(0);

    const searchTab = left.getChildren()[1] as TabNode;
    model.doAction(Action.moveNode(searchTab.getId(), tabset.getId(), "center", -1));

    expect(left.getChildren().length).toBe(1);
    expect(left.getSelected()).toBe(0);
    expect((left.getChildren()[0] as TabNode).getName()).toBe("Explorer");
  });

  it("drag only tab from border sets selected to -1", () => {
    const model = Model.fromJson(dragDockFixture);
    const bottom = getBorder(model, "bottom");
    const tabset = getFirstTabSet(model);

    const terminalTab = bottom.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(terminalTab.getId(), tabset.getId(), "center", -1));

    const outputTab = bottom.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(outputTab.getId(), tabset.getId(), "center", -1));

    expect(bottom.getChildren().length).toBe(0);
    expect(bottom.getSelected()).toBe(-1);
  });
});

describe("visibleTabs integrity", () => {
  it("drag out adjusts visibleTabs indices", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    expect(left.getVisibleTabs()).toEqual([0, 1]);

    const explorerTab = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(explorerTab.getId(), tabset.getId(), "center", -1));

    expect(left.getVisibleTabs()).toEqual([0]);
  });

  it("drag out removes dragged index from visibleTabs", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    const searchTab = left.getChildren()[1] as TabNode;
    model.doAction(Action.moveNode(searchTab.getId(), tabset.getId(), "center", -1));

    expect(left.getVisibleTabs()).toEqual([0]);
    expect(left.getChildren().length).toBe(1);
  });

  it("no visibleTabs index exceeds children length after drag", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    const explorerTab = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(explorerTab.getId(), tabset.getId(), "center", -1));

    for (const idx of left.getVisibleTabs()) {
      expect(idx).toBeLessThan(left.getChildren().length);
    }
  });

  it("drag all visible tabs clears visibleTabs", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    const tab0 = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab0.getId(), tabset.getId(), "center", -1));

    const tab1 = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab1.getId(), tabset.getId(), "center", -1));

    expect(left.getVisibleTabs()).toEqual([]);
    expect(left.getChildren().length).toBe(0);
  });
});

describe("Auto-hide for empty borders", () => {
  it("drag last tab sets dockState to hidden", () => {
    const model = Model.fromJson(dragDockFixture);
    const bottom = getBorder(model, "bottom");
    const tabset = getFirstTabSet(model);

    const tab0 = bottom.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab0.getId(), tabset.getId(), "center", -1));

    const tab1 = bottom.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab1.getId(), tabset.getId(), "center", -1));

    expect(bottom.getChildren().length).toBe(0);
    expect(bottom.getDockState()).toBe("hidden");
  });

  it("drag non-last tab preserves dockState expanded", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    const tab0 = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab0.getId(), tabset.getId(), "center", -1));

    expect(left.getChildren().length).toBe(1);
    expect(left.getDockState()).toBe("expanded");
  });

  it("auto-hidden border has zero children", () => {
    const model = Model.fromJson(dragDockFixture);
    const bottom = getBorder(model, "bottom");
    const tabset = getFirstTabSet(model);

    const tab0 = bottom.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab0.getId(), tabset.getId(), "center", -1));
    const tab1 = bottom.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab1.getId(), tabset.getId(), "center", -1));

    expect(bottom.getDockState()).toBe("hidden");
    expect(bottom.getChildren().length).toBe(0);
    expect(bottom.getSelected()).toBe(-1);
  });
});

describe("Drop into border", () => {
  it("drop into expanded border selects tab", () => {
    const model = Model.fromJson(dragDockFixture);
    const right = getBorder(model, "right");
    const tabset = getFirstTabSet(model);

    expect(right.getChildren().length).toBe(0);

    const mainTab = tabset.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(mainTab.getId(), right.getId(), "center", -1, true));

    expect(right.getChildren().length).toBe(1);
    expect(right.getSelected()).toBe(0);
  });

  it("drop into collapsed border auto-expands", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    model.doAction(Action.setDockState(left.getId(), "collapsed"));
    expect(left.getDockState()).toBe("collapsed");

    const mainTab = tabset.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(mainTab.getId(), left.getId(), "center", -1));

    expect(left.getDockState()).toBe("expanded");
    expect(left.getChildren().length).toBe(3);
  });

  it("drop into hidden border auto-expands", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    model.doAction(Action.setDockState(left.getId(), "hidden"));
    expect(left.getDockState()).toBe("hidden");

    const mainTab = tabset.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(mainTab.getId(), left.getId(), "center", -1));

    expect(left.getDockState()).toBe("expanded");
    expect(left.getChildren().length).toBe(3);
  });

  it("drop preserves existing tabs in target border", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    const mainTab = tabset.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(mainTab.getId(), left.getId(), "center", -1));

    expect(left.getChildren().length).toBe(3);
    expect((left.getChildren()[0] as TabNode).getName()).toBe("Explorer");
    expect((left.getChildren()[1] as TabNode).getName()).toBe("Search");
    expect((left.getChildren()[2] as TabNode).getName()).toBe("Main");
  });
});

describe("Round-trip integrity", () => {
  it("model toJson valid after drag out", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    const tab0 = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab0.getId(), tabset.getId(), "center", -1));

    const json = model.toJson();
    const restored = Model.fromJson(json);
    const restoredLeft = getBorder(restored, "left");

    expect(restoredLeft.getChildren().length).toBe(1);
    expect(restoredLeft.getSelected()).toBe(0);
    expect((restoredLeft.getChildren()[0] as TabNode).getName()).toBe("Search");
  });

  it("model toJson valid after drag between borders", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const right = getBorder(model, "right");

    const tab0 = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab0.getId(), right.getId(), "center", -1));

    const json = model.toJson();
    const restored = Model.fromJson(json);
    const restoredLeft = getBorder(restored, "left");
    const restoredRight = getBorder(restored, "right");

    expect(restoredLeft.getChildren().length).toBe(1);
    expect(restoredRight.getChildren().length).toBe(1);
    expect((restoredRight.getChildren()[0] as TabNode).getName()).toBe("Explorer");
  });

  it("no stale indices in serialized visibleTabs", () => {
    const model = Model.fromJson(dragDockFixture);
    const left = getBorder(model, "left");
    const tabset = getFirstTabSet(model);

    const tab0 = left.getChildren()[0] as TabNode;
    model.doAction(Action.moveNode(tab0.getId(), tabset.getId(), "center", -1));

    const json = model.toJson();
    const leftJson = json.borders?.find((b: any) => b.location === "left");

    if (leftJson?.visibleTabs) {
      for (const idx of leftJson.visibleTabs) {
        expect(idx).toBeLessThan(leftJson.children!.length);
        expect(idx).toBeGreaterThanOrEqual(0);
      }
    }
  });
});
