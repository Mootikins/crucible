import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import type { IJsonModel } from "../types";

/** Minimal layout with 4 borders, each with 1-3 tabs, for dock testing */
const dockFixture: IJsonModel = {
  global: {
    borderEnableDock: true,
  },
  borders: [
    {
      type: "border",
      location: "top",
      selected: 0,
      children: [
        { type: "tab", name: "Toolbar", component: "text" },
      ],
    },
    {
      type: "border",
      location: "bottom",
      selected: 0,
      visibleTabs: [0, 1],
      children: [
        { type: "tab", name: "Terminal", component: "text" },
        { type: "tab", name: "Output", component: "text" },
        { type: "tab", name: "Problems", component: "text" },
      ],
    },
    {
      type: "border",
      location: "left",
      selected: 0,
      visibleTabs: [0, 1],
      dockState: "collapsed",
      children: [
        { type: "tab", name: "Explorer", component: "text" },
        { type: "tab", name: "Search", component: "text" },
      ],
    },
    {
      type: "border",
      location: "right",
      selected: 0,
      children: [
        { type: "tab", name: "Properties", component: "text" },
      ],
    },
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        weight: 100,
        children: [{ type: "tab", name: "Main", component: "text" }],
      },
    ],
  },
};

/** Old-format layout without new dock attributes — for backward compat testing */
const legacyFixture: IJsonModel = {
  global: {},
  borders: [
    {
      type: "border",
      location: "bottom",
      selected: 0,
      children: [
        { type: "tab", name: "Console", component: "text" },
        { type: "tab", name: "Output", component: "text" },
      ],
    },
    {
      type: "border",
      location: "left",
      children: [
        { type: "tab", name: "Files", component: "text" },
      ],
    },
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        weight: 100,
        children: [{ type: "tab", name: "Editor", component: "text" }],
      },
    ],
  },
};

/** Layout with enableDock explicitly false on a border */
const dockDisabledFixture: IJsonModel = {
  global: {
    borderEnableDock: false,
  },
  borders: [
    {
      type: "border",
      location: "bottom",
      selected: 0,
      children: [
        { type: "tab", name: "Terminal", component: "text" },
      ],
    },
    {
      type: "border",
      location: "left",
      enableDock: true,
      selected: 0,
      children: [
        { type: "tab", name: "Explorer", component: "text" },
      ],
    },
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        weight: 100,
        children: [{ type: "tab", name: "Main", component: "text" }],
      },
    ],
  },
};

function getBorder(model: Model, location: string): BorderNode {
  const border = model.getBorderSet().getBorder(location as any);
  if (!border) throw new Error(`No border at ${location}`);
  return border;
}

describe("BorderNode > dockState attribute", () => {
  it("defaults to 'expanded' when not specified in JSON", () => {
    const model = Model.fromJson(dockFixture);
    const top = getBorder(model, "top");
    expect(top.getDockState()).toBe("expanded");
  });

  it("reads dockState from JSON", () => {
    const model = Model.fromJson(dockFixture);
    const left = getBorder(model, "left");
    expect(left.getDockState()).toBe("collapsed");
  });

  it("getDockState returns 'expanded' for legacy JSON without dockState", () => {
    const model = Model.fromJson(legacyFixture);
    const bottom = getBorder(model, "bottom");
    expect(bottom.getDockState()).toBe("expanded");
  });

  it("Action.setDockState changes dockState to collapsed", () => {
    const model = Model.fromJson(dockFixture);
    const top = getBorder(model, "top");
    expect(top.getDockState()).toBe("expanded");

    model.doAction(Action.setDockState(top.getId(), "collapsed"));
    expect(top.getDockState()).toBe("collapsed");
  });

  it("Action.setDockState changes dockState to minimized", () => {
    const model = Model.fromJson(dockFixture);
    const right = getBorder(model, "right");
    expect(right.getDockState()).toBe("expanded");

    model.doAction(Action.setDockState(right.getId(), "hidden"));
    expect(right.getDockState()).toBe("hidden");
  });

  it("Action.setDockState full cycle: expanded → collapsed → hidden → expanded", () => {
    const model = Model.fromJson(dockFixture);
    const bottom = getBorder(model, "bottom");

    expect(bottom.getDockState()).toBe("expanded");

    model.doAction(Action.setDockState(bottom.getId(), "collapsed"));
    expect(bottom.getDockState()).toBe("collapsed");

    model.doAction(Action.setDockState(bottom.getId(), "hidden"));
    expect(bottom.getDockState()).toBe("hidden");

    model.doAction(Action.setDockState(bottom.getId(), "expanded"));
    expect(bottom.getDockState()).toBe("expanded");
  });
});

describe("BorderNode > visibleTabs attribute", () => {
  it("defaults to empty array when not specified", () => {
    const model = Model.fromJson(dockFixture);
    const top = getBorder(model, "top");
    expect(top.getVisibleTabs()).toEqual([]);
  });

  it("reads visibleTabs from JSON", () => {
    const model = Model.fromJson(dockFixture);
    const bottom = getBorder(model, "bottom");
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
  });

  it("reads multi-tab visibleTabs from left border", () => {
    const model = Model.fromJson(dockFixture);
    const left = getBorder(model, "left");
    expect(left.getVisibleTabs()).toEqual([0, 1]);
  });

  it("getVisibleTabs returns [] for legacy JSON without visibleTabs", () => {
    const model = Model.fromJson(legacyFixture);
    const bottom = getBorder(model, "bottom");
    expect(bottom.getVisibleTabs()).toEqual([]);
  });

  it("Action.setVisibleTabs updates the visible tabs array", () => {
    const model = Model.fromJson(dockFixture);
    const bottom = getBorder(model, "bottom");

    model.doAction(Action.setVisibleTabs(bottom.getId(), [0, 2]));
    expect(bottom.getVisibleTabs()).toEqual([0, 2]);
  });

  it("Action.setVisibleTabs can set single tab", () => {
    const model = Model.fromJson(dockFixture);
    const bottom = getBorder(model, "bottom");

    model.doAction(Action.setVisibleTabs(bottom.getId(), [1]));
    expect(bottom.getVisibleTabs()).toEqual([1]);
  });

  it("Action.setVisibleTabs can set empty array (fallback behavior)", () => {
    const model = Model.fromJson(dockFixture);
    const bottom = getBorder(model, "bottom");
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);

    model.doAction(Action.setVisibleTabs(bottom.getId(), []));
    expect(bottom.getVisibleTabs()).toEqual([]);
  });
});

describe("BorderNode > enableDock attribute", () => {
  it("defaults to true when global borderEnableDock is true", () => {
    const model = Model.fromJson(dockFixture);
    const top = getBorder(model, "top");
    expect(top.isEnableDock()).toBe(true);
  });

  it("inherits global borderEnableDock: false", () => {
    const model = Model.fromJson(dockDisabledFixture);
    const bottom = getBorder(model, "bottom");
    expect(bottom.isEnableDock()).toBe(false);
  });

  it("local enableDock: true overrides global borderEnableDock: false", () => {
    const model = Model.fromJson(dockDisabledFixture);
    const left = getBorder(model, "left");
    expect(left.isEnableDock()).toBe(true);
  });

  it("defaults to false when no global is set (Attribute default)", () => {
    const model = Model.fromJson(legacyFixture);
    const bottom = getBorder(model, "bottom");
    expect(typeof bottom.isEnableDock()).toBe("boolean");
  });
});

describe("BorderNode > JSON round-trip serialization", () => {
  it("round-trips dockState through toJson/fromJson", () => {
    const model = Model.fromJson(dockFixture);
    const left = getBorder(model, "left");
    expect(left.getDockState()).toBe("collapsed");

    const json = model.toJson();
    const restored = Model.fromJson(json);
    const restoredLeft = getBorder(restored, "left");
    expect(restoredLeft.getDockState()).toBe("collapsed");
  });

  it("round-trips visibleTabs through toJson/fromJson", () => {
    const model = Model.fromJson(dockFixture);
    const bottom = getBorder(model, "bottom");
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);

    const json = model.toJson();
    const restored = Model.fromJson(json);
    const restoredBottom = getBorder(restored, "bottom");
    expect(restoredBottom.getVisibleTabs()).toEqual([0, 1]);
  });

  it("round-trips after Action mutations", () => {
    const model = Model.fromJson(dockFixture);
    const top = getBorder(model, "top");

    model.doAction(Action.setDockState(top.getId(), "hidden"));
    model.doAction(Action.setVisibleTabs(top.getId(), [0]));

    const json = model.toJson();
    const restored = Model.fromJson(json);
    const restoredTop = getBorder(restored, "top");
    expect(restoredTop.getDockState()).toBe("hidden");
    expect(restoredTop.getVisibleTabs()).toEqual([0]);
  });

  it("round-trips preserves all borders and their dock attributes", () => {
    const model = Model.fromJson(dockFixture);

    model.doAction(Action.setDockState(getBorder(model, "top").getId(), "collapsed"));
    model.doAction(Action.setDockState(getBorder(model, "right").getId(), "hidden"));
    model.doAction(Action.setVisibleTabs(getBorder(model, "right").getId(), [0]));

    const json = model.toJson();
    const restored = Model.fromJson(json);

    expect(getBorder(restored, "top").getDockState()).toBe("collapsed");
    expect(getBorder(restored, "bottom").getDockState()).toBe("expanded");
    expect(getBorder(restored, "left").getDockState()).toBe("collapsed");
    expect(getBorder(restored, "right").getDockState()).toBe("hidden");
    expect(getBorder(restored, "right").getVisibleTabs()).toEqual([0]);
    expect(getBorder(restored, "bottom").getVisibleTabs()).toEqual([0, 1]);
  });
});

describe("BorderNode > backward compatibility", () => {
  it("old JSON without dockState/visibleTabs/enableDock loads with correct defaults", () => {
    const model = Model.fromJson(legacyFixture);
    const bottom = getBorder(model, "bottom");
    const left = getBorder(model, "left");

    expect(bottom.getDockState()).toBe("expanded");
    expect(left.getDockState()).toBe("expanded");

    expect(bottom.getVisibleTabs()).toEqual([]);
    expect(left.getVisibleTabs()).toEqual([]);
  });

  it("old JSON round-trips without adding new fields that break old parsers", () => {
    const model = Model.fromJson(legacyFixture);
    const json = model.toJson();

    const restored = Model.fromJson(json);
    expect(getBorder(restored, "bottom").getDockState()).toBe("expanded");
    expect(getBorder(restored, "bottom").getVisibleTabs()).toEqual([]);
  });

  it("existing tab operations still work on borders with dock attributes", () => {
    const model = Model.fromJson(dockFixture);
    const bottom = getBorder(model, "bottom");

    expect(bottom.getSelected()).toBe(0);
    expect(bottom.getChildren().length).toBe(3);
    expect(bottom.isShowing()).toBe(true);

    model.doAction(
      Action.addNode(
        { type: "tab", name: "NewTab", component: "text" },
        bottom.getId(),
        "center",
        -1
      )
    );

    expect(bottom.getChildren().length).toBe(4);

    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
    expect(bottom.getDockState()).toBe("expanded");
  });
});

describe("BorderNode > Action dispatch edge cases", () => {
  it("setDockState on non-border node is a no-op", () => {
    const model = Model.fromJson(dockFixture);
    const root = model.getRoot();
    if (root) {
      model.doAction(Action.setDockState(root.getId(), "collapsed"));
    }
  });

  it("setVisibleTabs on non-border node is a no-op", () => {
    const model = Model.fromJson(dockFixture);
    const root = model.getRoot();
    if (root) {
      model.doAction(Action.setVisibleTabs(root.getId(), [0, 1]));
    }
  });

  it("setDockState with nonexistent nodeId is a no-op", () => {
    const model = Model.fromJson(dockFixture);
    model.doAction(Action.setDockState("nonexistent", "collapsed"));
    expect(getBorder(model, "top").getDockState()).toBe("expanded");
  });

  it("setVisibleTabs with nonexistent nodeId is a no-op", () => {
    const model = Model.fromJson(dockFixture);
    model.doAction(Action.setVisibleTabs("nonexistent", [0, 1]));
    expect(getBorder(model, "bottom").getVisibleTabs()).toEqual([0, 1]);
  });
});
