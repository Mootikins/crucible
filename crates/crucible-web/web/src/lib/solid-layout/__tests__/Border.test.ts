import { describe, it, expect, afterEach } from "vitest";
import { createRoot } from "solid-js";
import { Model } from "../../flexlayout/model/Model";
import { Action } from "../../flexlayout/model/Action";
import type { IJsonModel, IJsonBorderNode } from "../../flexlayout/types";
import { CLASSES } from "../../flexlayout/core/Types";
import { createLayoutBridge, type LayoutBridge } from "../bridge";

import { isHorizontalBorder, isStartEdge } from "../components/Border";
import {
  getDockIcon,
  buildBorderStripClass,
  buildBorderButtonClass,
  BORDER_BAR_SIZE,
} from "../components/BorderStrip";
import {
  computeNestingOrder,
  isVisibleBorder,
  computeInsets,
} from "../components/BorderLayout";

const identity = (cls: string) => cls;

function makeBorder(
  location: string,
  overrides: Partial<IJsonBorderNode> = {},
): IJsonBorderNode {
  return {
    type: "border",
    id: `border_${location}`,
    location,
    children: [],
    dockState: "expanded",
    selected: -1,
    priority: 0,
    enableDock: true,
    ...overrides,
  } as IJsonBorderNode;
}

const fourBorderLayout: IJsonModel = {
  global: {},
  borders: [
    makeBorder("top"),
    makeBorder("bottom"),
    makeBorder("left", {
      children: [
        { type: "tab", id: "tab-left-1", name: "Explorer", component: "text" },
      ],
    }),
    makeBorder("right", {
      children: [
        { type: "tab", id: "tab-right-1", name: "Outline", component: "text" },
      ],
    }),
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 100,
        children: [
          { type: "tab", id: "tab1", name: "Main", component: "text" },
        ],
      },
    ],
  },
};

function makeModel(json: IJsonModel = fourBorderLayout): Model {
  return Model.fromJson(json);
}

describe("isHorizontalBorder", () => {
  it("returns true for top", () => {
    expect(isHorizontalBorder("top")).toBe(true);
  });

  it("returns true for bottom", () => {
    expect(isHorizontalBorder("bottom")).toBe(true);
  });

  it("returns false for left", () => {
    expect(isHorizontalBorder("left")).toBe(false);
  });

  it("returns false for right", () => {
    expect(isHorizontalBorder("right")).toBe(false);
  });
});

describe("isStartEdge", () => {
  it("returns true for left", () => {
    expect(isStartEdge("left")).toBe(true);
  });

  it("returns true for top", () => {
    expect(isStartEdge("top")).toBe(true);
  });

  it("returns false for right", () => {
    expect(isStartEdge("right")).toBe(false);
  });

  it("returns false for bottom", () => {
    expect(isStartEdge("bottom")).toBe(false);
  });
});

describe("getDockIcon", () => {
  it("collapsed left shows rightward arrow", () => {
    expect(getDockIcon("collapsed", "left")).toBe("▶");
  });

  it("collapsed right shows leftward arrow", () => {
    expect(getDockIcon("collapsed", "right")).toBe("◀");
  });

  it("collapsed top shows downward arrow", () => {
    expect(getDockIcon("collapsed", "top")).toBe("▼");
  });

  it("collapsed bottom shows upward arrow", () => {
    expect(getDockIcon("collapsed", "bottom")).toBe("▲");
  });

  it("expanded left shows leftward arrow", () => {
    expect(getDockIcon("expanded", "left")).toBe("◀");
  });

  it("expanded right shows rightward arrow", () => {
    expect(getDockIcon("expanded", "right")).toBe("▶");
  });

  it("expanded top shows upward arrow", () => {
    expect(getDockIcon("expanded", "top")).toBe("▲");
  });

  it("expanded bottom shows downward arrow", () => {
    expect(getDockIcon("expanded", "bottom")).toBe("▼");
  });
});

describe("buildBorderStripClass", () => {
  it("includes base border class and location suffix", () => {
    const cls = buildBorderStripClass("left", false, identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__BORDER);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__BORDER_ + "left");
  });

  it("adds collapsed class when collapsed", () => {
    const cls = buildBorderStripClass("top", true, identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__BORDER__COLLAPSED);
  });

  it("does not add collapsed class when not collapsed", () => {
    const cls = buildBorderStripClass("top", false, identity);
    expect(cls).not.toContain(CLASSES.FLEXLAYOUT__BORDER__COLLAPSED);
  });

  it("appends custom class name", () => {
    const cls = buildBorderStripClass("right", false, identity, "my-custom");
    expect(cls).toContain("my-custom");
  });

  it("applies classNameMapper to classes", () => {
    const mapper = (cls: string) => `pfx-${cls}`;
    const cls = buildBorderStripClass("bottom", true, mapper);
    expect(cls).toContain(`pfx-${CLASSES.FLEXLAYOUT__BORDER}`);
    expect(cls).toContain(`pfx-${CLASSES.FLEXLAYOUT__BORDER__COLLAPSED}`);
  });
});

describe("buildBorderButtonClass", () => {
  it("includes base button class and location suffix", () => {
    const cls = buildBorderButtonClass("left", false, identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__BORDER_BUTTON);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__BORDER_BUTTON_ + "left");
  });

  it("adds selected class when selected", () => {
    const cls = buildBorderButtonClass("left", true, identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__BORDER_BUTTON__SELECTED);
    expect(cls).not.toContain(CLASSES.FLEXLAYOUT__BORDER_BUTTON__UNSELECTED);
  });

  it("adds unselected class when not selected", () => {
    const cls = buildBorderButtonClass("left", false, identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__BORDER_BUTTON__UNSELECTED);
    expect(cls).not.toContain(CLASSES.FLEXLAYOUT__BORDER_BUTTON__SELECTED);
  });

  it("appends custom class name", () => {
    const cls = buildBorderButtonClass("right", false, identity, "tab-cls");
    expect(cls).toContain("tab-cls");
  });
});

describe("computeNestingOrder", () => {
  it("sorts by priority descending (higher = outer)", () => {
    const borders = [
      makeBorder("left", { priority: 1 }),
      makeBorder("right", { priority: 3 }),
      makeBorder("top", { priority: 2 }),
    ];
    const sorted = computeNestingOrder(borders);
    expect(sorted[0].location).toBe("right");
    expect(sorted[1].location).toBe("top");
    expect(sorted[2].location).toBe("left");
  });

  it("breaks priority ties by location order: top, bottom, left, right", () => {
    const borders = [
      makeBorder("right"),
      makeBorder("left"),
      makeBorder("bottom"),
      makeBorder("top"),
    ];
    const sorted = computeNestingOrder(borders);
    expect(sorted.map((b) => b.location)).toEqual([
      "top",
      "bottom",
      "left",
      "right",
    ]);
  });

  it("higher priority overrides location order", () => {
    const borders = [
      makeBorder("top", { priority: 0 }),
      makeBorder("left", { priority: 5 }),
    ];
    const sorted = computeNestingOrder(borders);
    expect(sorted[0].location).toBe("left");
    expect(sorted[1].location).toBe("top");
  });

  it("returns empty array for empty input", () => {
    expect(computeNestingOrder([])).toEqual([]);
  });

  it("single border returns unchanged", () => {
    const borders = [makeBorder("left")];
    const sorted = computeNestingOrder(borders);
    expect(sorted).toHaveLength(1);
    expect(sorted[0].location).toBe("left");
  });
});

describe("isVisibleBorder", () => {
  it("returns true for a visible border with children", () => {
    const border = makeBorder("left", {
      children: [{ type: "tab", id: "t1", name: "X", component: "text" }],
    });
    expect(isVisibleBorder(border)).toBe(true);
  });

  it("returns false when show is false", () => {
    const border = makeBorder("left", { show: false } as any);
    expect(isVisibleBorder(border)).toBe(false);
  });

  it("returns false when autoHide enabled and no children", () => {
    const border = makeBorder("left", {
      enableAutoHide: true,
      children: [],
    } as any);
    expect(isVisibleBorder(border)).toBe(false);
  });

  it("returns true when autoHide enabled but has children", () => {
    const border = makeBorder("left", {
      enableAutoHide: true,
      children: [{ type: "tab", id: "t1", name: "X", component: "text" }],
    } as any);
    expect(isVisibleBorder(border)).toBe(true);
  });

  it("returns true when no children but autoHide not enabled", () => {
    const border = makeBorder("left", { children: [] });
    expect(isVisibleBorder(border)).toBe(true);
  });
});

describe("computeInsets", () => {
  it("returns zero insets when no borders", () => {
    expect(computeInsets([])).toEqual({ top: 0, right: 0, bottom: 0, left: 0 });
  });

  it("reserves space for collapsed borders", () => {
    const borders = [
      makeBorder("left", { dockState: "collapsed" }),
      makeBorder("top", { dockState: "collapsed" }),
    ];
    const insets = computeInsets(borders);
    expect(insets.left).toBe(BORDER_BAR_SIZE);
    expect(insets.top).toBe(BORDER_BAR_SIZE);
    expect(insets.right).toBe(0);
    expect(insets.bottom).toBe(0);
  });

  it("reserves space for expanded-unselected borders", () => {
    const borders = [
      makeBorder("right", { dockState: "expanded", selected: -1 }),
    ];
    const insets = computeInsets(borders);
    expect(insets.right).toBe(BORDER_BAR_SIZE);
  });

  it("does NOT reserve space for expanded-selected borders", () => {
    const borders = [
      makeBorder("bottom", { dockState: "expanded", selected: 0 }),
    ];
    const insets = computeInsets(borders);
    expect(insets.bottom).toBe(0);
  });

  it("reserves space for all 4 collapsed borders", () => {
    const borders = [
      makeBorder("top", { dockState: "collapsed" }),
      makeBorder("right", { dockState: "collapsed" }),
      makeBorder("bottom", { dockState: "collapsed" }),
      makeBorder("left", { dockState: "collapsed" }),
    ];
    const insets = computeInsets(borders);
    expect(insets.top).toBe(BORDER_BAR_SIZE);
    expect(insets.right).toBe(BORDER_BAR_SIZE);
    expect(insets.bottom).toBe(BORDER_BAR_SIZE);
    expect(insets.left).toBe(BORDER_BAR_SIZE);
  });

  it("skips hidden borders", () => {
    const borders = [
      makeBorder("left", { dockState: "collapsed", show: false } as any),
    ];
    const insets = computeInsets(borders);
    expect(insets.left).toBe(0);
  });
});

describe("BORDER_BAR_SIZE constant", () => {
  it("is 38px", () => {
    expect(BORDER_BAR_SIZE).toBe(38);
  });
});

describe("Border store integration", () => {
  let bridge: LayoutBridge | undefined;

  afterEach(() => {
    bridge?.dispose();
    bridge = undefined;
  });

  it("store contains border nodes from model", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      const borders = bridge.store.borders as IJsonBorderNode[];
      expect(borders).toBeDefined();
      expect(borders.length).toBe(4);

      const locations = borders.map((b) => b.location);
      expect(locations).toContain("top");
      expect(locations).toContain("bottom");
      expect(locations).toContain("left");
      expect(locations).toContain("right");

      dispose();
    });
  });

  it("border children are preserved in store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      const borders = bridge.store.borders as IJsonBorderNode[];
      const leftBorder = borders.find((b) => b.location === "left");
      expect(leftBorder).toBeDefined();
      expect(leftBorder!.children).toHaveLength(1);
      expect(leftBorder!.children![0].name).toBe("Explorer");

      dispose();
    });
  });

  it("setDockState action updates border dockState in store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      const bordersBefore = bridge.store.borders as IJsonBorderNode[];
      const leftBefore = bordersBefore.find((b) => b.location === "left");
      expect(leftBefore!.dockState).toBe("expanded");

      model.doAction(Action.setDockState("border_left", "collapsed"));

      const bordersAfter = bridge.store.borders as IJsonBorderNode[];
      const leftAfter = bordersAfter.find((b) => b.location === "left");
      expect(leftAfter!.dockState).toBe("collapsed");

      dispose();
    });
  });

  it("selectTab on border tab updates border selected index in store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      model.doAction(Action.selectTab("tab-left-1"));

      const borders = bridge.store.borders as IJsonBorderNode[];
      const leftBorder = borders.find((b) => b.location === "left");
      expect(leftBorder!.selected).toBe(0);

      dispose();
    });
  });

  it("dock state round-trip: collapse then expand", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      model.doAction(Action.setDockState("border_right", "collapsed"));
      let borders = bridge.store.borders as IJsonBorderNode[];
      let right = borders.find((b) => b.location === "right");
      expect(right!.dockState).toBe("collapsed");

      model.doAction(Action.setDockState("border_right", "expanded"));
      borders = bridge.store.borders as IJsonBorderNode[];
      right = borders.find((b) => b.location === "right");
      expect(right!.dockState).toBe("expanded");

      dispose();
    });
  });
});

describe("CSS class constants for borders", () => {
  it("FLEXLAYOUT__BORDER is correct", () => {
    expect(CLASSES.FLEXLAYOUT__BORDER).toBe("flexlayout__border");
  });

  it("FLEXLAYOUT__BORDER_ prefix is correct", () => {
    expect(CLASSES.FLEXLAYOUT__BORDER_).toBe("flexlayout__border_");
  });

  it("FLEXLAYOUT__BORDER__COLLAPSED is correct", () => {
    expect(CLASSES.FLEXLAYOUT__BORDER__COLLAPSED).toBe("flexlayout__border--collapsed");
  });

  it("FLEXLAYOUT__BORDER_BUTTON is correct", () => {
    expect(CLASSES.FLEXLAYOUT__BORDER_BUTTON).toBe("flexlayout__border_button");
  });

  it("FLEXLAYOUT__BORDER_BUTTON__SELECTED is correct", () => {
    expect(CLASSES.FLEXLAYOUT__BORDER_BUTTON__SELECTED).toBe("flexlayout__border_button--selected");
  });

  it("FLEXLAYOUT__BORDER_BUTTON__UNSELECTED is correct", () => {
    expect(CLASSES.FLEXLAYOUT__BORDER_BUTTON__UNSELECTED).toBe("flexlayout__border_button--unselected");
  });

  it("FLEXLAYOUT__BORDER_DOCK_BUTTON is correct", () => {
    expect(CLASSES.FLEXLAYOUT__BORDER_DOCK_BUTTON).toBe("flexlayout__border_dock_button");
  });

  it("FLEXLAYOUT__LAYOUT_BORDER_CONTAINER is correct", () => {
    expect(CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER).toBe("flexlayout__layout_border_container");
  });

  it("FLEXLAYOUT__LAYOUT_BORDER_CONTAINER_INNER is correct", () => {
    expect(CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER_INNER).toBe("flexlayout__layout_border_container_inner");
  });
});
