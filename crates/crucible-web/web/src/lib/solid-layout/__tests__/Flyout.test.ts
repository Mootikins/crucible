import { describe, it, expect, afterEach } from "vitest";
import { createRoot } from "solid-js";
import { Model } from "../../flexlayout/model/Model";
import { Action } from "../../flexlayout/model/Action";
import type { IJsonModel, IJsonBorderNode, IJsonTabNode } from "../../flexlayout/types";
import { createLayoutBridge, type LayoutBridge } from "../bridge";
import { DockLocation } from "../../flexlayout/core/DockLocation";
import { BORDER_BAR_SIZE } from "../components/BorderStrip";

import {
  findFlyoutBorder,
  toDockLocation,
  buildFlyoutRect,
  flyoutEdgeName,
  isAutoHide,
} from "../components/Flyout";

function makeBorder(
  location: string,
  overrides: Partial<IJsonBorderNode> = {},
): IJsonBorderNode {
  return {
    type: "border",
    id: `border_${location}`,
    location,
    children: [],
    dockState: "collapsed",
    selected: -1,
    priority: 0,
    enableDock: true,
    ...overrides,
  } as IJsonBorderNode;
}

function makeTab(id: string, name: string): IJsonTabNode {
  return { type: "tab", id, name, component: "text" };
}

const flyoutLayout: IJsonModel = {
  global: {},
  borders: [
    makeBorder("top"),
    makeBorder("bottom"),
    makeBorder("left", {
      children: [makeTab("tab-left-1", "Explorer"), makeTab("tab-left-2", "Search")],
    }),
    makeBorder("right", {
      children: [makeTab("tab-right-1", "Outline")],
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
        children: [makeTab("tab1", "Main")],
      },
    ],
  },
};

function makeModel(json: IJsonModel = flyoutLayout): Model {
  return Model.fromJson(json);
}

describe("findFlyoutBorder", () => {
  it("returns undefined when no border has flyoutTabId", () => {
    const borders = [
      makeBorder("left", { children: [makeTab("t1", "A")] }),
      makeBorder("right", { children: [makeTab("t2", "B")] }),
    ];
    expect(findFlyoutBorder(borders)).toBeUndefined();
  });

  it("finds the border with an active flyoutTabId", () => {
    const borders = [
      makeBorder("left", {
        children: [makeTab("t1", "A"), makeTab("t2", "B")],
        flyoutTabId: "t2",
      } as any),
      makeBorder("right", { children: [makeTab("t3", "C")] }),
    ];
    const result = findFlyoutBorder(borders);
    expect(result).toBeDefined();
    expect(result!.border.location).toBe("left");
    expect(result!.tab.id).toBe("t2");
    expect(result!.tabIndex).toBe(1);
  });

  it("returns undefined when flyoutTabId references a non-existent tab", () => {
    const borders = [
      makeBorder("left", {
        children: [makeTab("t1", "A")],
        flyoutTabId: "nonexistent",
      } as any),
    ];
    expect(findFlyoutBorder(borders)).toBeUndefined();
  });

  it("returns the first border with a flyout when multiple have flyoutTabId", () => {
    const borders = [
      makeBorder("left", {
        children: [makeTab("t1", "A")],
        flyoutTabId: "t1",
      } as any),
      makeBorder("right", {
        children: [makeTab("t2", "B")],
        flyoutTabId: "t2",
      } as any),
    ];
    const result = findFlyoutBorder(borders);
    expect(result!.border.location).toBe("left");
  });

  it("handles empty borders array", () => {
    expect(findFlyoutBorder([])).toBeUndefined();
  });
});

describe("toDockLocation", () => {
  it("maps left to DockLocation.LEFT", () => {
    expect(toDockLocation("left")).toBe(DockLocation.LEFT);
  });

  it("maps right to DockLocation.RIGHT", () => {
    expect(toDockLocation("right")).toBe(DockLocation.RIGHT);
  });

  it("maps top to DockLocation.TOP", () => {
    expect(toDockLocation("top")).toBe(DockLocation.TOP);
  });

  it("maps bottom to DockLocation.BOTTOM", () => {
    expect(toDockLocation("bottom")).toBe(DockLocation.BOTTOM);
  });

  it("defaults to LEFT for unknown location", () => {
    expect(toDockLocation("center")).toBe(DockLocation.LEFT);
  });
});

describe("buildFlyoutRect", () => {
  const leftBorder = makeBorder("left", { size: 200 } as any);
  const allBorders = [
    makeBorder("top"),
    makeBorder("bottom"),
    leftBorder,
    makeBorder("right"),
  ];

  it("returns a rect with positive dimensions for left border", () => {
    const rect = buildFlyoutRect(leftBorder, 0, 1000, 800, allBorders);
    expect(rect.width).toBeGreaterThanOrEqual(100);
    expect(rect.height).toBeGreaterThanOrEqual(100);
  });

  it("positions left flyout at the left inset edge", () => {
    const rect = buildFlyoutRect(leftBorder, 0, 1000, 800, allBorders);
    expect(rect.x).toBe(BORDER_BAR_SIZE);
  });

  it("positions right flyout at the right edge", () => {
    const rightBorder = makeBorder("right", { size: 200 } as any);
    const rect = buildFlyoutRect(rightBorder, 0, 1000, 800, allBorders);
    expect(rect.x + rect.width).toBeLessThanOrEqual(1000);
    expect(rect.x).toBeGreaterThan(500);
  });

  it("positions top flyout at the top inset edge", () => {
    const topBorder = makeBorder("top", { size: 200 } as any);
    const rect = buildFlyoutRect(topBorder, 0, 1000, 800, allBorders);
    expect(rect.y).toBe(BORDER_BAR_SIZE);
  });

  it("positions bottom flyout at the bottom edge", () => {
    const bottomBorder = makeBorder("bottom", { size: 200 } as any);
    const rect = buildFlyoutRect(bottomBorder, 0, 1000, 800, allBorders);
    expect(rect.y + rect.height).toBeLessThanOrEqual(800);
    expect(rect.y).toBeGreaterThan(400);
  });

  it("uses tabButtonRect for vertical alignment when provided", () => {
    const tabButtonRect = { x: 0, y: 200, width: 38, height: 30 };
    const rect = buildFlyoutRect(leftBorder, 0, 1000, 800, allBorders, tabButtonRect);
    expect(rect.y).toBe(200);
  });

  it("defaults primarySize to 200 when border has no size", () => {
    const noSizeBorder = makeBorder("left");
    const rect = buildFlyoutRect(noSizeBorder, 0, 1000, 800, allBorders);
    expect(rect.width).toBe(200);
  });

  it("respects MIN_FLYOUT_SIZE for small primarySize", () => {
    const smallBorder = makeBorder("left", { size: 10 } as any);
    const rect = buildFlyoutRect(smallBorder, 0, 1000, 800, allBorders);
    expect(rect.width).toBeGreaterThanOrEqual(100);
  });
});

describe("flyoutEdgeName", () => {
  it("returns left for left", () => {
    expect(flyoutEdgeName("left")).toBe("left");
  });

  it("returns right for right", () => {
    expect(flyoutEdgeName("right")).toBe("right");
  });

  it("returns top for top", () => {
    expect(flyoutEdgeName("top")).toBe("top");
  });

  it("returns bottom for bottom", () => {
    expect(flyoutEdgeName("bottom")).toBe("bottom");
  });

  it("defaults to bottom for unknown location", () => {
    expect(flyoutEdgeName("center")).toBe("bottom");
  });
});

describe("isAutoHide", () => {
  it("returns false when enableAutoHide is not set", () => {
    expect(isAutoHide(makeBorder("left"))).toBe(false);
  });

  it("returns true when enableAutoHide is true", () => {
    expect(isAutoHide(makeBorder("left", { enableAutoHide: true } as any))).toBe(true);
  });

  it("returns false when enableAutoHide is false", () => {
    expect(isAutoHide(makeBorder("left", { enableAutoHide: false } as any))).toBe(false);
  });
});

describe("Flyout store integration", () => {
  let bridge: LayoutBridge | undefined;

  afterEach(() => {
    bridge?.dispose();
    bridge = undefined;
  });

  it("openFlyout sets flyoutTabId on border in store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      model.doAction(Action.openFlyout("border_left", "tab-left-1"));

      const borders = bridge.store.borders as IJsonBorderNode[];
      const leftBorder = borders.find((b) => b.location === "left");
      expect(leftBorder!.flyoutTabId).toBe("tab-left-1");

      dispose();
    });
  });

  it("closeFlyout clears flyoutTabId from store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      model.doAction(Action.openFlyout("border_left", "tab-left-1"));
      model.doAction(Action.closeFlyout("border_left"));

      const borders = bridge.store.borders as IJsonBorderNode[];
      const leftBorder = borders.find((b) => b.location === "left");
      expect(leftBorder!.flyoutTabId).toBeUndefined();

      dispose();
    });
  });

  it("findFlyoutBorder detects active flyout from store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      model.doAction(Action.openFlyout("border_left", "tab-left-2"));

      const borders = bridge.store.borders as IJsonBorderNode[];
      const result = findFlyoutBorder(borders);
      expect(result).toBeDefined();
      expect(result!.border.location).toBe("left");
      expect(result!.tab.id).toBe("tab-left-2");
      expect(result!.tabIndex).toBe(1);

      dispose();
    });
  });

  it("opening flyout on different tab updates store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      model.doAction(Action.openFlyout("border_left", "tab-left-1"));
      let borders = bridge.store.borders as IJsonBorderNode[];
      let result = findFlyoutBorder(borders);
      expect(result!.tab.id).toBe("tab-left-1");

      model.doAction(Action.openFlyout("border_left", "tab-left-2"));
      borders = bridge.store.borders as IJsonBorderNode[];
      result = findFlyoutBorder(borders);
      expect(result!.tab.id).toBe("tab-left-2");

      dispose();
    });
  });

  it("flyout on right border positions correctly via buildFlyoutRect", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      model.doAction(Action.openFlyout("border_right", "tab-right-1"));

      const borders = bridge.store.borders as IJsonBorderNode[];
      const result = findFlyoutBorder(borders);
      expect(result).toBeDefined();
      expect(result!.border.location).toBe("right");

      const rect = buildFlyoutRect(
        result!.border,
        result!.tabIndex,
        1000,
        800,
        borders,
      );
      expect(rect.x).toBeGreaterThan(500);
      expect(rect.width).toBeGreaterThanOrEqual(100);

      dispose();
    });
  });
});
